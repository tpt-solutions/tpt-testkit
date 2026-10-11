//! Property-based strategies for TPT data formats, built on [proptest](https://docs.rs/proptest).
//!
//! Each strategy is a plain [`Strategy`] that you can combine with `proptest!`:
//!
//! ```ignore
//! proptest! {
//!     #![proptest_config(ProptestConfig::with_cases(10_000))]
//!     #[test]
//!     fn parser_never_panics(input in property::malformed_pdf_stream()) {
//!         let _ = my_pdf_parser::parse(&input);
//!     }
//! }
//! ```
//!
//! "Valid" strategies produce inputs that are well-formed for their format. "Invalid" and
//! "malformed" strategies produce inputs that are guaranteed to be rejected by a strict parser,
//! or that are corrupted copies of valid inputs. The malformed PDF strategy may still produce a
//! stream a lenient parser accepts; it is for "must not panic" checks, not "must reject" checks.

use std::fmt::Display;

use proptest::prelude::*;

/// Characters that tend to trip up text handling: NUL, BOM, zero-width joiner, bidi override,
/// a combining mark, the replacement character, the highest code point, and line breaks.
const TRICKY_CHARS: &[char] = &[
    '\0',
    '\u{FEFF}',
    '\u{200D}',
    '\u{202E}',
    '\u{0301}',
    '\u{FFFD}',
    '\u{10FFFF}',
    '\r',
    '\n',
];

/// Valid UTF-8 strings. Mostly arbitrary Unicode, with a bias towards [`TRICKY_CHARS`].
pub fn valid_utf8() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => any::<String>(),
        1 => prop::collection::vec(prop::sample::select(TRICKY_CHARS), 0..32)
            .prop_map(|chars| chars.into_iter().collect()),
    ]
}

/// `f64` values covering the IEEE-754 special cases: NaN (plain and with payload), both
/// infinities, both zeros, subnormals, and the extremes. Arbitrary bit patterns are included.
pub fn ieee754_special_floats() -> impl Strategy<Value = f64> {
    prop_oneof![
        Just(f64::NAN),
        Just(f64::from_bits(0x7FF8_0000_0000_0001)),
        Just(f64::INFINITY),
        Just(f64::NEG_INFINITY),
        Just(0.0),
        Just(-0.0),
        Just(f64::from_bits(1)),
        Just(-f64::from_bits(1)),
        Just(f64::MIN_POSITIVE),
        Just(f64::MAX),
        Just(f64::MIN),
        Just(f64::EPSILON),
        any::<f64>(),
    ]
}

/// Valid JSON text: arbitrary nested arrays and objects of finite values.
pub fn valid_json() -> impl Strategy<Value = String> {
    json_value().prop_map(|value| {
        serde_json::to_string(&value).expect("finite JSON values always serialize")
    })
}

/// JSON text that a strict parser must reject.
///
/// Each case is invalid by construction: an unterminated object, a trailing comma inside an
/// array, or garbage after a complete value.
pub fn invalid_json() -> impl Strategy<Value = String> {
    (json_value(), 0u8..3).prop_map(|(value, kind)| {
        let text = serde_json::to_string(&value).expect("finite JSON values always serialize");
        match kind {
            0 => format!("{{\"k\": {text}"),
            1 => format!("[{text},]"),
            _ => format!("{text}x"),
        }
    })
}

/// CSV text with a consistent column count per file, per RFC 4180. Fields are either plain or
/// quoted, and quoted fields may contain commas, doubled quotes and line breaks.
pub fn valid_csv() -> impl Strategy<Value = String> {
    (1usize..5)
        .prop_flat_map(|columns| {
            prop::collection::vec(prop::collection::vec(csv_field(), columns..=columns), 1..6)
        })
        .prop_map(|rows| {
            let mut out = String::new();
            for row in rows {
                out.push_str(&row.join(","));
                out.push('\n');
            }
            out
        })
}

/// CSV text that violates RFC 4180: a quote that is never closed, or a quote inside an
/// unquoted field.
pub fn invalid_csv() -> impl Strategy<Value = String> {
    (valid_csv(), 0u8..2).prop_map(|(mut text, kind)| {
        match kind {
            0 => text.push_str("\"unterminated field\n"),
            _ => text.push_str("ab\"cd\n"),
        }
        text
    })
}

/// A structurally well-formed PDF stream object with an arbitrary binary body and a correct
/// `/Length`.
pub fn valid_pdf_stream() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..512).prop_map(|body| build_pdf(&body))
}

/// A PDF stream object with one corruption applied: truncation, an overwritten byte, an inserted
/// byte, or a bogus `/Length` entry. Use for "must not panic" checks.
pub fn malformed_pdf_stream() -> impl Strategy<Value = Vec<u8>> {
    (
        valid_pdf_stream(),
        any::<usize>(),
        any::<u8>(),
        any::<i64>(),
        0u8..4,
    )
        .prop_map(|(bytes, at, byte, length, kind)| corrupt_pdf(bytes, at, byte, length, kind))
}

