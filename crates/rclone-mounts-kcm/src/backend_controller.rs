// SPDX-License-Identifier: GPL-2.0-or-later

//! BackendController — the root QObject the QML UI binds to.
//!
//! Lifecycle is driven by the C++ KCM shim purely through Qt signals/slots
//! (see `cpp/kcm_rclone_mounts.*`): the shim emits `loadRequested` /
//! `saveRequested` / `defaultsRequested`, and QML routes those to [`load`],
//! [`commit`], and [`reset`]. The controller reports its dirty state back via
//! the `dirty` property, which QML binds to `kcm.needsSave`. The C++ side knows
//! nothing about mounts, sources, or the backend.
//!
//! State model: `applied` is the last-loaded on-disk snapshot; `pending` is the
//! changeset the UI has built since. What QML displays is `applied.preview(pending)`,
//! re-serialized to JSON on every edit. We expose the lists as JSON strings
//! rather than a `QAbstractListModel` because cxx-qt-lib's `QVariant` can't wrap
//! a `QVariantMap`, so a list-of-maps model isn't constructible here; JSON keeps
//! rich per-row structure with only rock-solid `QString` bridge types. At the
//! handful-of-mounts scale a full re-serialize per edit is free.
//!
//! Threading: the potentially-slow backend ops — `load`, `commit`, and the
//! status poll — run their `async_io::block_on` on a worker thread and post
//! results back onto the GUI thread via a `CxxQtThread`. This matters most in
//! system scope, where the helper's first call raises a Polkit prompt that
//! blocks until the user answers: doing it on the GUI thread would freeze the
//! whole KCM behind the dialog. The pure in-memory edit paths (`upsert_*`,
//! `remove_*`, `reset`) stay synchronous — they touch no I/O. See
//! [[backend-threading]].

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        // True when there are uncommitted edits. QML binds kcm.needsSave to this.
        #[qproperty(bool, dirty)]
        // True while a load/commit is in flight; QML can show a busy indicator.
        #[qproperty(bool, busy)]
        // Last error, empty when the last op succeeded. Surfaced in the UI.
        #[qproperty(QString, error_string)]
        // Which scope the controller is bound to: false = the user's own mounts
        // (no helper), true = system-wide mounts (proxied through the privileged
        // helper, Polkit-authorized). QML reflects this; drive changes through
        // `set_scope`, not the generated setter, so the backend is rebuilt.
        #[qproperty(bool, system_scope)]
        // Whether the privileged helper is reachable at all (installed,
        // D-Bus-activatable). QML hides the scope tab switcher entirely and
        // falls back to user-only mounts when this is false, rather than
        // offering a "System mounts" tab that would just error out. `false`
        // until the startup probe completes.
        #[qproperty(bool, system_scope_available)]
        // JSON arrays of the *displayed* (applied + pending) state. QML does
        // JSON.parse(...) and uses the result directly as a ListView model.
        #[qproperty(QString, mounts_json)]
        #[qproperty(QString, sources_json)]
        // Static JSON array of `source_schema::KindSchema` — the single source
        // of truth for the source-type picker and its per-kind fields. Constant
        // for the process lifetime; set once at construction.
        #[qproperty(QString, kind_schemas_json)]
        // Static JSON array of `CredentialKindInfo` — which source kinds have
        // a shared/admin credential override the Credentials settings page
        // can manage, with the label/icon to show for each. Constant for the
        // process lifetime; set once at construction.
        #[qproperty(QString, credential_kinds_json)]
        // Multi-step interactive source setup (Google Drive OAuth, iCloud
        // 2FA), driven by `begin_interactive_source`/`submit_wizard_input`.
        // `wizard_state`: "idle" | "running" | "need_input" | "error" | "done".
        // QML binds/polls these the same way it does `busy`/`error_string` —
        // there's no push-signal mechanism here, just property-change
        // notifications.
        #[qproperty(QString, wizard_state)]
        // JSON `{"name","help","is_password"}` describing what rclone still
        // needs when wizard_state == "need_input"; empty otherwise.
        #[qproperty(QString, wizard_prompt_json)]
        #[qproperty(QString, wizard_error)]
        type BackendController = super::BackendControllerRust;

        /// (Re)load the on-disk state, discarding any pending edits.
        #[qinvokable]
        fn load(self: Pin<&mut BackendController>);

        /// Switch the active scope and reload for it. `system` true binds to the
        /// privileged helper (prompting for admin auth on first access); false
        /// binds to the user's own mounts. No-op if already on that scope and
        /// loaded — so re-selecting a tab doesn't re-prompt. Loading is lazy:
        /// nothing touches the system bus until the UI calls this with `true`.
        #[qinvokable]
        fn set_scope(self: Pin<&mut BackendController>, system: bool);

        /// Apply pending edits to disk, then reload. No-op if not dirty.
        #[qinvokable]
        fn commit(self: Pin<&mut BackendController>);

        /// Discard pending edits, reverting the display to the applied state.
        #[qinvokable]
        fn reset(self: Pin<&mut BackendController>);

        /// Create or replace a mount in the pending changeset. `id` is empty when
        /// creating (the controller derives a stable id from `display_name`) and
        /// set when editing an existing mount. `source` is a source id. `subpath`
        /// is a path within that source's remote to mount instead of its root
        /// (empty mounts the whole remote). `options_json` is a JSON object of
        /// the mount's tuning options (cache mode, size caps, umask, read-only);
        /// it deserializes straight into `MountOptions`, falling back to
        /// defaults if it can't be read. Returns the mount's resolved id (the
        /// freshly-derived one when `id` was empty, `id` unchanged otherwise) so
        /// a caller creating a new mount can pass it back on every subsequent
        /// live edit instead of leaving `id` empty and getting a fresh (and
        /// divergent) slug derived from a since-changed `display_name` each
        /// time. Empty on validation failure.
        #[qinvokable]
        fn upsert_mount(
            self: Pin<&mut BackendController>,
            id: &QString,
            display_name: &QString,
            source: &QString,
            subpath: &QString,
            mountpoint: &QString,
            options_json: &QString,
            enabled: bool,
        ) -> QString;

        /// Stage a mount for deletion (or drop a pending create).
        #[qinvokable]
        fn remove_mount(self: Pin<&mut BackendController>, name: &QString);

        /// Create or replace a source in the pending changeset. `id` is empty
        /// when creating (the controller derives a stable id from `display_name`)
        /// and set when editing. `kind` is the rclone type tag (smb/drive/webdav).
        /// `options_json` is a JSON object of string→string connection params.
        /// `secret` sets/rotates the stored password when non-empty; an empty
        /// string leaves the existing credential untouched (write-only secret).
        /// Returns the source's resolved id, same reasoning as `upsert_mount`'s
        /// return value. Empty on validation failure.
        #[qinvokable]
        fn upsert_source(
            self: Pin<&mut BackendController>,
            id: &QString,
            display_name: &QString,
            kind: &QString,
            options_json: &QString,
            secret: &QString,
        ) -> QString;

        /// Stage a source for deletion (or drop a pending create).
        #[qinvokable]
        fn remove_source(self: Pin<&mut BackendController>, name: &QString);

        /// Start a mount's unit now (live action, independent of Apply). Only
        /// valid for mounts that already exist on disk.
        #[qinvokable]
        fn start_mount(self: Pin<&mut BackendController>, name: &QString);

        /// Stop a mount's unit now (live action, independent of Apply).
        #[qinvokable]
        fn stop_mount(self: Pin<&mut BackendController>, name: &QString);

        /// Re-query the active state of every applied mount and refresh the
        /// model. Runs the queries on a worker thread and posts the result back,
        /// so the UI's poll timer never blocks the GUI thread — important once
        /// each query is a helper round-trip (system mode) rather than a local
        /// session-bus call.
        #[qinvokable]
        fn refresh_status(self: Pin<&mut BackendController>);

        /// Start a wizard-driven source setup (Google Drive OAuth, iCloud
        /// 2FA). `id` is empty when creating, set when reconnecting an
        /// existing source. `seed_json` is a flat string map of whatever the
        /// wizard's first step collected (Drive: optional client_id/
        /// client_secret/root_folder_id; iCloud: apple_id/password). Runs off
        /// the GUI thread; progress is reported via `wizard_state`/
        /// `wizard_prompt_json`/`wizard_error`.
        #[qinvokable]
        fn begin_interactive_source(
            self: Pin<&mut BackendController>,
            id: &QString,
            display_name: &QString,
            kind: &QString,
            seed_json: &QString,
        );

        /// Answer the prompt described by `wizard_prompt_json` (a 2FA code,
        /// or the literal "sms") and advance the flow.
        #[qinvokable]
        fn submit_wizard_input(self: Pin<&mut BackendController>, answer: &QString);

        /// Abandon the in-progress wizard flow (kills the driver/subprocess,
        /// drops the scratch config). Resets `wizard_state` to "idle".
        #[qinvokable]
        fn cancel_wizard(self: Pin<&mut BackendController>);

        /// Set (or, if both empty, clear) the system-scope admin override for
        /// a kind's OAuth client id/secret. Only valid in system scope.
        #[qinvokable]
        fn set_provider_override(
            self: Pin<&mut BackendController>,
            kind: &QString,
            client_id: &QString,
            client_secret: &QString,
        );

        /// Whether an admin override is currently stored for `kind`. Never
        /// returns the secret itself (mirrors `has_secret`'s write-only
        /// pattern) — just enough for the UI to show "configured" or not.
        #[qinvokable]
        fn provider_override_configured(self: Pin<&mut BackendController>, kind: &QString) -> bool;

        /// Whether `kind` has *some* client credential available without the
        /// user supplying their own (a build-time default or an admin
        /// override). When false, the wizard must require the user to fill
        /// in their own client id/secret — there's no fallback to sign in with.
        #[qinvokable]
        fn provider_default_available(self: Pin<&mut BackendController>, kind: &QString) -> bool;
    }

    // Opt the QObject into cross-thread updates: this generates `qt_thread()`,
    // whose handle a worker thread uses to queue closures back onto the GUI
    // thread. See [`refresh_status`] / [[backend-threading]].
    impl cxx_qt::Threading for BackendController {}
}

