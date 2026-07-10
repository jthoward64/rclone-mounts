// SPDX-License-Identifier: GPL-2.0-or-later

//! Drives rclone's own non-interactive config state machine
//! (`rclone config create --non-interactive` / `rclone config update
//! --continue --state=... --result=...`) for backends that need more than a
//! flat form: Google Drive's OAuth handshake and iCloud Drive's 2FA prompt.
//!
//! Each step is one `rclone` subprocess invocation. rclone answers with a
//! single JSON object on stdout: either it needs another answer (`State` is
//! non-empty, `Option` describes what's needed) or it's done (`State` is
//! empty) or it failed (`Error` is non-empty). This mirrors exactly what a
//! human driving `rclone config create` interactively would be prompted for,
//! just machine-readable instead of a TTY prompt.
//!
//! `--config` always points at a private scratch file, never a real
//! `rclone.conf` — the caller reads the finished remote section back out of
//! it once `Done` and is responsible for splitting it into non-secret
//! options (`sources.conf`) vs. secret values (the encrypted credential
//! blob) via [`crate::backend`]'s `secret_keys_for` table, then the scratch
//! file is dropped (deleted) without ever persisting plaintext secrets
//! outside this app's own encrypted store.

use crate::{Error, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::process::Command;

/// One field rclone still needs an answer for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverPrompt {
    /// rclone's internal option name, e.g. `config_is_local`, `config_2fa`.
    pub name: String,
    /// Human-readable prompt text, shown to the user verbatim.
    pub help: String,
    /// Whether the answer should be entered as a password (echo hidden).
    pub is_password: bool,
}

/// Heuristic: does this prompt look like a 2FA code request? rclone doesn't
/// expose a stable machine-readable "this is a 2FA step" flag, so callers
/// that need to distinguish it (iCloud) match on the option name/help text.
impl DriverPrompt {
    pub fn looks_like_2fa(&self) -> bool {
        let name = self.name.to_ascii_lowercase();
        let help = self.help.to_ascii_lowercase();
        name.contains("2fa")
            || name.contains("code")
            || help.contains("2fa")
            || help.contains("two-factor")
    }

    pub fn looks_like_local_browser_choice(&self) -> bool {
        self.name == "config_is_local"
    }
}

#[derive(Debug, Clone)]
pub enum DriverStep {
    /// Config finished. `remote_conf` is the complete rclone.conf-shaped
    /// `[<remote_name>]` section, read from the scratch config file.
    Done {
        remote_conf: String,
    },
    /// rclone needs one more answer to proceed.
    NeedInput {
        state: String,
        prompt: DriverPrompt,
    },
    Error(String),
}

#[derive(Debug, Deserialize)]
struct RawStep {
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Option")]
    option: Option<RawOption>,
    #[serde(rename = "Error")]
    error: String,
}

#[derive(Debug, Deserialize)]
struct RawOption {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Help", default)]
    help: String,
    #[serde(rename = "IsPassword", default)]
    is_password: bool,
}

impl DriverStep {
    fn from_stdout(
        stdout: &str,
        scratch_path: &std::path::Path,
        remote_name: &str,
    ) -> Result<Self> {
        let trimmed = stdout.trim();
        let raw: RawStep = serde_json::from_str(trimmed).map_err(|e| {
            Error::Systemd(format!(
                "couldn't parse `rclone config` output: {e} (got: {trimmed:?})"
            ))
        })?;
        if !raw.error.is_empty() {
            return Ok(DriverStep::Error(raw.error));
        }
        if raw.state.is_empty() {
            let remote_conf = read_remote_section(scratch_path, remote_name)?;
            return Ok(DriverStep::Done { remote_conf });
        }
        let Some(opt) = raw.option else {
            return Ok(DriverStep::Error(
                "rclone asked to continue but didn't say what it needs".to_string(),
            ));
        };
        Ok(DriverStep::NeedInput {
            state: raw.state,
            prompt: DriverPrompt {
                name: opt.name,
                help: opt.help,
                is_password: opt.is_password,
            },
        })
    }
}

