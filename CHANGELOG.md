# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- `property` feature: proptest strategies `valid_utf8`, `ieee754_special_floats`, `valid_json`,
  `invalid_json`, `valid_csv`, `invalid_csv`, `valid_pdf_stream`, `malformed_pdf_stream`, and the
  `check_round_trip` helper.
- `fuzz` feature: shared dictionaries (JSON, PDF, CSV, Unicode scripts, bidi controls), a
  libFuzzer-format dictionary writer, a malformed-input generator (truncation, invalid UTF-8,
  out-of-bounds lengths), an in-process no-panic harness, and crash saving with content-hash names.
- `suite` feature: manifest parsing and validation, SHA-256 verification, shared cache, offline
  mode (`TPT_SUITE_OFFLINE=1`), and the `tpt_suite_tests!` macro that generates one test per file.
  Downloading is not implemented yet.
