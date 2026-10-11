# tpt-testkit TODO

Decisions locked in: Cargo workspace · edition 2024 / MSRV 1.97 · MIT OR Apache-2.0 · GitHub Actions · GitHub-only distribution (git deps) · own golden fixture I/O (insta/similar for diffs only) · pure-Rust fuzz engine with sancov feedback · `unsafe` forbidden except in isolated `platform-isolation` feature.

Open item: GitHub org name for `repository` URLs and sibling-crate git deps (currently `<ORG>` placeholder).

## Phase 0 — Scaffold
- [x] Workspace `Cargo.toml` (resolver 3, shared package metadata, lints)
- [x] Crates: `tpt-testkit`, `tpt-testkit-macros`, `tpt-testkit-cli`, `tpt-testkit-integration`
- [x] Feature flags: golden, bench, fuzz, suite, property, platform-isolation, workspace-integration
- [x] `cargo check --workspace` passes
- [ ] Fill in `<ORG>` once known
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
- [ ] Milestone: all existing parser crates have golden coverage

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
- [ ] build.rs helper: download, cache (shared cache dir), verify SHA-256, offline mode — *cache, SHA-256 verification and offline mode are done in `suite`; the download transport and `build.rs` wiring are pending (see docs/suite.md)*
- [x] Manifest format for suites (URL, hash, version) — `docs/suite.md`
- [x] Proc-macro test generator: one `#[test]` per file in a suite — `tpt_suite_tests!`
- [ ] Add suites: Unicode BiDi, Unicode LineBreak, ICC profiles, PDFBox corpus
- [x] Decide licensing/redistribution handling for downloaded corpora — *default chosen: corpora are never committed or redistributed; manifests record the license and files are fetched into the local cache. Confirm.*
### Fuzz (Module C)
- [ ] Harness API (`fuzz_target`-style, 10-line harness) — *in-process `check_no_panic` / `check_corpus_dir` exist; a cargo-fuzz wrapper is not built*
- [ ] Pure-Rust in-process fuzz engine: mutators, corpus, scheduling
- [ ] Sancov coverage feedback (nightly only for instrumentation; runs on Windows/Linux/macOS)
- [ ] Corpus management and minimization
- [ ] Crash artifacts: save, replay, minify to smallest failing input — *save (content-hash names) and replay are done; minimization is not*
- [ ] CLI: crash dedup against known issues (stack/signature hashing)
- [x] Shared dictionaries: JSON tokens, PDF keywords, Unicode scripts, CSV
- [x] Malformed-input generator: truncation, invalid UTF-8, out-of-bounds lengths, etc.
- [ ] Harness templates for crates
- [ ] GitHub Actions: continuous / scheduled fuzzing with corpus caching
- [ ] Milestone: tpt-doc and tpt-data-parsers fuzzing continuously in CI

## Phase 4 — Property and Integration (weeks 8–9)
- [x] `strategy::valid_utf8()` — `property::valid_utf8`
- [x] `strategy::ieee754_special_floats()` — `property::ieee754_special_floats`
- [x] `strategy::malformed_pdf_stream()` — `property::malformed_pdf_stream`
- [x] Valid + invalid generators for JSON, CSV, PDF
- [x] Round-trip helper for serialization (tpt-uir, tpt-primitives) — `property::check_round_trip`
- [ ] `no_std` support utilities: alloc-backed fixtures, host-side emulators
- [ ] Config file listing top-20 critical crates (git URL, rev/tag)
- [ ] `workspace-integration` mega-test binary + smoke suite
- [ ] CI job running the mega-test against pinned sibling revs
- [ ] Full docs (rustdoc, examples per module), CHANGELOG — *CHANGELOG started; rustdoc examples for golden/bench/fuzz/property are incomplete*
- [ ] Tag and release v1.0.0 (git tag; GitHub release)

## Ongoing
- [ ] Keep every output CI-parseable (JSON)
- [ ] Verify zero production footprint on each release — *`scripts/check-prod-footprint.sh` passes on the current tree*