/// Read back the `[<remote_name>]` section rclone wrote into the scratch
/// config file, verbatim (whitespace-trimmed), as the raw ini-shaped text a
/// caller can run through [`crate::rclone_config::Document::parse`].
fn read_remote_section(scratch_path: &std::path::Path, remote_name: &str) -> Result<String> {
    let text = std::fs::read_to_string(scratch_path)
        .map_err(|e| Error::Systemd(format!("read scratch rclone config: {e}")))?;
    let doc = crate::rclone_config::Document::parse(&text)?;
    if !doc.sections().contains(&remote_name) {
        return Err(Error::Systemd(format!(
            "rclone finished but the scratch config has no [{remote_name}] section"
        )));
    }
    // Re-emitted via `Document::set` (not raw string formatting) so this
    // still refuses a control character even though the source here is
    // rclone's own scratch file, not directly attacker-controlled input —
    // one write path, one place that can reject a bad value.
    let entries: Vec<(String, String)> = doc
        .section_entries(remote_name)
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let mut out = crate::rclone_config::Document::default();
    for (k, v) in &entries {
        out.set(remote_name, k, v)?;
    }
    Ok(out.render())
}

/// Drives one `rclone config create`/`update --continue` session for a
/// single remote. Each step blocks the calling thread for however long the
/// underlying `rclone` subprocess takes — for Drive's local-browser OAuth
/// step that can be the whole time the user spends in their browser.
/// Callers MUST run this off the GUI thread (see `refresh_status`'s
/// worker-thread + `qt_thread().queue()` pattern in the KCM).
pub struct ConfigDriver {
    scratch_config: tempfile::NamedTempFile,
    remote_name: String,
}

/// Whether `key` is a password-type field for `kind_tag` that rclone stores
/// obscured rather than in the clear. `rclone config create`'s positional
/// `key value` args auto-obscure these; the `--<backend>-<option>` flag form
/// we use instead to keep them off argv (see [`ConfigDriver::start`]) does
/// not, so we have to obscure it ourselves before handing it over. Currently
/// just iCloud Drive's account password — Drive's `client_secret` is an
/// OAuth app secret rclone stores in the clear, not a user password.
fn needs_obscure(kind_tag: &str, key: &str) -> bool {
    matches!((kind_tag, key), ("iclouddrive", "password"))
}

/// The `RCLONE_<BACKEND>_<OPTION>` environment variable name rclone binds to
/// the `--<backend>-<option>` backend flag, per rclone's standard "every
/// flag is also an env var" convention.
fn backend_option_env_var(kind_tag: &str, key: &str) -> String {
    format!(
        "RCLONE_{}_{}",
        kind_tag.to_uppercase().replace('-', "_"),
        key.to_uppercase().replace('-', "_")
    )
}

