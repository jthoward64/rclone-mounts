// SPDX-License-Identifier: GPL-2.0-or-later

//! Deriving a stable, system-safe id from a freeform display name.
//!
//! Sources and mounts carry a `display_name` the user types (any unicode —
//! emoji, spaces, mixed case) and an internal `name` (the id) that keys the
//! rclone remote, the systemd unit, and the on-disk files. The id must match
//! [`crate::unit_writer::validate_name`] (`^[a-z0-9][a-z0-9-]{0,62}$`), so we
//! slugify the display name down to that alphabet and disambiguate collisions
//! with a numeric suffix. The id is assigned once at creation and never changes
//! when the display name is later edited, so units/files don't churn.

/// Reduce an arbitrary string to a `[a-z0-9-]` slug: lowercase ASCII
/// alphanumerics kept, every other run collapsed to a single `-`, no leading or
/// trailing `-`, capped at 50 chars. May return `""` (e.g. an all-emoji name).
pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.truncate(50);
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Derive a unique id from `display`, falling back to `fallback_prefix` when the
/// name slugifies to nothing. `exists` reports whether a candidate id is already
/// taken (by another live source/mount); collisions get a `-2`, `-3`, … suffix.
pub fn derive_id(display: &str, fallback_prefix: &str, exists: impl Fn(&str) -> bool) -> String {
    let base = {
        let s = slugify(display);
        if s.is_empty() {
            slugify(fallback_prefix)
        } else {
            s
        }
    };
    if !exists(&base) {
        return base;
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}");
        if !exists(&candidate) {
            return candidate;
        }
    }
    unreachable!("the integer space cannot be exhausted")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit_writer::validate_name;

    #[test]
    fn slugify_basics() {
        assert_eq!(slugify("Work Share"), "work-share");
        assert_eq!(slugify("  Photos 2026!! "), "photos-2026");
        assert_eq!(slugify("a___b---c"), "a-b-c");
        assert_eq!(slugify("😀"), "");
        assert_eq!(slugify("Müller"), "m-ller");
    }

    #[test]
    fn derived_ids_are_valid_names() {
        for input in ["Work Share", "😀", "  ", "ALLCAPS", "x".repeat(200).as_str()] {
            let id = derive_id(input, "source", |_| false);
            assert!(validate_name(&id).is_ok(), "invalid id {id:?} from {input:?}");
        }
    }

    #[test]
    fn collisions_get_suffixed() {
        let taken = ["work-share".to_string(), "work-share-2".to_string()];
        let id = derive_id("Work Share", "source", |c| taken.contains(&c.to_string()));
        assert_eq!(id, "work-share-3");
    }

    #[test]
    fn empty_slug_uses_fallback() {
        let id = derive_id("😀", "mount", |_| false);
        assert_eq!(id, "mount");
    }
}
