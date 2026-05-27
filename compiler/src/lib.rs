//! solflow_compiler — standalone SOL language compiler.
//!
//! Public surface for the lexer, parser, analyzer, codegen, and
//! supporting utilities. The `api` module provides the
//! result-returning compile pipeline that callers (the CLI here,
//! the WASM bridge in Phase B.4) should prefer over invoking the
//! lower-level modules directly.
//!
//! This crate is intentionally compiler-only — no VM, no host
//! runtime, no network code. See `UPSTREAM.md` for provenance and
//! what was deliberately not vendored.

pub mod lexer;
pub mod parser;
pub mod analyzer;
pub mod bytecode;
pub mod util;

// Phase B.2 additions — diagnostics + result-returning API. Land
// in commits 2 onwards; declared here so the module tree is stable
// from commit 1 and downstream consumers can import either tree.
//
// pub mod diagnostic;
// pub mod api;
