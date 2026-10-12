//! Fuzzing conventions for parsers: shared dictionaries, a malformed-input generator, and an
//! in-process "must not panic" harness.
//!
//! This module is pure Rust and runs on stable. It does not replace coverage-guided fuzzing with
//! cargo-fuzz. It covers the cheap, deterministic half of the job: every parser must survive
//! truncated, mis-encoded and mis-sized versions of its valid inputs, and must do so on every CI
//! run without a nightly toolchain.
//!
//! Typical use from a test:
//!
//! ```ignore
//! let seeds = fuzz::check_corpus_dir("tests/corpus", 1, |data| {
//!     let _ = my_parser::parse(data);
//! })?;
//! assert!(seeds.is_empty(), "parser panicked on {} inputs", seeds.len());
//! ```

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::panic;
use std::path::{Path, PathBuf};

mod engine;
mod minimize;
mod report;

pub use engine::{Engine, MAX_INPUT_LEN, Rng, mutate, read_corpus, save_corpus};
pub use minimize::{
    CrashGroup, dedup, minimize, minimize_corpus, minimize_crash, signature, unknown_groups,
};
pub use report::{
    CrashRecord, CrashReport, REPORT_SCHEMA, from_hex, read_report, to_hex, write_report,
};

/// Tokens that appear in JSON. Feed to a fuzzer with [`write_dictionary`].
pub const JSON_TOKENS: &[&[u8]] = &[
    b"{", b"}", b"[", b"]", b":", b",", b"\"", b"\\", b"true", b"false", b"null", b"\\u0000",
    b"\\uD800", b"1e309", b"-0", b"0.0e+0",
];

/// Keywords and delimiters that appear in PDF files.
pub const PDF_KEYWORDS: &[&[u8]] = &[
    b"%PDF-1.7",
    b"obj",
    b"endobj",
    b"stream",
    b"endstream",
    b"xref",
    b"trailer",
    b"startxref",
    b"%%EOF",
    b"<<",
    b">>",
    b"/Length",
    b"/Filter",
    b"/FlateDecode",
    b"/Type",
    b"/Pages",
    b" R",
    b"[",
    b"]",
    b"(",
    b")",
    b"<",
    b">",
];

/// Delimiters and line endings that appear in CSV files.
pub const CSV_TOKENS: &[&[u8]] = &[b",", b"\"", b"\"\"", b"\r\n", b"\n", b"\r", b"\0"];

/// Unicode script names, as they appear in property files such as `Scripts.txt`.
pub const UNICODE_SCRIPTS: &[&[u8]] = &[
    b"Latin",
    b"Greek",
    b"Cyrillic",
    b"Han",
    b"Arabic",
    b"Hebrew",
    b"Devanagari",
    b"Hiragana",
    b"Katakana",
    b"Thai",
    b"Common",
    b"Inherited",
];

/// UTF-8 encodings of bidi controls (U+202A..U+202E, U+2066..U+2069, U+200E, U+200F).
pub const BIDI_CONTROLS: &[&[u8]] = &[
    "\u{202A}".as_bytes(),
    "\u{202B}".as_bytes(),
    "\u{202C}".as_bytes(),
    "\u{202D}".as_bytes(),
    "\u{202E}".as_bytes(),
    "\u{2066}".as_bytes(),
    "\u{2067}".as_bytes(),
    "\u{2068}".as_bytes(),
    "\u{2069}".as_bytes(),
    "\u{200E}".as_bytes(),
    "\u{200F}".as_bytes(),
];

/// Byte sequences that are not valid UTF-8: stray continuation bytes, overlong encodings,
/// surrogates, code points above U+10FFFF, and truncated multi-byte sequences.
pub const INVALID_UTF8: &[&[u8]] = &[
    &[0xFF],
    &[0x80],
    &[0xC0, 0xAF],
    &[0xC1, 0xBF],
    &[0xED, 0xA0, 0x80],
    &[0xF4, 0x90, 0x80, 0x80],
    &[0xE2, 0x82],
    &[0xF0, 0x9F],
];

