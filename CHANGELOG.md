# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- `sancov` feature: `fuzz::sancov` edge-coverage feedback from nightly inline-8bit-counter
  instrumentation (Linux/macOS; CI-verified only).
- `nostd` feature builds as `#![no_std]` (core + alloc) when enabled alone; CI checks it on
  `thumbv7em-none-eabihf`.
- CLI: `bench-summarize --format json`.
- `property` feature: proptest strategies `valid_utf8`, `ieee754_special_floats`, `valid_json`,
  `invalid_json`, `valid_csv`, `invalid_csv`, `valid_pdf_stream`, `malformed_pdf_stream`, and the
  `check_round_trip` helper.
- `fuzz` feature: shared dictionaries (JSON, PDF, CSV, Unicode scripts, bidi controls), a
  libFuzzer-format dictionary writer, a malformed-input generator (truncation, invalid UTF-8,
  out-of-bounds lengths), an in-process no-panic harness, and crash saving with content-hash names.
- `suite` feature: manifest parsing and validation, SHA-256 verification, shared cache, offline
  mode (`TPT_SUITE_OFFLINE=1`), and the `tpt_suite_tests!` macro that generates one test per file.
  Downloading is not implemented yet.
- Fuzz engine: deterministic in-process mutation loop (`fuzz::Engine`) with feature-driven corpus
  growth, `minimize` / `minimize_crash` (1-minimal shrinking), `dedup` / `unknown_groups` (crash
  signatures that ignore digits and hex), `minimize_corpus`, and `read_corpus` / `save_corpus`.
  Scheduled workflow `.github/workflows/fuzz.yml` runs it nightly with a persisted corpus.
- `tpt-testkit-cli crash-dedup`: groups a JSON crash report by signature and fails on groups not
  in a known-signature list. The engine integration test writes the report via `TPT_FUZZ_REPORT`.
- Suite downloads via the system `curl` (`suite::fetch_with_curl`, HTTPS only), a `build.rs`
  helper (`suite::prepare_build`), and manifests for Unicode 17.0.0 BidiTest and LineBreakTest
  under `suites/`, plus an ICC manifest (`PRMG_RGB-sRGB_based.icc` and the `Probev2.zip` probe archive).
- `nostd` feature: `nostd::Budget`, a host-side fixed-heap emulator whose `alloc_vec` fails with
  `OutOfBudget` instead of aborting, for testing `no_std` + `alloc` failure paths.