use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use rclone_mounts_core::{
    Backend, Changeset, HelperBackend, LocalBackend, Mount, MountOptions, SecretValue, SourceDef,
    SourceKind,
    SourceMetadata, State,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

pub struct BackendControllerRust {
    dirty: bool,
    busy: bool,
    error_string: QString,
    system_scope: bool,
    system_scope_available: bool,
    mounts_json: QString,
    sources_json: QString,
    kind_schemas_json: QString,
    credential_kinds_json: QString,
    wizard_state: QString,
    wizard_prompt_json: QString,
    wizard_error: QString,

    /// Last snapshot read from disk.
    applied: State,
    /// Edits accumulated since the last load/commit.
    pending: Changeset,
    /// User-scope backend, built lazily on first successful load. Held in an
    /// `Arc` so the status-poll worker thread can share it (see
    /// [`ffi::BackendController::refresh_status`]).
    backend: Option<Arc<dyn Backend>>,
    /// Live systemd ActiveState per applied mount name, refreshed on load,
    /// after start/stop, and on the UI's poll timer.
    statuses: BTreeMap<String, String>,
    /// True while a background status poll is running, so a fast UI timer can't
    /// pile up worker threads when each query is slow (system mode).
    status_poll_in_flight: bool,
    /// The in-progress interactive source setup, if any. `Some` only between
    /// a `NeedInput` step and the next `submit_wizard_input`/`cancel_wizard`
    /// call — moved out to a worker thread for each step and moved back (or
    /// dropped) once that step resolves.
    wizard: Option<WizardSession>,
}

/// State carried across wizard steps: the live `ConfigDriver` (and its
/// scratch config file) plus enough context to finish building a `SourceDef`
/// once the flow completes.
struct WizardSession {
    driver: rclone_mounts_core::rclone_config_driver::ConfigDriver,
    kind: SourceKind,
    remote_name: String,
    display_name: String,
    /// The `state` token from the most recent `NeedInput` step, to answer via
    /// `continue_with`.
    pending_state: String,
}

/// UI-facing row for the Credentials settings page: which source kinds have
/// a shared/admin credential override, with the label/icon to show for each
/// (pulled from that kind's `source_schema::KindSchema`, not duplicated).
#[derive(Serialize)]
struct CredentialKindInfo {
    tag: String,
    label: String,
    icon: String,
}

fn credential_kinds() -> Vec<CredentialKindInfo> {
    rclone_mounts_core::source_schema::credential_capable_kinds()
        .iter()
        .filter_map(|tag| {
            rclone_mounts_core::source_schema::schema_for(tag).map(|schema| CredentialKindInfo {
                tag: schema.tag.to_string(),
                label: schema.label.to_string(),
                icon: schema.icon.to_string(),
            })
        })
        .collect()
}

impl Default for BackendControllerRust {
    fn default() -> Self {
        let kind_schemas_json = serde_json::to_string(rclone_mounts_core::source_schema::all_kind_schemas())
            .unwrap_or_else(|_| "[]".to_string());
        let credential_kinds_json =
            serde_json::to_string(&credential_kinds()).unwrap_or_else(|_| "[]".to_string());
        Self {
            dirty: false,
            busy: false,
            error_string: QString::default(),
            system_scope: false,
            // False until the startup probe in `load()` completes; the tab
            // switcher stays hidden (falling back to user-only mounts) until
            // then, rather than optimistically showing it.
            system_scope_available: false,
            mounts_json: QString::default(),
            sources_json: QString::default(),
            kind_schemas_json: QString::from(kind_schemas_json.as_str()),
            credential_kinds_json: QString::from(credential_kinds_json.as_str()),
            wizard_state: QString::from("idle"),
            wizard_prompt_json: QString::default(),
            wizard_error: QString::default(),
            applied: State::default(),
            pending: Changeset::default(),
            backend: None,
            statuses: BTreeMap::new(),
            status_poll_in_flight: false,
            wizard: None,
        }
    }
}

/// UI-facing projection of a mount. Shape is what the QML delegate consumes.
/// `active` is the systemd ActiveState (or "unsaved" for a pending-create
/// mount with no unit yet); `applied` gates the Start/Stop controls.
#[derive(Serialize)]
struct MountView {
    /// Internal id; QML passes it back for start/stop/edit/remove.
    name: String,
    /// Freeform name shown to the user.
    display_name: String,
    source: String,
    /// Path within the source's remote this mount points at, instead of its
    /// root; empty means "the whole remote".
    subpath: String,
    mountpoint: String,
    /// Tuning options (cache mode, size caps, umask, …), so the editor can
    /// prefill them; serialized with the same field names the editor sends back.
    options: MountOptions,
    enabled: bool,
    active: String,
    applied: bool,
}

/// UI-facing projection of a source. Carries the full option map so the editor
/// can prefill any per-kind field (host/url/user/…); `has_secret` drives the
/// write-only password affordance (the secret itself is never read back).
#[derive(Serialize)]
struct SourceView {
    /// Internal id; QML passes it back for edit/remove and as a mount's source.
    name: String,
    /// Freeform name shown to the user.
    display_name: String,
    kind: String,
    options: BTreeMap<String, String>,
    has_secret: bool,
}

impl From<&SourceMetadata> for SourceView {
    fn from(s: &SourceMetadata) -> Self {
        Self {
            name: s.name.clone(),
            display_name: s.display_name.clone(),
            kind: s.kind.clone(),
            options: s.options.clone(),
            has_secret: s.has_secret,
        }
    }
}

/// Install a stderr tracing subscriber once, so backend failures inside the KCM
/// are observable (respects `RUST_LOG`, defaults to `info`). KCMs have no
/// `main()`, so we lazily init on first use rather than at startup.
fn init_tracing() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        use tracing_subscriber::EnvFilter;
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .try_init();
    });
}

