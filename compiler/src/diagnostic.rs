//! Structured diagnostics for the SOL compiler.
//!
//! Replaces the upstream pattern of `eprintln! + std::process::exit(1)`
//! with first-class values that can be collected, returned, and
//! rendered by any consumer (CLI, WASM bridge, tests).
//!
//! Codes follow the provisional scheme in
//! `docs/sol-language/ERROR_REFERENCE.md`:
//!
//!   - `E0xxx` — parse errors (lexer + parser)
//!   - `E1xxx` — semantic errors (analyzer)
//!   - `E2xxx` — runtime errors (VM — out of B.1+B.2 scope)
//!   - `Wxxxx` — warnings (any phase)
//!
//! Source spans are defined here but not yet plumbed through every
//! production; for B.1+B.2 the `span` field is `None` on every
//! diagnostic except a few easy cases. Adding spans through the
//! lexer + parser is its own future commit batch.

use std::fmt;

/// Severity tier for a diagnostic. The CLI treats `Error` as
/// exit-nonzero; the IDE treats it as red underline; everyone
/// treats `Warning` as amber and `Note` as informational.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Note,
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticSeverity::Error => write!(f, "error"),
            DiagnosticSeverity::Warning => write!(f, "warning"),
            DiagnosticSeverity::Note => write!(f, "note"),
        }
    }
}

/// Which pipeline stage produced the diagnostic. Lets the IDE
/// group + filter (e.g. "show me only the parse errors") and lets
/// the CLI prefix output cleanly.
///
/// `Internal` is special — it marks an *internal compiler error*
/// (ICE). User code did not cause it; the compiler itself is at
/// fault (e.g. an analyzer arm reached an AST shape it doesn't
/// yet handle). The CLI and editor render ICEs differently from
/// user errors so the bug-report-vs-fix-your-code distinction is
/// visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiagnosticPhase {
    Lexer,
    Parser,
    Analyzer,
    Codegen,
    Runtime,
    Internal,
}

impl fmt::Display for DiagnosticPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticPhase::Lexer => write!(f, "lexer"),
            DiagnosticPhase::Parser => write!(f, "parser"),
            DiagnosticPhase::Analyzer => write!(f, "analyzer"),
            DiagnosticPhase::Codegen => write!(f, "codegen"),
            DiagnosticPhase::Runtime => write!(f, "runtime"),
            DiagnosticPhase::Internal => write!(f, "internal compiler error"),
        }
    }
}

/// 0-indexed byte range into the source string. Exclusive end.
/// Stable across line/column derivation (line/column depends on
/// source content; byte offsets do not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Derive a (line, column) pair from a 0-indexed byte offset.
    /// Both line and column are 1-indexed for human display.
    /// Returns the start position of this span. O(start) walk.
    pub fn to_line_col(&self, source: &str) -> (usize, usize) {
        let mut line: usize = 1;
        let mut col: usize = 1;
        for (idx, ch) in source.char_indices() {
            if idx >= self.start {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Borrow the slice of `source` this span covers. Returns None
    /// if the span is out of bounds (which would indicate a bug).
    pub fn slice<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.start..self.end)
    }
}

/// Secondary span pointing at related context — typically a
/// "previous definition was here" note for a redefinition error.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RelatedSpan {
    pub span: SourceSpan,
    pub message: String,
}

/// A diagnostic produced by any pipeline stage.
///
/// Field-order matches the natural display order: severity,
/// phase, code, message, then optional span + related notes +
/// help suggestion.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SolDiagnostic {
    pub severity: DiagnosticSeverity,
    pub phase: DiagnosticPhase,
    /// Stable error code (e.g. `"E1001"`, `"ICE0001"`). Stored as
    /// `String` rather than `&'static str` so the diagnostic can
    /// round-trip across serde / WASM boundaries. Constructors take
    /// `&'static str` for ergonomics and incur a single small
    /// allocation per diagnostic — negligible cost on an error path.
    pub code: String,
    pub message: String,
    pub span: Option<SourceSpan>,
    pub related: Vec<RelatedSpan>,
    pub help: Option<String>,
}

