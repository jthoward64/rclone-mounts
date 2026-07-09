// SPDX-License-Identifier: GPL-2.0-or-later

//! Backend abstraction: load current state, apply changesets atomically.
//!
//! `Backend` is the trait the KCM consumes. It has two production impls:
//! - [`LocalBackend`] — runs everything in-process via a [`UnitStore`] +
//!   [`SystemdControl`]. Used directly for user-mode and by the helper for
//!   system-mode (where it lives in the privileged process, not the KCM).
//! - [`HelperBackend`] — thin zbus client that forwards `load`/`apply` to the
//!   `dev.jthoward.RcloneMounts.Helper1` interface. Used by the KCM in
//!   system-mode; never sees plaintext credentials inside the privileged
//!   process boundary.
//!
//! Mount lifecycle (Start/Stop) is intentionally not part of `apply` — those
//! are live actions the UI fires independently via [`SystemdControl`] and
//! don't participate in the Apply/Cancel dirty-state model.

use crate::control::SystemdControl;
use crate::credentials::Scope;
use crate::mount::Mount;
use crate::rclone_config::Document;
use crate::source::{SourceDef, SourceKind};
use crate::store::UnitStore;
use crate::unit_writer;
use crate::{credentials, rclone_cli, Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Operations the KCM (and any other frontend) performs against a mount backend.
/// Impls choose whether to run locally or proxy via D-Bus.
#[async_trait]
pub trait Backend: Send + Sync {
    async fn load(&self) -> Result<State>;
    async fn apply(&self, changeset: Changeset) -> Result<()>;

    // Live mount lifecycle. Deliberately separate from `apply`/the dirty model:
    // these act immediately on the already-applied units and don't participate
    // in Apply/Cancel. They go through the `Backend` abstraction (not a raw
    // `SystemdControl`) so system-scope can proxy them to the helper, which owns
    // the privileged systemd.
    async fn start_mount(&self, name: &str) -> Result<()>;
    async fn stop_mount(&self, name: &str) -> Result<()>;
    /// systemd `ActiveState` for the mount's unit: `active`, `inactive`,
    /// `failed`, `activating`, …
    async fn mount_status(&self, name: &str) -> Result<String>;

    /// Store (or, if both are empty, clear) the system-scope admin override
    /// for an OAuth-style kind's client id/secret — the middle tier of the
    /// build-time < admin-override < per-source-user-override precedence.
    /// Only meaningful in system scope; a user-scope backend has no
    /// authority to set an org-wide default and rejects this.
    async fn set_provider_override(&self, kind: SourceKind, client_id: &str, client_secret: &str) -> Result<()>;
    /// The stored admin override for `kind`, if any. `None` in user scope
    /// (there is no admin override to read there) or if nothing was set.
    /// Never surfaced to QML raw — only consumed by credential-precedence
    /// resolution and a has-value boolean (mirrors `has_secret`).
    async fn provider_override(&self, kind: SourceKind) -> Result<Option<(String, String)>>;
}

/// The systemd unit name backing a mount. Single source of truth for the
/// `rclone-mount-<name>.service` convention shared by the writer and control.
pub fn mount_unit_name(name: &str) -> String {
    format!("rclone-mount-{name}.service")
}

/// Snapshot of what's currently on disk for this scope.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub sources: Vec<SourceMetadata>,
    pub mounts: Vec<Mount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMetadata {
    /// Internal id: rclone remote / file key. Slug, validated, immutable.
    pub name: String,
    /// Freeform name shown in the UI.
    pub display_name: String,
    pub kind: String,
    pub options: BTreeMap<String, String>,
    pub has_secret: bool,
}

/// A batch of edits to apply atomically. The backend validates the whole batch,
/// stages writes, then commits and reloads systemd — there is no partial-apply
/// state for callers to recover from.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Changeset {
    pub upsert_sources: Vec<SourceDef>,
    pub delete_sources: Vec<String>,
    pub upsert_mounts: Vec<Mount>,
    pub delete_mounts: Vec<String>,
}

/// On-disk format for the KCM's mount-state file. One TOML file with all mounts.
#[derive(Debug, Default, Serialize, Deserialize)]
struct MountsFile {
    mount: Vec<Mount>,
}

/// In-process backend. The helper's process runs one of these against `/etc`;
/// the KCM (user-mode path) runs one against `$HOME`.
pub struct LocalBackend {
    pub store: Box<dyn UnitStore>,
    pub control: Box<dyn SystemdControl>,
    pub scope: Scope,
    /// Path or specifier where credential blobs live, expressed as systemd
    /// understands it for `LoadCredentialEncrypted=`. Per-source path appended
    /// at render time as `{credential_dir_spec}/{source_name}`.
    pub credential_dir_spec: String,
}

impl LocalBackend {
    /// Construct the user-scope backend: stores under `$HOME`, session-bus
    /// systemd control, user-scope credential encryption. This is the path the
    /// KCM drives directly (no helper, no Polkit) for a user's own mounts.
    pub async fn new_user() -> Result<Self> {
        use crate::control::session::SessionSystemd;
        use crate::store::local::LocalUnitStore;
        Ok(Self {
            store: Box::new(LocalUnitStore::new_user_default()?),
            control: Box::new(SessionSystemd::new().await?),
            scope: Scope::User,
            credential_dir_spec: "%h/.config/rclone-mounts/credentials".into(),
        })
    }
}

