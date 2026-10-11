//! Platform-specific benchmark isolation: pinning the current thread to one CPU core.
//!
//! This is the only module in the workspace where `unsafe` is allowed. The FFI is declared
//! directly, so no native-binding crate is pulled in.
//!
//! Support by OS:
//!
//! - Linux: `sched_setaffinity` on the calling thread.
//! - Windows: `SetThreadAffinityMask` on the current thread.
//! - macOS and other targets: [`PinError::Unsupported`]. macOS has no public API for pinning a
//!   thread to a specific core.

use std::fmt;

/// Why a thread could not be pinned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinError {
    /// This platform has no thread-affinity API that tpt-testkit supports.
    Unsupported,
    /// The requested core index does not fit the affinity mask.
    CoreOutOfRange {
        /// The requested core.
        core: usize,
        /// The number of cores the mask can address.
        max: usize,
    },
    /// The operating system rejected the request.
    Rejected,
}

impl fmt::Display for PinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => write!(f, "thread pinning is not supported on this platform"),
            Self::CoreOutOfRange { core, max } => {
                write!(f, "core {core} is out of range (mask holds {max} cores)")
            }
            Self::Rejected => write!(f, "the operating system rejected the affinity request"),
        }
    }
}

impl std::error::Error for PinError {}

/// Pins the calling thread to `core`. Pin before measuring and keep the thread on that core
/// for the whole benchmark.
///
/// # Errors
///
/// Returns [`PinError`] when the platform does not support pinning, the core index is out of
/// range, or the OS refuses the request.
pub fn pin_current_thread(core: usize) -> Result<(), PinError> {
    imp::pin_current_thread(core)
}

#[cfg(target_os = "linux")]
mod imp {
    use super::PinError;

    /// Matches glibc's `cpu_set_t`: 1024 bits.
    const MASK_WORDS: usize = 16;
    const MAX_CORES: usize = MASK_WORDS * 64;

    unsafe extern "C" {
        fn sched_setaffinity(pid: i32, cpusetsize: usize, mask: *const u64) -> i32;
    }

    pub fn pin_current_thread(core: usize) -> Result<(), PinError> {
        if core >= MAX_CORES {
            return Err(PinError::CoreOutOfRange {
                core,
                max: MAX_CORES,
            });
        }
        let mut mask = [0u64; MASK_WORDS];
        mask[core / 64] = 1u64 << (core % 64);
        // SAFETY: `mask` is a live array of exactly `size_of_val(&mask)` bytes. pid 0 names the
        // calling thread on Linux.
        let rc = unsafe { sched_setaffinity(0, std::mem::size_of_val(&mask), mask.as_ptr()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(PinError::Rejected)
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::PinError;

    const MAX_CORES: usize = usize::BITS as usize;

    unsafe extern "system" {
        fn GetCurrentThread() -> isize;
        fn SetThreadAffinityMask(thread: isize, mask: usize) -> usize;
    }

    pub fn pin_current_thread(core: usize) -> Result<(), PinError> {
        if core >= MAX_CORES {
            return Err(PinError::CoreOutOfRange {
                core,
                max: MAX_CORES,
            });
        }
        // SAFETY: GetCurrentThread returns a pseudo-handle that is always valid for the calling
        // thread. SetThreadAffinityMask only reads its arguments and returns the previous mask,
        // which is zero on failure.
        let previous = unsafe { SetThreadAffinityMask(GetCurrentThread(), 1usize << core) };
        if previous == 0 {
            Err(PinError::Rejected)
        } else {
            Ok(())
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::PinError;

    pub fn pin_current_thread(_core: usize) -> Result<(), PinError> {
        Err(PinError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_range_core_is_rejected_without_touching_the_os() {
        let err = pin_current_thread(usize::MAX).unwrap_err();
        assert!(matches!(err, PinError::CoreOutOfRange { .. }), "{err}");
    }

    #[test]
    fn pinning_core_zero_succeeds_where_supported() {
        match pin_current_thread(0) {
            Ok(()) => {}
            Err(PinError::Unsupported) => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
}
