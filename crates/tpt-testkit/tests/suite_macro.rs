//! End-to-end check of the generated suite tests, against the committed fixture cache
//! (`.cargo/config.toml` points `TPT_TESTKIT_CACHE` at it).

#![cfg(feature = "suite")]

use tpt_testkit::suite::tpt_suite_tests;

tpt_suite_tests!("tests/fixtures/suite/manifest.json", check = check_hello);

fn check_hello(bytes: &[u8]) {
    assert_eq!(bytes, b"hello suite\n");
}