impl LocalBackend {
    /// Decrypt the existing credential for `name` (if any) and pull out
    /// exactly the secret keys this `kind` is known to carry, so a
    /// params-only source edit keeps whatever secrets the user already set.
    /// Returns an empty map when there's no credential or it carries none of
    /// `kind`'s secret keys.
    fn existing_secrets(&self, name: &str, kind: SourceKind) -> Result<BTreeMap<String, String>> {
        let Some(encrypted) = self.store.read_credential(name)? else {
            return Ok(BTreeMap::new());
        };
        let plain = credentials::decrypt(self.scope, &cred_id(), &encrypted)?;
        let text = String::from_utf8_lossy(&plain);
        Ok(extract_secrets(&text, secret_keys_for(kind)))
    }
}

impl State {
    /// Fold a changeset onto this state without persisting or validating,
    /// returning what the on-disk state *would* look like. The KCM uses this to
    /// render pending edits (the dirty preview) before the user clicks Apply.
    /// Reference validation is deliberately skipped here so a mid-edit dangling
    /// reference stays visible; `apply` enforces it.
    pub fn preview(&self, cs: &Changeset) -> State {
        fold(self.clone(), cs)
    }
}

#[async_trait]
impl Backend for LocalBackend {
    async fn load(&self) -> Result<State> {
        let sources_text = self.store.read_sources_conf()?;
        let doc = Document::parse(&sources_text)?;
        let mut sources = Vec::new();
        for name in doc.sections() {
            let kind_tag = doc.get(name, "type").unwrap_or("");
            sources.push(SourceMetadata {
                name: name.to_string(),
                // Old data (pre display names) has no display_name key; fall
                // back to the id so it still shows something sensible.
                display_name: doc.get(name, "display_name").unwrap_or(name).to_string(),
                kind: kind_tag.to_string(),
                options: collect_section_options(&doc, name),
                // A secret is "present" when the source's decrypted blob
                // carries any of that kind's known secret keys. The blob
                // exists for every source (it holds type+options too), so
                // file existence alone isn't enough — this mirrors the
                // `new_secrets.is_empty()` semantic the dirty-preview fold
                // uses, so the flag is stable across apply.
                has_secret: !self
                    .existing_secrets(name, SourceKind::from_tag(kind_tag).unwrap_or(SourceKind::Smb))?
                    .is_empty(),
            });
        }

        let mounts_text = self.store.read_mounts_state()?;
        let mut mounts = if mounts_text.is_empty() {
            Vec::new()
        } else {
            toml::from_str::<MountsFile>(&mounts_text)
                .map_err(|e| Error::ConfigParse(format!("mounts.toml: {e}")))?
                .mount
        };
        // Old data (pre display names) deserializes display_name as empty.
        for m in &mut mounts {
            if m.display_name.is_empty() {
                m.display_name = m.name.clone();
            }
        }

        Ok(State { sources, mounts })
    }

