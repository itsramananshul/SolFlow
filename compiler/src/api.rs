//! Result-returning compile pipeline.
//!
//! The public API surface for SolFlow + future WASM bridge.
//! Each stage returns a `CompileResult<T>` carrying either the
//! produced value, a list of `SolDiagnostic`s, or both (warnings
//! alongside a successful result).
//!
//! **B.2 status:** the API surface is final. The underlying
//! lexer/parser/analyzer/codegen calls still `process::exit(1)`
//! on errors — those conversions land in subsequent commits
//! (c3 lexer + parser, c4 analyzer, c5 codegen). After the
//! conversion commits the API will be honest about partial
//! failures.

use crate::analyzer::{Analyzer, TypeTable};
use crate::bytecode::{Codegen, Inst};
use crate::diagnostic::SolDiagnostic;
use crate::lexer::{Lexer, Token};
use crate::parser::{Parser, Program};

/// Result of any compile-pipeline call.
///
/// `value` is present on success and (for warnings only) on partial
/// success. `diagnostics` collects every error/warning/note the
/// stage produced. Use `has_errors()` to gate consumers.
#[derive(Debug)]
pub struct CompileResult<T> {
    pub value: Option<T>,
    pub diagnostics: Vec<SolDiagnostic>,
}

impl<T> CompileResult<T> {
    /// Construct a clean success.
    pub fn ok(value: T) -> Self {
        Self { value: Some(value), diagnostics: Vec::new() }
    }

    /// Construct a hard failure — no value, only diagnostics.
    pub fn err(diagnostics: Vec<SolDiagnostic>) -> Self {
        Self { value: None, diagnostics }
    }

    /// Construct a partial success — value present, diagnostics
    /// (typically warnings) reported alongside.
    pub fn partial(value: T, diagnostics: Vec<SolDiagnostic>) -> Self {
        Self { value: Some(value), diagnostics }
    }

    /// True if any diagnostic has `Error` severity.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == crate::diagnostic::DiagnosticSeverity::Error)
    }

    /// True if every diagnostic is non-error (warnings/notes only)
    /// AND a value is present.
    pub fn ok_with_warnings(&self) -> bool {
        self.value.is_some() && !self.has_errors()
    }
}

/// Successful analyze produces the AST + the populated type-table
/// arena. The arena is needed downstream by the codegen.
#[derive(Debug)]
pub struct AnalyzedProgram {
    pub program: Program,
    pub tt_arena: Vec<TypeTable>,
}

/// Successful compile produces the analyzed program + the emitted
/// bytecode. Bytecode is `Vec<Inst>` — the same shape the upstream
/// VM consumes.
#[derive(Debug)]
pub struct CompiledProgram {
    pub program: Program,
    pub tt_arena: Vec<TypeTable>,
    pub bytecode: Vec<Inst>,
}

// =============================================================
//  Lex
// =============================================================

/// Tokenize a SOL source string. **B.2 baseline** wraps the
/// verbatim `Lexer::from_str` (added below); the only failure mode
/// is the unrecognized-character `process::exit(1)` site at
/// `lexer.rs:298`. That conversion lands in c3.
pub fn lex_source(source: &str) -> CompileResult<Vec<Token>> {
    let mut lexer = Lexer::from_str(source);
    let tokens = lexer.tokens();
    CompileResult::ok(tokens)
}

// =============================================================
//  Parse
// =============================================================

/// Parse a SOL source string into a `Program` (AST). **B.2
/// baseline** wraps the verbatim `Parser::run`; the parser still
/// `process::exit(1)`s on the first error. Converted in c3.
pub fn parse_source(source: &str) -> CompileResult<Program> {
    let mut lexer = Lexer::from_str(source);
    let tokens = lexer.tokens();
    let mut parser = Parser::from(tokens);
    let program = parser.run();
    CompileResult::ok(program)
}

// =============================================================
//  Analyze
// =============================================================

/// Parse + analyze. **B.2 baseline** wraps the verbatim
/// `Analyzer::run`; the analyzer still `process::exit(1)`s on the
/// first error. Converted in c4.
pub fn analyze_source(source: &str) -> CompileResult<AnalyzedProgram> {
    let mut lexer = Lexer::from_str(source);
    let tokens = lexer.tokens();
    let mut parser = Parser::from(tokens);
    let mut program = parser.run();
    let mut analyzer = Analyzer::new();
    analyzer.run(&mut program);
    CompileResult::ok(AnalyzedProgram {
        program,
        tt_arena: analyzer.tt_arena,
    })
}

// =============================================================
//  Compile (full pipeline)
// =============================================================

/// Parse + analyze + code-generate. **B.2 baseline** wraps the
/// verbatim pipeline; the codegen still `process::exit(1)`s on
/// the two `bytecode.rs` error sites. Converted in c5.
pub fn compile_source(source: &str) -> CompileResult<CompiledProgram> {
    let mut lexer = Lexer::from_str(source);
    let tokens = lexer.tokens();
    let mut parser = Parser::from(tokens);
    let mut program = parser.run();
    let mut analyzer = Analyzer::new();
    analyzer.run(&mut program);
    let tt_arena_for_codegen = analyzer.tt_arena.clone();
    let mut codegen = Codegen::from(analyzer.tt_arena);
    let bytecode = codegen.gen_bcode(&program);
    CompileResult::ok(CompiledProgram {
        program,
        tt_arena: tt_arena_for_codegen,
        bytecode,
    })
}
