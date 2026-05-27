#!/usr/bin/env bash
# Build the WASM bundle that the SolFlow editor consumes.
#
# Output lands in `compiler-wasm/pkg/`. That directory IS committed
# so editor developers don't need a Rust toolchain to run the dev
# server. Rebuild whenever you change Rust code in `compiler/` or
# `compiler-wasm/`.
set -euo pipefail
cd "$(dirname "$0")"
wasm-pack build --release --target bundler --out-dir pkg
echo
echo "Built pkg/. Bundle size:"
ls -lh pkg/solflow_compiler_wasm_bg.wasm