    async fn apply(&self, changeset: Changeset) -> Result<()> {
        // 1. Validate names up front.
        for src in &changeset.upsert_sources {
            unit_writer::validate_name(&src.name)?;
        }
        for name in &changeset.delete_sources {
            unit_writer::validate_name(name)?;
        }
        for mount in &changeset.upsert_mounts {
            unit_writer::validate_name(&mount.name)?;
            unit_writer::validate_name(&mount.source)?;
        }
        for name in &changeset.delete_mounts {
            unit_writer::validate_name(name)?;
        }

        // 2. Fold the changeset into current state and cross-validate.
        let current = self.load().await?;
        let target = fold(current, &changeset);
        validate_references(&target)?;

        // 3. Persist sources.conf (round-trip preserving so untouched
        //    sections and outside-section comments survive).
        let mut doc = Document::parse(&self.store.read_sources_conf()?)?;
        for name in &changeset.delete_sources {
            doc.remove_section(name);
        }
        for src in &changeset.upsert_sources {
            // Replace the section wholesale: remove then re-set every key. This
            // loses any hand-edited comments *inside* the section, but that's
            // an acceptable tradeoff against leaving stale keys behind on a
            // type change. Comments elsewhere are preserved.
            doc.remove_section(&src.name);
            doc.set(&src.name, "type", source_kind_str(src.kind));
            // Our own metadata, not an rclone option; rclone never reads this
            // file (it reads the credential blob), so it's harmless here and is
            // filtered back out on load.
            doc.set(&src.name, "display_name", &src.display_name);
            for (k, v) in &src.options {
                doc.set(&src.name, k, v);
            }
        }
        self.store.write_sources_conf(&doc.render())?;

        // 4. Credentials. The encrypted blob is the *complete* rclone remote
        //    section (type + options + secrets), so it must be rebuilt on
        //    every source upsert — even a params-only edit, since the params
        //    live inside the blob. Secret keys not present in `new_secrets`
        //    carry forward from the existing blob (decrypted and re-extracted
        //    per that kind's known secret keys). This is the one place stored
        //    secrets are read back, and only by the process that owns the
        //    store (the helper for system scope; the KCM for user scope).
        for src in &changeset.upsert_sources {
            let mut secrets = self.existing_secrets(&src.name, src.kind)?;
            for (key, secret) in &src.new_secrets {
                let value = if secret.obscure {
                    rclone_cli::obscure(&secret.value)?
                } else {
                    secret.value.clone()
                };
                secrets.insert(key.clone(), value);
            }
            let blob = encode_credential_blob(src, &secrets);
            let encrypted = credentials::encrypt(self.scope, &cred_id(), blob.as_bytes())?;
            self.store.write_credential(&src.name, &encrypted)?;
        }
        for name in &changeset.delete_sources {
            self.store.delete_credential(name)?;
        }

        // 5. Persist mount state file (canonical form for KCM round-trip).
        let mounts_file = MountsFile { mount: target.mounts.clone() };
        let toml_text = toml::to_string_pretty(&mounts_file)
            .map_err(|e| Error::Systemd(format!("serialize mounts.toml: {e}")))?;
        self.store.write_mounts_state(&toml_text)?;

        // 6. Render and write .service units for upserts; delete units for removals.
        for name in &changeset.delete_mounts {
            // Drop any autostart symlink before removing the file (best effort;
            // a not-enabled unit just no-ops).
            let _ = self.control.disable(&mount_unit_name(name)).await;
            self.store.delete_unit(name)?;
        }
        // Re-render every mount, not just upserts: if a source it references
        // was edited, the unit text may need updating even though the Mount
        // itself didn't change. Cheap enough to do unconditionally.
        for mount in &target.mounts {
            let ctx = unit_writer::Context {
                credential_path: format!("{}/{}", self.credential_dir_spec, mount.source),
                ..Default::default()
            };
            let unit_text = unit_writer::render(mount, &ctx)?;
            self.store.write_unit(&mount.name, &unit_text)?;
        }

        // 7. daemon-reload so systemd picks up the new/changed unit files.
        self.control.reload().await?;

        // 8. Reconcile "mount automatically": enable units the user wants up at
        //    login, disable the rest. Enabling installs the WantedBy symlink so
        //    the mount comes up on next login; it does not start the mount now
        //    (that's the explicit Start action).
        for mount in &target.mounts {
            let unit = mount_unit_name(&mount.name);
            if mount.enabled {
                self.control.enable(&unit).await?;
            } else {
                self.control.disable(&unit).await?;
            }
        }
        Ok(())
    }

    async fn start_mount(&self, name: &str) -> Result<()> {
        unit_writer::validate_name(name)?;
        self.control.start(&mount_unit_name(name)).await
    }

    async fn stop_mount(&self, name: &str) -> Result<()> {
        unit_writer::validate_name(name)?;
        self.control.stop(&mount_unit_name(name)).await
    }

    async fn mount_status(&self, name: &str) -> Result<String> {
        unit_writer::validate_name(name)?;
        self.control.active_state(&mount_unit_name(name)).await
    }

    async fn set_provider_override(&self, kind: SourceKind, client_id: &str, client_secret: &str) -> Result<()> {
        if !matches!(self.scope, Scope::System) {
            return Err(Error::Systemd(
                "Provider credential overrides are system-scope only.".into(),
            ));
        }
        let name = provider_override_name(kind);
        if client_id.is_empty() && client_secret.is_empty() {
            self.store.delete_credential(&name)?;
            return Ok(());
        }
        let blob = format!("[{name}]\nclient_id = {client_id}\nclient_secret = {client_secret}\n");
        let encrypted = credentials::encrypt(self.scope, &cred_id(), blob.as_bytes())?;
        self.store.write_credential(&name, &encrypted)?;
        Ok(())
    }

    async fn provider_override(&self, kind: SourceKind) -> Result<Option<(String, String)>> {
        // Only ever meaningful in system scope: a user-scope process can't
        // decrypt system-scope credentials anyway (systemd-creds keys them
        // off TPM2/the host key), so there's nothing to read here.
        if !matches!(self.scope, Scope::System) {
            return Ok(None);
        }
        let name = provider_override_name(kind);
        let Some(encrypted) = self.store.read_credential(&name)? else {
            return Ok(None);
        };
        let plain = credentials::decrypt(self.scope, &cred_id(), &encrypted)?;
        let text = String::from_utf8_lossy(&plain);
        let doc = Document::parse(&text)?;
        let id = doc.get(&name, "client_id").unwrap_or("").to_string();
        let secret = doc.get(&name, "client_secret").unwrap_or("").to_string();
        if id.is_empty() || secret.is_empty() {
            return Ok(None);
        }
        Ok(Some((id, secret)))
    }
}

/// Reserved pseudo-source name for a kind's admin-override credential,
/// stored via the same encrypted-credential machinery as a real source but
/// never reachable as one: it starts with `_`, which `unit_writer::validate_name`
/// already rejects for any user-supplied source/mount name.
fn provider_override_name(kind: SourceKind) -> String {
    format!("__provider_override_{}", kind.as_tag())
}

