//! tpt-testkit: golden files, benchmarks, fuzzing, compatibility suites and property strategies.
//!
//! Must only ever be used as a `[dev-dependency]`.

#[cfg(feature = "bench")]
pub mod bench;
#[cfg(feature = "fuzz")]
pub mod fuzz;
#[cfg(feature = "golden")]
pub mod golden;
#[cfg(feature = "nostd")]
pub mod nostd;
#[cfg(feature = "platform-isolation")]
#[allow(unsafe_code)]
pub mod platform;
#[cfg(feature = "property")]
pub mod property;
#[cfg(feature = "suite")]
pub mod suite;
