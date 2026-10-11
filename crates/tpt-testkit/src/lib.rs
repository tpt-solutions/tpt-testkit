//! tpt-testkit: golden files, benchmarks, fuzzing, compatibility suites and property strategies.
//!
//! Must only ever be used as a `[dev-dependency]`.

#[cfg(feature = "bench")]
pub mod bench;
#[cfg(feature = "golden")]
pub mod golden;
#[cfg(feature = "platform-isolation")]
#[allow(unsafe_code)]
pub mod platform;