/// zbus client backend that forwards everything to the helper. Used by the KCM
/// in system mode; the helper itself owns the real [`LocalBackend`] inside its
/// privileged process.
///
/// Wire format for `apply` is TOML; chosen for human-diffability in helper logs
/// over JSON. Same `Changeset` type serialized on both sides.
pub struct HelperBackend {
    pub conn: zbus::Connection,
}

const HELPER_BUS: &str = "dev.jthoward.RcloneMounts.Helper";
const HELPER_PATH: &str = "/dev/jthoward/RcloneMounts/Helper";
const HELPER_IFACE: &str = "dev.jthoward.RcloneMounts.Helper1";

impl HelperBackend {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            conn: zbus::Connection::system().await?,
        })
    }
}

/// Whether the privileged helper is reachable at all — i.e. whether system
/// scope is worth offering in the UI. Checks the bus itself (is the service
/// name owned, or D-Bus-activatable) rather than calling into our own
/// interface, so this never raises a Polkit prompt and never surfaces a
/// scary error banner just because the helper isn't installed/enabled.
pub async fn system_scope_available() -> bool {
    let Ok(conn) = zbus::Connection::system().await else {
        return false;
    };
    let Ok(proxy) = zbus::fdo::DBusProxy::new(&conn).await else {
        return false;
    };
    if matches!(proxy.name_has_owner(HELPER_BUS.try_into().unwrap()).await, Ok(true)) {
        return true;
    }
    match proxy.list_activatable_names().await {
        Ok(names) => names.iter().any(|n| n.as_str() == HELPER_BUS),
        Err(_) => false,
    }
}

#[async_trait]
impl Backend for HelperBackend {
    async fn load(&self) -> Result<State> {
        let proxy = zbus::Proxy::new(&self.conn, HELPER_BUS, HELPER_PATH, HELPER_IFACE).await?;
        let sources: Vec<(String, String, String, BTreeMap<String, String>, bool)> =
            proxy.call("ListSources", &()).await?;
        // Mount tuning options ride the wire as a JSON blob (the fifth element):
        // `MountOptions` is a small struct with optional/enum fields that don't
        // map cleanly onto D-Bus basic types, and JSON keeps the helper and KCM
        // agreeing on one serde representation.
        let mounts: Vec<(String, String, String, String, String, String, bool)> =
            proxy.call("ListMounts", &()).await?;
        Ok(State {
            sources: sources
                .into_iter()
                .map(|(name, display_name, kind, options, has_secret)| SourceMetadata {
                    name,
                    display_name,
                    kind,
                    options,
                    has_secret,
                })
                .collect(),
            mounts: mounts
                .into_iter()
                .map(|(name, display_name, source, subpath, mountpoint, options_json, enabled)| Mount {
                    name,
                    display_name,
                    source,
                    subpath,
                    mountpoint: mountpoint.into(),
                    // Tolerate a malformed/empty blob by falling back to
                    // defaults rather than failing the whole load.
                    options: serde_json::from_str(&options_json).unwrap_or_default(),
                    enabled,
                })
                .collect(),
        })
    }

    async fn apply(&self, changeset: Changeset) -> Result<()> {
        let encoded = toml::to_string(&changeset)
            .map_err(|e| Error::Systemd(format!("serialize changeset: {e}")))?;
        let proxy = zbus::Proxy::new(&self.conn, HELPER_BUS, HELPER_PATH, HELPER_IFACE).await?;
        let _: () = proxy.call("ApplyChanges", &(encoded,)).await?;
        Ok(())
    }

    // The helper owns the system-bus systemd; it constructs the unit name and
    // runs the action under its own privilege. (Helper-side methods land with
    // system-mode wiring.)
    async fn start_mount(&self, name: &str) -> Result<()> {
        let proxy = zbus::Proxy::new(&self.conn, HELPER_BUS, HELPER_PATH, HELPER_IFACE).await?;
        let _: () = proxy.call("StartMount", &(name,)).await?;
        Ok(())
    }

    async fn stop_mount(&self, name: &str) -> Result<()> {
        let proxy = zbus::Proxy::new(&self.conn, HELPER_BUS, HELPER_PATH, HELPER_IFACE).await?;
        let _: () = proxy.call("StopMount", &(name,)).await?;
        Ok(())
    }

    async fn mount_status(&self, name: &str) -> Result<String> {
        let proxy = zbus::Proxy::new(&self.conn, HELPER_BUS, HELPER_PATH, HELPER_IFACE).await?;
        Ok(proxy.call("MountStatus", &(name,)).await?)
    }

    async fn set_provider_override(&self, kind: SourceKind, client_id: &str, client_secret: &str) -> Result<()> {
        let proxy = zbus::Proxy::new(&self.conn, HELPER_BUS, HELPER_PATH, HELPER_IFACE).await?;
        let _: () = proxy
            .call("SetProviderOverride", &(kind.as_tag(), client_id, client_secret))
            .await?;
        Ok(())
    }

    async fn provider_override(&self, kind: SourceKind) -> Result<Option<(String, String)>> {
        let proxy = zbus::Proxy::new(&self.conn, HELPER_BUS, HELPER_PATH, HELPER_IFACE).await?;
        let (has_value, client_id, client_secret): (bool, String, String) =
            proxy.call("ProviderOverride", &(kind.as_tag(),)).await?;
        Ok(has_value.then_some((client_id, client_secret)))
    }
}

