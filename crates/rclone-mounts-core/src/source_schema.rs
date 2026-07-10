// SPDX-License-Identifier: GPL-2.0-or-later

//! Declarative per-kind field schema: the single source of truth for which
//! rclone remote types the app offers and what connection fields each one
//! takes. Rust owns this table and serializes it to the KCM's QML frontend
//! (`kind_schemas_json`) so the type list and its fields never drift out of
//! sync between the two layers.
//!
//! Wizard-only kinds (OAuth / interactive-auth backends) carry no flat
//! fields here — their setup is driven by a multi-step flow instead of a
//! static form; see `rclone_config_driver`.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Text,
    Bool,
    Select,
}

/// One choice in a `Select` field's dropdown. `value` is the wire value
/// written to the source's options map; `label` is what the picker shows.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FieldOption {
    pub value: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct FieldSchema {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub field_type: FieldType,
    /// Choices for a `Select` field; empty for every other field type.
    pub options: &'static [FieldOption],
    /// Whether rclone can't actually connect without this field — the KCM
    /// marks it in the form and blocks OK/Continue until it's filled in.
    /// Bool/Select fields are never required: a checkbox's unset state and
    /// a select's blank/default choice are both meaningful values, not
    /// "missing input".
    pub required: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct KindSchema {
    pub tag: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub fields: &'static [FieldSchema],
    /// True for kinds whose setup is a multi-step interactive flow (OAuth,
    /// 2FA) rather than a static field form. The flat-schema editor doesn't
    /// apply; `fields` is empty for these.
    pub wizard_only: bool,
    /// From `backend_features::lookup` — whether this backend can stream an
    /// upload without buffering it first. Drives the KCM's cache-mode
    /// warning (mirrors rclone's own "it can't stream" mount-time log line).
    /// WebDAV's real value depends on its `vendor` option, not the kind
    /// alone, so it's fixed `false` here (true for every vendor except
    /// `rclone`) and the KCM overrides it per-source; see
    /// `backend_features::webdav_put_stream`.
    pub put_stream: bool,
    /// From `backend_features::lookup` — whether this backend allows
    /// duplicate filenames within a directory (a FUSE mount can only show
    /// one entry per name).
    pub duplicate_files: bool,
}

const fn text(key: &'static str, label: &'static str, placeholder: &'static str) -> FieldSchema {
    FieldSchema { key, label, placeholder, field_type: FieldType::Text, options: &[], required: false }
}

/// Like `text`, but rclone can't actually connect without it — the KCM
/// marks it and blocks OK/Continue until it's filled in.
const fn required_text(key: &'static str, label: &'static str, placeholder: &'static str) -> FieldSchema {
    FieldSchema { key, label, placeholder, field_type: FieldType::Text, options: &[], required: true }
}

const fn boolean(key: &'static str, label: &'static str) -> FieldSchema {
    FieldSchema { key, label, placeholder: "", field_type: FieldType::Bool, options: &[], required: false }
}

const fn select(key: &'static str, label: &'static str, options: &'static [FieldOption]) -> FieldSchema {
    FieldSchema { key, label, placeholder: "", field_type: FieldType::Select, options, required: false }
}

const SMB_FIELDS: &[FieldSchema] = &[
    required_text("host", "Host:", "files.example.com"),
    text("user", "User:", "alice"),
    text("domain", "Domain:", "optional"),
    text("port", "Port:", "445"),
];

const WEBDAV_VENDOR_OPTIONS: &[FieldOption] = &[
    FieldOption { value: "", label: "Not set" },
    FieldOption { value: "fastmail", label: "Fastmail Files" },
    FieldOption { value: "nextcloud", label: "Nextcloud" },
    FieldOption { value: "owncloud", label: "ownCloud" },
    FieldOption { value: "infinitescale", label: "Infinitescale" },
    FieldOption { value: "opencloud", label: "OpenCloud" },
    FieldOption { value: "sharepoint", label: "Sharepoint" },
    FieldOption { value: "sharepoint-ntlm", label: "Sharepoint (NTLM)" },
    FieldOption { value: "rclone", label: "rclone" },
    FieldOption { value: "other", label: "Other" },
];

const WEBDAV_FIELDS: &[FieldSchema] = &[
    required_text("url", "URL:", "https://dav.example.com/remote.php/dav/files/alice"),
    select("vendor", "Vendor:", WEBDAV_VENDOR_OPTIONS),
    text("user", "User:", "alice"),
];

const SFTP_FIELDS: &[FieldSchema] = &[
    required_text("host", "Host:", "sftp.example.com"),
    text("user", "User:", "(current user)"),
    text("port", "Port:", "22"),
    text("key_file", "Private key file:", "~/.ssh/id_ed25519"),
];

const FTP_FIELDS: &[FieldSchema] = &[
    required_text("host", "Host:", "ftp.example.com"),
    text("user", "User:", "(current user)"),
    text("port", "Port:", "21"),
    boolean("tls", "Use implicit FTPS:"),
    boolean("explicit_tls", "Use explicit FTPS (FTPES):"),
];

const DRIVE_FIELDS: &[FieldSchema] = &[
    text("client_id", "Client ID (optional):", "leave blank to use the default"),
    text("client_secret", "Client secret (optional):", "leave blank to use the default"),
    text("root_folder_id", "Root folder ID (optional):", "leave blank for the whole drive"),
];

const KIND_SCHEMAS: &[KindSchema] = &[
    KindSchema {
        tag: "smb",
        label: "SMB / Windows share",
        icon: "folder-network-symbolic",
        fields: SMB_FIELDS,
        wizard_only: false,
        put_stream: true,
        duplicate_files: false,
    },
    KindSchema {
        tag: "webdav",
        label: "WebDAV",
        icon: "folder-cloud-symbolic",
        fields: WEBDAV_FIELDS,
        wizard_only: false,
        // Real value is vendor-dependent — see the field doc comment above.
        put_stream: false,
        duplicate_files: false,
    },
    KindSchema {
        tag: "sftp",
        label: "SFTP",
        icon: "folder-network-symbolic",
        fields: SFTP_FIELDS,
        wizard_only: false,
        put_stream: true,
        duplicate_files: false,
    },
    KindSchema {
        tag: "ftp",
        label: "FTP / FTPS",
        icon: "folder-network-symbolic",
        fields: FTP_FIELDS,
        wizard_only: false,
        put_stream: true,
        duplicate_files: false,
    },
    KindSchema {
        tag: "drive",
        label: "Google Drive",
        icon: "folder-google-drive",
        fields: DRIVE_FIELDS,
        wizard_only: true,
        put_stream: true,
        duplicate_files: true,
    },
    KindSchema {
        tag: "iclouddrive",
        label: "iCloud Drive",
        icon: "folder-cloud-symbolic",
        fields: &[],
        wizard_only: true,
        put_stream: false,
        duplicate_files: false,
    },
];

pub fn all_kind_schemas() -> &'static [KindSchema] {
    KIND_SCHEMAS
}

