//! Baseline comparison with noise-aware regression detection.
//!
//! A benchmark regresses only when both hold:
//!
//! 1. The mean grew by more than `pct` percent (default 5%).
//! 2. The mean grew by more than `sigmas` times the combined standard deviation of the baseline
//!    and current runs (default 2). This keeps a noisy benchmark from failing a PR on a single
//!    unlucky run.
//!
//! Benchmarks only in the current run are `New`. Benchmarks only in the baseline are `Missing`:
//! reported, and failing only under `strict`.

use std::fmt::Write as _;

use crate::summary::{Entry, Summary};

/// Regression thresholds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Percent slowdown that counts as a regression.
    pub pct: f64,
    /// Multiple of the combined standard deviation the slowdown must also exceed.
    pub sigmas: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            pct: 5.0,
            sigmas: 2.0,
        }
    }
}

/// Outcome for one benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Within the thresholds.
    Unchanged,
    /// Slower beyond both thresholds.
    Regressed,
    /// Faster beyond both thresholds.
    Improved,
    /// Present in the current run only.
    New,
    /// Present in the baseline only.
    Missing,
}

impl Status {
    /// Short label used in reports.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Unchanged => "ok",
            Self::Regressed => "REGRESSED",
            Self::Improved => "improved",
            Self::New => "new",
            Self::Missing => "missing",
        }
    }
}

/// One benchmark in the comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Benchmark name.
    pub name: String,
    /// Baseline mean, if the benchmark was in the baseline.
    pub baseline_ns: Option<f64>,
    /// Current mean, if the benchmark ran.
    pub current_ns: Option<f64>,
    /// Percent change from baseline to current, when both exist and the baseline is positive.
    pub change_pct: Option<f64>,
    /// Outcome.
    pub status: Status,
}

/// Full comparison result.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Rows sorted by name.
    pub rows: Vec<Row>,
}

impl Report {
    /// Number of regressed benchmarks.
    #[must_use]
    pub fn regressions(&self) -> usize {
        self.count(Status::Regressed)
    }

    /// Number of benchmarks missing from the current run.
    #[must_use]
    pub fn missing(&self) -> usize {
        self.count(Status::Missing)
    }

    /// Whether the run should fail: any regression, or any missing benchmark when `strict`.
    #[must_use]
    pub fn failed(&self, strict: bool) -> bool {
        self.regressions() > 0 || (strict && self.missing() > 0)
    }

    fn count(&self, status: Status) -> usize {
        self.rows.iter().filter(|r| r.status == status).count()
    }
}

/// Compares `current` against `baseline`.
#[must_use]
pub fn compare(baseline: &Summary, current: &Summary, thresholds: Thresholds) -> Report {
    let mut rows = Vec::new();
    for base in &baseline.benchmarks {
        match current.benchmarks.iter().find(|c| c.name == base.name) {
            Some(cur) => rows.push(compare_pair(base, cur, thresholds)),
            None => rows.push(Row {
                name: base.name.clone(),
                baseline_ns: Some(base.mean_ns),
                current_ns: None,
                change_pct: None,
                status: Status::Missing,
            }),
        }
    }
    for cur in &current.benchmarks {
        if !baseline.benchmarks.iter().any(|b| b.name == cur.name) {
            rows.push(Row {
                name: cur.name.clone(),
                baseline_ns: None,
                current_ns: Some(cur.mean_ns),
                change_pct: None,
                status: Status::New,
            });
        }
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    Report { rows }
}

fn compare_pair(base: &Entry, cur: &Entry, thresholds: Thresholds) -> Row {
    let delta = cur.mean_ns - base.mean_ns;
    let change_pct = (base.mean_ns > 0.0).then(|| delta / base.mean_ns * 100.0);
    let noise = base.std_dev_ns.hypot(cur.std_dev_ns) * thresholds.sigmas;
    let significant = |d: f64| d.abs() > noise;
    let status = match change_pct {
        Some(pct) if pct > thresholds.pct && significant(delta) => Status::Regressed,
        Some(pct) if pct < -thresholds.pct && significant(delta) => Status::Improved,
        _ => Status::Unchanged,
    };
    Row {
        name: base.name.clone(),
        baseline_ns: Some(base.mean_ns),
        current_ns: Some(cur.mean_ns),
        change_pct,
        status,
    }
}

/// Fixed-width text table for terminals and CI logs.
#[must_use]
pub fn render_text(report: &Report) -> String {
    let width = report
        .rows
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(0)
        .max("benchmark".len());
    let mut out = format!(
        "{:<width$}  {:>14}  {:>14}  {:>9}  status\n",
        "benchmark", "baseline (ns)", "current (ns)", "change"
    );
    for row in &report.rows {
        let _ = writeln!(
            out,
            "{:<width$}  {:>14}  {:>14}  {:>9}  {}",
            row.name,
            fmt_ns(row.baseline_ns),
            fmt_ns(row.current_ns),
            fmt_pct(row.change_pct),
            row.status.label(),
        );
    }
    out
}

/// Markdown table for a PR comment.
#[must_use]
pub fn render_markdown(report: &Report) -> String {
    let mut out = String::from(
        "| Benchmark | Baseline (ns) | Current (ns) | Change | Status |\n|---|---:|---:|---:|---|\n",
    );
    for row in &report.rows {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} |",
            row.name,
            fmt_ns(row.baseline_ns),
            fmt_ns(row.current_ns),
            fmt_pct(row.change_pct),
            row.status.label(),
        );
    }
    out
}