/// Apply a changeset to a state in memory. Pure structural fold — it does *not*
/// check that every mount still references a live source; that's
/// [`validate_references`], called only on the apply path. The UI's dirty
/// preview ([`State::preview`]) folds without validating so a transient dangling
/// reference (e.g. mid-edit, source deleted before its mount) stays visible
/// instead of silently reverting the whole preview.
fn fold(mut state: State, cs: &Changeset) -> State {
    for name in &cs.delete_sources {
        state.sources.retain(|s| s.name != *name);
    }
    for src in &cs.upsert_sources {
        let kind = source_kind_str(src.kind).to_string();
        let entry = SourceMetadata {
            name: src.name.clone(),
            display_name: src.display_name.clone(),
            kind,
            options: src.options.clone(),
            has_secret: !src.new_secrets.is_empty(),
        };
        if let Some(existing) = state.sources.iter_mut().find(|s| s.name == src.name) {
            let preserved = existing.has_secret;
            *existing = entry;
            if src.new_secrets.is_empty() {
                existing.has_secret = preserved;
            }
        } else {
            state.sources.push(entry);
        }
    }

    for name in &cs.delete_mounts {
        state.mounts.retain(|m| m.name != *name);
    }
    for mount in &cs.upsert_mounts {
        if let Some(existing) = state.mounts.iter_mut().find(|m| m.name == mount.name) {
            *existing = mount.clone();
        } else {
            state.mounts.push(mount.clone());
        }
    }

    state
}

/// Every mount must reference a source that exists in `state`. Enforced on the
/// apply path so we never write a unit pointing at a missing remote.
fn validate_references(state: &State) -> Result<()> {
    let source_names: std::collections::HashSet<_> =
        state.sources.iter().map(|s| s.name.as_str()).collect();
    for m in &state.mounts {
        if !source_names.contains(m.source.as_str()) {
            return Err(Error::NotFound(format!(
                "The mount “{}” uses the source “{}”, which no longer exists. \
                 Pick a different source or remove the mount.",
                m.name, m.source
            )));
        }
    }
    Ok(())
}

/// Every connection option in a source's section. We read all keys rather than
/// probe a fixed allowlist so a hand-edited `sources.conf` (or a new rclone
/// option we don't model yet) round-trips instead of being silently dropped.
/// The two keys we own — `type` (surfaced separately as `kind`) and our
/// `display_name` metadata — are filtered out so they never leak into the
/// options map, the credential blob, or the rclone remote definition.
fn collect_section_options(doc: &Document, section: &str) -> BTreeMap<String, String> {
    doc.section_entries(section)
        .into_iter()
        .filter(|(k, _)| *k != "type" && *k != "display_name")
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn source_kind_str(kind: crate::source::SourceKind) -> &'static str {
    kind.as_tag()
}

/// The rclone config keys that hold sensitive material for each kind, kept
/// out of `sources.conf` (plaintext) and stored only in the encrypted
/// credential blob. An explicit, auditable table rather than "whatever's
/// left over" — misclassifying a field here is a security bug, not just a
/// UX one.
pub fn secret_keys_for(kind: crate::source::SourceKind) -> &'static [&'static str] {
    use crate::source::SourceKind::*;
    match kind {
        Smb | WebDav | Sftp | Ftp => &["pass"],
        Drive => &["token", "client_secret"],
        IcloudDrive => &["password", "trust_token"],
    }
}

/// The complete rclone remote section that becomes the credential payload:
/// `type`, every connection option, and every secret value for this kind.
/// systemd decrypts this into `%d/rclone-conf` at unit start; rclone reads it
/// via `--config=%d/rclone-conf`, so the mount is fully self-contained and
/// never touches `sources.conf` (which exists only as the KCM's editable
/// record). `secrets` values are already in their final on-disk form
/// (obscured where rclone requires it) — see [`SourceDef::new_secrets`].
fn encode_credential_blob(src: &SourceDef, secrets: &BTreeMap<String, String>) -> String {
    let mut out = format!("[{}]\ntype = {}\n", src.name, source_kind_str(src.kind));
    for (k, v) in &src.options {
        let _ = writeln!(out, "{k} = {v}");
    }
    for key in secret_keys_for(src.kind) {
        if let Some(value) = secrets.get(*key) {
            let _ = writeln!(out, "{key} = {value}");
        }
    }
    out
}

/// Pull exactly `keys` out of a decrypted rclone-conf blob, matching each key
/// exactly (not prefixes of it), tolerating optional spaces around `=`.
fn extract_secrets(blob: &str, keys: &[&str]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in blob.lines() {
        let Some(eq_idx) = line.find('=') else { continue };
        let key = line[..eq_idx].trim();
        if keys.contains(&key) {
            out.insert(key.to_string(), line[eq_idx + 1..].trim().to_string());
        }
    }
    out
}

