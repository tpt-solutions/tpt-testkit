//! `crash-dedup`: groups crashes from a fuzz report by normalized panic message, and flags the
//! groups that are not in a list of known signatures.
//!
//! The report format is written by `tpt_testkit::fuzz::write_report` (schema 1). The signature
//! algorithm must match `tpt_testkit::fuzz::signature`. The shared test vector in this file and
//! in `crates/tpt-testkit/src/fuzz/minimize.rs` keeps the two in step.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Supported report schema version.
const REPORT_SCHEMA: u32 = 1;

#[derive(Deserialize)]
struct Report {
    schema: u32,
    crashes: Vec<Record>,
}

#[derive(Deserialize)]
struct Record {
    message: String,
    input_hex: String,
}

/// One group in the output.
#[derive(Debug, Serialize)]
pub struct Group {
    /// Signature as 16 lowercase hex digits.
    pub signature: String,
    /// Number of crashes in this group.
    pub count: usize,
    /// Message of the first crash in the group.
    pub message: String,
    /// Shortest crashing input, as hex.
    pub smallest_hex: String,
    /// True when the signature appears in the known list.
    pub known: bool,
}

/// The full result of a dedup run.
#[derive(Debug, Serialize)]
pub struct Outcome {
    /// Output schema version.
    pub schema: u32,
    /// Number of groups whose signature is not known.
    pub new_groups: usize,
    /// All groups, sorted by signature.
    pub groups: Vec<Group>,
}

/// Groups the crashes in `report_path`, marking those whose signatures are in `known`.
///
/// `known` lines are hex signatures. Blank lines and lines starting with `#` are ignored.
///
/// # Errors
///
/// Returns a message for unreadable files, unsupported schemas, malformed hex, or malformed
/// signatures in the known list.
pub fn dedup(report_path: &Path, known_path: Option<&Path>) -> Result<Outcome, String> {
    let text = fs::read_to_string(report_path)
        .map_err(|e| format!("cannot read {}: {e}", report_path.display()))?;
    let report: Report = serde_json::from_str(&text).map_err(|e| format!("invalid report: {e}"))?;
    if report.schema != REPORT_SCHEMA {
        return Err(format!(
            "unsupported report schema {} (expected {REPORT_SCHEMA})",
            report.schema
        ));
    }

    let known = match known_path {
        Some(path) => read_known(path)?,
        None => BTreeSet::new(),
    };

    let mut groups: BTreeMap<u64, (usize, String, Vec<u8>)> = BTreeMap::new();
    for record in &report.crashes {
        let input = from_hex(&record.input_hex)
            .ok_or_else(|| format!("malformed input_hex for {:?}", record.message))?;
        let sig = signature(&record.message);
        let entry = groups
            .entry(sig)
            .or_insert_with(|| (0, record.message.clone(), input.clone()));
        entry.0 += 1;
        if (input.len(), &input) < (entry.2.len(), &entry.2) {
            entry.2 = input;
        }
    }

    let groups: Vec<Group> = groups
        .into_iter()
        .map(|(sig, (count, message, smallest))| Group {
            signature: format!("{sig:016x}"),
            count,
            message,
            smallest_hex: to_hex(&smallest),
            known: known.contains(&sig),
        })
        .collect();
    let new_groups = groups.iter().filter(|g| !g.known).count();
    Ok(Outcome {
        schema: REPORT_SCHEMA,
        new_groups,
        groups,
    })
}

/// Renders the outcome as plain text, one line per group.
pub fn render_text(outcome: &Outcome) -> String {
    let mut out = format!(
        "{} crash group(s), {} new\n",
        outcome.groups.len(),
        outcome.new_groups
    );
    for group in &outcome.groups {
        let status = if group.known { "known" } else { "NEW  " };
        out.push_str(&format!(
            "[{status}] {} x{}  {}  smallest={}\n",
            group.signature, group.count, group.message, group.smallest_hex
        ));
    }
    out
}

/// Stable signature for a panic message: FNV-1a over the message with digit runs (including
/// `0x`-prefixed hex) collapsed to `#`.
pub fn signature(message: &str) -> u64 {
    fnv1a(normalize(message).as_bytes())
}

fn normalize(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut chars = message.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            while matches!(chars.peek(), Some(n) if n.is_ascii_hexdigit() || *n == 'x' || *n == 'X')
            {
                chars.next();
            }
            out.push('#');
        } else {
            out.push(c);
        }
    }
    out
}

fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

fn read_known(path: &Path) -> Result<BTreeSet<u64>, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            u64::from_str_radix(line, 16)
                .map_err(|_| format!("known signature {line:?} is not hex"))
        })
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Must match `tpt_testkit::fuzz::signature` for the same message.
    #[test]
    fn signature_matches_library_test_vector() {
        assert_eq!(
            signature("index 12 out of range 0x7ffa1000"),
            signature("index 999 out of range 0x7ffb2222")
        );
        assert_eq!(signature("index 1 out of range"), SHARED_VECTOR);
    }

    const SHARED_VECTOR: u64 = 0xd753b54df3464306;

    #[test]
    fn groups_count_and_flag_known() {
        let dir = std::env::temp_dir().join(format!("tpt-crash-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let report = dir.join("report.json");
        fs::write(
            &report,
            r#"{"schema":1,"crashes":[
                {"message":"index 3 bad","input_hex":"6c6f6e67"},
                {"message":"index 9 bad","input_hex":"6162"},
                {"message":"other","input_hex":"7a7a"}]}"#,
        )
        .unwrap();
        let known = dir.join("known.txt");
        fs::write(&known, format!("# triaged\n{:016x}\n", signature("other"))).unwrap();

        let outcome = dedup(&report, Some(&known)).unwrap();
        assert_eq!(outcome.groups.len(), 2);
        assert_eq!(outcome.new_groups, 1);
        let bad = outcome
            .groups
            .iter()
            .find(|g| g.message == "index 3 bad")
            .unwrap();
        assert_eq!(bad.count, 2);
        assert_eq!(bad.smallest_hex, "6162");
        assert!(!bad.known);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_bad_known_signature() {
        let dir = std::env::temp_dir().join(format!("tpt-crash-bad-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let known = dir.join("known.txt");
        fs::write(&known, "not-hex\n").unwrap();
        assert!(read_known(&known).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hex_helpers_round_trip() {
        assert_eq!(from_hex(&to_hex(b"\x00\xff")).unwrap(), b"\x00\xff");
        assert_eq!(from_hex("abc"), None);
    }
}