fn changeset_is_empty(cs: &Changeset) -> bool {
    cs.upsert_sources.is_empty()
        && cs.delete_sources.is_empty()
        && cs.upsert_mounts.is_empty()
        && cs.delete_mounts.is_empty()
}

/// Query systemd ActiveState for each named mount, mapping any error to
/// "unknown" so one bad unit doesn't sink the batch. Async and thread-agnostic:
/// callers drive it with `block_on` on whichever thread they're on (a worker
/// for load/commit/poll, the GUI thread for the post-start/stop refresh).
async fn read_statuses(backend: &dyn Backend, names: &[String]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for name in names {
        let state = backend
            .mount_status(name)
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        map.insert(name.clone(), state);
    }
    map
}

/// What a wizard step resolved to, handed back to the GUI thread to apply.
enum WizardResult {
    Done { source_def: SourceDef },
    NeedInput { session: WizardSession, prompt_json: String },
    Failed(String),
}

/// Auto-answer rclone's "use the local browser?" prompt with "true" — this
/// app always uses the local-browser-redirect OAuth flow (never manual code
/// paste), so that choice is never surfaced to the user. Loops in case it's
/// asked more than once.
fn drain_auto_answerable(
    driver: &mut rclone_mounts_core::rclone_config_driver::ConfigDriver,
    mut step: rclone_mounts_core::rclone_config_driver::DriverStep,
) -> rclone_mounts_core::rclone_config_driver::DriverStep {
    use rclone_mounts_core::rclone_config_driver::DriverStep;
    loop {
        match step {
            DriverStep::NeedInput { state, prompt } if prompt.looks_like_local_browser_choice() => {
                step = match driver.continue_with(&state, "true") {
                    Ok(s) => s,
                    Err(e) => DriverStep::Error(e.to_string()),
                };
            }
            other => return other,
        }
    }
}