fn cred_id() -> String {
    // The credential id woven into the encrypted blob. Fixed string because
    // every unit uses `LoadCredentialEncrypted=rclone-conf:...` regardless of
    // which source it's for.
    "rclone-conf".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::SystemdControl;
    use crate::source::SourceKind;
    use crate::store::local::LocalUnitStore;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// No-op systemd control for the apply tests. The real `SessionSystemd`
    /// can't enable/disable units living in a tempdir (they aren't on systemd's
    /// search path), and these tests are about the store/credential side, not
    /// live unit management — that's covered by the control module's own smoke
    /// test. Returns success for every action and a fixed state.
    struct NoopControl;

    #[async_trait]
    impl SystemdControl for NoopControl {
        async fn reload(&self) -> Result<()> {
            Ok(())
        }
        async fn start(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn stop(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn restart(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn enable(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn disable(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn active_state(&self, _: &str) -> Result<String> {
            Ok("inactive".into())
        }
    }

    /// Like [`NoopControl`] but records which units were enabled/disabled, so a
    /// test can assert the "mount automatically" reconciliation. Cloneable and
    /// shares its log, so the test keeps a handle after the backend takes one.
    #[derive(Clone, Default)]
    struct RecordingControl {
        enabled: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        disabled: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl SystemdControl for RecordingControl {
        async fn reload(&self) -> Result<()> {
            Ok(())
        }
        async fn start(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn stop(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn restart(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn enable(&self, unit: &str) -> Result<()> {
            self.enabled.lock().unwrap().push(unit.to_string());
            Ok(())
        }
        async fn disable(&self, unit: &str) -> Result<()> {
            self.disabled.lock().unwrap().push(unit.to_string());
            Ok(())
        }
        async fn active_state(&self, _: &str) -> Result<String> {
            Ok("inactive".into())
        }
    }

    fn fixture() -> (TempDir, LocalBackend) {
        fixture_with(Box::new(NoopControl))
    }

    fn fixture_with(control: Box<dyn SystemdControl>) -> (TempDir, LocalBackend) {
        let dir = TempDir::new().unwrap();
        let store = Box::new(LocalUnitStore {
            config_dir: dir.path().join("config"),
            credential_dir: dir.path().join("creds"),
            unit_dir: dir.path().join("units"),
            file_mode: 0o600,
        });
        let backend = LocalBackend {
            store,
            control,
            scope: Scope::User,
            credential_dir_spec: "%h/.config/rclone-mounts/credentials".into(),
        };
        (dir, backend)
    }

    fn smb_source(name: &str, with_secret: bool) -> SourceDef {
        let mut options = BTreeMap::new();
        options.insert("host".into(), "files.example.com".into());
        options.insert("user".into(), "alice".into());
        let mut new_secrets = BTreeMap::new();
        if with_secret {
            new_secrets.insert(
                "pass".into(),
                crate::source::SecretValue { value: "hunter2".into(), obscure: true },
            );
        }
        SourceDef {
            name: name.into(),
            display_name: name.into(),
            kind: SourceKind::Smb,
            options,
            new_secrets,
        }
    }

    fn sftp_source(name: &str, with_secret: bool) -> SourceDef {
        let mut options = BTreeMap::new();
        options.insert("host".into(), "sftp.example.com".into());
        options.insert("user".into(), "alice".into());
        let mut new_secrets = BTreeMap::new();
        if with_secret {
            new_secrets.insert(
                "pass".into(),
                crate::source::SecretValue { value: "hunter2".into(), obscure: true },
            );
        }
        SourceDef { name: name.into(), display_name: name.into(), kind: SourceKind::Sftp, options, new_secrets }
    }

    fn ftp_source(name: &str, with_secret: bool) -> SourceDef {
        let mut options = BTreeMap::new();
        options.insert("host".into(), "ftp.example.com".into());
        let mut new_secrets = BTreeMap::new();
        if with_secret {
            new_secrets.insert(
                "pass".into(),
                crate::source::SecretValue { value: "hunter2".into(), obscure: true },
            );
        }
        SourceDef { name: name.into(), display_name: name.into(), kind: SourceKind::Ftp, options, new_secrets }
    }

    #[test]
    fn sftp_source_round_trips() {
        let (_d, b) = fixture();
        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![sftp_source("box", true)],
            ..Default::default()
        }))
        .unwrap();
        let state = async_io::block_on(b.load()).unwrap();
        assert_eq!(state.sources[0].kind, "sftp");
        assert_eq!(state.sources[0].options.get("host").map(String::as_str), Some("sftp.example.com"));
        assert!(state.sources[0].has_secret);
    }

    #[test]
    fn ftp_source_round_trips() {
        let (_d, b) = fixture();
        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![ftp_source("box", true)],
            ..Default::default()
        }))
        .unwrap();
        let state = async_io::block_on(b.load()).unwrap();
        assert_eq!(state.sources[0].kind, "ftp");
        assert!(state.sources[0].has_secret);
    }

    fn sample_mount(name: &str, source: &str) -> Mount {
        Mount {
            name: name.into(),
            display_name: name.into(),
            source: source.into(),
            subpath: String::new(),
            mountpoint: PathBuf::from(format!("/tmp/mnt/{name}")),
            options: Default::default(),
            enabled: false,
        }
    }

    #[test]
    fn empty_load_returns_empty_state() {
        let (_d, b) = fixture();
        let state = async_io::block_on(b.load()).unwrap();
        assert!(state.sources.is_empty());
        assert!(state.mounts.is_empty());
    }

    #[test]
    fn apply_then_load_round_trips_sources() {
        let (_d, b) = fixture();
        let cs = Changeset {
            upsert_sources: vec![smb_source("work", true)],
            ..Default::default()
        };
        async_io::block_on(b.apply(cs)).unwrap();

        let state = async_io::block_on(b.load()).unwrap();
        assert_eq!(state.sources.len(), 1);
        let src = &state.sources[0];
        assert_eq!(src.name, "work");
        assert_eq!(src.kind, "smb");
        assert_eq!(src.options.get("host").map(String::as_str), Some("files.example.com"));
        assert_eq!(src.options.get("user").map(String::as_str), Some("alice"));
        assert!(src.has_secret, "source created with a password should load as has_secret");
    }

    #[test]
    fn load_preserves_options_outside_the_known_set() {
        let (_d, b) = fixture();
        let mut src = smb_source("work", false);
        // A key the old hardcoded allowlist didn't include: it must still
        // survive a write/read round-trip now that we read all section keys.
        src.options.insert("spdif".into(), "yes".into());
        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![src],
            ..Default::default()
        }))
        .unwrap();
        let state = async_io::block_on(b.load()).unwrap();
        assert_eq!(
            state.sources[0].options.get("spdif").map(String::as_str),
            Some("yes")
        );
        // And our metadata keys never leak into the options map.
        assert!(!state.sources[0].options.contains_key("type"));
        assert!(!state.sources[0].options.contains_key("display_name"));
    }

    #[test]
    fn passwordless_source_loads_without_secret() {
        let (_d, b) = fixture();
        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![smb_source("nopass", false)],
            ..Default::default()
        }))
        .unwrap();
        let state = async_io::block_on(b.load()).unwrap();
        assert!(!state.sources[0].has_secret, "no secret was supplied");
    }

    #[test]
    fn display_name_round_trips_and_stays_out_of_options() {
        let (_d, b) = fixture();
        let mut src = smb_source("work", true);
        src.display_name = "😀 Work Share".into();
        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![src],
            ..Default::default()
        }))
        .unwrap();

        let state = async_io::block_on(b.load()).unwrap();
        let s = &state.sources[0];
        assert_eq!(s.name, "work");
        assert_eq!(s.display_name, "😀 Work Share");
        // The display name is our metadata, not an rclone connection option.
        assert!(!s.options.contains_key("display_name"), "leaked: {:?}", s.options);
    }

    #[test]
    fn apply_with_unknown_source_reference_fails() {
        let (_d, b) = fixture();
        let cs = Changeset {
            upsert_mounts: vec![sample_mount("oops", "no-such-source")],
            ..Default::default()
        };
        let err = async_io::block_on(b.apply(cs)).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)), "got: {err:?}");
    }

    #[test]
    fn apply_writes_unit_file_for_mount() {
        let (dir, b) = fixture();
        let cs = Changeset {
            upsert_sources: vec![smb_source("work", true)],
            upsert_mounts: vec![sample_mount("work", "work")],
            ..Default::default()
        };
        async_io::block_on(b.apply(cs)).unwrap();
        let unit_path = dir.path().join("units/rclone-mount-work.service");
        let unit = std::fs::read_to_string(&unit_path).unwrap();
        assert!(unit.contains("Description=rclone mount: work"));
        assert!(unit.contains("LoadCredentialEncrypted=rclone-conf:%h/.config/rclone-mounts/credentials/work"));
    }

    #[test]
    fn delete_mount_removes_unit_file_but_keeps_source() {
        let (dir, b) = fixture();
        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![smb_source("work", true)],
            upsert_mounts: vec![sample_mount("home", "work")],
            ..Default::default()
        }))
        .unwrap();

        async_io::block_on(b.apply(Changeset {
            delete_mounts: vec!["home".into()],
            ..Default::default()
        }))
        .unwrap();

        let unit_path = dir.path().join("units/rclone-mount-home.service");
        assert!(!unit_path.exists());
        let state = async_io::block_on(b.load()).unwrap();
        assert_eq!(state.sources.len(), 1);
    }

    #[test]
    fn extract_secrets_matches_only_requested_keys() {
        let blob = "[w]\ntype = smb\npass = ABC123\ntoken = should-not-appear\n";
        let found = extract_secrets(blob, &["pass"]);
        assert_eq!(found.get("pass").map(String::as_str), Some("ABC123"));
        assert!(!found.contains_key("token"), "token wasn't requested");
        assert!(!found.contains_key("type"), "type isn't a secret key");

        assert_eq!(extract_secrets("pass=XYZ", &["pass"]).get("pass").map(String::as_str), Some("XYZ"));
        assert!(extract_secrets("password = nope\n", &["pass"]).is_empty(), "password != pass");
        assert!(extract_secrets("type = smb\nhost = h\n", &["pass"]).is_empty());
    }

    #[test]
    fn extract_secrets_pulls_multiple_keys() {
        let blob = "[d]\ntype = drive\ntoken = {\"a\":1}\nclient_secret = shh\nroot_folder_id = xyz\n";
        let found = extract_secrets(blob, secret_keys_for(SourceKind::Drive));
        assert_eq!(found.len(), 2);
        assert_eq!(found.get("token").map(String::as_str), Some("{\"a\":1}"));
        assert_eq!(found.get("client_secret").map(String::as_str), Some("shh"));
    }

    #[test]
    fn secret_keys_for_smb_and_webdav_is_just_pass() {
        assert_eq!(secret_keys_for(SourceKind::Smb), &["pass"]);
        assert_eq!(secret_keys_for(SourceKind::WebDav), &["pass"]);
    }

    fn decrypt_blob(b: &LocalBackend, name: &str) -> String {
        let enc = b
            .store
            .read_credential(name)
            .unwrap()
            .expect("credential should exist");
        let plain = crate::credentials::decrypt(b.scope, &cred_id(), &enc).unwrap();
        String::from_utf8(plain).unwrap()
    }

    #[test]
    fn editing_params_without_secret_preserves_password() {
        let (_d, b) = fixture();
        // Create a source with a password.
        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![smb_source("work", true)],
            ..Default::default()
        }))
        .unwrap();
        let original_pass = extract_secrets(&decrypt_blob(&b, "work"), &["pass"]).remove("pass");
        assert!(original_pass.is_some(), "password should be stored on create");

        // Edit only the host; supply no new secret.
        let mut edited = smb_source("work", false);
        edited.options.insert("host".into(), "newhost.example.com".into());
        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![edited],
            ..Default::default()
        }))
        .unwrap();

        let blob = decrypt_blob(&b, "work");
        assert!(blob.contains("host = newhost.example.com"), "new host: {blob}");
        assert!(blob.contains("type = smb"), "type present: {blob}");
        assert_eq!(
            extract_secrets(&blob, &["pass"]).remove("pass"),
            original_pass,
            "password carried forward"
        );
    }

    #[test]
    fn multi_key_secret_round_trip_updates_one_key_and_preserves_the_other() {
        let (_d, b) = fixture();
        let mut drive = SourceDef {
            name: "gd".into(),
            display_name: "gd".into(),
            kind: SourceKind::Drive,
            options: BTreeMap::new(),
            new_secrets: BTreeMap::from([
                ("token".into(), crate::source::SecretValue { value: "tok-v1".into(), obscure: false }),
                ("client_secret".into(), crate::source::SecretValue { value: "cs-v1".into(), obscure: true }),
            ]),
        };
        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![drive.clone()],
            ..Default::default()
        }))
        .unwrap();
        let blob = decrypt_blob(&b, "gd");
        let secrets = extract_secrets(&blob, secret_keys_for(SourceKind::Drive));
        assert_eq!(secrets.get("token").map(String::as_str), Some("tok-v1"));
        let obscured_cs_v1 = secrets.get("client_secret").cloned().unwrap();
        assert_ne!(obscured_cs_v1, "cs-v1", "obscure:true value must not be stored as plaintext");

        // Update only the token; leave client_secret untouched.
        drive.new_secrets = BTreeMap::from([(
            "token".into(),
            crate::source::SecretValue { value: "tok-v2".into(), obscure: false },
        )]);
        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![drive],
            ..Default::default()
        }))
        .unwrap();
        let blob = decrypt_blob(&b, "gd");
        let secrets = extract_secrets(&blob, secret_keys_for(SourceKind::Drive));
        assert_eq!(secrets.get("token").map(String::as_str), Some("tok-v2"));
        assert_eq!(secrets.get("client_secret"), Some(&obscured_cs_v1), "untouched key carried forward byte-for-byte");
    }

    #[test]
    fn apply_enables_automatic_mounts_and_disables_others() {
        let control = RecordingControl::default();
        let (_d, b) = fixture_with(Box::new(control.clone()));

        let mut auto = sample_mount("auto", "work");
        auto.enabled = true;
        let manual = sample_mount("manual", "work"); // enabled: false

        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![smb_source("work", true)],
            upsert_mounts: vec![auto, manual],
            ..Default::default()
        }))
        .unwrap();

        let enabled = control.enabled.lock().unwrap();
        let disabled = control.disabled.lock().unwrap();
        assert!(
            enabled.contains(&"rclone-mount-auto.service".to_string()),
            "auto mount should be enabled: {enabled:?}"
        );
        assert!(
            disabled.contains(&"rclone-mount-manual.service".to_string()),
            "manual mount should be disabled: {disabled:?}"
        );
    }

    #[test]
    fn delete_source_removes_credential() {
        let (dir, b) = fixture();
        async_io::block_on(b.apply(Changeset {
            upsert_sources: vec![smb_source("work", true)],
            ..Default::default()
        }))
        .unwrap();
        let cred_path = dir.path().join("creds/work");
        assert!(cred_path.exists());

        async_io::block_on(b.apply(Changeset {
            delete_sources: vec!["work".into()],
            ..Default::default()
        }))
        .unwrap();
        assert!(!cred_path.exists());
    }
}
