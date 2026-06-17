use serde::Serialize;
use crate::value::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Chunk {
    pub instructions: Vec<Instruction>,
    pub constants: Vec<Value>,
    pub locals_count: u16,
    pub locals_names: Vec<String>,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            constants: Vec::new(),
            locals_count: 0,
            locals_names: Vec::new(),
        }
    }

    pub fn add_constant(&mut self, val: Value) -> u16 {
        let idx = self.constants.len();
        self.constants.push(val);
        idx as u16
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
