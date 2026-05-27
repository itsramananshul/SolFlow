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
pub mod diagnostic;
pub mod api;

// Convenience re-exports for the most-used public types. Callers
// can `use solflow_compiler::{compile_source, SolDiagnostic};`
// without having to remember which module each type lives in.
pub use api::{
    AnalyzedProgram, CompileResult, CompiledProgram, analyze_source, compile_source, lex_source,
    parse_source,
};
pub use diagnostic::{
    DiagnosticPhase, DiagnosticSeverity, RelatedSpan, SolDiagnostic, SourceSpan, codes,
    format_diagnostic,
};
