//! Golden-file assertions.
//!
//! Fixtures live in `tests/fixtures/` of the crate that invokes the macro. Three modes:
//!
//! - **Text** ([`tpt_assert_golden!`](crate::tpt_assert_golden)): exact match after
//!   normalizing CRLF to LF, so fixtures are byte-identical on every OS.
//! - **Binary** ([`tpt_assert_golden_bytes!`](crate::tpt_assert_golden_bytes)): exact byte
//!   match. Stored as JSON with the bytes hex-encoded in 32-byte lines for readable diffs.
//! - **Float-tolerant** ([`tpt_assert_golden_float!`](crate::tpt_assert_golden_float)):
//!   JSON tree walk with a per-value tolerance.
//!
//! Set `TPT_UPDATE_GOLDEN=1` to create or overwrite fixtures instead of comparing.
//!
//! The I/O is implemented here rather than delegated to `insta`; `similar` is used only to
//! render diffs.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Number, Value, json};

/// Environment variable that switches golden assertions from comparing to writing.
pub const UPDATE_ENV: &str = "TPT_UPDATE_GOLDEN";

/// Bytes per hex line in binary fixtures.
const HEX_BYTES_PER_LINE: usize = 32;

/// Returns `<manifest_dir>/tests/fixtures/<name>`.
#[must_use]
pub fn fixture_path(manifest_dir: &str, name: &str) -> PathBuf {
    Path::new(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Whether `TPT_UPDATE_GOLDEN=1` is set.
#[must_use]
pub fn update_requested() -> bool {
    std::env::var(UPDATE_ENV).is_ok_and(|v| v == "1")
}

/// Replaces CRLF with LF.
#[must_use]
pub fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Compares `actual` against the text fixture at `fixture`, or writes it when `update` is set.
///
/// # Errors
///
/// Returns a human-readable message when the fixture is missing, unreadable, or differs.
pub fn check_text(fixture: &Path, actual: &str, update: bool) -> Result<(), String> {
    let rendered = normalize_newlines(actual);
    sync(fixture, &rendered, update, |expected| {
        let expected = normalize_newlines(expected);
        if expected == rendered {
            return Ok(());
        }
        Err(render_text_diff(&expected, &rendered))
    })
}

/// Compares `actual` bytes against the binary fixture at `fixture`, or writes it when `update`
/// is set.
///
/// # Errors
///
/// Returns a human-readable message when the fixture is missing, malformed, or differs.
pub fn check_bytes(fixture: &Path, actual: &[u8], update: bool) -> Result<(), String> {
    let rendered = render_bytes(actual)?;
    sync(fixture, &rendered, update, |expected| {
        let expected = decode_bytes(expected)?;
        if expected == actual {
            return Ok(());
        }
        Err(render_bytes_diff(&expected, actual))
    })
}

/// Compares a float tree against the JSON fixture at `fixture` within `epsilon`, or writes it
/// when `update` is set.
///
/// # Errors
///
/// Returns a human-readable message when the fixture is missing, malformed, or any value falls
/// outside the tolerance.
pub fn check_float(
    fixture: &Path,
    actual: &impl GoldenFloat,
    epsilon: f64,
    update: bool,
) -> Result<(), String> {
    let actual = actual.to_golden_value();
    let rendered = render_json(&actual)?;
    sync(fixture, &rendered, update, |expected| {
        let expected: Value =
            serde_json::from_str(expected).map_err(|e| format!("malformed float fixture: {e}"))?;
        let mut mismatches = Vec::new();
        compare_floats("$", &expected, &actual, epsilon, &mut mismatches);
        if mismatches.is_empty() {
            return Ok(());
        }
        let mut msg = format!("float golden mismatch (epsilon = {epsilon:e}):\n");
        for line in mismatches {
            let _ = writeln!(msg, "  {line}");
        }
        Err(msg)
    })
}

/// Shared create/update/compare driver for every mode.
fn sync(
    fixture: &Path,
    rendered: &str,
    update: bool,
    compare: impl FnOnce(&str) -> Result<(), String>,
) -> Result<(), String> {
    if update {
        return write_fixture(fixture, rendered);
    }
    match fs::read_to_string(fixture) {
        Ok(expected) => compare(&expected).map_err(|msg| {
            format!(
                "golden mismatch for {}\n{msg}\nRun with {UPDATE_ENV}=1 to accept the new output.",
                fixture.display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "golden fixture missing: {}\nCreate it with {UPDATE_ENV}=1 cargo test.",
            fixture.display()
        )),
        Err(e) => Err(format!(
            "cannot read golden fixture {}: {e}",
            fixture.display()
        )),
    }
}

fn write_fixture(fixture: &Path, rendered: &str) -> Result<(), String> {
    if fs::read_to_string(fixture).is_ok_and(|current| current == rendered) {
        return Ok(());
    }
    if let Some(parent) = fixture.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    fs::write(fixture, rendered)
        .map_err(|e| format!("cannot write golden fixture {}: {e}", fixture.display()))?;
    eprintln!("tpt-testkit: wrote golden fixture {}", fixture.display());
    Ok(())
}

fn render_text_diff(expected: &str, actual: &str) -> String {
    similar::TextDiff::from_lines(expected, actual)
        .unified_diff()
        .header("expected", "actual")
        .to_string()
}

fn render_json(value: &Value) -> Result<String, String> {
    let mut s = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    s.push('\n');
    Ok(s)
}

fn render_bytes(bytes: &[u8]) -> Result<String, String> {
    let data: Vec<String> = bytes.chunks(HEX_BYTES_PER_LINE).map(hex_encode).collect();
    render_json(&json!({
        "encoding": "hex",
        "len": bytes.len(),
        "data": data,
    }))
}

fn decode_bytes(fixture: &str) -> Result<Vec<u8>, String> {
    let value: Value =
        serde_json::from_str(fixture).map_err(|e| format!("malformed binary fixture: {e}"))?;
    if value.get("encoding").and_then(Value::as_str) != Some("hex") {
        return Err("binary fixture must declare \"encoding\": \"hex\"".to_owned());
    }
    let lines = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or("binary fixture is missing a \"data\" array")?;
    let mut bytes = Vec::new();
    for line in lines {
        let line = line.as_str().ok_or("binary fixture data must be strings")?;
        bytes.extend(hex_decode(line)?);
    }
    if let Some(len) = value.get("len").and_then(Value::as_u64)
        && len != bytes.len() as u64
    {
        return Err(format!(
            "binary fixture declares len {len} but contains {} bytes",
            bytes.len()
        ));
    }
    Ok(bytes)
}

fn render_bytes_diff(expected: &[u8], actual: &[u8]) -> String {
    let mut msg = format!(
        "binary mismatch: expected {} bytes, actual {} bytes\n",
        expected.len(),
        actual.len()
    );
    let first = expected
        .iter()
        .zip(actual)
        .position(|(e, a)| e != a)
        .unwrap_or_else(|| expected.len().min(actual.len()));
    let _ = writeln!(msg, "first difference at byte offset {first}");
    let _ = writeln!(msg, "  expected: {}", hex_window(expected, first));
    let _ = writeln!(msg, "  actual:   {}", hex_window(actual, first));
    msg
}

fn hex_window(bytes: &[u8], at: usize) -> String {
    let start = at.saturating_sub(8);
    let end = (at + 8).min(bytes.len());
    if start >= end {
        return "<end of data>".to_owned();
    }
    hex_encode(&bytes[start..end])
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if !s.is_ascii() || !s.len().is_multiple_of(2) {
        return Err(format!("invalid hex line: {s:?}"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| format!("invalid hex line: {s:?}"))
        })
        .collect()
}

/// Values that can be stored in a float-tolerant golden fixture.
///
/// NaN and infinities have no JSON representation, so they are stored as the strings `"NaN"`,
/// `"Infinity"` and `"-Infinity"`. Implement this for your own types by building a
/// [`Value`] tree with [`float_value`].
pub trait GoldenFloat {
    /// Converts `self` into a JSON tree with every float encoded by [`float_value`].
    fn to_golden_value(&self) -> Value;
}

/// Encodes one float for a golden fixture. Finite values, including `-0.0`, become JSON numbers;
/// non-finite values become the strings listed on [`GoldenFloat`].
#[must_use]
pub fn float_value(f: f64) -> Value {
    if f.is_nan() {
        Value::String("NaN".to_owned())
    } else if f == f64::INFINITY {
        Value::String("Infinity".to_owned())
    } else if f == f64::NEG_INFINITY {
        Value::String("-Infinity".to_owned())
    } else {
        Value::Number(Number::from_f64(f).expect("finite f64 always converts to a JSON number"))
    }
}

impl GoldenFloat for f64 {
    fn to_golden_value(&self) -> Value {
        float_value(*self)
    }
}

impl GoldenFloat for f32 {
    fn to_golden_value(&self) -> Value {
        float_value(f64::from(*self))
    }
}

impl<T: GoldenFloat + ?Sized> GoldenFloat for &T {
    fn to_golden_value(&self) -> Value {
        (**self).to_golden_value()
    }
}

impl<T: GoldenFloat> GoldenFloat for [T] {
    fn to_golden_value(&self) -> Value {
        Value::Array(self.iter().map(GoldenFloat::to_golden_value).collect())
    }
}

impl<T: GoldenFloat, const N: usize> GoldenFloat for [T; N] {
    fn to_golden_value(&self) -> Value {
        self.as_slice().to_golden_value()
    }
}

impl<T: GoldenFloat> GoldenFloat for Vec<T> {
    fn to_golden_value(&self) -> Value {
        self.as_slice().to_golden_value()
    }
}

/// Whether two finite floats agree within `epsilon`, using both an absolute and a relative bound:
/// `|a - b| <= epsilon` or `|a - b| <= epsilon * max(|a|, |b|)`.
#[must_use]
pub fn floats_close(a: f64, b: f64, epsilon: f64) -> bool {
    let diff = (a - b).abs();
    diff <= epsilon || diff <= epsilon * a.abs().max(b.abs())
}

/// Walks `expected` and `actual` in parallel and records each mismatch at `path`.
///
/// Numbers compare with [`floats_close`]. Zeros of either sign are equal. Strings (the encoded
/// special floats) must match exactly. Objects need the same keys and arrays the same length.
pub fn compare_floats(
    path: &str,
    expected: &Value,
    actual: &Value,
    epsilon: f64,
    out: &mut Vec<String>,
) {
    match (expected, actual) {
        (Value::Number(e), Value::Number(a)) => {
            let (e, a) = (
                e.as_f64().unwrap_or(f64::NAN),
                a.as_f64().unwrap_or(f64::NAN),
            );
            if !floats_close(e, a, epsilon) {
                out.push(format!(
                    "{path}: expected {e}, actual {a} (|diff| = {:e})",
                    (e - a).abs()
                ));
            }
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                out.push(format!(
                    "{path}: expected {} elements, actual {}",
                    e.len(),
                    a.len()
                ));
            }
            for (i, (e, a)) in e.iter().zip(a).enumerate() {
                compare_floats(&format!("{path}[{i}]"), e, a, epsilon, out);
            }
        }
        (Value::Object(e), Value::Object(a)) => {
            for (key, e_val) in e {
                match a.get(key) {
                    Some(a_val) => {
                        compare_floats(&format!("{path}.{key}"), e_val, a_val, epsilon, out);
                    }
                    None => out.push(format!("{path}.{key}: missing from actual")),
                }
            }
            for key in a.keys().filter(|k| !e.contains_key(*k)) {
                out.push(format!("{path}.{key}: not present in expected"));
            }
        }
        (e, a) if e == a => {}
        (e, a) => out.push(format!("{path}: expected {e}, actual {a}")),
    }
}

/// Asserts that `actual` matches the text fixture `name` under the calling crate's
/// `tests/fixtures/`. Panics with a diff on mismatch.
#[track_caller]
pub fn assert_text(manifest_dir: &str, name: &str, actual: impl AsRef<str>) {
    if let Err(msg) = check_text(
        &fixture_path(manifest_dir, name),
        actual.as_ref(),
        update_requested(),
    ) {
        panic!("{msg}");
    }
}

/// Asserts that `actual` bytes match the binary fixture `name`. Panics on mismatch.
#[track_caller]
pub fn assert_bytes(manifest_dir: &str, name: &str, actual: impl AsRef<[u8]>) {
    if let Err(msg) = check_bytes(
        &fixture_path(manifest_dir, name),
        actual.as_ref(),
        update_requested(),
    ) {
        panic!("{msg}");
    }
}

/// Asserts that `actual` matches the float fixture `name` within `epsilon`. Panics on mismatch.
#[track_caller]
pub fn assert_float(manifest_dir: &str, name: &str, actual: &impl GoldenFloat, epsilon: f64) {
    if let Err(msg) = check_float(
        &fixture_path(manifest_dir, name),
        actual,
        epsilon,
        update_requested(),
    ) {
        panic!("{msg}");
    }
}

/// Golden text assertion. Fixture: `tests/fixtures/<name>`, relative to the calling crate.
///
/// ```ignore
/// tpt_testkit::tpt_assert_golden!(rendered_layout, "layout.expected.txt");
/// ```
#[macro_export]
macro_rules! tpt_assert_golden {
    ($actual:expr, $name:expr $(,)?) => {
        $crate::golden::assert_text(env!("CARGO_MANIFEST_DIR"), $name, &$actual)
    };
}

/// Golden binary assertion. Fixture: `tests/fixtures/<name>`, stored as hex-encoded JSON.
///
/// ```ignore
/// tpt_testkit::tpt_assert_golden_bytes!(encoded_pdf, "doc.expected.json");
/// ```
#[macro_export]
macro_rules! tpt_assert_golden_bytes {
    ($actual:expr, $name:expr $(,)?) => {
        $crate::golden::assert_bytes(env!("CARGO_MANIFEST_DIR"), $name, &$actual)
    };
}

/// Golden float assertion with tolerance. Fixture: `tests/fixtures/<name>`.
///
/// ```ignore
/// tpt_testkit::tpt_assert_golden_float!(matrix, "matrix.expected.json", epsilon = 1e-6);
/// ```
#[macro_export]
macro_rules! tpt_assert_golden_float {
    ($actual:expr, $name:expr, epsilon = $eps:expr $(,)?) => {
        $crate::golden::assert_float(env!("CARGO_MANIFEST_DIR"), $name, &$actual, $eps)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("tpt-testkit-golden-unit")
            .join(format!("{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("fixture.txt")
    }

    #[test]
    fn normalizes_crlf() {
        assert_eq!(normalize_newlines("a\r\nb\r\n"), "a\nb\n");
    }

    #[test]
    fn hex_round_trips() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(hex_decode(&hex_encode(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn hex_rejects_bad_input() {
        assert!(hex_decode("abc").is_err());
        assert!(hex_decode("zz").is_err());
        assert!(hex_decode("é0").is_err());
    }

    #[test]
    fn close_uses_absolute_and_relative_bounds() {
        assert!(floats_close(0.0, 1e-9, 1e-6));
        assert!(floats_close(1e9, 1e9 + 1000.0, 1e-6));
        assert!(!floats_close(1.0, 1.1, 1e-6));
        assert!(floats_close(-0.0, 0.0, 0.0));
    }

    #[test]
    fn special_floats_encode_as_strings() {
        assert_eq!(float_value(f64::NAN), json!("NaN"));
        assert_eq!(float_value(f64::INFINITY), json!("Infinity"));
        assert_eq!(float_value(f64::NEG_INFINITY), json!("-Infinity"));
        assert_eq!(float_value(-0.0).to_string(), "-0.0");
    }

    #[test]
    fn walk_reports_paths() {
        let expected = json!({"a": [1.0, 2.0], "b": "NaN", "gone": 1});
        let actual = json!({"a": [1.0, 2.5], "b": "NaN", "extra": 3});
        let mut out = Vec::new();
        compare_floats("$", &expected, &actual, 1e-6, &mut out);
        assert_eq!(out.len(), 3, "{out:#?}");
        assert!(out[0].starts_with("$.a[1]"));
        assert!(out.iter().any(|l| l.contains("$.gone")));
        assert!(out.iter().any(|l| l.contains("$.extra")));
    }

    #[test]
    fn text_create_then_compare() {
        let path = scratch("text");
        assert!(
            check_text(&path, "hello\r\nworld\n", false)
                .unwrap_err()
                .contains("missing")
        );
        check_text(&path, "hello\r\nworld\n", true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello\nworld\n");
        check_text(&path, "hello\nworld\n", false).unwrap();
        let err = check_text(&path, "hello\nthere\n", false).unwrap_err();
        assert!(err.contains("-world") && err.contains("+there"), "{err}");
    }

    #[test]
    fn bytes_round_trip_and_detect_change() {
        let path = scratch("bytes");
        let data: Vec<u8> = (0..100).collect();
        check_bytes(&path, &data, true).unwrap();
        check_bytes(&path, &data, false).unwrap();
        let mut changed = data.clone();
        changed[70] ^= 1;
        let err = check_bytes(&path, &changed, false).unwrap_err();
        assert!(err.contains("offset 70"), "{err}");
    }

    #[test]
    fn bytes_reject_wrong_encoding_and_length() {
        let path = scratch("bytes-bad");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"encoding":"base64","data":[]}"#).unwrap();
        assert!(check_bytes(&path, b"", false).unwrap_err().contains("hex"));
        fs::write(&path, r#"{"encoding":"hex","len":2,"data":["00"]}"#).unwrap();
        assert!(
            check_bytes(&path, b"\0", false)
                .unwrap_err()
                .contains("declares len")
        );
    }

    #[test]
    fn float_create_then_compare_within_tolerance() {
        let path = scratch("float");
        let actual = vec![vec![1.0_f64, f64::NAN], vec![f64::INFINITY, -0.0]];
        check_float(&path, &actual, 1e-6, true).unwrap();
        let nearly = vec![vec![1.0 + 1e-9, f64::NAN], vec![f64::INFINITY, 0.0]];
        check_float(&path, &nearly, 1e-6, false).unwrap();
        let off = vec![vec![1.1, f64::NAN], vec![f64::INFINITY, 0.0]];
        let err = check_float(&path, &off, 1e-6, false).unwrap_err();
        assert!(err.contains("$[0][0]"), "{err}");
        let wrong_inf = vec![vec![1.0, f64::NAN], vec![f64::NEG_INFINITY, 0.0]];
        assert!(check_float(&path, &wrong_inf, 1e-6, false).is_err());
    }
}
