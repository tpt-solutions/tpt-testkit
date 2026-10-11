//! Compatibility suites: external test vectors (Unicode BiDi, ICC profiles, PDFBox corpus, ...)
//! that the TPT crates must conform to.
//!
//! A suite is described by a JSON manifest checked into the repository:
//!
//! ```json
//! {
//!   "name": "unicode-bidi",
//!   "version": "16.0.0",
//!   "license": "Unicode-3.0",
//!   "files": [
//!     { "path": "BidiTest.txt", "url": "https://www.unicode.org/...", "sha256": "..." }
//!   ]
//! }
//! ```
//!
//! Each file is stored in the shared cache at `<cache>/<name>/<version>/<path>`. Cached copies
//! are re-verified against their SHA-256 on every use, so a corrupted or tampered file is never
//! handed to a test. Set `TPT_SUITE_OFFLINE=1` to forbid network access. In that mode a missing
//! or stale file is an error rather than a download.
//!
//! Use [`tpt_suite_tests!`](crate::tpt_suite_tests) to generate one `#[test]` per manifest entry.
//!
//! Network download is not wired up yet. [`ensure_file`] takes the transport as a closure, and
//! [`no_fetch`] is the placeholder used until a transport is chosen.

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Re-export so `tpt_testkit::suite::tpt_suite_tests!` works alongside the module.
pub use tpt_testkit_macros::tpt_suite_tests;

/// A parsed and validated suite manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Suite name, used as the first cache directory component. `[a-z0-9][a-z0-9_-]*`.
    pub name: String,
    /// Upstream version. Changing it gives a fresh cache directory.
    pub version: String,
    /// SPDX-style license identifier of the suite's data.
    pub license: String,
    /// Files in the suite.
    pub files: Vec<Entry>,
}

/// One file in a suite.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Relative path inside the suite. Must not contain `..` or be absolute.
    pub path: String,
    /// HTTPS location of the file.
    pub url: String,
    /// Lowercase hex SHA-256 of the file contents.
    pub sha256: String,
}

impl Manifest {
    /// Parses and validates a manifest.
    ///
    /// # Errors
    ///
    /// Returns [`SuiteError::InvalidManifest`] when the JSON is malformed or any field fails
    /// validation: bad name or version, unsafe path, non-HTTPS URL, malformed hash, or a
    /// duplicate path.
    pub fn from_json(text: &str) -> Result<Self, SuiteError> {
        let mut manifest: Self =
            serde_json::from_str(text).map_err(|e| SuiteError::InvalidManifest(e.to_string()))?;
        manifest.validate()?;
        for entry in &mut manifest.files {
            entry.sha256.make_ascii_lowercase();
        }
        Ok(manifest)
    }

    /// Finds the entry with the given relative path.
    pub fn entry(&self, path: &str) -> Option<&Entry> {
        self.files.iter().find(|entry| entry.path == path)
    }

    fn validate(&self) -> Result<(), SuiteError> {
        if !is_valid_name(&self.name) {
            return Err(invalid(format!(
                "suite name {:?} is not [a-z0-9][a-z0-9_-]*",
                self.name
            )));
        }
        if self.version.is_empty() || self.version.contains(['/', '\\']) {
            return Err(invalid(format!(
                "version {:?} must be non-empty and contain no separators",
                self.version
            )));
        }
        let mut seen = std::collections::BTreeSet::new();
        for entry in &self.files {
            if !is_safe_relative_path(&entry.path) {
                return Err(invalid(format!(
                    "path {:?} must be relative with no `..`",
                    entry.path
                )));
            }
            if !entry.url.starts_with("https://") {
                return Err(invalid(format!("url {:?} must use https://", entry.url)));
            }
            if entry.sha256.len() != 64 || !entry.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(invalid(format!(
                    "sha256 for {:?} must be 64 hex digits",
                    entry.path
                )));
            }
            if !seen.insert(entry.path.as_str()) {
                return Err(invalid(format!("duplicate path {:?}", entry.path)));
            }
        }
        Ok(())
    }
}

/// Why a suite file could not be made available.
#[derive(Debug)]
pub enum SuiteError {
    /// The manifest JSON is malformed or fails validation.
    InvalidManifest(String),
    /// The requested path is not listed in the manifest.
    UnknownEntry {
        /// The path that was asked for.
        path: String,
    },
    /// File contents do not match the manifest hash.
    HashMismatch {
        /// Relative path of the file in the suite.
        path: String,
        /// Hash the manifest expects.
        expected: String,
        /// Hash of the bytes that were read.
        actual: String,
    },
    /// The file is missing from the cache and `TPT_SUITE_OFFLINE` forbids downloading it.
    Offline {
        /// Relative path of the file in the suite.
        path: String,
    },
    /// The fetcher could not produce the file.
    Fetch {
        /// URL that was requested.
        url: String,
        /// Message from the fetcher.
        message: String,
    },
    /// A filesystem operation failed.
    Io(io::Error),
}

