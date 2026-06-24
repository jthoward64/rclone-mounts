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
use crate::source::SourceDef;
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
}

/// Snapshot of what's currently on disk for this scope.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub sources: Vec<SourceMetadata>,
    pub mounts: Vec<Mount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMetadata {
    pub name: String,
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
    /// Decrypt the existing credential for `name` (if any) and pull the stored
    /// obscured password out of it, so a params-only source edit keeps the
    /// password the user already set. Returns `None` when there's no credential
    /// or it carries no password (a password-less source).
    fn existing_obscured_pass(&self, name: &str) -> Result<Option<String>> {
        let Some(encrypted) = self.store.read_credential(name)? else {
            return Ok(None);
        };
        let plain = credentials::decrypt(self.scope, &cred_id(), &encrypted)?;
        let text = String::from_utf8_lossy(&plain);
        Ok(extract_pass(&text))
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
        let sources = doc
            .sections()
            .iter()
            .map(|name| SourceMetadata {
                name: name.to_string(),
                kind: doc.get(name, "type").unwrap_or("").to_string(),
                options: collect_section_options(&doc, name),
                has_secret: false, // probe pending; see [[probe-credential]]
            })
            .collect();

        let mounts_text = self.store.read_mounts_state()?;
        let mounts = if mounts_text.is_empty() {
            Vec::new()
        } else {
            toml::from_str::<MountsFile>(&mounts_text)
                .map_err(|e| Error::ConfigParse(format!("mounts.toml: {e}")))?
                .mount
        };

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
            for (k, v) in &src.options {
                doc.set(&src.name, k, v);
            }
        }
        self.store.write_sources_conf(&doc.render())?;

        // 4. Credentials. The encrypted blob is the *complete* rclone remote
        //    section (type + options + obscured pass), so it must be rebuilt on
        //    every source upsert — even a params-only edit, since the params
        //    live inside the blob. When no new secret is supplied we decrypt the
        //    existing blob to carry the stored password forward. This is the one
        //    place a stored password is read back, and only by the process that
        //    owns the store (the helper for system scope; the KCM for user scope).
        for src in &changeset.upsert_sources {
            let obscured_pass = match &src.new_secret {
                Some(secret) => Some(rclone_cli::obscure(secret)?),
                None => self.existing_obscured_pass(&src.name)?,
            };
            let blob = encode_credential_blob(src, obscured_pass.as_deref());
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
        Ok(())
    }
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

#[async_trait]
impl Backend for HelperBackend {
    async fn load(&self) -> Result<State> {
        let proxy = zbus::Proxy::new(&self.conn, HELPER_BUS, HELPER_PATH, HELPER_IFACE).await?;
        let sources: Vec<(String, String, BTreeMap<String, String>)> =
            proxy.call("ListSources", &()).await?;
        let mounts: Vec<(String, String, String, bool)> =
            proxy.call("ListMounts", &()).await?;
        Ok(State {
            sources: sources
                .into_iter()
                .map(|(name, kind, options)| SourceMetadata {
                    name,
                    kind,
                    options,
                    has_secret: false,
                })
                .collect(),
            mounts: mounts
                .into_iter()
                .map(|(name, source, mountpoint, enabled)| Mount {
                    name,
                    source,
                    mountpoint: mountpoint.into(),
                    options: Default::default(),
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
            kind,
            options: src.options.clone(),
            has_secret: src.new_secret.is_some(),
        };
        if let Some(existing) = state.sources.iter_mut().find(|s| s.name == src.name) {
            let preserved = existing.has_secret;
            *existing = entry;
            if src.new_secret.is_none() {
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
                "mount {:?} references unknown source {:?}",
                m.name, m.source
            )));
        }
    }
    Ok(())
}

fn collect_section_options(doc: &Document, section: &str) -> BTreeMap<String, String> {
    // The rclone_config crate doesn't yet expose all-keys iteration, so we
    // probe known options. A future Document::iter_section() will replace this.
    let mut out = BTreeMap::new();
    for key in &[
        "host", "url", "user", "domain", "port", "client_id", "client_secret", "token",
        "scope", "team_drive", "root_folder_id", "vendor", "case_insensitive",
    ] {
        if let Some(v) = doc.get(section, key) {
            out.insert((*key).to_string(), v.to_string());
        }
    }
    out
}

fn source_kind_str(kind: crate::source::SourceKind) -> &'static str {
    kind.as_tag()
}

/// The complete rclone remote section that becomes the credential payload:
/// `type`, every connection option, and the obscured password. systemd decrypts
/// this into `%d/rclone-conf` at unit start; rclone reads it via
/// `--config=%d/rclone-conf`, so the mount is fully self-contained and never
/// touches `sources.conf` (which exists only as the KCM's editable record).
fn encode_credential_blob(src: &SourceDef, obscured_pass: Option<&str>) -> String {
    let mut out = format!("[{}]\ntype = {}\n", src.name, source_kind_str(src.kind));
    for (k, v) in &src.options {
        let _ = writeln!(out, "{k} = {v}");
    }
    if let Some(pass) = obscured_pass {
        let _ = writeln!(out, "pass = {pass}");
    }
    out
}

/// Pull the `pass = …` value out of a decrypted rclone-conf blob. Matches the
/// `pass` key exactly (not `password`/other keys), tolerating optional spaces
/// around `=`.
fn extract_pass(blob: &str) -> Option<String> {
    blob.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("pass")?.trim_start();
        let value = rest.strip_prefix('=')?.trim();
        Some(value.to_string())
    })
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
    use crate::control::session::SessionSystemd;
    use crate::source::SourceKind;
    use crate::store::local::LocalUnitStore;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, LocalBackend) {
        let dir = TempDir::new().unwrap();
        let store = Box::new(LocalUnitStore {
            config_dir: dir.path().join("config"),
            credential_dir: dir.path().join("creds"),
            unit_dir: dir.path().join("units"),
            file_mode: 0o600,
        });
        let control = async_io::block_on(async { SessionSystemd::new().await.unwrap() });
        let backend = LocalBackend {
            store,
            control: Box::new(control),
            scope: Scope::User,
            credential_dir_spec: "%h/.config/rclone-mounts/credentials".into(),
        };
        (dir, backend)
    }

    fn smb_source(name: &str, with_secret: bool) -> SourceDef {
        let mut options = BTreeMap::new();
        options.insert("host".into(), "files.example.com".into());
        options.insert("user".into(), "alice".into());
        SourceDef {
            name: name.into(),
            kind: SourceKind::Smb,
            options,
            new_secret: if with_secret { Some("hunter2".into()) } else { None },
        }
    }

    fn sample_mount(name: &str, source: &str) -> Mount {
        Mount {
            name: name.into(),
            source: source.into(),
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
    fn extract_pass_matches_only_the_pass_key() {
        assert_eq!(
            extract_pass("[w]\ntype = smb\npass = ABC123\n").as_deref(),
            Some("ABC123")
        );
        assert_eq!(extract_pass("pass=XYZ").as_deref(), Some("XYZ"));
        assert_eq!(extract_pass("password = nope\n"), None);
        assert_eq!(extract_pass("type = smb\nhost = h\n"), None);
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
        let original_pass = extract_pass(&decrypt_blob(&b, "work"));
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
        assert_eq!(extract_pass(&blob), original_pass, "password carried forward");
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
