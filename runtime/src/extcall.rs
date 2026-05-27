//! ExtCall handler contract — Phase C C.4 c76.
//!
//! The VM stays browser-safe (no networking, no `tokio`, no
//! `serde_json`). When `Inst::ExtCall` fires, the VM hands every-
//! thing the call needs to an installed `ExtCallHandler` and waits
//! for the typed result.
//!
//! Browser-sim doesn't install a handler — the VM falls back to
//! `RunError::ExtCallBlocked` (the existing C.1 behavior). The
//! controller installs a handler that dispatches to the connector
//! registry.
//!
//! ## Type surface
//!
//! Only primitive SOL types cross this boundary in C.4: `int`,
//! `float`, `bool`, `string`, `void`. Compound types (struct,
//! array) error with `ExtCallError::Unsupported`. That's the
//! defensible MVP — compound marshalling can land in a later
//! milestone without changing this trait.

use crate::RunError;
use std::sync::Arc;

/// Primitive SOL types the ExtCall boundary supports today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtCallType {
    Int,
    Float,
    Bool,
    String,
    Void,
}

impl ExtCallType {
    pub fn name(self) -> &'static str {
        match self {
            ExtCallType::Int => "int",
            ExtCallType::Float => "float",
            ExtCallType::Bool => "bool",
            ExtCallType::String => "string",
            ExtCallType::Void => "void",
        }
    }
}

/// Materialized arg/return value across the VM↔handler boundary.
/// `String` is fully owned so the handler can pass it across
/// async boundaries without VM-internal references.
#[derive(Debug, Clone)]
pub enum ExtCallValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Void,
}

impl ExtCallValue {
    pub fn ty(&self) -> ExtCallType {
        match self {
            ExtCallValue::Int(_) => ExtCallType::Int,
            ExtCallValue::Float(_) => ExtCallType::Float,
            ExtCallValue::Bool(_) => ExtCallType::Bool,
            ExtCallValue::String(_) => ExtCallType::String,
            ExtCallValue::Void => ExtCallType::Void,
        }
    }
}

/// Context passed to the handler. References into the VM's
/// scratch buffer — handlers can copy whatever they need.
#[derive(Debug)]
pub struct ExtCallContext<'a> {
    pub function_name: &'a str,
    pub url: &'a str,
    pub args: &'a [ExtCallValue],
    pub ret_type: ExtCallType,
}

/// Errors the handler can return. Plain enums (no thiserror) so
/// the runtime crate stays dependency-light — the controller
/// translates these into `RunError::ExtCallFailed`.
#[derive(Debug, Clone)]
pub enum ExtCallError {
    /// Connector or runtime layer says this type isn't supported
    /// at the boundary (compound types in C.4).
    Unsupported { reason: String },
    /// Connector returned a value whose type doesn't match the
    /// SOL function's declared return.
    TypeMismatch { expected: ExtCallType, got: ExtCallType },
    /// Any other connector-side failure — carries a free-form
    /// message the editor renders verbatim. The full structured
    /// error lives in the controller's run-event log (C.5).
    Failed {
        connector: String,
        fn_name: String,
        message: String,
    },
}

impl ExtCallError {
    /// Convenience: build a "failed" error from a plain message.
    pub fn failed(connector: impl Into<String>, fn_name: impl Into<String>, message: impl Into<String>) -> Self {
        ExtCallError::Failed {
            connector: connector.into(),
            fn_name: fn_name.into(),
            message: message.into(),
        }
    }
}

impl From<ExtCallError> for RunError {
    fn from(e: ExtCallError) -> Self {
        match e {
            ExtCallError::Failed { connector, fn_name, message } => {
                RunError::ExtCallFailed { connector, function_name: fn_name, message }
            }
            ExtCallError::Unsupported { reason } => RunError::ExtCallFailed {
                connector: "(unknown)".into(),
                function_name: "(unknown)".into(),
                message: format!("unsupported type at ExtCall boundary: {reason}"),
            },
            ExtCallError::TypeMismatch { expected, got } => RunError::ExtCallFailed {
                connector: "(unknown)".into(),
                function_name: "(unknown)".into(),
                message: format!(
                    "ExtCall return type mismatch: expected {}, got {}",
                    expected.name(),
                    got.name()
                ),
            },
        }
    }
}

/// What the VM calls. Implementations live OUTSIDE this crate
/// (controller side) so the runtime stays browser-safe.
///
/// Synchronous because the VM is. Implementations that wrap async
/// work (the controller) use `tokio::runtime::Handle::block_on`
/// inside a `spawn_blocking` thread — already how `execute_run`
/// drives the VM.
pub trait ExtCallHandler: Send + Sync {
    fn handle(&self, ctx: ExtCallContext<'_>) -> Result<ExtCallValue, ExtCallError>;
}

/// Type alias the rest of the crate uses everywhere.
pub type ExtCallHandlerArc = Arc<dyn ExtCallHandler>;

// =============================================================
//  Type bridging — compiler::Type → ExtCallType
// =============================================================

use solflow_compiler::parser::Type;

/// Map the compiler's `Type` to an `ExtCallType`. Returns
/// `ExtCallError::Unsupported` for compound types.
pub fn try_ext_call_type(t: &Type) -> Result<ExtCallType, ExtCallError> {
    match t {
        Type::Integer => Ok(ExtCallType::Int),
        Type::Float => Ok(ExtCallType::Float),
        Type::Bool => Ok(ExtCallType::Bool),
        Type::String => Ok(ExtCallType::String),
        Type::Void => Ok(ExtCallType::Void),
        other => Err(ExtCallError::Unsupported {
            reason: format!("type `{other:?}` not supported at ExtCall boundary"),
        }),
    }
}