/// Writes `tokens` in libFuzzer / AFL dictionary format, one quoted entry per line.
///
/// Quotes, backslashes and non-printable bytes are written as `\xHH` escapes, so any token
/// round-trips through the fuzzer's dictionary parser.
///
/// # Errors
///
/// Returns any I/O error from `out`.
pub fn write_dictionary(tokens: &[&[u8]], out: &mut impl Write) -> io::Result<()> {
    for token in tokens {
        out.write_all(b"\"")?;
        for &byte in *token {
            if byte == b'"' || byte == b'\\' || !(0x20..0x7f).contains(&byte) {
                write!(out, "\\x{byte:02x}")?;
            } else {
                out.write_all(&[byte])?;
            }
        }
        out.write_all(b"\"\n")?;
    }
    Ok(())
}

/// Every proper prefix of `input`, from the empty slice up to but not including `input` itself.
pub fn truncations(input: &[u8]) -> impl Iterator<Item = &[u8]> {
    (0..input.len()).map(move |len| &input[..len])
}

/// Copies of `input` with each entry of [`INVALID_UTF8`] inserted at the start, middle and end.
pub fn invalid_utf8_insertions(input: &[u8]) -> Vec<Vec<u8>> {
    let positions = [0, input.len() / 2, input.len()];
    let mut out = Vec::with_capacity(INVALID_UTF8.len() * positions.len());
    for sequence in INVALID_UTF8 {
        for &at in &positions {
            let mut variant = Vec::with_capacity(input.len() + sequence.len());
            variant.extend_from_slice(&input[..at]);
            variant.extend_from_slice(sequence);
            variant.extend_from_slice(&input[at..]);
            out.push(variant);
        }
    }
    out
}

/// Copies of `input` with a 4-byte length field overwritten by an out-of-range value.
///
/// The field is written at every `stride`-th offset, in both little- and big-endian order, using
/// `u32::MAX`, `i32::MAX` and `0x0001_0000`. Those are the values that most often break size
/// checks that use `as usize` or unchecked addition.
///
/// # Panics
///
/// Panics if `stride` is zero.
pub fn out_of_bounds_lengths(input: &[u8], stride: usize) -> Vec<Vec<u8>> {
    assert!(stride > 0, "stride must be non-zero");
    const VALUES: [u32; 3] = [u32::MAX, i32::MAX as u32, 0x0001_0000];
    let mut out = Vec::new();
    for at in (0..input.len().saturating_sub(3)).step_by(stride) {
        for value in VALUES {
            for bytes in [value.to_le_bytes(), value.to_be_bytes()] {
                let mut variant = input.to_vec();
                variant[at..at + 4].copy_from_slice(&bytes);
                out.push(variant);
            }
        }
    }
    out
}

/// All malformed variants of `input`: the empty input, every `stride`-th truncation, invalid
/// UTF-8 insertions, and out-of-bounds length fields.
///
/// The output size is roughly `input.len() / stride` times 24 plus a constant, and each variant
/// is a full copy. Keep seeds to a few kilobytes, or raise `stride`.
///
/// # Panics
///
/// Panics if `stride` is zero.
pub fn malformed_variants(input: &[u8], stride: usize) -> Vec<Vec<u8>> {
    assert!(stride > 0, "stride must be non-zero");
    let mut out = vec![Vec::new()];
    out.extend(truncations(input).step_by(stride).map(<[u8]>::to_vec));
    out.extend(invalid_utf8_insertions(input));
    out.extend(out_of_bounds_lengths(input, stride));
    out
}

/// An input that made the target panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The exact bytes that triggered the panic.
    pub input: Vec<u8>,
    /// The panic message, or a placeholder when the payload is not a string.
    pub message: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "panic on {} byte input: {}",
            self.input.len(),
            self.message
        )
    }
}

/// Runs `target` on every input and collects the ones that panic.
///
/// The target is expected to return normally (with `Ok` or `Err`) for every input. Panics are
/// the only failure this reports.
pub fn check_no_panic<I, F>(inputs: I, target: F) -> Vec<Failure>
where
    I: IntoIterator<Item = Vec<u8>>,
    F: Fn(&[u8]),
{
    inputs
        .into_iter()
        .filter_map(|input| {
            let outcome = panic::catch_unwind(panic::AssertUnwindSafe(|| target(&input)));
            outcome.err().map(|payload| Failure {
                message: panic_message(payload.as_ref()),
                input,
            })
        })
        .collect()
}

