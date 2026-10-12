//! tpt-testkit CLI: benchmark summaries and baseline comparison for CI.
//!
//! ```text
//! tpt-testkit-cli bench-summarize --criterion-dir <dir> --out <summary.json> [--csv <file>]
//!                 [--format text|json]
//! tpt-testkit-cli bench-compare --baseline <summary.json> --current <summary.json>
//!                 [--threshold-pct <n>] [--noise-sigmas <n>] [--format text|markdown|json]
//!                 [--strict]
//! tpt-testkit-cli crash-dedup --report <report.json> [--known <signatures.txt>]
//!                 [--format text|json]
//! ```
//!
//! Every command that prints a result supports `--format json`, and errors go to stderr with a
//! non-zero exit code, so CI can parse stdout and branch on the exit code.
//!
//! Exit codes: `0` success, `1` regression (or missing benchmark with `--strict`; or a crash
//! group not in `--known` for `crash-dedup`), `2` usage or I/O error.

mod compare;
mod crash;
mod summary;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use compare::{Thresholds, compare, render_markdown, render_text};
use summary::Summary;

const USAGE: &str = "usage:
  tpt-testkit-cli bench-summarize --criterion-dir <dir> --out <summary.json> [--csv <file>]
                  [--format text|json]
  tpt-testkit-cli bench-compare --baseline <summary.json> --current <summary.json>
                  [--threshold-pct <n>] [--noise-sigmas <n>] [--format text|markdown|json]
                  [--strict]
  tpt-testkit-cli crash-dedup --report <report.json> [--known <signatures.txt>]
                  [--format text|json]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    match args.first().map(String::as_str) {
        Some("bench-summarize") => summarize(&args[1..]),
        Some("bench-compare") => bench_compare(&args[1..]),
        Some("crash-dedup") => crash_dedup(&args[1..]),
        Some("-h" | "--help") | None => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        Some(other) => Err(format!("unknown command {other:?}")),
    }
}

fn summarize(args: &[String]) -> Result<ExitCode, String> {
    let criterion_dir = PathBuf::from(required(args, "--criterion-dir")?);
    let out = PathBuf::from(required(args, "--out")?);
    let entries = summary::collect(&criterion_dir)?;
    let summary = Summary::new(entries);
    write(&out, &summary.to_json())?;
    let csv = option(args, "--csv")?;
    if let Some(csv) = &csv {
        write(Path::new(csv), &summary.to_csv())?;
    }
    match option(args, "--format")?.as_deref().unwrap_or("text") {
        "text" => {
            println!(
                "wrote {} benchmarks to {}",
                summary.benchmarks.len(),
                out.display()
            );
            if let Some(csv) = &csv {
                println!("wrote {csv}");
            }
        }
        "json" => println!(
            "{}",
            serde_json::json!({
                "benchmarks": summary.benchmarks.len(),
                "json": out.display().to_string(),
                "csv": csv,
            })
        ),
        other => return Err(format!("unknown --format {other:?}")),
    }
    Ok(ExitCode::SUCCESS)
}

fn bench_compare(args: &[String]) -> Result<ExitCode, String> {
    let baseline = Summary::load(Path::new(&required(args, "--baseline")?))?;
    let current = Summary::load(Path::new(&required(args, "--current")?))?;
    let defaults = Thresholds::default();
    let thresholds = Thresholds {
        pct: parse_f64(args, "--threshold-pct", defaults.pct)?,
        sigmas: parse_f64(args, "--noise-sigmas", defaults.sigmas)?,
    };
    let strict = args.iter().any(|a| a == "--strict");
    let report = compare(&baseline, &current, thresholds);
    let rendered = match option(args, "--format")?.as_deref().unwrap_or("text") {
        "text" => render_text(&report),
        "markdown" => render_markdown(&report),
        "json" => {
            let rows: Vec<_> = report
                .rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "name": r.name,
                        "baseline_ns": r.baseline_ns,
                        "current_ns": r.current_ns,
                        "change_pct": r.change_pct,
                        "status": r.status.label(),
                    })
                })
                .collect();
            let mut s = serde_json::to_string_pretty(&serde_json::json!({
                "regressions": report.regressions(),
                "missing": report.missing(),
                "rows": rows,
            }))
            .map_err(|e| e.to_string())?;
            s.push('\n');
            s
        }
        other => return Err(format!("unknown --format {other:?}")),
    };
    print!("{rendered}");
    if report.failed(strict) {
        eprintln!(
            "bench-compare: {} regression(s), {} missing benchmark(s)",
            report.regressions(),
            report.missing()
        );
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

fn crash_dedup(args: &[String]) -> Result<ExitCode, String> {
    let report = PathBuf::from(required(args, "--report")?);
    let known = option(args, "--known")?.map(PathBuf::from);
    let outcome = crash::dedup(&report, known.as_deref())?;
    let rendered = match option(args, "--format")?.as_deref().unwrap_or("text") {
        "text" => crash::render_text(&outcome),
        "json" => {
            let mut s = serde_json::to_string_pretty(&outcome).map_err(|e| e.to_string())?;
            s.push('\n');
            s
        }
        other => return Err(format!("unknown --format {other:?}")),
    };
    print!("{rendered}");
    if outcome.new_groups > 0 {
        eprintln!("crash-dedup: {} new crash group(s)", outcome.new_groups);
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

fn required(args: &[String], flag: &str) -> Result<String, String> {
    option(args, flag)?.ok_or_else(|| format!("missing required {flag}"))
}

fn option(args: &[String], flag: &str) -> Result<Option<String>, String> {
    match args.iter().position(|a| a == flag) {
        None => Ok(None),
        Some(i) => args
            .get(i + 1)
            .cloned()
            .map(Some)
            .ok_or_else(|| format!("{flag} needs a value")),
    }
}

fn parse_f64(args: &[String], flag: &str, default: f64) -> Result<f64, String> {
    match option(args, flag)? {
        None => Ok(default),
        Some(v) => v
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n >= 0.0)
            .ok_or_else(|| format!("{flag} must be a non-negative number, got {v:?}")),
    }
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|e| format!("cannot write {}: {e}", path.display()))
}