/// Checks that `decode(encode(value))` gives back `value`.
///
/// Returns a message describing the mismatch or decode failure. Use it with `prop_assert!` so
/// proptest can shrink the failing input:
///
/// ```ignore
/// prop_assert_eq!(check_round_trip(&value, encode, decode), Ok(()));
/// ```
///
/// # Errors
///
/// Returns `Err` when decoding fails or when the decoded value differs from the original.
pub fn check_round_trip<T, E, D, Er>(value: &T, encode: E, decode: D) -> Result<(), String>
where
    T: PartialEq + std::fmt::Debug,
    E: FnOnce(&T) -> Vec<u8>,
    D: FnOnce(&[u8]) -> Result<T, Er>,
    Er: Display,
{
    let bytes = encode(value);
    let decoded = decode(&bytes).map_err(|err| format!("decode failed: {err}"))?;
    if &decoded == value {
        Ok(())
    } else {
        Err(format!(
            "round trip changed the value: {value:?} became {decoded:?}"
        ))
    }
}

fn json_value() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        any::<i64>().prop_map(serde_json::Value::from),
        prop::num::f64::NORMAL.prop_map(serde_json::Value::from),
        valid_utf8().prop_map(serde_json::Value::String),
    ];
    leaf.prop_recursive(3, 32, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(serde_json::Value::Array),
            prop::collection::btree_map(valid_utf8(), inner, 0..4)
                .prop_map(|map| serde_json::Value::Object(map.into_iter().collect())),
        ]
    })
}

fn csv_field() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9 ]{0,8}",
        valid_utf8().prop_map(|text| format!("\"{}\"", text.replace('"', "\"\""))),
    ]
}

fn build_pdf(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 96);
    out.extend_from_slice(b"%PDF-1.7\n1 0 obj\n");
    out.extend_from_slice(format!("<< /Length {} >>\nstream\n", body.len()).as_bytes());
    out.extend_from_slice(body);
    out.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
    out
}

fn corrupt_pdf(mut bytes: Vec<u8>, at: usize, byte: u8, length: i64, kind: u8) -> Vec<u8> {
    match kind {
        0 => bytes.truncate(at % (bytes.len() + 1)),
        1 => {
            let index = at % bytes.len().max(1);
            if let Some(slot) = bytes.get_mut(index) {
                *slot = byte;
            }
        }
        2 => {
            let index = at % (bytes.len() + 1);
            bytes.insert(index, byte);
        }
        _ => {
            let index = at % (bytes.len() + 1);
            let tail = bytes.split_off(index);
            bytes.extend_from_slice(format!(" /Length {length} ").as_bytes());
            bytes.extend(tail);
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parses_json(text: &str) -> bool {
        serde_json::from_str::<serde_json::Value>(text).is_ok()
    }

    fn parses_csv(text: &str) -> bool {
        // Strict RFC 4180 check for the shapes these generators produce: quoted fields close,
        // and quotes only appear at the start of a field.
        let mut in_quotes = false;
        let mut field_start = true;
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if in_quotes {
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                    } else {
                        in_quotes = false;
                    }
                }
                field_start = false;
                continue;
            }
            match c {
                '"' if field_start => {
                    in_quotes = true;
                    field_start = false;
                }
                '"' => return false,
                ',' | '\n' => field_start = true,
                _ => field_start = false,
            }
        }
        !in_quotes
    }

    proptest! {
        #[test]
        fn valid_utf8_is_valid(s in valid_utf8()) {
            prop_assert!(std::str::from_utf8(s.as_bytes()).is_ok());
        }

        #[test]
        fn valid_json_parses(text in valid_json()) {
            prop_assert!(parses_json(&text));
        }

        #[test]
        fn invalid_json_is_rejected(text in invalid_json()) {
            prop_assert!(!parses_json(&text));
        }

        #[test]
        fn valid_csv_is_accepted(text in valid_csv()) {
            prop_assert!(parses_csv(&text));
        }

        #[test]
        fn invalid_csv_is_rejected(text in invalid_csv()) {
            prop_assert!(!parses_csv(&text));
        }

        #[test]
        fn valid_pdf_has_correct_length(bytes in valid_pdf_stream()) {
            let text = String::from_utf8_lossy(&bytes);
            let declared: usize = text
                .split("/Length ")
                .nth(1)
                .and_then(|rest| rest.split(' ').next())
                .and_then(|n| n.parse().ok())
                .expect("declared length");
            let body_start = bytes.windows(7).position(|w| w == b"stream\n").expect("stream") + 7;
            prop_assert_eq!(declared, bytes.len() - body_start - b"\nendstream\nendobj\n%%EOF\n".len());
        }

        #[test]
        fn malformed_pdf_size_is_bounded(bytes in malformed_pdf_stream()) {
            // Body (512) + envelope (~75) + one inserted /Length entry (~32) at most.
            prop_assert!(bytes.len() <= 512 + 96 + 32);
        }

        #[test]
        fn ieee754_specials_round_trip_bits(f in ieee754_special_floats()) {
            let encoded = f.to_bits().to_le_bytes();
            let decoded = f64::from_bits(u64::from_le_bytes(encoded));
            prop_assert_eq!(decoded.to_bits(), f.to_bits());
        }

        #[test]
        fn round_trip_helper_accepts_identity(s in valid_utf8()) {
            prop_assert_eq!(
                check_round_trip(&s, |v| v.as_bytes().to_vec(), |b| String::from_utf8(b.to_vec())),
                Ok(())
            );
        }
    }

    #[test]
    fn round_trip_helper_reports_mismatch() {
        let result = check_round_trip(&1_u8, |v| vec![*v], |_| Ok::<u8, String>(2));
        assert!(result.is_err());
    }

    #[test]
    fn round_trip_helper_reports_decode_failure() {
        let result = check_round_trip(&1_u8, |v| vec![*v], |_| Err::<u8, _>("boom"));
        assert_eq!(result, Err("decode failed: boom".to_owned()));
    }
}
