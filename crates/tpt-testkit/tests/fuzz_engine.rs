//! Scheduled in-process fuzzing of the committed fixtures.
//!
//! Environment knobs (all optional):
//! - `TPT_FUZZ_ITERATIONS`: mutate-and-execute steps (default 2000).
//! - `TPT_FUZZ_SEED`: RNG seed (default 1).
//! - `TPT_FUZZ_CORPUS`: directory to load extra seeds from and save the final corpus to.
//! - `TPT_FUZZ_CRASHES`: directory to write crashing inputs to.
//! - `TPT_FUZZ_REPORT`: path of a JSON crash report (read by `tpt-testkit-cli crash-dedup`).

#![cfg(feature = "fuzz")]

use std::env;
use std::path::PathBuf;

use tpt_testkit::fuzz::{self, Engine};

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Parses the input and reports where a JSON error occurred, so the engine prefers inputs that
/// reach new error positions. Valid JSON reports a single shared feature.
fn target(data: &[u8]) -> Vec<u64> {
    let _ = std::str::from_utf8(data);
    match serde_json::from_slice::<serde_json::Value>(data) {
        Ok(_) => vec![u64::MAX],
        Err(err) => vec![(u64::from(err.line() as u32) << 32) | u64::from(err.column() as u32)],
    }
}

#[test]
fn fixtures_survive_scheduled_fuzzing() {
    let corpus_dir = env::var_os("TPT_FUZZ_CORPUS").map(PathBuf::from);
    let mut seeds = fuzz::read_corpus("tests/fixtures").expect("fixtures are readable");
    if let Some(dir) = corpus_dir.as_ref().filter(|dir| dir.is_dir()) {
        seeds.extend(fuzz::read_corpus(dir).expect("corpus directory is readable"));
    }

    let dictionary: Vec<&[u8]> = fuzz::JSON_TOKENS
        .iter()
        .chain(fuzz::PDF_KEYWORDS)
        .chain(fuzz::CSV_TOKENS)
        .copied()
        .collect();
    let mut engine = Engine::new(env_u64("TPT_FUZZ_SEED", 1), seeds).with_dictionary(&dictionary);
    engine.run(env_u64("TPT_FUZZ_ITERATIONS", 2000), target);

    eprintln!(
        "fuzz: {} executions, {} corpus entries, {} features, {} crashes",
        engine.executions(),
        engine.corpus().len(),
        engine.features_seen(),
        engine.crashes().len()
    );

    if let Some(dir) = corpus_dir {
        fuzz::save_corpus(engine.corpus(), &dir).expect("corpus is writable");
    }
    if let Some(dir) = env::var_os("TPT_FUZZ_CRASHES") {
        fuzz::save_failures(engine.crashes(), dir).expect("crash directory is writable");
    }

    if let Some(path) = env::var_os("TPT_FUZZ_REPORT") {
        fuzz::write_report(engine.crashes(), path).expect("report path is writable");
    }

    let groups = fuzz::dedup(engine.crashes());
    assert!(
        groups.is_empty(),
        "{} crash groups; first: {:?}",
        groups.len(),
        groups.first().map(|g| (&g.message, &g.smallest))
    );
}
