//! Structured runtime errors.
//!
//! Returned from `VM::step()` / `VM::run()` instead of panicking
//! for user-facing failure modes. The boundary still has a
//! `catch_unwind` for the truly-invariant panics that remain in
//! the VM (compiler-emit-violates-spec class), but the cases
//! below are common enough that returning them as values gives a
//! much better debugger UX.

use std::fmt;

/// Categorized runtime failures the VM produces as values rather
/// than panics. Every variant carries enough context for the
/// editor to render a useful message; we deliberately keep the
/// payloads small + cloneable.
#[derive(Debug, Clone)]
pub enum RunError {
    /// Integer division (`IntDiv`) with denominator 0.
    DivByZero,

    /// `GetElem` / `SetElem` with an out-of-bounds index.
    IndexOutOfBounds {
        index: usize,
        length: usize,
    },

    /// Pop attempted on an empty value stack. Shouldn't happen
    /// from any well-typed program; if it does, the compiler
    /// emitted bytecode that violates the canonical contract.
    /// Returned as a value here (rather than panicked) so the
    /// editor can still recover and surface the bug.
    StackUnderflow,

    /// `step_limit` exceeded. Browser-side safety guard against
    /// infinite loops in user code — the canonical CLI VM has no
    /// such limit. Configurable via `VM::with_step_limit`.
    StepLimit {
        limit: usize,
    },

    /// `Inst::ExtCall` was reached but no `ExtCallHandler` was
    /// installed on the VM. The browser-sim VM hits this on every
    /// ExtCall by design — see `runtime/UPSTREAM.md`. The
    /// controller installs a handler so this variant is reserved
    /// for the browser path.
    ExtCallBlocked {
        function_name: String,
        url: String,
    },

    /// `Inst::ExtCall` reached a handler, but the handler returned
    /// an error. Carries the connector name, the SOL function
    /// name, and a free-form message the editor renders verbatim.
    /// (Structured connector-error variants live in the
    /// controller's run-event log — Phase C.5.)
    ExtCallFailed {
        connector: String,
        function_name: String,
        message: String,
    },

    /// Heap object's variant didn't match what the instruction
    /// expected (e.g. `GetField` on a String). Surfaces compiler
    /// bugs; rare in well-formed bytecode.
    HeapShapeMismatch {
        expected: &'static str,
        got: &'static str,
    },
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::DivByZero => write!(f, "division by zero"),
            RunError::IndexOutOfBounds { index, length } => write!(
                f,
                "array index out of bounds: index {index}, length {length}",
            ),
            RunError::StackUnderflow => write!(f, "stack underflow"),
            RunError::StepLimit { limit } => write!(
                f,
                "execution step limit exceeded ({limit} instructions)",
            ),
            RunError::ExtCallBlocked { function_name, url } => write!(
                f,
                "external call to `{function_name}` at `{url}` blocked: \
                 external calls are not available in browser simulation",
            ),
            RunError::ExtCallFailed { connector, function_name, message } => write!(
                f,
                "external call to `{function_name}` via connector `{connector}` failed: {message}",
            ),
            RunError::HeapShapeMismatch { expected, got } => write!(
                f,
                "heap shape mismatch: expected {expected}, got {got}",
            ),
        }
    }
}