impl ConfigDriver {
    /// Starts a fresh config session: `rclone config create <remote_name>
    /// <kind_tag> --non-interactive --config <scratch>`. `initial_kv` seeds
    /// whatever's known up front (e.g. Drive's `client_id`/`client_secret`/
    /// `scope`, or iCloud's `apple_id`/`password`) — these ride in as
    /// `RCLONE_<BACKEND>_<OPTION>` environment variables on the child
    /// process rather than as `key value` command-line arguments. Unlike
    /// argv (world-readable via `ps`/`/proc/<pid>/cmdline` for the life of
    /// the subprocess), a process's environment is only readable by its own
    /// user and root, so this keeps the iCloud account password and any
    /// OAuth client secret out of the one channel any local user can watch.
    pub fn start(
        kind_tag: &str,
        remote_name: &str,
        initial_kv: &BTreeMap<String, String>,
    ) -> Result<(Self, DriverStep)> {
        let scratch_config = tempfile::Builder::new()
            .prefix("rclone-mounts-scratch-")
            .tempfile()
            .map_err(|e| Error::Systemd(format!("create scratch config: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                scratch_config.path(),
                std::fs::Permissions::from_mode(0o600),
            );
        }

        let mut cmd = Command::new("rclone");
        cmd.arg("config")
            .arg("create")
            .arg(remote_name)
            .arg(kind_tag);
        for (k, v) in initial_kv {
            let v = if needs_obscure(kind_tag, k) {
                crate::rclone_cli::obscure(v)?
            } else {
                v.clone()
            };
            cmd.env(backend_option_env_var(kind_tag, k), v);
        }
        cmd.arg("--non-interactive")
            .arg("--config")
            .arg(scratch_config.path());

        let step = run(&mut cmd, scratch_config.path(), remote_name)?;
        let driver = Self {
            scratch_config,
            remote_name: remote_name.to_string(),
        };
        Ok((driver, step))
    }

    /// Answers the most recent `NeedInput`'s prompt and advances the state
    /// machine: `rclone config update <remote_name> --continue --state
    /// <state> --result <answer> --non-interactive --config <scratch>`.
    pub fn continue_with(&mut self, state: &str, answer: &str) -> Result<DriverStep> {
        let mut cmd = Command::new("rclone");
        cmd.arg("config")
            .arg("update")
            .arg(&self.remote_name)
            .arg("--continue")
            .arg("--state")
            .arg(state)
            .arg("--result")
            .arg(answer)
            .arg("--non-interactive")
            .arg("--config")
            .arg(self.scratch_config.path());
        run(&mut cmd, self.scratch_config.path(), &self.remote_name)
    }
}

fn run(cmd: &mut Command, scratch_path: &std::path::Path, remote_name: &str) -> Result<DriverStep> {
    let output = cmd
        .output()
        .map_err(|e| Error::Systemd(format!("spawn `rclone config`: {e}")))?;
    if !output.status.success() && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Ok(DriverStep::Error(stderr.trim().to_string()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    DriverStep::from_stdout(&stdout, scratch_path, remote_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch_with(contents: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{contents}").unwrap();
        f
    }

    #[test]
    fn parses_need_input_step() {
        let json = r#"{"State":"*oauth-islocal,,","Option":{"Name":"config_is_local","Help":"Use web browser?","Default":true,"IsPassword":false},"Error":""}"#;
        let scratch = scratch_with("");
        let step = DriverStep::from_stdout(json, scratch.path(), "gd").unwrap();
        match step {
            DriverStep::NeedInput { state, prompt } => {
                assert_eq!(state, "*oauth-islocal,,");
                assert_eq!(prompt.name, "config_is_local");
                assert!(!prompt.is_password);
                assert!(prompt.looks_like_local_browser_choice());
            }
            other => panic!("expected NeedInput, got {other:?}"),
        }
    }

    #[test]
    fn parses_2fa_prompt() {
        let json = r#"{"State":"*2fa,,","Option":{"Name":"config_2fa","Help":"Enter your 2FA code or type sms","IsPassword":false},"Error":""}"#;
        let scratch = scratch_with("");
        let step = DriverStep::from_stdout(json, scratch.path(), "ic").unwrap();
        match step {
            DriverStep::NeedInput { prompt, .. } => assert!(prompt.looks_like_2fa()),
            other => panic!("expected NeedInput, got {other:?}"),
        }
    }

    #[test]
    fn empty_state_reads_finished_remote_section() {
        let json = r#"{"State":"","Option":null,"Error":""}"#;
        let scratch = scratch_with(
            "[gd]\ntype = drive\ntoken = {\"access_token\":\"x\"}\n\n[other]\ntype = smb\n",
        );
        let step = DriverStep::from_stdout(json, scratch.path(), "gd").unwrap();
        match step {
            DriverStep::Done { remote_conf } => {
                assert!(remote_conf.contains("type = drive"));
                assert!(remote_conf.contains("token ="));
                assert!(
                    !remote_conf.contains("other"),
                    "must not leak unrelated sections: {remote_conf}"
                );
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    #[test]
    fn error_field_takes_priority() {
        let json = r#"{"State":"","Option":null,"Error":"invalid password"}"#;
        let scratch = scratch_with("");
        let step = DriverStep::from_stdout(json, scratch.path(), "gd").unwrap();
        assert!(matches!(step, DriverStep::Error(e) if e == "invalid password"));
    }

    #[test]
    fn malformed_json_is_an_error() {
        let scratch = scratch_with("");
        let err = DriverStep::from_stdout("not json", scratch.path(), "gd").unwrap_err();
        assert!(matches!(err, Error::Systemd(_)));
    }

    #[test]
    fn done_without_expected_section_errors() {
        let json = r#"{"State":"","Option":null,"Error":""}"#;
        let scratch = scratch_with("[unrelated]\ntype = smb\n");
        let err = DriverStep::from_stdout(json, scratch.path(), "gd").unwrap_err();
        assert!(matches!(err, Error::Systemd(_)));
    }
}
