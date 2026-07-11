// SPDX-License-Identifier: GPL-2.0-or-later

//! Resolution of the OAuth client id/secret used for Google Drive's `rclone
//! config create` invocation, in precedence order: a per-source user
//! override beats the admin override (read directly when the caller is
//! already system-scope, otherwise via the lightweight cross-scope D-Bus
//! read — see [`crate::backend::fetch_shared_provider_credential`], since an
//! admin-shared credential is meant to work for every local user's own
//! mounts too) beats a build-time compiled default. If none apply, rclone
//! falls back to its own shared client (subject to Google's shared rate
//! limits) — this function never requires the user to have one, though
//! callers (e.g. the sign-in wizard) may choose to require it anyway.

use crate::backend::Backend;
use crate::source::SourceKind;
use crate::Result;

/// Build-time default, baked in via `build.rs` from `RCLONE_MOUNTS_DRIVE_CLIENT_ID`/
/// `_SECRET` if the packager set them. `None` when neither was set at compile
/// time. Deliberately not wrapped in any `Q_PROPERTY` or `Backend` method —
/// read only here, when building the `rclone config create` argv.
fn build_time_drive_credentials() -> Option<(&'static str, &'static str)> {
    let id = option_env!("RCLONE_MOUNTS_DRIVE_CLIENT_ID")?;
    let secret = option_env!("RCLONE_MOUNTS_DRIVE_CLIENT_SECRET")?;
    if id.is_empty() || secret.is_empty() {
        return None;
    }
    Some((id, secret))
}

/// Resolve which client_id/client_secret to pass to `rclone config create`
/// for a Drive source: per-source user override > admin/global override >
/// build-time compiled default > `None` (rclone's own built-in shared client).
pub async fn resolve_drive_client_credentials(
    backend: &dyn Backend,
    user_client_id: Option<&str>,
    user_client_secret: Option<&str>,
) -> Result<Option<(String, String)>> {
    if let (Some(id), Some(secret)) = (user_client_id, user_client_secret) {
        if !id.is_empty() && !secret.is_empty() {
            return Ok(Some((id.to_string(), secret.to_string())));
        }
    }
    if let Some(pair) = backend.provider_override(SourceKind::Drive).await? {
        return Ok(Some(pair));
    }
    // `backend.provider_override` only ever sees the admin override when the
    // active backend is itself system-scope. A user-scope backend can't
    // decrypt that credential directly, but the whole point of an admin
    // sharing it is that every local user's *own* Drive mounts get to use
    // it too — so fall back to the lightweight, no-admin-auth D-Bus read.
    // Best-effort: if the helper isn't installed/reachable, treat that the
    // same as "no shared credential" rather than failing sign-in over it.
    if let Ok(Some(pair)) = crate::backend::fetch_shared_provider_credential(SourceKind::Drive).await
    {
        return Ok(Some(pair));
    }
    Ok(build_time_drive_credentials().map(|(a, b)| (a.to_string(), b.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Changeset, State};
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// A fake `Backend` that returns a fixed provider override, for testing
    /// precedence resolution without any real store/subprocess.
    struct FakeBackend {
        override_pair: Mutex<Option<(String, String)>>,
    }

    #[async_trait]
    impl Backend for FakeBackend {
        async fn load(&self) -> Result<State> {
            unimplemented!()
        }
        async fn apply(&self, _: Changeset) -> Result<()> {
            unimplemented!()
        }
        async fn start_mount(&self, _: &str) -> Result<()> {
            unimplemented!()
        }
        async fn stop_mount(&self, _: &str) -> Result<()> {
            unimplemented!()
        }
        async fn mount_status(&self, _: &str) -> Result<String> {
            unimplemented!()
        }
        async fn set_provider_override(&self, _: SourceKind, _: &str, _: &str) -> Result<()> {
            unimplemented!()
        }
        async fn provider_override(&self, _: SourceKind) -> Result<Option<(String, String)>> {
            Ok(self.override_pair.lock().unwrap().clone())
        }
    }

    #[test]
    fn user_override_wins_over_everything() {
        let backend = FakeBackend {
            override_pair: Mutex::new(Some(("admin-id".into(), "admin-secret".into()))),
        };
        let result = async_io::block_on(resolve_drive_client_credentials(
            &backend,
            Some("user-id"),
            Some("user-secret"),
        ))
        .unwrap();
        assert_eq!(
            result,
            Some(("user-id".to_string(), "user-secret".to_string()))
        );
    }

    #[test]
    fn admin_override_wins_when_no_user_override() {
        let backend = FakeBackend {
            override_pair: Mutex::new(Some(("admin-id".into(), "admin-secret".into()))),
        };
        let result =
            async_io::block_on(resolve_drive_client_credentials(&backend, None, None)).unwrap();
        assert_eq!(
            result,
            Some(("admin-id".to_string(), "admin-secret".to_string()))
        );
    }

    #[test]
    fn falls_back_to_none_when_nothing_set() {
        // Build-time default is whatever this dev build was compiled with
        // (normally unset in CI/test), so we only assert the fallback chain
        // reaches "no admin override, no user override" — the build-time
        // tier itself is exercised implicitly (compiles to None here).
        let backend = FakeBackend {
            override_pair: Mutex::new(None),
        };
        let result =
            async_io::block_on(resolve_drive_client_credentials(&backend, None, None)).unwrap();
        assert_eq!(
            result,
            build_time_drive_credentials().map(|(a, b)| (a.to_string(), b.to_string()))
        );
    }

    #[test]
    fn partial_user_override_is_ignored() {
        // Only one of client_id/client_secret supplied — not a valid override,
        // falls through to the next tier.
        let backend = FakeBackend {
            override_pair: Mutex::new(Some(("admin-id".into(), "admin-secret".into()))),
        };
        let result = async_io::block_on(resolve_drive_client_credentials(
            &backend,
            Some("user-id"),
            None,
        ))
        .unwrap();
        assert_eq!(
            result,
            Some(("admin-id".to_string(), "admin-secret".to_string()))
        );
    }
}
