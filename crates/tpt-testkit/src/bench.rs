//! Reproducible benchmarking on top of [criterion](https://docs.rs/criterion).
//!
//! Every benchmark is named `area/format/size` (for example `parse/json/small`) so CI can track
//! each metric over time. [`bench_named`] enforces that shape. Use [`validate_name`] to check
//! names in tooling.
//!
//! Output is produced by criterion under `target/criterion/`. The `tpt-testkit-cli` tool turns
//! that into the JSON/CSV summary and compares it against a baseline.

use std::fmt;
use std::hint::black_box;

use criterion::{Bencher, Criterion};

/// Cache line size assumed when touching memory to evict caches.
const CACHE_LINE: usize = 64;

/// Default buffer size for [`thrash_caches`]. Larger than typical last-level caches.
pub const DEFAULT_THRASH_BYTES: usize = 64 * 1024 * 1024;

/// Why a benchmark name was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    /// The name does not have exactly three `/`-separated segments.
    SegmentCount {
        /// How many segments were found.
        found: usize,
    },
    /// A segment is empty or does not match `[a-z0-9][a-z0-9_-]*`.
    InvalidSegment {
        /// The offending segment.
        segment: String,
    },
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SegmentCount { found } => {
                write!(f, "expected 3 segments (area/format/size), found {found}")
            }
            Self::InvalidSegment { segment } => write!(
                f,
                "segment {segment:?} must match [a-z0-9][a-z0-9_-]* and be non-empty"
            ),
        }
    }
}

impl std::error::Error for NameError {}

/// Checks that `name` is `area/format/size` with lowercase, dash- or underscore-separated
/// segments.
///
/// # Errors
///
/// Returns [`NameError`] describing the first problem found.
pub fn validate_name(name: &str) -> Result<(), NameError> {
    let segments: Vec<&str> = name.split('/').collect();
    if segments.len() != 3 {
        return Err(NameError::SegmentCount {
            found: segments.len(),
        });
    }
    for segment in segments {
        if !is_valid_segment(segment) {
            return Err(NameError::InvalidSegment {
                segment: segment.to_owned(),
            });
        }
    }
    Ok(())
}

fn is_valid_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Registers a benchmark under a validated `area/format/size` name.
///
/// # Panics
///
/// Panics if `name` fails [`validate_name`]. Names are fixed in source, so a bad name is a bug.
pub fn bench_named<F>(c: &mut Criterion, name: &str, f: F)
where
    F: FnMut(&mut Bencher),
{
    if let Err(e) = validate_name(name) {
        panic!("invalid benchmark name {name:?}: {e}");
    }
    c.bench_function(name, f);
}

/// Runs `f` `iterations` times before measurement so caches, branch predictors and allocators
/// reach a steady state. Results are passed through [`black_box`] so the work is not elided.
pub fn warm_up<T>(iterations: usize, mut f: impl FnMut() -> T) {
    for _ in 0..iterations {
        black_box(f());
    }
}

/// Evicts CPU caches by writing one byte per cache line across a `bytes`-sized buffer.
///
/// Call this between benchmarks when cold-cache behavior matters. Returns a checksum that
/// depends on the buffer, so the writes are not optimized out.
#[must_use]
pub fn thrash_caches(bytes: usize) -> u64 {
    let mut buf = vec![0u8; bytes];
    for line in buf.chunks_mut(CACHE_LINE) {
        line[0] = line[0].wrapping_add(1);
    }
    black_box(buf.iter().step_by(CACHE_LINE).map(|&b| u64::from(b)).sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_area_format_size() {
        assert_eq!(validate_name("parse/json/small"), Ok(()));
        assert_eq!(validate_name("golden/text_v2/10k-rows"), Ok(()));
        assert_eq!(validate_name("a1/b2/3"), Ok(()));
    }

    #[test]
    fn rejects_wrong_segment_count() {
        assert_eq!(
            validate_name("parse/json"),
            Err(NameError::SegmentCount { found: 2 })
        );
        assert_eq!(
            validate_name("a/b/c/d"),
            Err(NameError::SegmentCount { found: 4 })
        );
    }

    #[test]
    fn rejects_bad_segments() {
        for bad in [
            "Parse/json/small",
            "parse//small",
            "parse/json /small",
            "parse/json/",
            "/json/small",
            "parse/js.on/small",
        ] {
            assert!(
                matches!(validate_name(bad), Err(NameError::InvalidSegment { .. })),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn warm_up_runs_exact_iterations() {
        let mut calls = 0;
        warm_up(7, || calls += 1);
        assert_eq!(calls, 7);
    }

    #[test]
    fn thrash_is_deterministic_for_size() {
        assert_eq!(thrash_caches(4096), thrash_caches(4096));
        assert_ne!(thrash_caches(4096), thrash_caches(8192));
    }
}