impl SolDiagnostic {
    /// Shorthand for an error diagnostic without span or help.
    pub fn error(phase: DiagnosticPhase, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            phase,
            code: code.to_string(),
            message: message.into(),
            span: None,
            related: Vec::new(),
            help: None,
        }
    }

    /// Shorthand for a warning diagnostic without span or help.
    pub fn warning(phase: DiagnosticPhase, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            phase,
            code: code.to_string(),
            message: message.into(),
            span: None,
            related: Vec::new(),
            help: None,
        }
    }

    /// Shorthand for an internal compiler error. Always tagged with
    /// the `Internal` phase and `Error` severity, with a built-in
    /// help string instructing the user to file a bug. Use this when
    /// the compiler reaches a state it can't reason about (e.g. an
    /// AST shape an analyzer arm doesn't handle), not for *user*
    /// errors — those go through `error()`.
    pub fn internal(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            phase: DiagnosticPhase::Internal,
            code: code.to_string(),
            message: message.into(),
            span: None,
            related: Vec::new(),
            help: Some("this is a bug in solflow_compiler; please report it".to_string()),
        }
    }

    /// Attach a source span. Builder-style.
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Attach a related-source note. Builder-style; chainable.
    pub fn with_related(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.related.push(RelatedSpan { span, message: message.into() });
        self
    }

    /// Attach a help string with a fix suggestion. Builder-style.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// Format a diagnostic in a `cargo`-style block:
///
/// ```text
/// error[E0001]: missing initializer in `let` declaration
///   --> path/to/file.sol:5:14
///     |
///   5 |     let x: int = ;
///     |              ^^
///     = help: provide an initializer or drop the `=`
/// ```
///
/// For now (B.2 baseline) we render a simplified single-line form
/// because spans aren't yet plumbed through every production:
///
/// ```text
/// error[E0001] parser: missing initializer in `let` declaration
/// ```
///
/// Once spans land everywhere, the second form upgrades to the
/// first; the API doesn't change.
pub fn format_diagnostic(d: &SolDiagnostic, source: Option<&str>, source_label: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}[{}] {}: {}", d.severity, d.code, d.phase, d.message));
    if let Some(span) = d.span {
        if let Some(src) = source {
            let (line, col) = span.to_line_col(src);
            let label = source_label.unwrap_or("<source>");
            out.push('\n');
            out.push_str(&format!("  --> {label}:{line}:{col}"));
        }
    }
    for r in &d.related {
        out.push('\n');
        if let (Some(src), Some(label)) = (source, source_label) {
            let (line, col) = r.span.to_line_col(src);
            out.push_str(&format!("  note: {} ({}:{}:{})", r.message, label, line, col));
        } else {
            out.push_str(&format!("  note: {}", r.message));
        }
    }
    if let Some(h) = &d.help {
        out.push('\n');
        out.push_str(&format!("  help: {h}"));
    }
    out
}

// =============================================================
//  Diagnostic codes — provisional scheme
// =============================================================
//
// Stable string constants. Centralized so the parser/analyzer can
// reference them by name rather than typing the code literal at
// each emit site. New entries are added here first, then referenced.
//
// The same codes appear in `docs/sol-language/ERROR_REFERENCE.md`;
// any rename here MUST update that file in lockstep.

pub mod codes {
    // ---------- Parse errors (E0xxx) ----------
    pub const PARSE_LEX_BAD_CHAR:                &str = "E0008";