impl fmt::Display for SuiteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidManifest(reason) => write!(f, "invalid suite manifest: {reason}"),
            Self::UnknownEntry { path } => {
                write!(f, "{path:?} is not listed in the suite manifest")
            }
            Self::HashMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "sha256 mismatch for {path:?}: expected {expected}, got {actual}"
            ),
            Self::Offline { path } => write!(
                f,
                "{path:?} is not cached and TPT_SUITE_OFFLINE forbids downloading it"
            ),
            Self::Fetch { url, message } => write!(f, "failed to fetch {url}: {message}"),
            Self::Io(err) => write!(f, "suite cache I/O error: {err}"),
        }
    }
}

impl std::error::Error for SuiteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for SuiteError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// Where cached suite files live and whether downloads are allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Root of the shared cache.
    pub cache_root: PathBuf,
    /// When true, never download. Missing or stale files become [`SuiteError::Offline`].
    pub offline: bool,
}

impl Options {
    /// Builds options from the environment.
    ///
    /// - Cache root: `TPT_TESTKIT_CACHE` if set, otherwise `tpt-testkit` under the platform cache
    ///   directory (`%LOCALAPPDATA%` on Windows, `$XDG_CACHE_HOME` or `~/.cache` elsewhere).
    /// - Offline: `TPT_SUITE_OFFLINE=1`.
    pub fn from_env() -> Self {
        Self {
            cache_root: default_cache_root(),
            offline: env::var("TPT_SUITE_OFFLINE").is_ok_and(|value| value == "1"),
        }
    }
}

/// Placeholder fetcher that always fails. Pass it to [`ensure_file`] until a download transport
/// is chosen. Files must already be cached, or the offline check fails with a clear message.
///
/// # Errors
///
/// Always returns `Err`.
pub fn no_fetch(url: &str) -> Result<Vec<u8>, String> {
    Err(format!(
        "no download transport is configured; place the file in the cache manually ({url})"
    ))
}

/// Returns the cache path for `path` inside `manifest`, downloading it first if needed.
///
/// A cached file is used only if its SHA-256 matches the manifest. A download is verified before
/// it is written, and the write goes through a temporary file so a partial download never looks
/// like a cached one.
///
/// # Errors
///
/// Returns [`SuiteError::UnknownEntry`] for paths not in the manifest, [`SuiteError::Offline`]
/// when a download would be needed but is forbidden, [`SuiteError::Fetch`] when the fetcher
/// fails, [`SuiteError::HashMismatch`] when the bytes do not match the manifest, and
/// [`SuiteError::Io`] for filesystem failures.
pub fn ensure_file<F>(
    manifest: &Manifest,
    path: &str,
    options: &Options,
    fetch: F,
) -> Result<PathBuf, SuiteError>
where
    F: Fn(&str) -> Result<Vec<u8>, String>,
{
    let entry = manifest
        .entry(path)
        .ok_or_else(|| SuiteError::UnknownEntry {
            path: path.to_owned(),
        })?;
    let target = options
        .cache_root
        .join(&manifest.name)
        .join(&manifest.version)
        .join(&entry.path);

    if target.is_file() {
        let cached = fs::read(&target)?;
        match verify_sha256(&entry.path, &cached, &entry.sha256) {
            Ok(()) => return Ok(target),
            Err(err) if options.offline => return Err(err),
            Err(_) => {}
        }
    }

    if options.offline {
        return Err(SuiteError::Offline {
            path: entry.path.clone(),
        });
    }

    let bytes = fetch(&entry.url).map_err(|message| SuiteError::Fetch {
        url: entry.url.clone(),
        message,
    })?;
    verify_sha256(&entry.path, &bytes, &entry.sha256)?;

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let partial = PathBuf::from(format!("{}.part", target.display()));
    fs::write(&partial, &bytes)?;
    fs::rename(&partial, &target)?;
    Ok(target)
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Checks `bytes` against `expected` (lowercase hex SHA-256).
///
/// # Errors
///
/// Returns [`SuiteError::HashMismatch`] naming `path` when the digests differ.
pub fn verify_sha256(path: &str, bytes: &[u8], expected: &str) -> Result<(), SuiteError> {
    let actual = sha256_hex(bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(SuiteError::HashMismatch {
            path: path.to_owned(),
            expected: expected.to_owned(),
            actual,
        })
    }
}

/// Platform cache directory for suite files, honoring `TPT_TESTKIT_CACHE`.
pub fn default_cache_root() -> PathBuf {
    if let Some(dir) = env::var_os("TPT_TESTKIT_CACHE") {
        return PathBuf::from(dir);
    }
    let base = if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
    };
    base.unwrap_or_else(env::temp_dir).join("tpt-testkit")
}

