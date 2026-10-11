#!/usr/bin/env bash
# Fails if tpt-testkit appears in any normal (non-dev, non-build) dependency edge.
# `cargo tree -i` prints the root line plus one line per dependent; with no normal
# dependents it prints only the root, so anything beyond one line is a violation.
set -euo pipefail

cd "$(dirname "$0")/.."

lines=$(cargo tree -e normal -i tpt-testkit --workspace --prefix none --no-dedupe | grep -c . || true)

if [ "$lines" -ne 1 ]; then
    echo "error: tpt-testkit appears in a normal dependency graph:" >&2
    cargo tree -e normal -i tpt-testkit --workspace >&2
    exit 1
fi

echo "ok: tpt-testkit is dev-dependency only"