/// Split a finished remote's rclone.conf section into non-secret options
/// (destined for `sources.conf`) and secret values (destined for the
/// encrypted credential blob), per that kind's `secret_keys_for` table.
fn source_def_from_remote_conf(
    remote_conf: &str,
    kind: SourceKind,
    remote_name: &str,
    display_name: &str,
) -> rclone_mounts_core::Result<SourceDef> {
    let doc = rclone_mounts_core::rclone_config::Document::parse(remote_conf)?;
    let secret_keys = rclone_mounts_core::backend::secret_keys_for(kind);
    let mut options = BTreeMap::new();
    let mut new_secrets = BTreeMap::new();
    for (k, v) in doc.section_entries(remote_name) {
        if k == "type" {
            continue;
        }
        if secret_keys.contains(&k) {
            // Already in final on-disk form (rclone wrote it): obscure:false
            // means it's stored byte-for-byte, not re-obscured.
            new_secrets.insert(k.to_string(), SecretValue { value: v.to_string(), obscure: false });
        } else {
            options.insert(k.to_string(), v.to_string());
        }
    }
    if kind == SourceKind::IcloudDrive {
        // UI-only reminder heuristic (rclone's trust token carries no
        // machine-readable expiry): stamp "now" so the sources list can warn
        // as the ~30-day validity window approaches. Unix seconds, plain
        // option — no date crate needed, and it's harmless to show alongside
        // the other connection params.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        options.insert("_trust_token_stamped_at".to_string(), now.to_string());
    }
    Ok(SourceDef {
        name: remote_name.to_string(),
        display_name: display_name.to_string(),
        kind,
        options,
        new_secrets,
    })
}

/// Turn a completed/failed driver step into a [`WizardResult`], consuming the
/// driver (dropped on `Done`/`Failed`, carried forward in the session on
/// `NeedInput`).
fn finish_step(
    driver: rclone_mounts_core::rclone_config_driver::ConfigDriver,
    step: rclone_mounts_core::rclone_config_driver::DriverStep,
    kind: SourceKind,
    remote_name: String,
    display_name: String,
) -> WizardResult {
    use rclone_mounts_core::rclone_config_driver::DriverStep;
    match step {
        DriverStep::Done { remote_conf } => {
            match source_def_from_remote_conf(&remote_conf, kind, &remote_name, &display_name) {
                Ok(source_def) => WizardResult::Done { source_def },
                Err(e) => WizardResult::Failed(format!("Sign-in finished but the result couldn’t be read. {e}")),
            }
        }
        DriverStep::NeedInput { state, prompt } => {
            // iCloud's 2FA step accepts the literal "sms" as an answer to
            // fall back to a text message instead of a push-notification
            // code; surface that as a flag so the UI can offer a button
            // instead of making the user know to type the magic word.
            let sms_available = prompt.looks_like_2fa();
            let prompt_json = serde_json::json!({
                "name": prompt.name,
                "help": prompt.help,
                "is_password": prompt.is_password,
                "sms_available": sms_available,
            })
            .to_string();
            WizardResult::NeedInput {
                session: WizardSession { driver, kind, remote_name, display_name, pending_state: state },
                prompt_json,
            }
        }
        DriverStep::Error(e) => WizardResult::Failed(e),
    }
}

