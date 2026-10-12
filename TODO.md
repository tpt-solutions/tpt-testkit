# tpt-testkit TODO

Decisions locked in: Cargo workspace · edition 2024 / MSRV 1.97 · MIT OR Apache-2.0 · GitHub Actions · GitHub-only distribution (git deps) · own golden fixture I/O (insta/similar for diffs only) · pure-Rust fuzz engine with sancov feedback · `unsafe` forbidden except in isolated `platform-isolation` feature.

Org: `tpt-solutions` (repository URLs and sibling-crate git deps).

## Phase 0 — Scaffold
- [x] Workspace `Cargo.toml` (resolver 3, shared package metadata, lints)
- [x] Crates: `tpt-testkit`, `tpt-testkit-macros`, `tpt-testkit-cli`
- [x] Feature flags: golden, bench, fuzz, suite, property, platform-isolation
- [x] `cargo check --workspace` passes
- [x] Fill in `<ORG>` once known — `tpt-solutions`
- [x] Add `rust-toolchain.toml`, `rustfmt.toml`, `clippy` config, `deny.toml` (cargo-deny: pure-Rust, license check)
- [x] CI: GitHub Actions for fmt, clippy, test on Linux/macOS/Windows
- [x] CI check that tpt-testkit never appears in a normal (non-dev) dependency graph
- [x] README with dev-dependency usage and workflow

## Phase 1 — Golden (weeks 1–2)
- [x] Fixture path resolution: `tests/fixtures/<name>` relative to caller crate (`CARGO_MANIFEST_DIR`)
- [x] `TPT_UPDATE_GOLDEN=1` create/update behavior; fail with clear message when fixture missing
- [x] Text mode: exact match, LF/CRLF normalization so files are byte-identical on all OSes
- [x] Binary mode: exact match, hex/base64-in-JSON encoding
- [x] Float-tolerant mode: JSON tree walk with epsilon (abs + rel), NaN/Inf/-0.0 handling
- [x] Readable diffs (pretty-assertions / similar)
- [x] `tpt_assert_golden!` macro
- [x] `tpt_assert_golden_float!` macro with `epsilon =`
- [x] Decide insta's role (optional adapter vs. diff-only) and document
- [x] Cross-OS CI test proving byte-for-byte reproducibility
- [x] Unit + integration tests for every mode, including update flow
- [x] Document `tests/fixtures/` convention; migration guide for existing repos
- [ ] Milestone: all existing parser crates have golden coverage — *tpt-doc (hl7v2, edi, fhir, xlsx, docx, ubl) and tpt-data-parsers (all 5) done and merged; tpt-doc-pdf/layout/sign and other org parser repos (tpt-text, tpt-raster, ...) are not covered*

## Phase 2 — Bench and CI (weeks 3–4)
- [x] Criterion wrapper with standard group naming (`area/format/size`) and name validation
- [x] Machine-readable JSON + CSV summary output
- [x] Baseline storage format and location
- [x] CLI `bench-compare` subcommand: fail on >5% regression (threshold configurable, noise-aware)
- [x] Thread pinning (platform-isolation feature, per-OS)
- [x] Cache clearing / warm-up utilities
- [x] GitHub Actions workflow: run benches, compare to baseline, comment/fail on PR
- [x] Tests for compare logic using fixture summaries
- [ ] Milestone: regressions automatically block PRs

## Phase 3 — Fuzz and Suites (weeks 5–7)
### Suite manager (Module D)
- [x] build.rs helper: download, cache (shared cache dir), verify SHA-256, offline mode — `suite::prepare_build`, `suite::fetch_with_curl` (curl, HTTPS only)
- [x] Manifest format for suites (URL, hash, version) — `docs/suite.md`
- [x] Proc-macro test generator: one `#[test]` per file in a suite — `tpt_suite_tests!`
- [ ] Add suites: Unicode BiDi and LineBreak and ICC test profiles are done (`suites/`); PDFBox corpus is blocked: the `pdfbox-testfiles` repo (Google Git and GitHub mirror) has no PDF files at master or in any mirror commit, so there is nothing to pin
- [x] Decide licensing/redistribution handling for downloaded corpora — *default chosen: corpora are never committed or redistributed; manifests record the license and files are fetched into the local cache. Confirm.*
### Fuzz (Module C)
- [x] Harness API (`fuzz_target`-style, 10-line harness) — `fuzz::Engine` plus the template in `docs/fuzz.md`; in-process, no cargo-fuzz wrapper
- [x] Pure-Rust in-process fuzz engine: mutators, corpus, scheduling — *uniform parent scheduling; feature-driven corpus growth*
- [x] Sancov coverage feedback (nightly only for instrumentation) — `fuzz::sancov` (feature `sancov`); *verified only by the `sancov` CI job on Linux/macOS: MSVC has no `__start___sancov_cntrs`/`__stop___sancov_cntrs` section-bound symbols, so Windows MSVC builds do not link with instrumentation (uninstrumented builds work)*
- [x] Corpus management and minimization — `read_corpus` / `save_corpus` / `minimize_corpus`
- [x] Crash artifacts: save, replay, minify to smallest failing input
- [x] CLI: crash dedup against known issues (stack/signature hashing) — `tpt-testkit-cli crash-dedup`; signatures are over panic messages, not stack hashes
- [x] Shared dictionaries: JSON tokens, PDF keywords, Unicode scripts, CSV
- [x] Malformed-input generator: truncation, invalid UTF-8, out-of-bounds lengths, etc.
- [x] Harness templates for crates — `docs/fuzz.md`
- [x] GitHub Actions: continuous / scheduled fuzzing with corpus caching — `.github/workflows/fuzz.yml` (nightly; not yet run on GitHub)
- [x] Milestone: tpt-doc and tpt-data-parsers fuzzing continuously in CI — *merged in both repos (`testkit/` crate + `.github/workflows/testkit.yml`, nightly corpus-cached fuzz); first CI runs were still in progress when recorded*

## Phase 4 — Property (weeks 8–9)
- [x] `strategy::valid_utf8()` — `property::valid_utf8`
- [x] `strategy::ieee754_special_floats()` — `property::ieee754_special_floats`
- [x] `strategy::malformed_pdf_stream()` — `property::malformed_pdf_stream`
- [x] Valid + invalid generators for JSON, CSV, PDF
- [x] Round-trip helper for serialization (tpt-uir, tpt-primitives) — `property::check_round_trip`
- [x] `no_std` support utilities: alloc-backed fixtures, host-side emulators — `nostd::Budget`; the `nostd`-only build is `#![no_std]` and CI checks it on `thumbv7em-none-eabihf` — *no real no_std downstream crate to test against yet*
- [x] Full docs (rustdoc, examples per module), CHANGELOG — `cargo doc -D warnings` passes; runnable examples in bench, suite, property, fuzz, nostd; golden examples are `ignore` (need fixture files)
- [ ] Tag and release v1.0.0 (git tag; GitHub release)

## Ongoing
- [x] Keep every output CI-parseable (JSON) — every CLI command that prints a result supports `--format json`
- [ ] Verify zero production footprint on each release — *`scripts/check-prod-footprint.sh` passes on the current tree*
