# Compatibility suites

External test vectors (Unicode BiDi, LineBreak, ICC profiles, PDFBox corpus, ...) are described
by a JSON manifest checked into the repository. The files themselves are not committed. They are
fetched into a shared cache and verified against SHA-256 before any test reads them.

## Manifest

```json
{
  "name": "unicode-bidi",
  "version": "16.0.0",
  "license": "Unicode-3.0",
  "files": [
    { "path": "BidiTest.txt", "url": "https://www.unicode.org/Public/16.0.0/ucd/BidiTest.txt", "sha256": "<64 hex>" }
  ]
}
```

Validation rejects: names outside `[a-z0-9][a-z0-9_-]*`, paths that are absolute or contain `..`,
non-`https://` URLs, hashes that are not 64 hex digits, and duplicate paths. Unknown fields are
rejected so typos fail loudly.

## Cache and offline mode

- Location: `TPT_TESTKIT_CACHE` if set; otherwise `%LOCALAPPDATA%\tpt-testkit` on Windows and
  `$XDG_CACHE_HOME/tpt-testkit` (or `~/.cache/tpt-testkit`) elsewhere.
- Layout: `<cache>/<name>/<version>/<path>`. Bumping `version` gives a fresh directory.
- Every use re-hashes the cached file. A mismatch triggers a re-fetch, unless offline.
- `TPT_SUITE_OFFLINE=1` forbids all network access. A missing or stale file is then an error.
- Downloads are verified before they are written, and go through a `.part` file and a rename, so
  a partial download never looks like a cached one.

## Generating tests

```rust
tpt_testkit::suite::tpt_suite_tests!("tests/suites/unicode-bidi.json", check = check_bidi);

fn check_bidi(bytes: &[u8]) {
    // parse `bytes` and assert on the result
}
```

This emits one `#[test]` per manifest entry, named `suite_<path>` with non-alphanumerics mapped to
`_`. Editing the manifest triggers a rebuild, because the expansion includes it via `include_str!`.

## Current limits

- **No download transport yet.** `ensure_file` takes the fetcher as a closure. The generated tests
  use `no_fetch`, which fails with a clear message, so files must already be cached until a
  transport is chosen.
- **No `build.rs` helper yet.** Fetching happens at test runtime, not at build time.
- **Licensing default.** Corpora are never committed or redistributed by this repository. Each
  manifest records its license, and files are fetched on demand into the local cache.