/// Resolve the initial key/value seed for a wizard-only kind's `rclone
/// config create` call and start the driver. Drive additionally resolves its
/// three-tier client credential precedence here, off the GUI thread (the
/// admin-override tier may be a helper/D-Bus round-trip in system scope).
async fn start_wizard(
    kind: SourceKind,
    remote_name: String,
    display_name: String,
    seed: BTreeMap<String, String>,
    backend: Option<&(dyn Backend + 'static)>,
) -> WizardResult {
    let mut initial_kv = seed.clone();
    if kind == SourceKind::Drive {
        let user_id = seed.get("client_id").filter(|s| !s.is_empty()).cloned();
        let user_secret = seed.get("client_secret").filter(|s| !s.is_empty()).cloned();
        let creds = match backend {
            Some(b) => {
                rclone_mounts_core::oauth_credentials::resolve_drive_client_credentials(
                    b,
                    user_id.as_deref(),
                    user_secret.as_deref(),
                )
                .await
            }
            None => Ok(None),
        };
        match creds {
            Ok(Some((id, secret))) => {
                initial_kv.insert("client_id".to_string(), id);
                initial_kv.insert("client_secret".to_string(), secret);
            }
            Ok(None) => {
                initial_kv.remove("client_id");
                initial_kv.remove("client_secret");
            }
            Err(e) => return WizardResult::Failed(format!("Couldn’t resolve Google Drive credentials. {e}")),
        }
        initial_kv.insert("scope".to_string(), "drive".to_string());
    }

    let (mut driver, step) =
        match rclone_mounts_core::rclone_config_driver::ConfigDriver::start(kind.as_tag(), &remote_name, &initial_kv) {
            Ok(v) => v,
            Err(e) => return WizardResult::Failed(format!("Couldn’t start sign-in. {e}")),
        };
    let step = drain_auto_answerable(&mut driver, step);
    finish_step(driver, step, kind, remote_name, display_name)
}

impl ffi::BackendController {
    /// Apply a finished/failed/paused wizard step to controller state. Always
    /// runs on the GUI thread (called from inside a `qt_thread().queue`
    /// closure).
    fn apply_wizard_result(mut self: Pin<&mut Self>, outcome: WizardResult) {
        match outcome {
            WizardResult::Done { source_def } => {
                {
                    let mut rust = self.as_mut().rust_mut();
                    rust.wizard = None;
                    let p = &mut rust.pending;
                    p.upsert_sources.retain(|s| s.name != source_def.name);
                    p.delete_sources.retain(|n| n != &source_def.name);
                    p.upsert_sources.push(source_def);
                }
                self.as_mut().set_wizard_prompt_json(QString::default());
                self.as_mut().set_wizard_error(QString::default());
                self.as_mut().set_wizard_state(QString::from("done"));
                self.as_mut().refresh();
            }
            WizardResult::NeedInput { session, prompt_json } => {
                self.as_mut().rust_mut().wizard = Some(session);
                self.as_mut().set_wizard_prompt_json(QString::from(prompt_json.as_str()));
                self.as_mut().set_wizard_error(QString::default());
                self.as_mut().set_wizard_state(QString::from("need_input"));
            }
            WizardResult::Failed(msg) => {
                self.as_mut().rust_mut().wizard = None;
                self.as_mut().set_wizard_prompt_json(QString::default());
                self.as_mut().set_wizard_error(QString::from(msg.as_str()));
                self.as_mut().set_wizard_state(QString::from("error"));
            }
        }
    }

    fn begin_interactive_source(
        mut self: Pin<&mut Self>,
        id: &QString,
        display_name: &QString,
        kind: &QString,
        seed_json: &QString,
    ) {
        let kind_tag = kind.to_string();
        let Some(kind) = SourceKind::from_tag(&kind_tag) else {
            self.as_mut()
                .set_wizard_error(QString::from(format!("“{kind_tag}” isn’t a source type this version supports.").as_str()));
            self.as_mut().set_wizard_state(QString::from("error"));
            return;
        };
        let seed: BTreeMap<String, String> = match serde_json::from_str(&seed_json.to_string()) {
            Ok(v) => v,
            Err(e) => {
                self.as_mut()
                    .set_wizard_error(QString::from(format!("Those settings couldn’t be read. {e}").as_str()));
                self.as_mut().set_wizard_state(QString::from("error"));
                return;
            }
        };
        let display = display_name.to_string();
        let remote_name = self.as_ref().resolve_id(&id.to_string(), &display, true);
        let backend = self.as_ref().rust().backend.clone();

        self.as_mut().set_wizard_error(QString::default());
        self.as_mut().set_wizard_prompt_json(QString::default());
        self.as_mut().set_wizard_state(QString::from("running"));

        let thread = self.as_ref().qt_thread();
        std::thread::spawn(move || {
            let outcome = async_io::block_on(start_wizard(kind, remote_name, display, seed, backend.as_deref()));
            let _ = thread.queue(move |mut obj| {
                obj.as_mut().apply_wizard_result(outcome);
            });
        });
    }

    fn submit_wizard_input(mut self: Pin<&mut Self>, answer: &QString) {
        let answer = answer.to_string();
        let Some(session) = self.as_mut().rust_mut().wizard.take() else {
            self.as_mut()
                .set_wizard_error(QString::from("There’s no sign-in in progress."));
            self.as_mut().set_wizard_state(QString::from("error"));
            return;
        };
        self.as_mut().set_wizard_state(QString::from("running"));
        let thread = self.as_ref().qt_thread();
        std::thread::spawn(move || {
            let WizardSession { mut driver, kind, remote_name, display_name, pending_state } = session;
            let step = driver
                .continue_with(&pending_state, &answer)
                .unwrap_or_else(|e| rclone_mounts_core::rclone_config_driver::DriverStep::Error(e.to_string()));
            let step = drain_auto_answerable(&mut driver, step);
            let outcome = finish_step(driver, step, kind, remote_name, display_name);
            let _ = thread.queue(move |mut obj| {
                obj.as_mut().apply_wizard_result(outcome);
            });
        });
    }

    fn cancel_wizard(mut self: Pin<&mut Self>) {
        // Dropping the session drops its ConfigDriver, which drops the
        // scratch tempfile (and, best-effort, any still-running subprocess).
        self.as_mut().rust_mut().wizard = None;
        self.as_mut().set_wizard_prompt_json(QString::default());
        self.as_mut().set_wizard_error(QString::default());
        self.as_mut().set_wizard_state(QString::from("idle"));
    }

    fn set_provider_override(mut self: Pin<&mut Self>, kind: &QString, client_id: &QString, client_secret: &QString) {
        let kind_tag = kind.to_string();
        let Some(kind) = SourceKind::from_tag(&kind_tag) else {
            self.as_mut()
                .set_error_string(QString::from(format!("“{kind_tag}” isn’t a source type this version supports.").as_str()));
            return;
        };
        let Some(backend) = self.as_ref().rust().backend.clone() else {
            self.as_mut().set_error_string(QString::from(
                "The rclone settings module isn’t ready yet. Try closing and reopening it.",
            ));
            return;
        };
        let client_id = client_id.to_string();
        let client_secret = client_secret.to_string();
        self.as_mut().set_busy(true);
        let thread = self.as_ref().qt_thread();
        std::thread::spawn(move || {
            let result = async_io::block_on(backend.set_provider_override(kind, &client_id, &client_secret));
            let _ = thread.queue(move |mut obj| {
                match result {
                    Ok(()) => obj.as_mut().set_error_string(QString::default()),
                    Err(e) => obj.as_mut().set_error_string(QString::from(
                        format!("Couldn’t save the override. {e}").as_str(),
                    )),
                }
                obj.as_mut().set_busy(false);
            });
        });
    }

    fn provider_override_configured(self: Pin<&mut Self>, kind: &QString) -> bool {
        let Some(k) = SourceKind::from_tag(&kind.to_string()) else {
            return false;
        };
        let Some(backend) = self.as_ref().rust().backend.clone() else {
            return false;
        };
        async_io::block_on(backend.provider_override(k)).ok().flatten().is_some()
    }

    fn provider_default_available(self: Pin<&mut Self>, kind: &QString) -> bool {
        let Some(k) = SourceKind::from_tag(&kind.to_string()) else {
            return false;
        };
        let Some(backend) = self.as_ref().rust().backend.clone() else {
            // No backend yet (still loading): assume unavailable so the UI
            // requires explicit credentials rather than silently permitting
            // a sign-in that would fail server-side anyway.
            return false;
        };
        match k {
            SourceKind::Drive => async_io::block_on(rclone_mounts_core::oauth_credentials::resolve_drive_client_credentials(
                backend.as_ref(),
                None,
                None,
            ))
            .ok()
            .flatten()
            .is_some(),
            // Other kinds don't have a build-time/admin credential tier —
            // nothing to require here.
            _ => true,
        }
    }

    fn load(mut self: Pin<&mut Self>) {
        init_tracing();
        self.as_mut().set_busy(true);
        self.as_mut().set_error_string(QString::default());

        let system = self.as_ref().rust().system_scope;
        let thread = self.as_ref().qt_thread();

        // Build the backend, load state, and read initial statuses off the GUI
        // thread. System scope proxies to the helper over the system bus, whose
        // first call raises the Polkit prompt — running it here means the dialog
        // doesn't freeze the KCM. Results post back via `thread.queue`.
        std::thread::spawn(move || {
            // Cheap, side-effect-free bus query (no Polkit prompt, no error
            // banner) so the UI can decide whether "System mounts" is worth
            // offering at all. Runs alongside the real load, not gating it.
            let system_available = async_io::block_on(rclone_mounts_core::backend::system_scope_available());

            let outcome: rclone_mounts_core::Result<(Arc<dyn Backend>, State, BTreeMap<String, String>)> =
                async_io::block_on(async {
                    let backend: Arc<dyn Backend> = if system {
                        Arc::new(HelperBackend::new().await?)
                    } else {
                        Arc::new(LocalBackend::new_user().await?)
                    };
                    let state = backend.load().await?;
                    let names: Vec<String> = state.mounts.iter().map(|m| m.name.clone()).collect();
                    let statuses = read_statuses(backend.as_ref(), &names).await;
                    Ok((backend, state, statuses))
                });

            let _ = thread.queue(move |mut obj| {
                obj.as_mut().set_system_scope_available(system_available);
                match outcome {
                    Ok((backend, state, statuses)) => {
                        tracing::info!(
                            system,
                            sources = state.sources.len(),
                            mounts = state.mounts.len(),
                            "loaded state"
                        );
                        {
                            let mut rust = obj.as_mut().rust_mut();
                            rust.backend = Some(backend);
                            rust.applied = state;
                            rust.pending = Changeset::default();
                            rust.statuses = statuses;
                        }
                        obj.as_mut().refresh();
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "load failed");
                        obj.as_mut().set_error_string(QString::from(
                            format!("Couldn’t load your mounts. {e}").as_str(),
                        ));
                    }
                }
                obj.as_mut().set_busy(false);
            });
        });
    }

    fn set_scope(mut self: Pin<&mut Self>, system: bool) {
        // No-op if we're already on this scope and have a live backend, so
        // re-selecting the current tab doesn't reload (or re-prompt for auth).
        if self.as_ref().rust().system_scope == system && self.as_ref().rust().backend.is_some() {
            return;
        }
        self.as_mut().set_system_scope(system);
        self.as_mut().load();
    }

    fn commit(mut self: Pin<&mut Self>) {
        if changeset_is_empty(&self.as_ref().rust().pending) {
            return;
        }
        if self.as_ref().rust().backend.is_none() {
            self.as_mut()
                .set_error_string(QString::from("The rclone settings module isn’t ready yet. Try closing and reopening it."));
            return;
        }

        self.as_mut().set_busy(true);
        self.as_mut().set_error_string(QString::default());

        let pending = self.as_ref().rust().pending.clone();
        let backend = Arc::clone(self.as_ref().rust().backend.as_ref().unwrap());
        let thread = self.as_ref().qt_thread();

        // Apply + reload off the GUI thread: system scope prompts for the
        // modify-system Polkit auth here, which must not freeze the KCM.
        std::thread::spawn(move || {
            let outcome: rclone_mounts_core::Result<(State, BTreeMap<String, String>)> =
                async_io::block_on(async {
                    backend.apply(pending).await?;
                    let state = backend.load().await?;
                    let names: Vec<String> =
                        state.mounts.iter().map(|m| m.name.clone()).collect();
                    let statuses = read_statuses(backend.as_ref(), &names).await;
                    Ok((state, statuses))
                });

            let _ = thread.queue(move |mut obj| {
                match outcome {
                    Ok((state, statuses)) => {
                        {
                            let mut rust = obj.as_mut().rust_mut();
                            rust.applied = state;
                            rust.pending = Changeset::default();
                            rust.statuses = statuses;
                        }
                        obj.as_mut().refresh();
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "commit failed");
                        obj.as_mut().set_error_string(QString::from(
                            format!("Couldn’t save your changes. {e}").as_str(),
                        ));
                    }
                }
                obj.as_mut().set_busy(false);
            });
        });
    }

    fn reset(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending = Changeset::default();
        self.as_mut().set_error_string(QString::default());
        self.as_mut().refresh();
    }

    fn upsert_mount(
        mut self: Pin<&mut Self>,
        id: &QString,
        display_name: &QString,
        source: &QString,
        subpath: &QString,
        mountpoint: &QString,
        options_json: &QString,
        enabled: bool,
    ) -> QString {
        // A malformed blob shouldn't lose the user's edit; fall back to the
        // default tuning and surface a note. An empty string (older callers /
        // no options) also lands on defaults.
        let options: MountOptions = {
            let raw = options_json.to_string();
            if raw.trim().is_empty() {
                MountOptions::default()
            } else {
                match serde_json::from_str(&raw) {
                    Ok(o) => o,
                    Err(e) => {
                        self.as_mut().set_error_string(QString::from(
                            format!("Those mount options couldn’t be read; using defaults. {e}")
                                .as_str(),
                        ));
                        MountOptions::default()
                    }
                }
            }
        };
        let display = display_name.to_string();
        let id = self.as_ref().resolve_id(&id.to_string(), &display, false);
        let mount = Mount {
            name: id.clone(),
            display_name: display,
            source: source.to_string(),
            subpath: subpath.to_string().trim_matches('/').to_string(),
            mountpoint: PathBuf::from(mountpoint.to_string()),
            options,
            enabled,
        };
        {
            let mut rust = self.as_mut().rust_mut();
            let p = &mut rust.pending;
            p.upsert_mounts.retain(|m| m.name != mount.name);
            p.delete_mounts.retain(|n| n != &mount.name);
            p.upsert_mounts.push(mount);
        }
        self.as_mut().refresh();
        QString::from(id.as_str())
    }

    fn remove_mount(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        let in_applied = self.as_ref().rust().applied.mounts.iter().any(|m| m.name == name);
        {
            let mut rust = self.as_mut().rust_mut();
            let p = &mut rust.pending;
            p.upsert_mounts.retain(|m| m.name != name);
            // Only stage a delete if the mount actually exists on disk; dropping
            // a pending-only create just needs the upsert removed above.
            if in_applied && !p.delete_mounts.contains(&name) {
                p.delete_mounts.push(name);
            }
        }
        self.as_mut().refresh();
    }

    fn upsert_source(
        mut self: Pin<&mut Self>,
        id: &QString,
        display_name: &QString,
        kind: &QString,
        options_json: &QString,
        secret: &QString,
    ) -> QString {
        let kind_tag = kind.to_string();
        let Some(kind) = SourceKind::from_tag(&kind_tag) else {
            self.as_mut()
                .set_error_string(QString::from(format!("“{kind_tag}” isn’t a source type this version supports.").as_str()));
            return QString::default();
        };
        let options: BTreeMap<String, String> = match serde_json::from_str(&options_json.to_string()) {
            Ok(o) => o,
            Err(e) => {
                self.as_mut()
                    .set_error_string(QString::from(format!("Those source settings couldn’t be read. {e}").as_str()));
                return QString::default();
            }
        };
        if let Err(e) = rclone_mounts_core::source_schema::validate_options_against_schema(&kind_tag, &options) {
            self.as_mut().set_error_string(QString::from(e.as_str()));
            return QString::default();
        }
        let secret = secret.to_string();
        let display = display_name.to_string();
        let id = self.as_ref().resolve_id(&id.to_string(), &display, true);
        let mut new_secrets = BTreeMap::new();
        if !secret.is_empty() {
            new_secrets.insert("pass".to_string(), SecretValue { value: secret, obscure: true });
        }
        let def = SourceDef {
            name: id.clone(),
            display_name: display,
            kind,
            options,
            new_secrets,
        };
        {
            let mut rust = self.as_mut().rust_mut();
            let p = &mut rust.pending;
            p.upsert_sources.retain(|s| s.name != def.name);
            p.delete_sources.retain(|n| n != &def.name);
            p.upsert_sources.push(def);
        }
        self.as_mut().set_error_string(QString::default());
        self.as_mut().refresh();
        QString::from(id.as_str())
    }

    fn remove_source(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        let in_applied = self.as_ref().rust().applied.sources.iter().any(|s| s.name == name);
        {
            let mut rust = self.as_mut().rust_mut();
            let p = &mut rust.pending;
            p.upsert_sources.retain(|s| s.name != name);
            if in_applied && !p.delete_sources.contains(&name) {
                p.delete_sources.push(name);
            }
        }
        self.as_mut().refresh();
    }

    fn start_mount(mut self: Pin<&mut Self>, name: &QString) {
        self.as_mut().lifecycle(name, true);
    }

    fn stop_mount(mut self: Pin<&mut Self>, name: &QString) {
        self.as_mut().lifecycle(name, false);
    }

    fn refresh_status(mut self: Pin<&mut Self>) {
        // Coalesce: if the previous poll is still in flight (slow helper
        // round-trips), skip this tick rather than spawning another worker.
        if self.as_ref().rust().status_poll_in_flight {
            return;
        }
        let (backend, names) = {
            let this = self.as_ref();
            let rust = this.rust();
            let Some(backend) = rust.backend.as_ref() else {
                return;
            };
            let names: Vec<String> = rust.applied.mounts.iter().map(|m| m.name.clone()).collect();
            (Arc::clone(backend), names)
        };
        if names.is_empty() {
            return;
        }

        let thread = self.as_ref().qt_thread();
        self.as_mut().rust_mut().status_poll_in_flight = true;
        // Run the per-mount status queries off the GUI thread; post the result
        // back to update the model. `async_io::block_on` here runs on the worker,
        // not the GUI thread, so a blocking helper call can't freeze the UI.
        std::thread::spawn(move || {
            let map = async_io::block_on(read_statuses(backend.as_ref(), &names));
            let _ = thread.queue(move |mut obj| {
                {
                    let mut rust = obj.as_mut().rust_mut();
                    rust.status_poll_in_flight = false;
                    rust.statuses = map;
                }
                obj.as_mut().refresh();
            });
        });
    }

    /// Shared body for start/stop: fire the live action, surface any error,
    /// then re-read status and refresh the model.
    fn lifecycle(mut self: Pin<&mut Self>, name: &QString, start: bool) {
        let name = name.to_string();
        self.as_mut().set_error_string(QString::default());
        if self.as_ref().rust().backend.is_none() {
            self.as_mut()
                .set_error_string(QString::from("The rclone settings module isn’t ready yet. Try closing and reopening it."));
            return;
        }
        let result = {
            let this = self.as_ref();
            let backend = this.rust().backend.as_ref().unwrap();
            async_io::block_on(async {
                if start {
                    backend.start_mount(&name).await
                } else {
                    backend.stop_mount(&name).await
                }
            })
        };
        if let Err(e) = result {
            let verb = if start { "start" } else { "stop" };
            tracing::error!(error = %e, mount = %name, "{verb} failed");
            self.as_mut()
                .set_error_string(QString::from(format!("Couldn’t {verb} “{name}”. {e}").as_str()));
        }
        self.as_mut().fetch_statuses();
        self.as_mut().refresh();
    }

    /// Resolve the id for an upsert: keep an explicit id (editing an existing
    /// row), or derive a fresh stable id from the display name, unique against
    /// the currently displayed sources/mounts.
    fn resolve_id(self: Pin<&Self>, id: &str, display: &str, is_source: bool) -> String {
        if !id.is_empty() {
            return id.to_string();
        }
        let rust = self.rust();
        let displayed = rust.applied.preview(&rust.pending);
        let existing: std::collections::HashSet<String> = if is_source {
            displayed.sources.iter().map(|s| s.name.clone()).collect()
        } else {
            displayed.mounts.iter().map(|m| m.name.clone()).collect()
        };
        let fallback = if is_source { "source" } else { "mount" };
        rclone_mounts_core::naming::derive_id(display, fallback, |c| existing.contains(c))
    }

    /// Re-query systemd ActiveState for every applied mount. Plain helper (not a
    /// QML invokable); callers refresh the model afterwards. Runs synchronously
    /// on the caller's thread — used only on the post-start/stop path where a
    /// query has already just happened, not the initial load/poll.
    fn fetch_statuses(mut self: Pin<&mut Self>) {
        let backend_and_names = {
            let this = self.as_ref();
            let rust = this.rust();
            rust.backend.as_ref().map(|b| {
                let names: Vec<String> =
                    rust.applied.mounts.iter().map(|m| m.name.clone()).collect();
                (Arc::clone(b), names)
            })
        };
        let map = match backend_and_names {
            Some((backend, names)) => async_io::block_on(read_statuses(backend.as_ref(), &names)),
            None => BTreeMap::new(),
        };
        self.as_mut().rust_mut().statuses = map;
    }

    /// Recompute the displayed JSON + dirty flag from `applied` + `pending`.
    fn refresh(mut self: Pin<&mut Self>) {
        let (mounts_json, sources_json, dirty) = {
            let this = self.as_ref();
            let rust = this.rust();
            let displayed = rust.applied.preview(&rust.pending);
            let applied_names: std::collections::HashSet<&str> =
                rust.applied.mounts.iter().map(|m| m.name.as_str()).collect();
            let mounts: Vec<MountView> = displayed
                .mounts
                .iter()
                .map(|m| {
                    let applied = applied_names.contains(m.name.as_str());
                    let active = if applied {
                        rust.statuses
                            .get(&m.name)
                            .cloned()
                            .unwrap_or_else(|| "unknown".into())
                    } else {
                        "unsaved".into()
                    };
                    MountView {
                        name: m.name.clone(),
                        display_name: m.display_name.clone(),
                        source: m.source.clone(),
                        subpath: m.subpath.clone(),
                        mountpoint: m.mountpoint.display().to_string(),
                        options: m.options.clone(),
                        enabled: m.enabled,
                        active,
                        applied,
                    }
                })
                .collect();
            let sources: Vec<SourceView> = displayed.sources.iter().map(SourceView::from).collect();
            (
                serde_json::to_string(&mounts).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&sources).unwrap_or_else(|_| "[]".into()),
                !changeset_is_empty(&rust.pending),
            )
        };
        self.as_mut().set_mounts_json(QString::from(mounts_json.as_str()));
        self.as_mut().set_sources_json(QString::from(sources_json.as_str()));
        self.as_mut().set_dirty(dirty);
    }
}
