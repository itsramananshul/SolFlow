//! Result-returning compile pipeline.
//!
//! The public API surface for SolFlow + future WASM bridge.
//! Each stage returns a `CompileResult<T>` carrying either the
//! produced value, a list of `SolDiagnostic`s, or both (warnings
//! alongside a successful result).
//!
//! **B.2 status:** the API surface is final. Lexer, parser,
//! analyzer, and codegen all return `SolDiagnostic` values
//! (c3 + c4 + c5). Every compile-time error path in the crate is
//! now a value, not a `process::exit(1)`.

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

/// Tokenize a SOL source string. Lex errors (e.g. unrecognized
/// characters) are returned as diagnostics; the lexer skips bad
/// characters and continues so callers see every lex error in one
/// pass.
pub fn lex_source(source: &str) -> CompileResult<Vec<Token>> {
    let mut lexer = Lexer::from_str(source);
    let tokens = lexer.tokens();
    let diags = std::mem::take(&mut lexer.diagnostics);
    if diags.is_empty() {
        CompileResult::ok(tokens)
    } else {
        CompileResult::partial(tokens, diags)
    }
}

// =============================================================
//  Parse
// =============================================================

/// Parse a SOL source string into a `Program` (AST). Lex and
/// parse errors are returned as diagnostics; the parser uses
/// panic-mode recovery (sync to next top-level keyword) so a
/// single error doesn't hide the rest of the file's structure.
pub fn parse_source(source: &str) -> CompileResult<Program> {
    let mut lexer = Lexer::from_str(source);
    let tokens = lexer.tokens();
    let mut diagnostics = std::mem::take(&mut lexer.diagnostics);

    let mut parser = Parser::from(tokens);
    let program = parser.run();
    diagnostics.append(&mut parser.diagnostics);

    if diagnostics.is_empty() {
        CompileResult::ok(program)
    } else {
        CompileResult::partial(program, diagnostics)
    }
}

// =============================================================
//  Analyze
// =============================================================

/// Parse + analyze. Lex / parse / semantic diagnostics propagate.
/// The analyzer reports every semantic error it encounters (no
/// longer aborts on the first one); if any errors are present after
/// analysis, no `AnalyzedProgram` is returned.
pub fn analyze_source(source: &str) -> CompileResult<AnalyzedProgram> {
    let mut lexer = Lexer::from_str(source);
    let tokens = lexer.tokens();
    let mut diagnostics = std::mem::take(&mut lexer.diagnostics);

    let mut parser = Parser::from(tokens);
    let mut program = parser.run();
    diagnostics.append(&mut parser.diagnostics);

    // If parse errors fired, stop here — running the analyzer on a
    // partially-recovered AST is more likely to cascade than help.
    if diagnostics
        .iter()
        .any(|d| d.severity == crate::diagnostic::DiagnosticSeverity::Error)
    {
        return CompileResult::err(diagnostics);
    }

    let mut analyzer = Analyzer::new();
    analyzer.run(&mut program);
    diagnostics.append(&mut analyzer.diagnostics);

    let has_errors = diagnostics
        .iter()
        .any(|d| d.severity == crate::diagnostic::DiagnosticSeverity::Error);
    if has_errors {
        return CompileResult::err(diagnostics);
    }

    let result = AnalyzedProgram {
        program,
        tt_arena: analyzer.tt_arena,
    };
    if diagnostics.is_empty() {
        CompileResult::ok(result)
    } else {
        CompileResult::partial(result, diagnostics)
    }
}

// =============================================================
//  Compile (full pipeline)
// =============================================================

/// Parse + analyze + code-generate. Errors from any stage are
/// collected; a hard error at any stage short-circuits and the
/// `CompileResult` is returned without a value.
pub fn compile_source(source: &str) -> CompileResult<CompiledProgram> {
    let mut lexer = Lexer::from_str(source);
    let tokens = lexer.tokens();
    let mut diagnostics = std::mem::take(&mut lexer.diagnostics);

    let mut parser = Parser::from(tokens);
    let mut program = parser.run();
    diagnostics.append(&mut parser.diagnostics);

    if diagnostics
        .iter()
        .any(|d| d.severity == crate::diagnostic::DiagnosticSeverity::Error)
    {
        return CompileResult::err(diagnostics);
    }

    let mut analyzer = Analyzer::new();
    analyzer.run(&mut program);
    diagnostics.append(&mut analyzer.diagnostics);

    if diagnostics
        .iter()
        .any(|d| d.severity == crate::diagnostic::DiagnosticSeverity::Error)
    {
        return CompileResult::err(diagnostics);
    }

    let tt_arena_for_codegen = analyzer.tt_arena.clone();
    let mut codegen = Codegen::from(analyzer.tt_arena);
    let bytecode = codegen.gen_bcode(&program);
    diagnostics.append(&mut codegen.diagnostics);

    if diagnostics
        .iter()
        .any(|d| d.severity == crate::diagnostic::DiagnosticSeverity::Error)
    {
        return CompileResult::err(diagnostics);
    }

    let result = CompiledProgram {
        program,
        tt_arena: tt_arena_for_codegen,
        bytecode,
    };
    if diagnostics.is_empty() {
        CompileResult::ok(result)
    } else {
        CompileResult::partial(result, diagnostics)
    }
}
