//! End-to-end golden tests through the public macros. Fixtures are committed under
//! `tests/fixtures/`; regenerate them with `TPT_UPDATE_GOLDEN=1 cargo test`.

use tpt_testkit::{tpt_assert_golden, tpt_assert_golden_bytes, tpt_assert_golden_float};

#[test]
fn text_fixture() {
    let rendered = format!("line {}\r\nline {}\r\n", 1, 2);
    tpt_assert_golden!(rendered, "text.expected.txt");
}

#[test]
fn binary_fixture() {
    let bytes: Vec<u8> = (0..=99).map(|i| (i * 7) as u8).collect();
    tpt_assert_golden_bytes!(bytes, "binary.expected.json");
}

#[test]
fn float_fixture() {
    let matrix = [[1.0_f64, 0.5], [f64::NAN, -0.0]];
    tpt_assert_golden_float!(matrix, "matrix.expected.json", epsilon = 1e-6);
}
