// SPDX-License-Identifier: GPL-2.0-or-later

//! Static table of rclone backend capabilities relevant to FUSE mounting.
//!
//! Hand-derived from rclone's Go source, not probed live: `rclone backend
//! features <remote>:` only returns after `NewFs` fully constructs the
//! backend, and several backends (sftp, ftp, drive's OAuth handshake, ...)
//! dial the real server to do that — probing would mean shelling out with
//! the user's live decrypted secret and eating multi-second network
//! timeouts just to render a form. `PutStream`/`DuplicateFiles` are
//! compiled-in `fs.Features{}` values set once at Fs construction; for every
//! backend below except WebDAV they don't depend on the specific remote
//! instance, so a table keyed by backend type is both cheap and accurate.
//!
//! WebDAV is the one exception: it sets `PutStream` based on the `vendor`
//! config value (see [`webdav_put_stream`]), so it's handled as a function
//! instead of a table entry.
//!
//! Table current as of rclone v1.74.1 (checked against a clone of
//! <https://github.com/rclone/rclone> at that tag). Excludes rclone's
//! wrapper/meta backends (alias, archive, cache, chunker, combine, compress,
//! crypt, hasher, union) — their features pass through whatever remote
//! they wrap, so a fixed entry would be meaningless — and the local-disk
//! and in-memory test backends, which aren't offered as mount sources here.
//!
//! To re-derive an entry for backend `X` from a fresh rclone checkout:
//! - `put_stream`: does `backend/X/X.go` define
//!   `func (f *Fs) PutStream(ctx context.Context, in io.Reader, src fs.ObjectInfo, options ...fs.OpenOption) (fs.Object, error)`?
//!   (grep `^func \(f \*Fs\) PutStream\(`; ignore commented-out/renamed ones,
//!   e.g. sharefile's `FIXMEPutStream` and pikpak's commented assertion —
//!   both mean "no".)
//! - `duplicate_files`: does its `(&fs.Features{...})` literal in `NewFs`
//!   set `DuplicateFiles: true`? (grep `DuplicateFiles` in that file;
//!   absence means the Go zero value, `false`.)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendFeatures {
    /// Can upload a file without knowing its size/content up front (i.e.
    /// without buffering it to disk first). When this is `false` and the
    /// mount is writable with a cache mode below `Writes`, rclone logs its
    /// own "vfs-cache-mode writes or full is recommended... it can't
    /// stream" warning at mount time — this is what the KCM's cache-mode
    /// warning mirrors.
    pub put_stream: bool,
    /// Whether the backend allows two files with the same name in the same
    /// directory. A FUSE mount can only show one entry per name, so sources
    /// with this set can have files invisibly shadowed in the mount.
    pub duplicate_files: bool,
}

const fn f(put_stream: bool, duplicate_files: bool) -> BackendFeatures {
    BackendFeatures {
        put_stream,
        duplicate_files,
    }
}

/// (backend type string, features), matching rclone.conf's `type =` value.
const BACKEND_FEATURES: &[(&str, BackendFeatures)] = &[
    ("azureblob", f(true, false)),
    ("azurefiles", f(true, false)),
    ("b2", f(true, false)),
    ("box", f(true, false)),
    ("cloudinary", f(false, false)),
    ("doi", f(true, false)),
    ("drime", f(true, false)),
    ("drive", f(true, true)),
    ("dropbox", f(true, false)),
    ("fichier", f(false, true)),
    ("filefabric", f(false, false)),
    ("filelu", f(false, false)),
    ("filen", f(true, false)),
    ("filescom", f(true, false)),
    ("ftp", f(true, false)),
    ("gofile", f(true, true)),
    ("google cloud storage", f(true, false)),
    ("google photos", f(false, false)),
    ("hdfs", f(true, false)),
    ("hidrive", f(true, false)),
    ("http", f(true, false)),
    ("huaweidrive", f(false, false)),
    ("iclouddrive", f(false, false)),
    ("imagekit", f(false, false)),
    ("internetarchive", f(false, false)),
    ("internxt", f(false, false)),
    ("jottacloud", f(false, false)),
    ("koofr", f(true, false)),
    ("linkbox", f(false, false)),
    ("mailru", f(false, false)),
    ("mega", f(false, true)),
    ("netstorage", f(true, false)),
    ("onedrive", f(false, false)),
    ("opendrive", f(false, false)),
    ("oracleobjectstorage", f(true, false)),
    ("pcloud", f(false, false)),
    ("pikpak", f(false, false)),
    ("pixeldrain", f(true, false)),
    ("premiumizeme", f(false, false)),
    ("protondrive", f(false, false)),
    ("putio", f(false, true)),
    ("qingstor", f(false, false)),
    ("quatrix", f(false, false)),
    ("s3", f(true, false)),
    ("seafile", f(true, false)),
    ("sftp", f(true, false)),
    ("shade", f(false, false)),
    ("sharefile", f(false, false)),
    ("sia", f(true, false)),
    ("smb", f(true, false)),
    ("storj", f(true, false)),
    ("sugarsync", f(true, false)),
    ("swift", f(true, false)),
    ("ulozto", f(false, true)),
    ("yandex", f(true, false)),
    ("zoho", f(false, false)),
];

/// Look up a non-WebDAV backend's static features by its rclone.conf `type`
/// string. `None` for WebDAV (use [`webdav_put_stream`] instead) or any
/// backend not yet in the table above.
pub fn lookup(kind: &str) -> Option<BackendFeatures> {
    BACKEND_FEATURES
        .iter()
        .find(|(tag, _)| *tag == kind)
        .map(|(_, features)| *features)
}

/// WebDAV only sets `PutStream` when talking to another rclone instance
/// (`vendor = rclone`, i.e. `rclone serve webdav`) — every real-world WebDAV
/// server (Nextcloud, ownCloud, Sharepoint, generic/"other", ...) gets it
/// unconditionally disabled in `Fs.fillConfig` (see
/// `backend/webdav/webdav.go`'s `canStream` field). `DuplicateFiles` is
/// unset (`false`) for every vendor.
pub fn webdav_put_stream(vendor: &str) -> bool {
    vendor == "rclone"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_up_known_backend() {
        assert_eq!(lookup("drive"), Some(f(true, true)));
        assert_eq!(lookup("iclouddrive"), Some(f(false, false)));
    }

    #[test]
    fn unknown_backend_is_none() {
        assert_eq!(lookup("webdav"), None);
        assert_eq!(lookup("not-a-real-backend"), None);
    }

    #[test]
    fn webdav_only_streams_for_the_rclone_vendor() {
        assert!(webdav_put_stream("rclone"));
        assert!(!webdav_put_stream("nextcloud"));
        assert!(!webdav_put_stream("other"));
        assert!(!webdav_put_stream(""));
    }

    #[test]
    fn table_has_no_duplicate_entries() {
        let mut tags: Vec<&str> = BACKEND_FEATURES.iter().map(|(tag, _)| *tag).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), BACKEND_FEATURES.len());
    }
}
