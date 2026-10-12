//! Sancov edge-coverage feedback for [`Engine`](super::Engine).
//!
//! The Rust compiler can instrument code with LLVM `SanitizerCoverage` inline 8-bit counters.
//! The instrumented binary calls [`__sanitizer_cov_8bit_counters_init`] at startup with the
//! counter region. This module records the region, clears it before an execution and turns the
//! counters that were hit into engine features afterwards.
//!
//! Instrumentation is nightly-only. Build with:
//!
//! ```text
//! RUSTFLAGS="-C passes=sancov-module -C llvm-args=-sanitizer-coverage-level=3 \
//!            -C llvm-args=-sanitizer-coverage-inline-8bit-counters" \
//!   cargo +nightly test -p tpt-testkit --features sancov --test sancov
//! ```
//!
//! Without those flags the module still builds and runs. [`is_instrumented`] is false and
//! [`trace`] returns no features, which makes the engine fall back to its manual-feature mode.
//!
//! Counters are process-global, so only one input can be traced at a time. The engine executes
//! inputs sequentially, which satisfies this.
//!
//! ```
//! use tpt_testkit::fuzz::sancov;
//!
//! // Not instrumented in a normal build: no features, and nothing breaks.
//! let features = sancov::trace(|| 1 + 1);
//! assert!(features.1.is_empty() || sancov::is_instrumented());
//! ```

use std::sync::Mutex;

/// Counter regions registered by instrumented modules, as `(start address, length)`.
static REGIONS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// Called by instrumented code at startup with its counter region `[start, stop)`.
///
/// # Safety
///
/// The compiler-generated caller passes a valid, writable region that lives for the whole
/// process. It must not be called by hand.
#[unsafe(no_mangle)]
pub extern "C" fn __sanitizer_cov_8bit_counters_init(start: *mut u8, stop: *mut u8) {
    let (start, stop) = (start as usize, stop as usize);
    if stop <= start {
        return;
    }
    let mut regions = REGIONS.lock().unwrap_or_else(|e| e.into_inner());
    if !regions.iter().any(|&(s, _)| s == start) {
        regions.push((start, stop - start));
    }
}

/// Reports whether any instrumented counter region is registered.
pub fn is_instrumented() -> bool {
    counter_count() > 0
}

/// Total number of edge counters across all registered regions.
pub fn counter_count() -> usize {
    let regions = REGIONS.lock().unwrap_or_else(|e| e.into_inner());
    regions.iter().map(|&(_, len)| len).sum()
}

/// Zeroes every counter.
pub fn reset() {
    let regions = REGIONS.lock().unwrap_or_else(|e| e.into_inner());
    for &(start, len) in regions.iter() {
        // SAFETY: the region was registered by the compiler-generated init call and is writable
        // for the life of the process. Counters are plain bytes, so a racing write from another
        // thread can only produce a stale count, never undefined behavior in practice.
        unsafe { std::ptr::write_bytes(start as *mut u8, 0, len) };
    }
}

/// Returns one feature per edge that was hit since the last [`reset`].
///
/// A feature combines the edge index with a log2 bucket of its hit count, so an input that
/// takes a loop more often is also "new". The result is sorted by edge.
pub fn features() -> Vec<u64> {
    let regions = REGIONS.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = Vec::new();
    let mut base = 0_u64;
    for &(start, len) in regions.iter() {
        // SAFETY: see `reset`. The slice is only read, byte by byte.
        let counters = unsafe { std::slice::from_raw_parts(start as *const u8, len) };
        for (index, &count) in counters.iter().enumerate() {
            if count != 0 {
                let bucket = u64::from(8 - count.leading_zeros()); // 1..=8
                out.push(((base + index as u64) << 4) | bucket);
            }
        }
        base += len as u64;
    }
    out
}

/// Runs `f` with cleared counters and returns its result with the features it exercised.
///
/// Use it inside an engine target: `|data| sancov::trace(|| parse(data)).1`.
pub fn trace<T>(f: impl FnOnce() -> T) -> (T, Vec<u64>) {
    reset();
    let value = f();
    (value, features())
}
