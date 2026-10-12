//! JSON crash reports: the hand-off from the in-process engine to CI tooling.
//!
//! `tpt-testkit-cli crash-dedup` reads these reports. It cannot depend on this crate, because
//! tpt-testkit must never appear in a normal dependency graph, so the report format is the
//! contract between them.
//!
//! Schema version 1:
//!
//! ```json
//! { "schema": 1, "crashes": [ { "message": "index out of range", "input_hex": "00ff" } ] }
//! ```

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::Failure;

/// Current report schema version.
pub const REPORT_SCHEMA: u32 = 1;

/// A crash report file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashReport {
    /// Schema version the file was written with.
    pub schema: u32,
    /// One record per distinct crashing input.
    pub crashes: Vec<CrashRecord>,
}

/// One crashing input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashRecord {
    /// Panic message.
    pub message: String,
    /// Lowercase hex of the input bytes.
    pub input_hex: String,
}

/// Writes `failures` as a JSON report at `path`, creating parent directories as needed.
///
/// # Errors
///
/// Returns any I/O error from creating the directory or writing the file.
pub fn write_report(failures: &[Failure], path: impl AsRef<Path>) -> io::Result<()> {
    let report = CrashReport {
        schema: REPORT_SCHEMA,
        crashes: failures
            .iter()
            .map(|failure| CrashRecord {
                message: failure.message.clone(),
                input_hex: to_hex(&failure.input),
            })
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&report).map_err(io::Error::other)?;
    text.push('\n');

    let path = path.as_ref();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)
}

/// Reads a report written by [`write_report`].
///
/// # Errors
///
/// Returns a message if the file cannot be read, is not a version-1 report, or contains a
/// malformed hex string.
pub fn read_report(path: impl AsRef<Path>) -> Result<Vec<Failure>, String> {
    let path = path.as_ref();
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let report: CrashReport =
        serde_json::from_str(&text).map_err(|e| format!("invalid crash report: {e}"))?;
    if report.schema != REPORT_SCHEMA {
        return Err(format!(
            "unsupported crash report schema {} (expected {REPORT_SCHEMA})",
            report.schema
        ));
    }
    report
        .crashes
        .into_iter()
        .map(|record| {
            let input = from_hex(&record.input_hex)
                .ok_or_else(|| format!("malformed input_hex for {:?}", record.message))?;
            Ok(Failure {
                input,
                message: record.message,
            })
        })
        .collect()
}

/// Lowercase hex encoding of `bytes`.
pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

/// Decodes hex produced by [`to_hex`]. Accepts either case. Returns `None` on odd length or a
/// non-hex character.
pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(16)?;
            let low = char::from(pair[1]).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let bytes = [0x00, 0x7f, 0x80, 0xff];
        assert_eq!(to_hex(&bytes), "007f80ff");
        assert_eq!(from_hex("007F80ff").unwrap(), bytes);
        assert_eq!(from_hex(""), Some(Vec::new()));
    }

    #[test]
    fn hex_rejects_bad_input() {
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("zz"), None);
    }

    #[test]
    fn report_round_trips_through_disk() {
        let path = std::env::temp_dir().join(format!("tpt-report-{}.json", std::process::id()));
        let failures = vec![Failure {
            input: b"\x00boom".to_vec(),
            message: "index 3 out of range".to_owned(),
        }];
        write_report(&failures, &path).unwrap();
        assert_eq!(read_report(&path).unwrap(), failures);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn report_rejects_other_schema_versions() {
        let path = std::env::temp_dir().join(format!("tpt-report-v9-{}.json", std::process::id()));
        fs::write(&path, r#"{"schema": 9, "crashes": []}"#).unwrap();
        assert!(read_report(&path).unwrap_err().contains("schema 9"));
        fs::remove_file(&path).unwrap();
    }
}
