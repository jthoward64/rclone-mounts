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
//! Threading: backend ops (file I/O + a session-bus daemon-reload) are run with
//! `async_io::block_on` directly on the GUI thread. They're fast and local;
//! if a future op can block (e.g. a network probe) this should move to a worker
//! thread with results posted back via a `CxxQtThread`. See [[backend-threading]].

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
        // JSON arrays of the *displayed* (applied + pending) state. QML does
        // JSON.parse(...) and uses the result directly as a ListView model.
        #[qproperty(QString, mounts_json)]
        #[qproperty(QString, sources_json)]
        type BackendController = super::BackendControllerRust;

        /// (Re)load the on-disk state, discarding any pending edits.
        #[qinvokable]
        fn load(self: Pin<&mut BackendController>);

        /// Apply pending edits to disk, then reload. No-op if not dirty.
        #[qinvokable]
        fn commit(self: Pin<&mut BackendController>);

        /// Discard pending edits, reverting the display to the applied state.
        #[qinvokable]
        fn reset(self: Pin<&mut BackendController>);

        /// Create or replace a mount in the pending changeset.
        #[qinvokable]
        fn upsert_mount(
            self: Pin<&mut BackendController>,
            name: &QString,
            source: &QString,
            mountpoint: &QString,
            enabled: bool,
        );

        /// Stage a mount for deletion (or drop a pending create).
        #[qinvokable]
        fn remove_mount(self: Pin<&mut BackendController>, name: &QString);
    }
}

use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use rclone_mounts_core::{Backend, Changeset, LocalBackend, Mount, SourceMetadata, State};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Default)]
pub struct BackendControllerRust {
    dirty: bool,
    busy: bool,
    error_string: QString,
    mounts_json: QString,
    sources_json: QString,

    /// Last snapshot read from disk.
    applied: State,
    /// Edits accumulated since the last load/commit.
    pending: Changeset,
    /// User-scope backend, built lazily on first successful load.
    backend: Option<Box<dyn Backend>>,
}

/// UI-facing projection of a mount. Shape is what the QML delegate consumes.
#[derive(Serialize)]
struct MountView {
    name: String,
    source: String,
    mountpoint: String,
    enabled: bool,
}

impl From<&Mount> for MountView {
    fn from(m: &Mount) -> Self {
        Self {
            name: m.name.clone(),
            source: m.source.clone(),
            mountpoint: m.mountpoint.display().to_string(),
            enabled: m.enabled,
        }
    }
}

/// UI-facing projection of a source. SMB-centric for now (host/user); richer
/// per-kind fields land with the source editor.
#[derive(Serialize)]
struct SourceView {
    name: String,
    kind: String,
    host: String,
    user: String,
    has_secret: bool,
}

impl From<&SourceMetadata> for SourceView {
    fn from(s: &SourceMetadata) -> Self {
        Self {
            name: s.name.clone(),
            kind: s.kind.clone(),
            host: s.options.get("host").cloned().unwrap_or_default(),
            user: s.options.get("user").cloned().unwrap_or_default(),
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

impl ffi::BackendController {
    fn load(mut self: Pin<&mut Self>) {
        init_tracing();
        self.as_mut().set_busy(true);
        self.as_mut().set_error_string(QString::default());

        let outcome: rclone_mounts_core::Result<(Box<dyn Backend>, State)> =
            async_io::block_on(async {
                let backend = LocalBackend::new_user().await?;
                let state = backend.load().await?;
                Ok((Box::new(backend) as Box<dyn Backend>, state))
            });

        match outcome {
            Ok((backend, state)) => {
                tracing::info!(
                    sources = state.sources.len(),
                    mounts = state.mounts.len(),
                    "loaded user state"
                );
                {
                    let mut rust = self.as_mut().rust_mut();
                    rust.backend = Some(backend);
                    rust.applied = state;
                    rust.pending = Changeset::default();
                }
                self.as_mut().refresh();
            }
            Err(e) => {
                tracing::error!(error = %e, "load failed");
                self.as_mut()
                    .set_error_string(QString::from(format!("Failed to load mounts: {e}").as_str()));
            }
        }
        self.as_mut().set_busy(false);
    }

    fn commit(mut self: Pin<&mut Self>) {
        if changeset_is_empty(&self.as_ref().rust().pending) {
            return;
        }
        if self.as_ref().rust().backend.is_none() {
            self.as_mut()
                .set_error_string(QString::from("Backend not initialised; reload first."));
            return;
        }

        self.as_mut().set_busy(true);
        self.as_mut().set_error_string(QString::default());

        let pending = self.as_ref().rust().pending.clone();
        let outcome: rclone_mounts_core::Result<State> = {
            let this = self.as_ref();
            let backend = this.rust().backend.as_ref().unwrap();
            async_io::block_on(async {
                backend.apply(pending).await?;
                backend.load().await
            })
        };

        match outcome {
            Ok(state) => {
                {
                    let mut rust = self.as_mut().rust_mut();
                    rust.applied = state;
                    rust.pending = Changeset::default();
                }
                self.as_mut().refresh();
            }
            Err(e) => {
                tracing::error!(error = %e, "commit failed");
                self.as_mut()
                    .set_error_string(QString::from(format!("Failed to apply changes: {e}").as_str()));
            }
        }
        self.as_mut().set_busy(false);
    }

    fn reset(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending = Changeset::default();
        self.as_mut().set_error_string(QString::default());
        self.as_mut().refresh();
    }

    fn upsert_mount(
        mut self: Pin<&mut Self>,
        name: &QString,
        source: &QString,
        mountpoint: &QString,
        enabled: bool,
    ) {
        let mount = Mount {
            name: name.to_string(),
            source: source.to_string(),
            mountpoint: PathBuf::from(mountpoint.to_string()),
            options: Default::default(),
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

    /// Recompute the displayed JSON + dirty flag from `applied` + `pending`.
    fn refresh(mut self: Pin<&mut Self>) {
        let (mounts_json, sources_json, dirty) = {
            let this = self.as_ref();
            let rust = this.rust();
            let displayed = rust
                .applied
                .preview(&rust.pending)
                .unwrap_or_else(|_| rust.applied.clone());
            let mounts: Vec<MountView> = displayed.mounts.iter().map(MountView::from).collect();
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
