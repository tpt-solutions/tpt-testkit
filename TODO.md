# tpt-testkit TODO

Decisions locked in: Cargo workspace · edition 2024 / MSRV 1.97 · MIT OR Apache-2.0 · GitHub Actions · GitHub-only distribution (git deps) · own golden fixture I/O (insta/similar for diffs only) · pure-Rust fuzz engine with sancov feedback · `unsafe` forbidden except in isolated `platform-isolation` feature.

Open item: GitHub org name for `repository` URLs and sibling-crate git deps (currently `<ORG>` placeholder).

## Phase 0 — Scaffold
- [x] Workspace `Cargo.toml` (resolver 3, shared package metadata, lints)
- [x] Crates: `tpt-testkit`, `tpt-testkit-macros`, `tpt-testkit-cli`, `tpt-testkit-integration`
- [x] Feature flags: golden, bench, fuzz, suite, property, platform-isolation, workspace-integration
- [x] `cargo check --workspace` passes
- [ ] Fill in `<ORG>` once known
- [ ] Add `rust-toolchain.toml`, `rustfmt.toml`, `clippy` config, `deny.toml` (cargo-deny: pure-Rust, license check)
- [ ] CI: GitHub Actions for fmt, clippy, test on Linux/macOS/Windows
- [ ] CI check that tpt-testkit never appears in a normal (non-dev) dependency graph
- [ ] README with dev-dependency usage and workflow

## Phase 1 — Golden (weeks 1–2)
- [ ] Fixture path resolution: `tests/fixtures/<name>` relative to caller crate (`CARGO_MANIFEST_DIR`)
- [ ] `TPT_UPDATE_GOLDEN=1` create/update behavior; fail with clear message when fixture missing
- [ ] Text mode: exact match, LF/CRLF normalization so files are byte-identical on all OSes
- [ ] Binary mode: exact match, hex/base64-in-JSON encoding
- [ ] Float-tolerant mode: JSON tree walk with epsilon (abs + rel), NaN/Inf/-0.0 handling
- [ ] Readable diffs (pretty-assertions / similar)
- [ ] `tpt_assert_golden!` macro
- [ ] `tpt_assert_golden_float!` macro with `epsilon =`
- [ ] Decide insta's role (optional adapter vs. diff-only) and document
- [ ] Cross-OS CI test proving byte-for-byte reproducibility
- [ ] Unit + integration tests for every mode, including update flow
- [ ] Document `tests/fixtures/` convention; migration guide for existing repos
- [ ] Milestone: all existing parser crates have golden coverage

## Phase 2 — Bench and CI (weeks 3–4)
- [ ] Criterion wrapper with standard group naming (`area/format/size`) and name validation
- [ ] Machine-readable JSON + CSV summary output
- [ ] Baseline storage format and location
- [ ] CLI `bench-compare` subcommand: fail on >5% regression (threshold configurable, noise-aware)
- [ ] Thread pinning (platform-isolation feature, per-OS)
- [ ] Cache clearing / warm-up utilities
- [ ] GitHub Actions workflow: run benches, compare to baseline, comment/fail on PR
- [ ] Tests for compare logic using fixture summaries
- [ ] Milestone: regressions automatically block PRs

## Phase 3 — Fuzz and Suites (weeks 5–7)
### Suite manager (Module D)
- [ ] build.rs helper: download, cache (shared cache dir), verify SHA-256, offline mode
- [ ] Manifest format for suites (URL, hash, version)
- [ ] Proc-macro test generator: one `#[test]` per file in a suite
- [ ] Add suites: Unicode BiDi, Unicode LineBreak, ICC profiles, PDFBox corpus
- [ ] Decide licensing/redistribution handling for downloaded corpora
### Fuzz (Module C)
- [ ] Harness API (`fuzz_target`-style, 10-line harness)
- [ ] Pure-Rust in-process fuzz engine: mutators, corpus, scheduling
- [ ] Sancov coverage feedback (nightly only for instrumentation; runs on Windows/Linux/macOS)
- [ ] Corpus management and minimization
- [ ] Crash artifacts: save, replay, minify to smallest failing input
- [ ] CLI: crash dedup against known issues (stack/signature hashing)
- [ ] Shared dictionaries: JSON tokens, PDF keywords, Unicode scripts, CSV
- [ ] Malformed-input generator: truncation, invalid UTF-8, out-of-bounds lengths, etc.
- [ ] Harness templates for crates
- [ ] GitHub Actions: continuous / scheduled fuzzing with corpus caching
- [ ] Milestone: tpt-doc and tpt-data-parsers fuzzing continuously in CI

## Phase 4 — Property and Integration (weeks 8–9)
- [ ] `strategy::valid_utf8()`
- [ ] `strategy::ieee754_special_floats()`
- [ ] `strategy::malformed_pdf_stream()`
- [ ] Valid + invalid generators for JSON, CSV, PDF
- [ ] Round-trip helper for serialization (tpt-uir, tpt-primitives)
- [ ] `no_std` support utilities: alloc-backed fixtures, host-side emulators
- [ ] Config file listing top-20 critical crates (git URL, rev/tag)
- [ ] `workspace-integration` mega-test binary + smoke suite
- [ ] CI job running the mega-test against pinned sibling revs
- [ ] Full docs (rustdoc, examples per module), CHANGELOG
- [ ] Tag and release v1.0.0 (git tag; GitHub release)

## Ongoing
- [ ] Keep every output CI-parseable (JSON)
- [ ] Verify zero production footprint on each release
