# tpt-testkit

Unified testing, benchmarking and fuzzing infrastructure for the TPT ecosystem.

**Dev-dependency only.** tpt-testkit must never appear in a production dependency
graph. CI enforces this with `scripts/check-prod-footprint.sh`.

## Crates

| Crate | Purpose |
| --- | --- |
| `tpt-testkit` | Library: golden files, bench, fuzz, suite and property APIs (feature-gated). |
| `tpt-testkit-macros` | Proc macros, e.g. per-file test generation for compatibility suites. |
| `tpt-testkit-cli` | Command-line tool: `bench-compare`, crash deduplication, corpus tools. |

## Using it in a crate

Add it under `[dev-dependencies]` only:

```toml
[dev-dependencies]
tpt-testkit = { git = "https://github.com/tpt-solutions/tpt-testkit", tag = "v0.1.0" }
```

Feature flags on `tpt-testkit`: `golden` (default), `bench`, `fuzz`, `suite`,
`property`, `nostd`, `platform-isolation`.

## Development

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.97.0).

```sh
cargo check --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
bash scripts/check-prod-footprint.sh
cargo deny check
```

## Conventions

- `unsafe` is forbidden everywhere except the isolated `platform-isolation` feature.
- Golden fixtures live in `tests/fixtures/` in the calling crate (see [docs/golden.md](docs/golden.md)). Regenerate them with
  `TPT_UPDATE_GOLDEN=1`.
- Benchmarks follow the `area/format/size` naming scheme and are compared against the PR base commit (see [docs/bench.md](docs/bench.md)).
- All test and benchmark output must be machine-readable (JSON/CSV) for CI.

## License

Licensed under either of MIT or Apache-2.0, at your option.
