//! Runs the malformed-input generator over the committed golden fixtures. Each parser here must
//! return normally for every input, valid or not.

#![cfg(feature = "fuzz")]

use tpt_testkit::fuzz;

#[test]
fn fixtures_survive_malformed_inputs() {
    let failures = fuzz::check_corpus_dir("tests/fixtures", 7, |data| {
        let _ = std::str::from_utf8(data);
        let _ = serde_json::from_slice::<serde_json::Value>(data);
    })
    .expect("fixtures directory is readable");
    assert!(
        failures.is_empty(),
        "{} inputs panicked; first: {:?}",
        failures.len(),
        failures.first()
    );
}