fn fmt_ns(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| format!("{v:.1}"))
}

fn fmt_pct(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| format!("{v:+.2}%"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, mean: f64, sd: f64) -> Entry {
        Entry {
            name: name.into(),
            mean_ns: mean,
            std_dev_ns: sd,
        }
    }

    fn summary(entries: Vec<Entry>) -> Summary {
        Summary::new(entries)
    }

    fn status_of(report: &Report, name: &str) -> Status {
        report.rows.iter().find(|r| r.name == name).unwrap().status
    }

    #[test]
    fn large_quiet_slowdown_is_a_regression() {
        let base = summary(vec![entry("a/b/c", 100.0, 1.0)]);
        let cur = summary(vec![entry("a/b/c", 110.0, 1.0)]);
        let report = compare(&base, &cur, Thresholds::default());
        assert_eq!(status_of(&report, "a/b/c"), Status::Regressed);
        assert_eq!(report.regressions(), 1);
        assert!(report.failed(false));
    }

    #[test]
    fn slowdown_below_percentage_threshold_passes() {
        let base = summary(vec![entry("a/b/c", 100.0, 0.0)]);
        let cur = summary(vec![entry("a/b/c", 104.0, 0.0)]);
        let report = compare(&base, &cur, Thresholds::default());
        assert_eq!(status_of(&report, "a/b/c"), Status::Unchanged);
        assert!(!report.failed(false));
    }

    #[test]
    fn slowdown_within_noise_passes() {
        // 10% slower, but the combined noise is 2 * sqrt(20^2 + 20^2) ~= 56, so 10 is noise.
        let base = summary(vec![entry("a/b/c", 100.0, 20.0)]);
        let cur = summary(vec![entry("a/b/c", 110.0, 20.0)]);
        let report = compare(&base, &cur, Thresholds::default());
        assert_eq!(status_of(&report, "a/b/c"), Status::Unchanged);
    }

    #[test]
    fn speedup_is_reported_but_passes() {
        let base = summary(vec![entry("a/b/c", 100.0, 1.0)]);
        let cur = summary(vec![entry("a/b/c", 50.0, 1.0)]);
        let report = compare(&base, &cur, Thresholds::default());
        assert_eq!(status_of(&report, "a/b/c"), Status::Improved);
        assert!(!report.failed(true));
    }

    #[test]
    fn new_and_missing_benchmarks() {
        let base = summary(vec![entry("a/b/gone", 1.0, 0.0)]);
        let cur = summary(vec![entry("a/b/fresh", 1.0, 0.0)]);
        let report = compare(&base, &cur, Thresholds::default());
        assert_eq!(status_of(&report, "a/b/gone"), Status::Missing);
        assert_eq!(status_of(&report, "a/b/fresh"), Status::New);
        assert!(!report.failed(false), "missing is only fatal when strict");
        assert!(report.failed(true));
    }

    #[test]
    fn custom_thresholds_apply() {
        let base = summary(vec![entry("a/b/c", 100.0, 0.0)]);
        let cur = summary(vec![entry("a/b/c", 104.0, 0.0)]);
        let strict = Thresholds {
            pct: 3.0,
            sigmas: 2.0,
        };
        assert_eq!(
            status_of(&compare(&base, &cur, strict), "a/b/c"),
            Status::Regressed
        );
    }

    #[test]
    fn fixture_files_produce_expected_report() {
        let base = Summary::from_json(include_str!("../tests/fixtures/baseline.json")).unwrap();
        let cur = Summary::from_json(include_str!("../tests/fixtures/current.json")).unwrap();
        let report = compare(&base, &cur, Thresholds::default());
        let expected = [
            ("golden/text/large", Status::Unchanged),
            ("golden/text/small", Status::Regressed),
            ("parse/json/large", Status::Improved),
            ("parse/json/small", Status::Unchanged),
            ("parse/csv/small", Status::Missing),
            ("parse/pdf/medium", Status::New),
        ];
        for (name, status) in expected {
            assert_eq!(status_of(&report, name), status, "{name}");
        }
        assert_eq!(report.regressions(), 1);
        assert_eq!(report.missing(), 1);
    }

    #[test]
    fn renders_text_and_markdown() {
        let base = summary(vec![entry("a/b/c", 100.0, 0.0)]);
        let cur = summary(vec![entry("a/b/c", 120.0, 0.0)]);
        let report = compare(&base, &cur, Thresholds::default());
        assert!(render_text(&report).contains("REGRESSED"));
        let md = render_markdown(&report);
        assert!(
            md.contains("| `a/b/c` | 100.0 | 120.0 | +20.00% | REGRESSED |"),
            "{md}"
        );
    }
}
