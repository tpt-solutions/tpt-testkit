//! Checks the committed suite manifests without touching the network.

#![cfg(feature = "suite")]

use std::fs;
use std::path::Path;

use tpt_testkit::suite::Manifest;

fn load(relative: &str) -> Manifest {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    Manifest::from_json(&fs::read_to_string(&path).unwrap())
        .unwrap_or_else(|err| panic!("{relative}: {err}"))
}

#[test]
fn unicode_bidi_manifest_is_valid() {
    let manifest = load("../../suites/unicode-bidi.json");
    assert_eq!(manifest.license, "Unicode-3.0");
    assert!(manifest.entry("BidiTest.txt").is_some());
}

#[test]
fn unicode_line_break_manifest_is_valid() {
    let manifest = load("../../suites/unicode-line-break.json");
    assert_eq!(manifest.license, "Unicode-3.0");
    assert!(manifest.entry("LineBreakTest.txt").is_some());
}

#[test]
fn icc_test_profiles_manifest_is_valid() {
    let manifest = load("../../suites/icc-test-profiles.json");
    assert_eq!(manifest.license, "LicenseRef-ICC-Free-Use");
    assert!(manifest.entry("PRMG_RGB-sRGB_based.icc").is_some());
    assert!(manifest.entry("Probev2.zip").is_some());
}

/// Downloads the real Unicode files over HTTPS (requires curl and network access). Run with
/// `cargo test -p tpt-testkit --all-features --test suite_manifests -- --ignored`, and set
/// `TPT_TESTKIT_CACHE` to a scratch directory to force a download.
#[test]
#[ignore = "downloads from unicode.org"]
fn unicode_suites_download_and_verify() {
    use tpt_testkit::suite::{Options, ensure_file, fetch_with_curl};

    let options = Options::from_env();
    for relative in [
        "../../suites/unicode-bidi.json",
        "../../suites/unicode-line-break.json",
        "../../suites/icc-test-profiles.json",
    ] {
        let manifest = load(relative);
        for entry in &manifest.files {
            let path = ensure_file(&manifest, &entry.path, &options, fetch_with_curl)
                .unwrap_or_else(|err| panic!("{}: {err}", entry.path));
            assert!(fs::metadata(&path).unwrap().len() > 0);
        }
    }
}
