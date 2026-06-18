pub mod lexer;
pub mod parser;
pub mod ast;
pub mod instruction;
pub mod compiler;
pub mod vm;
pub mod workflow;
pub mod value;
pub mod analysis;
pub mod crypto;

#[deprecated(since = "0.2.0", note = "use the bytecode VM (vm, compiler, instruction modules) instead")]
pub mod interpreter;
pub mod format;

pub use lexer::Lexer;
pub use parser::Parser;
pub use instruction::{Instruction, Chunk};
pub use compiler::Compiler;
pub use vm::{Vm, NativeFunc, VmSnapshot, StepResult, TraceEvent, TraceKind};
pub use workflow::{WorkflowExecutor, WorkflowState};
pub use value::Value;
pub use ast::*;
pub use analysis::{extract_capabilities, analyze_workflow, WorkflowAnalysis, WorkflowCallSite};
pub use crypto::Keypair;
pub use format::{format_source, format_program};
