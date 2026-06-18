use serde::Serialize;
use crate::value::Value;

/// A compiled user-defined function. Top-level `fn`s are compiled into
/// the same instruction stream as the workflow body, each starting at
/// `entry_pc`. A `Call` to a name found here is a real call (push a
/// frame, bind args, jump to `entry_pc`); names not found fall back to
/// native functions and built-ins.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FuncInfo {
    pub name: String,
    /// First instruction of the function body.
    pub entry_pc: usize,
    /// Number of declared parameters (bound to local slots 0..param_count).
    pub param_count: u16,
    /// Total locals the function uses (params plus `let` bindings).
    pub locals_count: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Chunk {
    pub instructions: Vec<Instruction>,
    pub constants: Vec<Value>,
    /// Locals of the workflow (entry) frame.
    pub locals_count: u16,
    /// Local names of the workflow (entry) frame.
    pub locals_names: Vec<String>,
    /// User-defined functions callable from the workflow or each other.
    pub functions: Vec<FuncInfo>,
    /// Byte span `(start, end)` into the source for each instruction
    /// (parallel to `instructions`), or `None` where there is no mapping.
    /// Statement-boundary instructions carry the span of their statement,
    /// which drives the execution trace's source mapping.
    pub instruction_spans: Vec<Option<(usize, usize)>>,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            constants: Vec::new(),
            locals_count: 0,
            locals_names: Vec::new(),
            functions: Vec::new(),
            instruction_spans: Vec::new(),
        }
    }

    /// Source span mapped to instruction `pc`, if any.
    pub fn span_at(&self, pc: usize) -> Option<(usize, usize)> {
        self.instruction_spans.get(pc).copied().flatten()
    }

    pub fn add_constant(&mut self, val: Value) -> u16 {
        let idx = self.constants.len();
        self.constants.push(val);
        idx as u16
    }

    /// Look up a user-defined function by name.
    pub fn function(&self, name: &str) -> Option<&FuncInfo> {
        self.functions.iter().find(|f| f.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Instruction {
    PushInt(i64),
    PushFloat(f64),
    PushBool(bool),
    PushChar(char),
    PushStr(u16),
    PushUnit,
    LoadLocal(u16),
    LoadName(u16),
    StoreLocal(u16),
    MakeArray(u16),
    MakeStruct(u16),
    MakeEnum(u16, u16),
    MemberAccess(u16),
    Index,
    Neg,
    Not,
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    Jump(u32),
    JumpIfFalse(u32),
    Pop,
    Call(u16, u8),
    Return,
    WorkflowCall,
    ModuleCall(u16),
    Len,
    StoreField(u16),
    StmtBoundary,
    Halt,
}
