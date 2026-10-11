# Golden files

Golden tests compare a test's output with a fixture file that is committed to the repository.
When a test fails, run `diff` on the fixture to see what changed. Developers should not have to
hand-write `assert_eq!` against large expected values.

## Convention: `tests/fixtures/`

- Fixtures live in `tests/fixtures/` of the crate that owns the test. Path resolution uses that
  crate's `CARGO_MANIFEST_DIR`, so `tpt_assert_golden!(x, "layout.expected.txt")` reads
  `<crate>/tests/fixtures/layout.expected.txt`. Subdirectories are allowed.
- Name fixtures `<test>.expected.<ext>`, for example `parse_json.expected.json`.
- Fixtures are committed. `.gitattributes` forces LF line endings for `tests/fixtures/**`.
- Never edit a fixture by hand unless you are reviewing it. Regenerate it instead.

## Creating and updating fixtures

```sh
TPT_UPDATE_GOLDEN=1 cargo test -p your-crate
git diff -- tests/fixtures/   # review every change before committing
```

Without `TPT_UPDATE_GOLDEN=1`, a missing fixture fails with a message that says how to create it.
A mismatch fails with a unified diff (text, binary) or a list of mismatched paths (float). The
message also says how to accept the new output.

## Modes

| Macro | Actual value | Fixture format | Comparison |
| --- | --- | --- | --- |
| `tpt_assert_golden!(actual, name)` | `AsRef<str>` | Text | Exact after CRLF to LF normalization |
| `tpt_assert_golden_bytes!(actual, name)` | `AsRef<[u8]>` | JSON `{"encoding":"hex","len":N,"data":[...]}`, 32 bytes per line | Exact byte match |
| `tpt_assert_golden_float!(actual, name, epsilon = e)` | `GoldenFloat` | Pretty JSON | Per-value tolerance |

### Float tolerance

A number matches when `|a - b| <= epsilon` or `|a - b| <= epsilon * max(|a|, |b|)`. This is the
absolute-plus-relative bound.

Special values have no JSON representation, so they are stored as strings:

- `NaN` matches `NaN`.
- `Infinity` and `-Infinity` must match their sign exactly.
- `-0.0` and `0.0` are equal, since the sign of zero is not meaningful for these tests.

`GoldenFloat` is implemented for `f32`, `f64`, slices, arrays, `Vec` and references. For your own
types, build the tree with `float_value` for each float and return a `serde_json::Value`.

## Migration guide for an existing repo

1. Add `tpt-testkit` under `[dev-dependencies]` only (see the README).
2. Find the hand-written `assert_eq!` checks against large or structured expected values.
3. Replace each one with a golden macro. Put the expected value in `tests/fixtures/`.
4. Run `TPT_UPDATE_GOLDEN=1 cargo test`. Review the generated fixtures against the old expected
   values. They should match what the old assertions required.
5. Delete the old inline expected values and commit the fixtures.
6. For floating-point output, use `tpt_assert_golden_float!` with the smallest epsilon that
   passes on every platform. Do not use a larger value to hide a regression.

## Why not insta

`insta` is the standard Rust snapshot library, and the design lists it as the golden backend.
TPT-Testkit implements its own fixture I/O instead. The reasons are:

- Fixtures must be byte-identical across Linux, macOS and Windows, and the normalization rules
  need to be explicit and tested.
- The float-tolerant mode needs a JSON tree walk with per-value epsilon, which `insta` does not
  provide.
- `similar` is used for diffs, which is the part of `insta` that is useful here.

An `insta` adapter could be added later if it is needed. It is not part of Phase 1.