pub fn schema_for(tag: &str) -> Option<&'static KindSchema> {
    KIND_SCHEMAS.iter().find(|k| k.tag == tag)
}

/// Tags of kinds that have a shared/admin credential override the KCM's
/// Credentials settings page can manage (see `Backend::provider_override`).
/// Every entry here must also appear in `KIND_SCHEMAS` — the Credentials
/// page reuses that schema's `label`/`icon` for display rather than
/// duplicating them.
const CREDENTIAL_CAPABLE_KINDS: &[&str] = &["drive"];

pub fn credential_capable_kinds() -> &'static [&'static str] {
    CREDENTIAL_CAPABLE_KINDS
}

/// Reject any option key that isn't part of this kind's flat schema. Wizard-only
/// kinds skip this check entirely — their options are produced by the
/// interactive driver, not a form the KCM can validate against a static list.
pub fn validate_options_against_schema(
    kind_tag: &str,
    options: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    let Some(schema) = schema_for(kind_tag) else {
        return Err(format!("\u{201c}{kind_tag}\u{201d} isn\u{2019}t a source type this version supports."));
    };
    if schema.wizard_only {
        return Ok(());
    }
    for key in options.keys() {
        if !schema.fields.iter().any(|f| f.key == key) {
            return Err(format!("\u{201c}{key}\u{201d} isn\u{2019}t a valid field for {}.", schema.label));
        }
    }
    for field in schema.fields {
        if field.field_type == FieldType::Bool {
            if let Some(v) = options.get(field.key) {
                if v != "true" && v != "false" {
                    return Err(format!("\u{201c}{}\u{201d} must be true or false.", field.label));
                }
            }
        }
        if field.field_type == FieldType::Select {
            if let Some(v) = options.get(field.key) {
                if !field.options.iter().any(|o| o.value == v.as_str()) {
                    return Err(format!("\u{201c}{}\u{201d} isn\u{2019}t a valid choice for {}.", v, field.label));
                }
            }
        }
    }
    if kind_tag == "ftp" && options.get("tls").map(String::as_str) == Some("true") && options.get("explicit_tls").map(String::as_str) == Some("true") {
        return Err("Implicit and explicit FTPS can\u{2019}t both be enabled \u{2014} pick one.".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn every_kind_schema_serializes_non_empty() {
        let json = serde_json::to_string(all_kind_schemas()).unwrap();
        assert!(json.len() > 10);
        for kind in all_kind_schemas() {
            assert!(json.contains(kind.tag), "missing {} in serialized schema", kind.tag);
        }
    }

    // Every kind's put_stream/duplicate_files must match
    // backend_features::lookup — catches the schema entry going stale after
    // the reference table is updated (or vice versa). WebDAV is excluded:
    // its true value is vendor-dependent, not a fixed per-kind fact (see
    // `backend_features::webdav_put_stream`), so KindSchema intentionally
    // carries a fixed `false` fallback there instead of a table lookup.
    #[test]
    fn kind_schema_features_match_backend_features_table() {
        for kind in all_kind_schemas() {
            if kind.tag == "webdav" {
                continue;
            }
            let expected = crate::backend_features::lookup(kind.tag)
                .unwrap_or_else(|| panic!("{} is missing from backend_features::BACKEND_FEATURES", kind.tag));
            assert_eq!(kind.put_stream, expected.put_stream, "put_stream mismatch for {}", kind.tag);
            assert_eq!(kind.duplicate_files, expected.duplicate_files, "duplicate_files mismatch for {}", kind.tag);
        }
    }

    #[test]
    fn wizard_only_kinds_carry_no_flat_fields() {
        assert!(schema_for("drive").unwrap().wizard_only);
        assert!(schema_for("iclouddrive").unwrap().wizard_only);
    }

    #[test]
    fn validate_rejects_unknown_option_key() {
        let mut opts = BTreeMap::new();
        opts.insert("bogus".to_string(), "x".to_string());
        assert!(validate_options_against_schema("smb", &opts).is_err());
    }

    #[test]
    fn validate_accepts_known_keys_and_bool_values() {
        let mut opts = BTreeMap::new();
        opts.insert("host".to_string(), "h".to_string());
        opts.insert("tls".to_string(), "true".to_string());
        assert!(validate_options_against_schema("ftp", &opts).is_ok());
    }

    #[test]
    fn validate_rejects_non_bool_value_for_bool_field() {
        let mut opts = BTreeMap::new();
        opts.insert("tls".to_string(), "yes".to_string());
        assert!(validate_options_against_schema("ftp", &opts).is_err());
    }

    #[test]
    fn validate_skips_field_check_for_wizard_only_kinds() {
        let mut opts = BTreeMap::new();
        opts.insert("token".to_string(), "whatever".to_string());
        assert!(validate_options_against_schema("drive", &opts).is_ok());
    }

    #[test]
    fn validate_rejects_unknown_kind() {
        let opts = BTreeMap::new();
        assert!(validate_options_against_schema("nope", &opts).is_err());
    }

    #[test]
    fn validate_rejects_both_ftp_tls_modes_at_once() {
        let mut opts = BTreeMap::new();
        opts.insert("tls".to_string(), "true".to_string());
        opts.insert("explicit_tls".to_string(), "true".to_string());
        assert!(validate_options_against_schema("ftp", &opts).is_err());
    }

    #[test]
    fn validate_accepts_a_single_ftp_tls_mode() {
        let mut opts = BTreeMap::new();
        opts.insert("tls".to_string(), "true".to_string());
        assert!(validate_options_against_schema("ftp", &opts).is_ok());
        let mut opts = BTreeMap::new();
        opts.insert("explicit_tls".to_string(), "true".to_string());
        assert!(validate_options_against_schema("ftp", &opts).is_ok());
        assert!(validate_options_against_schema("ftp", &BTreeMap::new()).is_ok());
    }
}
