#![allow(missing_docs)] // criterion_group! generates an undocumented entry point.

//! Benchmarks for tpt-testkit itself. Also exercises the bench pipeline end to end in CI.

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use tpt_testkit::bench::bench_named;
use tpt_testkit::golden::normalize_newlines;

fn configure() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(1))
        .sample_size(20)
}

fn golden_text(c: &mut Criterion) {
    let small = "line\r\n".repeat(16);
    let large = "line\r\n".repeat(16 * 1024);
    bench_named(c, "golden/text/small", |b| {
        b.iter(|| normalize_newlines(black_box(&small)));
    });
    bench_named(c, "golden/text/large", |b| {
        b.iter(|| normalize_newlines(black_box(&large)));
    });
}

criterion_group! {
    name = benches;
    config = configure();
    targets = golden_text
}
criterion_main!(benches);
