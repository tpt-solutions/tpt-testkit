# Fuzzing

tpt-testkit's fuzz support has two layers:

1. **In-process engine** (`fuzz` feature, pure Rust, stable toolchain). Deterministic mutation,
   corpus growth driven by features reported by the target, crash minimization, crash dedup, and
   corpus persistence. This is what CI runs.
2. **Malformed-input generator** (`fuzz` feature). Truncations, invalid UTF-8 insertions and
   out-of-bounds length fields for a seed input. Use it on every parser's fixtures.

Coverage-guided fuzzing with sancov edge feedback needs nightly instrumentation and is not
implemented here.

## Harness template

A complete harness for a parser:

```rust
use tpt_testkit::fuzz::{Engine, JSON_TOKENS};

#[test]
fn my_parser_survives_fuzzing() {
    let seeds = tpt_testkit::fuzz::read_corpus("tests/fixtures").unwrap();
    let mut engine = Engine::new(1, seeds).with_dictionary(JSON_TOKENS);
    engine.run(20_000, |data| {
        let _ = my_parser::parse(data); // must return, never panic
        Vec::new()                      // report branch/error IDs here to guide the corpus
    });
    let groups = tpt_testkit::fuzz::dedup(engine.crashes());
    assert!(groups.is_empty(), "{groups:#?}");
}
```

Returning features is what makes the corpus grow. For example, return the error position of a
failed parse, or an ID you set manually at each branch. Without features the engine only mutates
the seeds.

## Crash workflow

1. `dedup(engine.crashes())` groups crashes by normalized panic message. Digits and hex literals
   are collapsed, so the same bug with different indices is one group.
2. `minimize_crash(&group.smallest, target)` shrinks the input to a 1-minimal reproducer.
3. `save_failures(engine.crashes(), dir)` writes each input as `crash-<fnv1a>` for replay with
   `std::fs::read`.
4. `unknown_groups(&groups, &known)` filters out signatures already triaged, so CI reports only
   new crashes.

## Corpus workflow

- `read_corpus(dir)` loads seeds in name order. `save_corpus(engine.corpus(), dir)` writes them
  under content-hash names, so repeated runs do not duplicate entries.
- `minimize_corpus(&inputs, features)` keeps the smallest set of inputs that together cover every
  feature. Run it before committing a corpus.

## Scheduled CI

`.github/workflows/fuzz.yml` runs `tests/fuzz_engine.rs` every night for 500,000 executions. It
is configured by environment variables:

| Variable | Meaning |
|---|---|
| `TPT_FUZZ_ITERATIONS` | Mutate-and-execute steps (default 2000) |
| `TPT_FUZZ_SEED` | RNG seed (default 1) |
| `TPT_FUZZ_CORPUS` | Directory to load extra seeds from and save the corpus to |
| `TPT_FUZZ_CRASHES` | Directory to write crashing inputs to |
| `TPT_FUZZ_REPORT` | Path of a JSON crash report (see below) |

The corpus directory is cached between runs, so coverage accumulates over time. When the job
fails, the workflow runs `tpt-testkit-cli crash-dedup` on the report, uploads the crashing inputs
and the report as the `fuzz-crashes` artifact.

## Crash report and `crash-dedup`

`write_report` writes a schema-1 JSON report of distinct crashes (message plus hex input).
`tpt-testkit-cli` cannot depend on this crate, so it reads the report itself:

```text
tpt-testkit-cli crash-dedup --report target/fuzz-report.json [--known triaged.txt] [--format text|json]
```

- Groups crashes by signature (the normalized message, FNV-1a), with a count and the shortest
  input per group.
- `--known` lists triaged signatures, one hex value per line, with `#` comments allowed. Groups not
  in the list are marked `NEW`.
- Exit code `1` when any group is new, `0` otherwise, `2` on usage or I/O errors.
- `--format json` prints the same data as JSON (`schema: 1`) for other CI tools.

The signature algorithm is duplicated in the library and the CLI. A shared test vector
(`signature("index 1 out of range") == 0xd753b54df3464306`) fails in either place if they drift.

## Dictionaries

`JSON_TOKENS`, `PDF_KEYWORDS`, `CSV_TOKENS`, `UNICODE_SCRIPTS` and `BIDI_CONTROLS` are byte
tokens. `write_dictionary` writes them in libFuzzer/AFL format if you later move to cargo-fuzz.
