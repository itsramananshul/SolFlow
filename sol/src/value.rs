//! Runtime value representation for the Sol interpreter.
//!
//! The [`Value`] enum is the universal runtime type for all Sol values.
//! It supports:
//! - Primitive types: `Bool`, `Int`, `Float`, `Char`, `Str`
//! - Composite types: `Array`, `Struct`, `Enum`
//! - `Unit` (the void type, returned by statements with no value)
//! - `RemoteRef` — a lightweight reference to a value stored on a remote
//!   controller, used in the distributed workflow forwarding protocol.

use std::collections::HashMap;
use std::fmt;
use serde::{Deserialize, Serialize};

/// A runtime value in the Sol language.
///
/// Values are cloneable, comparable, and serialisable (for network transfer
/// between controllers during workflow forwarding).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// A boolean value: `true` or `false`.
    Bool(bool),
    /// A signed 64-bit integer.
    Int(i64),
    /// A 64-bit floating-point number.
    Float(f64),
    /// A single Unicode character.
    Char(char),
    /// A heap-allocated string.
    Str(String),
    /// A heterogeneous array of values.
    Array(Vec<Value>),
    /// An anonymous struct mapping field names to values.
    Struct(HashMap<String, Value>),
    /// An enum variant: `(enum_name, variant_name)`.
    Enum(String, String),
    /// The unit / void value, representing "no value".
    Unit,
    /// A module reference — a first-class handle to a networked application.
    /// Used with the `::` operator for remote procedure calls.
    Module(String),
    /// A remote reference — a lightweight pointer to a value stored on
    /// another controller. Used in the workflow forwarding protocol.
    RemoteRef {
        /// The unique identifier for this value on the owning controller.
        id: String,
        /// The URL of the controller that owns this value.
        owner: String,
    },
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::Char(c) => write!(f, "'{}'", c),
            Value::Str(s) => write!(f, "{}", s),
            Value::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Struct(fields) => {
                write!(f, "{{ ")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, " }}")
            }
            Value::Enum(name, variant) => write!(f, "{}::{}", name, variant),
            Value::Unit => write!(f, "unit"),
            Value::RemoteRef { id, owner } => write!(f, "RemoteRef({}@{})", id, owner),
            Value::Module(path) => write!(f, "module({})", path),
        }
    }
}