fn invalid(reason: String) -> SuiteError {
    SuiteError::InvalidManifest(reason)
}

fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn is_safe_relative_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: &[u8] = b"hello suite\n";

    fn manifest_json(sha: &str) -> String {
        format!(
            r#"{{"name":"demo","version":"1.0.0","license":"MIT","files":[
                {{"path":"data/a.txt","url":"https://example.invalid/a.txt","sha256":"{sha}"}}
            ]}}"#
        )
    }

    fn fresh_cache(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("tpt-suite-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn options(cache: &Path, offline: bool) -> Options {
        Options {
            cache_root: cache.to_path_buf(),
            offline,
        }
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn parses_and_lowercases_hashes() {
        let manifest = Manifest::from_json(&manifest_json(&"AB".repeat(32))).unwrap();
        assert_eq!(manifest.files[0].sha256, "ab".repeat(32));
        assert!(manifest.entry("data/a.txt").is_some());
    }

    #[test]
    fn rejects_unsafe_paths_urls_and_hashes() {
        let good = "a".repeat(64);
        for bad in [
            manifest_json(&good).replace("data/a.txt", "../escape.txt"),
            manifest_json(&good).replace("data/a.txt", "/abs.txt"),
            manifest_json(&good).replace("https://", "http://"),
            manifest_json("deadbeef"),
            manifest_json(&good).replace("\"name\":\"demo\"", "\"name\":\"Demo!\""),
        ] {
            assert!(
                matches!(
                    Manifest::from_json(&bad),
                    Err(SuiteError::InvalidManifest(_))
                ),
                "accepted: {bad}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_paths() {
        let entry = r#"{"path":"x","url":"https://e.invalid/x","sha256":"AA"}"#;
        let json =
            format!(r#"{{"name":"d","version":"1","license":"MIT","files":[{entry},{entry}]}}"#);
        assert!(Manifest::from_json(&json).is_err());
    }

    #[test]
    fn uses_cached_file_when_hash_matches() {
        let cache = fresh_cache("hit");
        let manifest = Manifest::from_json(&manifest_json(&sha256_hex(DATA))).unwrap();
        let target = cache.join("demo/1.0.0/data/a.txt");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, DATA).unwrap();

        let got = ensure_file(&manifest, "data/a.txt", &options(&cache, true), no_fetch).unwrap();
        assert_eq!(got, target);
        fs::remove_dir_all(&cache).unwrap();
    }

    #[test]
    fn offline_mode_refuses_to_download() {
        let cache = fresh_cache("offline");
        let manifest = Manifest::from_json(&manifest_json(&sha256_hex(DATA))).unwrap();
        let err = ensure_file(&manifest, "data/a.txt", &options(&cache, true), |_| {
            Ok(DATA.to_vec())
        })
        .unwrap_err();
        assert!(matches!(err, SuiteError::Offline { .. }));
    }

    #[test]
    fn downloads_verifies_and_caches() {
        let cache = fresh_cache("download");
        let manifest = Manifest::from_json(&manifest_json(&sha256_hex(DATA))).unwrap();
        let path = ensure_file(&manifest, "data/a.txt", &options(&cache, false), |_| {
            Ok(DATA.to_vec())
        })
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), DATA);
        assert!(!PathBuf::from(format!("{}.part", path.display())).exists());
        fs::remove_dir_all(&cache).unwrap();
    }

    #[test]
    fn rejects_download_with_wrong_hash_and_writes_nothing() {
        let cache = fresh_cache("badhash");
        let manifest = Manifest::from_json(&manifest_json(&sha256_hex(DATA))).unwrap();
        let err = ensure_file(&manifest, "data/a.txt", &options(&cache, false), |_| {
            Ok(b"tampered".to_vec())
        })
        .unwrap_err();
        assert!(matches!(err, SuiteError::HashMismatch { .. }));
        assert!(!cache.join("demo/1.0.0/data/a.txt").exists());
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn replaces_corrupted_cache_entry_when_online() {
        let cache = fresh_cache("corrupt");
        let manifest = Manifest::from_json(&manifest_json(&sha256_hex(DATA))).unwrap();
        let target = cache.join("demo/1.0.0/data/a.txt");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"corrupt").unwrap();

        ensure_file(&manifest, "data/a.txt", &options(&cache, false), |_| {
            Ok(DATA.to_vec())
        })
        .unwrap();
        assert_eq!(fs::read(&target).unwrap(), DATA);
        fs::remove_dir_all(&cache).unwrap();
    }

    #[test]
    fn unknown_path_is_reported() {
        let manifest = Manifest::from_json(&manifest_json(&sha256_hex(DATA))).unwrap();
        let err = ensure_file(
            &manifest,
            "nope",
            &options(Path::new("unused"), true),
            no_fetch,
        )
        .unwrap_err();
        assert!(matches!(err, SuiteError::UnknownEntry { .. }));
    }
}
