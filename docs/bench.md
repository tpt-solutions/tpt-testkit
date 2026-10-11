# Benchmarks

Benchmarks use [criterion](https://docs.rs/criterion) through `tpt_testkit::bench`. The
`tpt-testkit-cli` tool turns criterion's output into summaries and compares them, and CI uses that
to block regressions on pull requests.

## Naming

Every benchmark is named `area/format/size`, for example `parse/json/small`. Each segment is
lowercase and matches `[a-z0-9][a-z0-9_-]*`. Register benchmarks with `bench_named`, which panics on
an invalid name, so CI never sees a name it cannot track.

```rust
use tpt_testkit::bench::bench_named;

fn parse_json(c: &mut Criterion) {
    bench_named(c, "parse/json/small", |b| b.iter(|| parse(black_box(INPUT))));
}
```

## Running

```sh
cargo bench -p your-crate --features tpt-testkit/bench
tpt-testkit-cli bench-summarize --criterion-dir target/criterion --out target/tpt-bench/summary.json
```

`bench-summarize` reads the latest run (`new/`) of every benchmark. It fails if no results exist or
if a name appears twice.

## Summary format

`summary.json` has schema version 1 and is sorted by name:

```json
{ "schema": 1, "benchmarks": [ { "name": "parse/json/small", "mean_ns": 895.8, "std_dev_ns": 27.1 } ] }
```

`--csv <file>` also writes `name,mean_ns,std_dev_ns`. Bump `SCHEMA_VERSION` in the CLI when the
layout changes. The CLI rejects other versions.

## Comparing

```sh
tpt-testkit-cli bench-compare --baseline base.json --current summary.json \
  [--threshold-pct 5] [--noise-sigmas 2] [--format text|markdown|json] [--strict]
```

A benchmark regresses only when **both** hold:

1. Its mean grew by more than `--threshold-pct` (default 5%).
2. The growth exceeds `--noise-sigmas` times the combined standard deviation of the two runs
   (default 2). A noisy benchmark can't fail a PR on one unlucky run.

Benchmarks only in the current run are reported as `new`. Benchmarks only in the baseline are
reported as `missing`, and they fail only with `--strict`.

Exit codes: `0` success, `1` regression (or missing benchmark with `--strict`), `2` usage or I/O
error.

## Baseline

There is no baseline file committed to the repository. The baseline is the **base commit of the
pull request, benchmarked on the same CI runner** as the head. This avoids a stored baseline that
drifts with hardware. `--baseline` accepts any summary file, so a local comparison works too.

## CI

`.github/workflows/bench.yml` runs on every pull request. It benchmarks the base commit and the head
on one runner, compares them, and posts the markdown table as a comment. Forks don't get a write
token, so their comments are skipped and the comparison still runs. A regression fails the job.

## Isolation helpers

- `bench::warm_up(n, f)` runs `f` before measurement.
- `bench::thrash_caches(bytes)` evicts CPU caches by touching a buffer larger than the
  last-level cache. Use it when cold-cache behavior matters.
- `platform::pin_current_thread(core)` (feature `platform-isolation`) pins the calling thread on
  Linux and Windows. macOS has no public API for this and returns `PinError::Unsupported`.
  This is the only module that uses `unsafe`.