    pub const PARSE_EMPTY_INITIALIZER:           &str = "E0001";
    pub const PARSE_MISSING_SEMI:                &str = "E0002";
    pub const PARSE_UNKNOWN_DECLARATION:         &str = "E0003";
    pub const PARSE_EXT_NOT_FUNCTION:            &str = "E0004";
    pub const PARSE_MISSING_DELIMITER:           &str = "E0005";  // missing }, ), ], etc.
    pub const PARSE_BAD_ARRAY_SIZE:              &str = "E0006";
    pub const PARSE_INVALID_TYPE:                &str = "E0007";
    pub const PARSE_NOT_EXPRESSION:              &str = "E0009";
    pub const PARSE_BAD_NAME:                    &str = "E0010";  // identifier expected
    pub const PARSE_BAD_FIELD:                   &str = "E0011";
    pub const PARSE_BAD_PARAM:                   &str = "E0012";
    pub const PARSE_BAD_MEMBER:                  &str = "E0013";
    pub const PARSE_BAD_ENUM_VALUE:              &str = "E0014";
    pub const PARSE_BAD_IMPORT:                  &str = "E0015";
    pub const PARSE_BAD_STATEMENT:               &str = "E0016";

    // ---------- Semantic errors (E1xxx) ----------
    pub const SEMA_UNDEFINED_NAME:               &str = "E1001";
    pub const SEMA_REDEFINITION:                 &str = "E1002";
    pub const SEMA_WRONG_CONDITION_TYPE:         &str = "E1003";
    pub const SEMA_FOR_IN_NOT_ARRAY:             &str = "E1004";
    pub const SEMA_ILLEGAL_RETURN:               &str = "E1005";
    pub const SEMA_ARITH_TYPE_MISMATCH:          &str = "E1006";
    pub const SEMA_ARITH_BAD_TYPE:               &str = "E1007";
    pub const SEMA_COMPARE_TYPE_MISMATCH:        &str = "E1008";
    pub const SEMA_LOGIC_NEEDS_BOOL:             &str = "E1009";
    pub const SEMA_BITWISE_NEEDS_INT:            &str = "E1010";
    pub const SEMA_NEGATE_NEEDS_NUMBER:          &str = "E1011";
    pub const SEMA_BANG_BAD_TYPE:                &str = "E1012";
    pub const SEMA_TILDE_NEEDS_INT:              &str = "E1013";
    pub const SEMA_ASSIGN_TYPE_MISMATCH:         &str = "E1014";
    pub const SEMA_CALL_UNDEFINED:               &str = "E1015";
    pub const SEMA_CALL_NOT_FUNCTION:            &str = "E1016";
    pub const SEMA_CALL_WRONG_ARITY:             &str = "E1017";
    pub const SEMA_CALL_WRONG_ARG_TYPE:          &str = "E1018";
    pub const SEMA_MEMBER_NOT_STRUCT:            &str = "E1019";
    pub const SEMA_UNKNOWN_STRUCT:               &str = "E1020";
    pub const SEMA_NOT_A_STRUCT:                 &str = "E1021";
    pub const SEMA_NO_SUCH_FIELD:                &str = "E1022";
    pub const SEMA_NOT_AN_ENUM:                  &str = "E1023";
    pub const SEMA_NO_SUCH_VARIANT:              &str = "E1024";
    pub const SEMA_BAD_INDEX_TYPE:               &str = "E1025";
    pub const SEMA_INDEX_NOT_ARRAY:              &str = "E1026";
    pub const SEMA_RPC_BAD_SHAPE:                &str = "E1027";
    pub const SEMA_RPC_WRONG_ARITY:              &str = "E1028";
    pub const SEMA_NOT_VARIABLE:                 &str = "E1029";
    pub const SEMA_UNSUPPORTED_BINOP:            &str = "E1030";
    pub const SEMA_UNSUPPORTED_UNOP:             &str = "E1031";

    // ---------- Codegen errors (E0xxx range continued — emitter
    //            errors are still compile-time) ----------
    pub const CODEGEN_BAD_LHS:                   &str = "E0050";
    pub const CODEGEN_MISSING_EXT_ENDPOINT:      &str = "E0051";

    // ---------- Internal compiler errors (ICE) ----------
    //
    // ICE_xxxx codes signal a bug in solflow_compiler itself —
    // user code did not directly cause them. See `SolDiagnostic::internal`.
    pub const ICE_UNHANDLED_AST:                 &str = "ICE0001";
    pub const ICE_MISSING_TYPE_INFO:             &str = "ICE0002";
}
