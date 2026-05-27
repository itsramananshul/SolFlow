#!/usr/bin/env bash
# Regenerate the AST JSON fixtures the importer's vitest suite
# loads. Run after every change to the AST shape, the importer,
# or the compiler's serde output.
#
# Usage:  npm run regen:fixtures
#         (or: bash scripts/regenerate-ast-fixtures.sh)
#
# Why a script + commit: the importer tests run as pure Node (no
# WASM in the test runtime), so we ship pre-generated AST JSON
# alongside each .sol fixture. This script + committed JSON keeps
# CI deterministic and developer-friendly.

set -euo pipefail
cd "$(dirname "$0")/.."

FIXDIR="src/graph/import/__fixtures__"
if [ ! -d "$FIXDIR" ]; then
  echo "fixtures dir not found: $FIXDIR" >&2
  exit 1
fi

count=0
for sol in "$FIXDIR"/*.sol; do
  base=$(basename "$sol" .sol)
  out="$FIXDIR/${base}.ast.json"
  echo "  $sol -> $out"
  cargo run --quiet -p solflow_compiler_wasm --example dump_ast -- "$sol" > "$out"
  count=$((count + 1))
done

echo
echo "Regenerated $count fixture(s). Review the diff + commit:"
git diff --stat -- "$FIXDIR"
