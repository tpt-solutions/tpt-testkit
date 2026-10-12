//! Sancov feedback test. Meaningful only when built with the instrumentation flags documented in
//! `tpt_testkit::fuzz::sancov`; otherwise it checks the uninstrumented fallback.

#![cfg(feature = "sancov")]

use tpt_testkit::fuzz::{Engine, sancov};

fn parse(data: &[u8]) {
    if data.first() == Some(&b'A') {
        if data.get(1) == Some(&b'B') {
            if data.get(2) == Some(&b'C') {
                std::hint::black_box(3);
            }
            std::hint::black_box(2);
        }
        std::hint::black_box(1);
    }
}

#[test]
fn deeper_inputs_exercise_more_edges() {
    let shallow = sancov::trace(|| parse(b"x")).1;
    let deep = sancov::trace(|| parse(b"ABC")).1;
    if sancov::is_instrumented() {
        assert!(deep.len() > shallow.len(), "{deep:?} vs {shallow:?}");
    } else {
        assert!(shallow.is_empty() && deep.is_empty());
    }
}

#[test]
fn engine_grows_corpus_from_edge_coverage() {
    if !sancov::is_instrumented() {
        eprintln!("not instrumented; skipping coverage-guided assertions");
        return;
    }
    let mut engine = Engine::new(5, vec![b"A".to_vec()]);
    engine.run(5_000, |data| sancov::trace(|| parse(data)).1);
    assert!(engine.features_seen() > 2);
    assert!(engine.corpus().len() > 1);
}