/// Runs `target` on every regular file in `dir`, then on [`malformed_variants`] of each one.
///
/// Files are processed in name order so results are reproducible. `stride` is passed to
/// [`malformed_variants`].
///
/// # Errors
///
/// Returns any I/O error from reading the directory or its files.
pub fn check_corpus_dir<F>(
    dir: impl AsRef<Path>,
    stride: usize,
    target: F,
) -> io::Result<Vec<Failure>>
where
    F: Fn(&[u8]),
{
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<io::Result<_>>()?;
    paths.retain(|path| path.is_file());
    paths.sort();

    let mut failures = Vec::new();
    for path in paths {
        let seed = fs::read(&path)?;
        let inputs = std::iter::once(seed.clone()).chain(malformed_variants(&seed, stride));
        failures.extend(check_no_panic(inputs, &target));
    }
    Ok(failures)
}

/// Writes each failing input to `dir` as `crash-<hash>`, where `<hash>` is FNV-1a over the bytes.
///
/// The name depends only on the input, so saving the same crash twice overwrites the same file,
/// and the file can be replayed later with `fs::read` and the same target.
///
/// # Errors
///
/// Returns any I/O error from creating `dir` or writing a file.
pub fn save_failures(failures: &[Failure], dir: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir)?;
    failures
        .iter()
        .map(|failure| {
            let path = dir.join(format!("crash-{:016x}", fnv1a(&failure.input)));
            fs::write(&path, &failure.input)?;
            Ok(path)
        })
        .collect()
}

fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_escapes_quotes_backslashes_and_controls() {
        let mut out = Vec::new();
        write_dictionary(&[b"a\"b\\c\n", b"\xFF"], &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\"a\\x22b\\x5cc\\x0a\"\n\"\\xff\"\n"
        );
    }

    #[test]
    fn builtin_dictionaries_are_non_empty() {
        for dict in [
            JSON_TOKENS,
            PDF_KEYWORDS,
            CSV_TOKENS,
            UNICODE_SCRIPTS,
            BIDI_CONTROLS,
        ] {
            assert!(!dict.is_empty());
        }
    }

    #[test]
    fn truncations_are_proper_prefixes() {
        let got: Vec<&[u8]> = truncations(b"abc").collect();
        assert_eq!(got, vec![&b""[..], &b"a"[..], &b"ab"[..]]);
    }

    #[test]
    fn invalid_utf8_inserts_are_rejected_by_std() {
        for variant in invalid_utf8_insertions(b"hello") {
            // Every variant contains an invalid sequence, so it can never be valid UTF-8.
            assert!(std::str::from_utf8(&variant).is_err());
        }
    }

    #[test]
    fn length_overwrites_cover_both_endians() {
        let variants = out_of_bounds_lengths(&[0_u8; 4], 1);
        assert_eq!(variants.len(), 6);
        assert!(variants.contains(&vec![0xFF, 0xFF, 0xFF, 0xFF]));
        assert!(variants.contains(&vec![0, 0, 0x01, 0x00]));
    }

    #[test]
    fn short_inputs_have_no_length_fields() {
        assert!(out_of_bounds_lengths(&[1, 2, 3], 1).is_empty());
    }

    #[test]
    fn malformed_variants_include_the_empty_input() {
        assert!(malformed_variants(b"seed", 1).contains(&Vec::new()));
    }

    #[test]
    fn check_no_panic_collects_only_panicking_inputs() {
        let failures = check_no_panic(vec![b"ok".to_vec(), b"boom".to_vec()], |data| {
            assert_ne!(data, b"boom", "exploded");
        });
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].input, b"boom");
        assert!(failures[0].message.contains("exploded"));
    }

    #[test]
    fn save_failures_names_by_content_hash() {
        let dir = std::env::temp_dir().join(format!("tpt-fuzz-test-{}", std::process::id()));
        let failure = Failure {
            input: b"crash".to_vec(),
            message: String::new(),
        };
        let paths = save_failures(std::slice::from_ref(&failure), &dir).unwrap();
        assert_eq!(fs::read(&paths[0]).unwrap(), b"crash");
        assert_eq!(
            paths[0].file_name().unwrap().to_str().unwrap(),
            format!("crash-{:016x}", fnv1a(b"crash"))
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
