//! tpt-testkit: golden files, benchmarks, fuzzing, compatibility suites and property strategies.
//!
//! Must only ever be used as a `[dev-dependency]`.

// With only `nostd` enabled the crate has no std dependency, so it builds for bare-metal targets.
#![cfg_attr(
    not(any(
        feature = "bench",
        feature = "fuzz",
        feature = "golden",
        feature = "platform-isolation",
        feature = "property",
        feature = "suite"
    )),
    no_std
)]

#[cfg(feature = "nostd")]
extern crate alloc;

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
