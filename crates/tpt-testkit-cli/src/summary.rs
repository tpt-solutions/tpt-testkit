//! Benchmark summary: reads criterion output and writes the machine-readable JSON/CSV files
//! that `bench-compare` consumes.
//!
//! Schema version 1 (`summary.json`):
//!
//! ```json
//! { "schema": 1, "benchmarks": [ { "name": "golden/text/small", "mean_ns": 895.8, "std_dev_ns": 27.1 } ] }
//! ```
//!
//! Entries are sorted by name so the file diffs cleanly.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Current `summary.json` schema version. Bump it when the layout changes.
pub const SCHEMA_VERSION: u32 = 1;

/// One benchmark's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Full benchmark id, for example `parse/json/small`.
    pub name: String,
    /// Mean time per iteration, in nanoseconds.
    pub mean_ns: f64,
    /// Standard deviation of the per-iteration time, in nanoseconds.
    pub std_dev_ns: f64,
}

/// A set of benchmark results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    /// Schema version the file was written with.
    pub schema: u32,
    /// Results, sorted by name.
    pub benchmarks: Vec<Entry>,
}

impl Summary {
    /// Builds a summary, sorting entries by name.
    #[must_use]
    pub fn new(mut benchmarks: Vec<Entry>) -> Self {
        benchmarks.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            schema: SCHEMA_VERSION,
            benchmarks,
        }
    }

    /// Pretty JSON with a trailing newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).expect("summary always serializes");
        s.push('\n');
        s
    }

    /// CSV with the header `name,mean_ns,std_dev_ns`.
    #[must_use]
    pub fn to_csv(&self) -> String {
        let mut s = String::from("name,mean_ns,std_dev_ns\n");
        for e in &self.benchmarks {
            s.push_str(&format!("{},{},{}\n", e.name, e.mean_ns, e.std_dev_ns));
        }
        s
    }

    /// Parses a `summary.json` document, rejecting unknown schema versions.
    ///
    /// # Errors
    ///
    /// Returns a message when the text is not a valid summary or the schema differs.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let summary: Self =
            serde_json::from_str(text).map_err(|e| format!("invalid summary JSON: {e}"))?;
        if summary.schema != SCHEMA_VERSION {
            return Err(format!(
                "unsupported summary schema {} (expected {SCHEMA_VERSION})",
                summary.schema
            ));
        }
        Ok(summary)
    }

    /// Reads and parses a summary file.
    ///
    /// # Errors
    ///
    /// Returns a message when the file cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text =
            fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Self::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Collects the latest run (`new/`) of every benchmark under a criterion output directory.
///
/// # Errors
///
/// Returns a message when the directory cannot be read, a result is incomplete, two results
/// share a name, or no results exist at all.
pub fn collect(criterion_dir: &Path) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    walk(criterion_dir, &mut entries)?;
    if entries.is_empty() {
        return Err(format!(
            "no criterion results found under {} (run `cargo bench --features bench` first)",
            criterion_dir.display()
        ));
    }
    let mut names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    if let Some(dup) = names.windows(2).find(|w| w[0] == w[1]) {
        return Err(format!("benchmark {} was found more than once", dup[0]));
    }
    Ok(entries)
}

fn walk(dir: &Path, out: &mut Vec<Entry>) -> Result<(), String> {
    let read = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for item in read {
        let path = item.map_err(|e| e.to_string())?.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("new")
            && path.join("benchmark.json").is_file()
        {
            out.push(read_result(&path)?);
        } else {
            walk(&path, out)?;
        }
    }
    Ok(())
}

fn read_result(new_dir: &Path) -> Result<Entry, String> {
    let meta = read_json(&new_dir.join("benchmark.json"))?;
    let estimates = read_json(&new_dir.join("estimates.json"))?;
    let name = meta
        .get("full_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("{} has no full_id", new_dir.display()))?;
    let point = |key: &str| {
        estimates
            .get(key)
            .and_then(|v| v.get("point_estimate"))
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| format!("{name}: estimates.json is missing {key}.point_estimate"))
    };
    Ok(Entry {
        name: name.to_owned(),
        mean_ns: point("mean")?,
        std_dev_ns: point("std_dev")?,
    })
}

fn read_json(path: &Path) -> Result<serde_json::Value, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("invalid JSON in {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trips_and_sorts() {
        let summary = Summary::new(vec![
            Entry {
                name: "b/x/1".into(),
                mean_ns: 2.0,
                std_dev_ns: 0.5,
            },
            Entry {
                name: "a/x/1".into(),
                mean_ns: 1.0,
                std_dev_ns: 0.25,
            },
        ]);
        assert_eq!(summary.benchmarks[0].name, "a/x/1");
        let parsed = Summary::from_json(&summary.to_json()).unwrap();
        assert_eq!(parsed, summary);
    }

    #[test]
    fn csv_has_header_and_rows() {
        let summary = Summary::new(vec![Entry {
            name: "a/b/c".into(),
            mean_ns: 10.0,
            std_dev_ns: 1.5,
        }]);
        assert_eq!(summary.to_csv(), "name,mean_ns,std_dev_ns\na/b/c,10,1.5\n");
    }

    #[test]
    fn rejects_other_schema_versions() {
        let err = Summary::from_json(r#"{"schema": 99, "benchmarks": []}"#).unwrap_err();
        assert!(err.contains("unsupported summary schema 99"), "{err}");
    }
}
