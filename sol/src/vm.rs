use std::collections::HashMap;
use crate::instruction::{Chunk, Instruction};
use crate::value::Value;

pub type NativeFunc = Box<dyn Fn(&[Value]) -> Result<Value, String> + Send>;

// Captured stdout for environments without a real stdout (WASM/browser).
// `print` appends here; the host drains it with `take_output()` after a run.
use std::cell::RefCell;
thread_local! { static SOL_OUTPUT: RefCell<String> = RefCell::new(String::new()); }
pub fn push_output(s: &str) { SOL_OUTPUT.with(|o| o.borrow_mut().push_str(s)); }
pub fn take_output() -> String { SOL_OUTPUT.with(|o| std::mem::take(&mut *o.borrow_mut())) }

/// One activation record on the call stack. Created when a user-defined
/// function is called; `locals` holds the CALLER's locals to restore on
/// return, and `return_pc` is where the caller resumes.
#[derive(Debug, Clone)]
pub struct Frame {
    pub return_pc: usize,
    pub locals: Vec<Value>,
}

/// Maximum call-stack depth. A program that recurses past this fails with
/// a clear "call stack overflow" rather than blowing the host stack.
const MAX_CALL_DEPTH: usize = 256;

#[derive(Debug, Clone)]
pub struct VmSnapshot {
    pub pc: usize,
    pub stack: Vec<Value>,
    pub locals: Vec<Value>,
    pub pending_result: Option<Value>,
    pub frames: Vec<Frame>,
}

pub struct Vm {
    pub chunk: Chunk,
    pub pc: usize,
    pub stack: Vec<Value>,
    pub locals: Vec<Value>,
    /// Call stack of caller frames. Empty while executing the workflow
    /// (entry) frame; one entry per active user-function call.
    pub frames: Vec<Frame>,
    pub native_funcs: HashMap<String, NativeFunc>,
    pub pending_call: Option<(String, Value)>,
    pub pending_result: Option<Value>,
    pub completed: bool,
    pub step_count: u64,
    ignore_next_boundary: bool,
}

impl Vm {
    pub fn new(chunk: Chunk) -> Self {
        let locals_count = chunk.locals_count as usize;
        Self {
            chunk,
            pc: 0,
            stack: Vec::new(),
            locals: vec![Value::Unit; locals_count],
            frames: Vec::new(),
            native_funcs: HashMap::new(),
            pending_call: None,
            pending_result: None,
            completed: false,
            step_count: 0,
            ignore_next_boundary: false,
        }
    }

    pub fn register_native(&mut self, name: &str, func: NativeFunc) {
        self.native_funcs.insert(name.to_string(), func);
    }

    pub fn save(&self) -> VmSnapshot {
        VmSnapshot {
            pc: self.pc,
            stack: self.stack.clone(),
            locals: self.locals.clone(),
            pending_result: self.pending_result.clone(),
            frames: self.frames.clone(),
        }
    }

    pub fn restore(&mut self, snap: &VmSnapshot) {
        self.pc = snap.pc;
        self.stack = snap.stack.clone();
        self.locals = snap.locals.clone();
        self.pending_result = snap.pending_result.clone();
        self.frames = snap.frames.clone();
    }

    pub fn reset(&mut self) {
        self.pc = 0;
        self.stack.clear();
        self.locals = vec![Value::Unit; self.chunk.locals_count as usize];
        self.frames.clear();
        self.pending_call = None;
        self.pending_result = None;
        self.completed = false;
        self.step_count = 0;
        self.ignore_next_boundary = false;
    }

    pub fn step(&mut self, budget: u64) -> Result<StepResult, String> {
        if self.completed {
            return Ok(StepResult::Completed(Value::Unit));
        }

        if let Some(result) = self.pending_result.take() {
            self.stack.push(result);
        }

        if let Some((cap, params)) = self.pending_call.take() {
            self.step_count += 1;
            return Ok(StepResult::RemoteCall { capability: cap, params });
        }

        let mut stmts_ran: u64 = 0;

        while stmts_ran < budget && self.pc < self.chunk.instructions.len() {
            let instr = self.chunk.instructions[self.pc].clone();
            let is_boundary = matches!(instr, Instruction::StmtBoundary);
            let result = match self.exec_instruction(&instr) {
                Ok(r) => r,
                Err(e) => return Ok(StepResult::Failed(e)),
            };
            match result {
                InsResult::Continue => {
                    self.pc += 1;
                    if is_boundary {
                        if self.ignore_next_boundary {
                            self.ignore_next_boundary = false;
                        } else {
                            stmts_ran += 1;
                        }
                    }
                }
                InsResult::ContinueNoAdvance => {}
                InsResult::Returned(val) => {
                    match self.frames.pop() {
                        // Returning from a user-function call: restore the
                        // caller's locals + pc and hand the return value
                        // back on the shared operand stack.
                        Some(frame) => {
                            self.locals = frame.locals;
                            self.pc = frame.return_pc;
                            self.stack.push(val);
                        }
                        // No caller frame: the workflow itself returned.
                        None => {
                            self.completed = true;
                            return Ok(StepResult::Completed(val));
                        }
                    }
                }
                InsResult::RemoteCall(cap, params) => {
                    self.step_count += 1;
                    return Ok(StepResult::RemoteCall { capability: cap, params });
                }
                InsResult::CallFunc(name, args) => {
                    if let Some(func) = self.chunk.function(&name).cloned() {
                        // Real user-defined function call.
                        if args.len() != func.param_count as usize {
                            return Ok(StepResult::Failed(format!(
                                "function '{}' expects {} argument(s), got {}",
                                name, func.param_count, args.len()
                            )));
                        }
                        if self.frames.len() >= MAX_CALL_DEPTH {
                            return Ok(StepResult::Failed(format!(
                                "call stack overflow calling '{}' (recursion too deep)",
                                name
                            )));
                        }
                        // Bind args to the callee's local slots 0..param_count.
                        let mut callee_locals = vec![Value::Unit; func.locals_count as usize];
                        for (i, a) in args.into_iter().enumerate() {
                            callee_locals[i] = a;
                        }
                        let caller_locals = std::mem::replace(&mut self.locals, callee_locals);
                        self.frames.push(Frame {
                            return_pc: self.pc + 1,
                            locals: caller_locals,
                        });
                        self.pc = func.entry_pc;
                    } else if let Some(func) = self.native_funcs.get(&name) {
                        let result = (func)(&args)?;
                        self.stack.push(result);
                        self.pc += 1;
                    } else {
                        let result = self.exec_builtin(&name, &args)?;
                        self.stack.push(result);
                        self.pc += 1;
                    }
                }
            }
        }

        if self.completed || self.pc >= self.chunk.instructions.len() {
            self.completed = true;
            let result = self.stack.pop().unwrap_or(Value::Unit);
            return Ok(StepResult::Completed(result));
        }

        Ok(StepResult::Yielded(stmts_ran))
    }

    pub fn resolve_remote_call(&mut self, _capability: &str, result: Value) {
        self.pending_result = Some(result);
        self.pending_call = None;
        self.pc += 1;
        self.ignore_next_boundary = true;
    }

    fn exec_instruction(&mut self, instr: &Instruction) -> Result<InsResult, String> {
        match instr {
            Instruction::PushInt(n) => {
                self.stack.push(Value::Int(*n));
                Ok(InsResult::Continue)
            }
            Instruction::PushFloat(n) => {
                self.stack.push(Value::Float(*n));
                Ok(InsResult::Continue)
            }
            Instruction::PushBool(b) => {
                self.stack.push(Value::Bool(*b));
                Ok(InsResult::Continue)
            }
            Instruction::PushChar(c) => {
                self.stack.push(Value::Char(*c));
                Ok(InsResult::Continue)
            }
            Instruction::PushStr(idx) => {
                let s = self.chunk.constants[*idx as usize].clone();
                self.stack.push(s);
                Ok(InsResult::Continue)
            }
            Instruction::PushUnit => {
                self.stack.push(Value::Unit);
                Ok(InsResult::Continue)
            }
            Instruction::LoadLocal(slot) => {
                let val = self.locals[*slot as usize].clone();
                self.stack.push(val);
                Ok(InsResult::Continue)
            }
            Instruction::LoadName(idx) => {
                let name = match &self.chunk.constants[*idx as usize] {
                    Value::Str(s) => s.clone(),
                    _ => return Err("LoadName constant must be a string".into()),
                };
                let pos = self.chunk.locals_names.iter().position(|n| n == &name);
                match pos {
                    Some(slot) => {
                        self.stack.push(self.locals[slot].clone());
                    }
                    None => return Err(format!("variable '{}' not found", name)),
                }
                Ok(InsResult::Continue)
            }
            Instruction::StoreLocal(slot) => {
                let val = self.stack.pop().ok_or_else(|| "stack empty for StoreLocal".to_string())?;
                self.locals[*slot as usize] = val;
                Ok(InsResult::Continue)
            }
            Instruction::MakeArray(n) => {
                let n = *n as usize;
                let mut elements = Vec::with_capacity(n);
                for _ in 0..n {
                    let val = self.stack.pop().ok_or_else(|| "stack empty for MakeArray".to_string())?;
                    elements.push(val);
                }
                elements.reverse();
                self.stack.push(Value::Array(elements));
                Ok(InsResult::Continue)
            }
            Instruction::MakeStruct(n) => {
                let n = *n as usize;
                let mut pairs: Vec<(String, Value)> = Vec::with_capacity(n);
                for _ in 0..n {
                    let val = self.stack.pop().ok_or_else(|| "stack empty for MakeStruct (val)".to_string())?;
                    let key = self.stack.pop().ok_or_else(|| "stack empty for MakeStruct (key)".to_string())?;
                    match key {
                        Value::Str(k) => pairs.push((k, val)),
                        _ => return Err("struct key must be a string".into()),
                    }
                }
                pairs.reverse();
                let map: HashMap<String, Value> = pairs.into_iter().collect();
                self.stack.push(Value::Struct(map));
                Ok(InsResult::Continue)
            }
            Instruction::MakeEnum(enum_idx, var_idx) => {
                let enum_name = match &self.chunk.constants[*enum_idx as usize] {
                    Value::Str(s) => s.clone(),
                    _ => return Err("enum name constant must be a string".into()),
                };
                let variant = match &self.chunk.constants[*var_idx as usize] {
                    Value::Str(s) => s.clone(),
                    _ => return Err("variant name constant must be a string".into()),
                };
                self.stack.push(Value::Enum(enum_name, variant));
                Ok(InsResult::Continue)
            }
            Instruction::MemberAccess(idx) => {
                let field = match &self.chunk.constants[*idx as usize] {
                    Value::Str(s) => s.clone(),
                    _ => return Err("field name constant must be a string".into()),
                };
                let obj = self.stack.pop().ok_or_else(|| "stack empty for MemberAccess".to_string())?;
                match obj {
                    Value::Struct(map) => {
                        let val = map.get(&field).cloned()
                            .ok_or_else(|| format!("field '{}' not found", field))?;
                        self.stack.push(val);
                    }
                    _ => return Err(format!("cannot access field '{}' on {}", field, obj)),
                }
                Ok(InsResult::Continue)
            }
            Instruction::StoreField(idx) => {
                let field = match &self.chunk.constants[*idx as usize] {
                    Value::Str(s) => s.clone(),
                    _ => return Err("field name constant must be a string".into()),
                };
                let obj = self.stack.pop().ok_or_else(|| "stack empty for StoreField (obj)".to_string())?;
                let val = self.stack.pop().ok_or_else(|| "stack empty for StoreField (val)".to_string())?;
                match obj {
                    Value::Struct(mut map) => {
                        map.insert(field, val);
                        self.stack.push(Value::Struct(map));
                    }
                    _ => return Err("cannot assign to field of non-struct".into()),
                }
                Ok(InsResult::Continue)
            }
            Instruction::Index => {
                let idx = self.stack.pop().ok_or_else(|| "stack empty for Index (idx)".to_string())?;
                let arr = self.stack.pop().ok_or_else(|| "stack empty for Index (arr)".to_string())?;
                match (arr, idx) {
                    (Value::Array(items), Value::Int(i)) => {
                        let i = i as usize;
                        let val = items.get(i).cloned()
                            .ok_or_else(|| format!("index {} out of bounds", i))?;
                        self.stack.push(val);
                    }
                    (arr_val, idx_val) => {
                        return Err(format!("cannot index {} with {}", arr_val, idx_val));
                    }
                }
                Ok(InsResult::Continue)
            }
            Instruction::Len => {
                let val = self.stack.pop().ok_or_else(|| "stack empty for Len".to_string())?;
                match val {
                    Value::Str(s) => self.stack.push(Value::Int(s.len() as i64)),
                    Value::Array(arr) => self.stack.push(Value::Int(arr.len() as i64)),
                    _ => return Err(format!("cannot take length of {}", val)),
                }
                Ok(InsResult::Continue)
            }
            Instruction::Neg => {
                let val = self.stack.pop().ok_or_else(|| "stack empty for Neg".to_string())?;
                match val {
                    Value::Int(n) => self.stack.push(Value::Int(-n)),
                    _ => return Err(format!("cannot negate {}", val)),
                }
                Ok(InsResult::Continue)
            }
            Instruction::Not => {
                let val = self.stack.pop().ok_or_else(|| "stack empty for Not".to_string())?;
                match val {
                    Value::Bool(b) => self.stack.push(Value::Bool(!b)),
                    _ => return Err(format!("cannot apply 'not' to {}", val)),
                }
                Ok(InsResult::Continue)
            }
            Instruction::Add => self.bin_op(|a, b| a + b, |a, b| a + b, "add"),
            Instruction::Sub => self.bin_op(|a, b| a - b, |a, b| a - b, "subtract"),
            Instruction::Mul => self.bin_op(|a, b| a * b, |a, b| a * b, "multiply"),
            Instruction::Div => {
                let r = self.stack.pop().ok_or_else(|| "stack empty for divide".to_string())?;
                let l = self.stack.pop().ok_or_else(|| "stack empty for divide".to_string())?;
                let result = match (l, r) {
                    (Value::Int(a), Value::Int(b)) => {
                        if b == 0 { return Err("division by zero".into()); }
                        Value::Int(a / b)
                    }
                    (Value::Float(a), Value::Float(b)) => {
                        if b == 0.0 { return Err("division by zero".into()); }
                        Value::Float(a / b)
                    }
                    (Value::Int(a), Value::Float(b)) => {
                        if b == 0.0 { return Err("division by zero".into()); }
                        Value::Float(a as f64 / b)
                    }
                    (Value::Float(a), Value::Int(b)) => {
                        if b == 0 { return Err("division by zero".into()); }
                        Value::Float(a / b as f64)
                    }
                    (l, r) => return Err(format!("cannot divide {} and {}", l, r)),
                };
                self.stack.push(result);
                Ok(InsResult::Continue)
            }
            Instruction::Eq => self.eq_op(false),
            Instruction::Ne => self.eq_op(true),
            Instruction::Lt => self.cmp_op(|a, b| a < b, |a, b| a < b),
            Instruction::Gt => self.cmp_op(|a, b| a > b, |a, b| a > b),
            Instruction::Le => self.cmp_op(|a, b| a <= b, |a, b| a <= b),
            Instruction::Ge => self.cmp_op(|a, b| a >= b, |a, b| a >= b),
            Instruction::And => {
                let r = self.stack.pop().ok_or_else(|| "stack empty for And".to_string())?;
                let l = self.stack.pop().ok_or_else(|| "stack empty for And".to_string())?;
                match (l, r) {
                    (Value::Bool(a), Value::Bool(b)) => self.stack.push(Value::Bool(a && b)),
                    (l, r) => return Err(format!("cannot 'and' {} and {}", l, r)),
                }
                Ok(InsResult::Continue)
            }
            Instruction::Or => {
                let r = self.stack.pop().ok_or_else(|| "stack empty for Or".to_string())?;
                let l = self.stack.pop().ok_or_else(|| "stack empty for Or".to_string())?;
                match (l, r) {
                    (Value::Bool(a), Value::Bool(b)) => self.stack.push(Value::Bool(a || b)),
                    (l, r) => return Err(format!("cannot 'or' {} and {}", l, r)),
                }
                Ok(InsResult::Continue)
            }
            Instruction::Jump(offset) => {
                self.pc = *offset as usize;
                Ok(InsResult::ContinueNoAdvance)
            }
            Instruction::JumpIfFalse(offset) => {
                let cond = self.stack.pop().ok_or_else(|| "stack empty for JumpIfFalse".to_string())?;
                let truthy = match &cond {
                    Value::Bool(b) => *b,
                    Value::Int(n) => *n != 0,
                    _ => return Err(format!("cannot use {} as condition", cond)),
                };
                if !truthy {
                    self.pc = *offset as usize;
                    Ok(InsResult::ContinueNoAdvance)
                } else {
                    Ok(InsResult::Continue)
                }
            }
            Instruction::Pop => {
                self.stack.pop();
                Ok(InsResult::Continue)
            }
            Instruction::Call(name_idx, arg_count) => {
                let name = match &self.chunk.constants[*name_idx as usize] {
                    Value::Str(s) => s.clone(),
                    _ => return Err("call target name must be a string constant".into()),
                };
                let arg_count = *arg_count as usize;
                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    let val = self.stack.pop().ok_or_else(|| "stack empty for Call args".to_string())?;
                    args.push(val);
                }
                args.reverse();
                Ok(InsResult::CallFunc(name, args))
            }
            Instruction::Return => {
                let val = self.stack.pop().unwrap_or(Value::Unit);
                Ok(InsResult::Returned(val))
            }
            Instruction::WorkflowCall => {
                let params = self.stack.pop().ok_or_else(|| "stack empty for WorkflowCall params".to_string())?;
                let cap_name = self.stack.pop().ok_or_else(|| "stack empty for WorkflowCall cap".to_string())?;
                match cap_name {
                    Value::Str(cap) => {
                        let p = params.clone();
                        self.pending_call = Some((cap.clone(), params));
                        Ok(InsResult::RemoteCall(cap, p))
                    }
                    _ => Err("capability name must be a string".into()),
                }
            }
            Instruction::ModuleCall(name_idx) => {
                let rpc_name = match &self.chunk.constants[*name_idx as usize] {
                    Value::Str(s) => s.clone(),
                    _ => return Err("RPC name must be a string constant".into()),
                };
                let args_count = 1;
                let mut args = Vec::with_capacity(args_count);
                for _ in 0..args_count {
                    let val = self.stack.pop().ok_or_else(|| "stack empty for ModuleCall args".to_string())?;
                    args.push(val);
                }
                args.reverse();
                let namespace = self.stack.pop().ok_or_else(|| "stack empty for ModuleCall namespace".to_string())?;
                let module_path = match namespace {
                    Value::Module(path) => path,
                    Value::Str(s) => s,
                    other => return Err(format!("namespace must be a module or string, got {}", other)),
                };
                let params = args.into_iter().next().unwrap_or(Value::Struct(HashMap::new()));
                let p = params.clone();
                let cap = format!("{}::{}", module_path, rpc_name);
                self.pending_call = Some((cap.clone(), params));
                Ok(InsResult::RemoteCall(cap, p))
            }
            Instruction::StmtBoundary => {
                Ok(InsResult::Continue)
            }
            Instruction::Halt => {
                Ok(InsResult::Returned(self.stack.pop().unwrap_or(Value::Unit)))
            }
        }
    }

    fn exec_builtin(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "print" => {
                // Route to the capture buffer (WASM/browser have no stdout).
                let mut line = String::new();
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { line.push(' '); }
                    line.push_str(&format!("{}", arg));
                }
                line.push('\n');
                push_output(&line);
                Ok(Value::Unit)
            }
            "len" => {
                if args.len() != 1 {
                    return Err("len() takes exactly 1 argument".into());
                }
                match &args[0] {
                    Value::Str(s) => Ok(Value::Int(s.len() as i64)),
                    Value::Array(arr) => Ok(Value::Int(arr.len() as i64)),
                    _ => Err(format!("cannot take length of {}", args[0])),
                }
            }
            "to_str" => {
                if args.len() != 1 {
                    return Err("to_str() takes exactly 1 argument".into());
                }
                Ok(Value::Str(format!("{}", args[0])))
            }
            "type_name" => {
                if args.len() != 1 {
                    return Err("type_name() takes exactly 1 argument".into());
                }
                let name = match &args[0] {
                    Value::Bool(_) => "bool",
                    Value::Int(_) => "int",
                    Value::Float(_) => "float",
                    Value::Char(_) => "char",
                    Value::Str(_) => "str",
                    Value::Array(_) => "array",
                    Value::Struct(_) => "struct",
                    Value::Enum(_, _) => "enum",
                    Value::Unit => "unit",
                    Value::RemoteRef { .. } => "remote_ref",
                    Value::Module(_) => "module",
                };
                Ok(Value::Str(name.to_string()))
            }
            _ => Err(format!("function '{}' not found", name)),
        }
    }

    fn bin_op(
        &mut self,
        int_op: fn(i64, i64) -> i64,
        float_op: fn(f64, f64) -> f64,
        label: &str,
    ) -> Result<InsResult, String> {
        let r = self.stack.pop().ok_or_else(|| format!("stack empty for {}", label))?;
        let l = self.stack.pop().ok_or_else(|| format!("stack empty for {}", label))?;
        let result = match (l, r) {
            (Value::Int(a), Value::Int(b)) => Value::Int(int_op(a, b)),
            (Value::Float(a), Value::Float(b)) => Value::Float(float_op(a, b)),
            (Value::Int(a), Value::Float(b)) => Value::Float(float_op(a as f64, b)),
            (Value::Float(a), Value::Int(b)) => Value::Float(float_op(a, b as f64)),
            (Value::Str(a), Value::Str(b)) if label == "add" => Value::Str(format!("{}{}", a, b)),
            (l, r) => return Err(format!("cannot {} {} and {}", label, l, r)),
        };
        self.stack.push(result);
        Ok(InsResult::Continue)
    }

    /// `==` / `!=`. Numeric operands compare by value (with int/float
    /// coercion); every other value type (bool, char, str, enum, struct,
    /// array, unit) compares structurally. This is what lets a workflow
    /// branch on an enum returned by a helper, e.g. `status == Paid`.
    fn eq_op(&mut self, negate: bool) -> Result<InsResult, String> {
        let r = self.stack.pop().ok_or_else(|| "stack empty for equality".to_string())?;
        let l = self.stack.pop().ok_or_else(|| "stack empty for equality".to_string())?;
        let eq = match (&l, &r) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            _ => l == r,
        };
        self.stack.push(Value::Bool(if negate { !eq } else { eq }));
        Ok(InsResult::Continue)
    }

    fn cmp_op(
        &mut self,
        int_op: fn(i64, i64) -> bool,
        float_op: fn(f64, f64) -> bool,
    ) -> Result<InsResult, String> {
        let r = self.stack.pop().ok_or_else(|| "stack empty for comparison".to_string())?;
        let l = self.stack.pop().ok_or_else(|| "stack empty for comparison".to_string())?;
        let result = match (&l, &r) {
            (Value::Int(a), Value::Int(b)) => int_op(*a, *b),
            (Value::Float(a), Value::Float(b)) => float_op(*a, *b),
            (Value::Int(a), Value::Float(b)) => float_op(*a as f64, *b),
            (Value::Float(a), Value::Int(b)) => float_op(*a, *b as f64),
            (l, r) => return Err(format!("cannot compare {} and {}", l, r)),
        };
        self.stack.push(Value::Bool(result));
        Ok(InsResult::Continue)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepResult {
    Completed(Value),
    Yielded(u64),
    RemoteCall { capability: String, params: Value },
    Failed(String),
}

enum InsResult {
    Continue,
    ContinueNoAdvance,
    Returned(Value),
    RemoteCall(String, Value),
    CallFunc(String, Vec<Value>),
}

#[cfg(test)]
mod function_call_tests {
    use super::*;
    use crate::compiler::Compiler;
    use crate::parser::Parser;

    /// Run a whole program to completion and return (output lines, result).
    fn run(src: &str) -> (Vec<String>, Result<Value, String>) {
        let prog = Parser::new(src).parse().expect("parse");
        let chunk = Compiler::new().compile(&prog).expect("compile");
        let mut vm = Vm::new(chunk);
        let _ = take_output(); // clear any residue on this thread
        let result = loop {
            match vm.step(1_000_000) {
                Ok(StepResult::Completed(v)) => break Ok(v),
                Ok(StepResult::Yielded(_)) => continue,
                Ok(StepResult::Failed(e)) => break Err(e),
                Ok(StepResult::RemoteCall { capability, .. }) => {
                    break Err(format!("unexpected remote call: {capability}"))
                }
                Err(e) => break Err(e),
            }
        };
        let out = take_output();
        let lines = if out.is_empty() {
            vec![]
        } else {
            out.trim_end_matches('\n').split('\n').map(|s| s.to_string()).collect()
        };
        (lines, result)
    }

    #[test]
    fn single_helper_call_returns_value() {
        let (_out, r) = run(r#"
            fn dbl(x: int) <- int { return x * 2; }
            workflow "w" { return dbl(21); }
        "#);
        assert_eq!(r, Ok(Value::Int(42)));
    }

    #[test]
    fn helper_prints_run_in_order() {
        let (out, r) = run(r#"
            fn greet(n: str) { print(n); }
            workflow "w" { greet("hello"); greet("world"); return 0; }
        "#);
        assert_eq!(out, vec!["hello", "world"]);
        assert_eq!(r, Ok(Value::Int(0)));
    }

    #[test]
    fn nested_calls_thread_values() {
        let (_o, r) = run(r#"
            fn add(a: int, b: int) <- int { return a + b; }
            fn calc(x: int) <- int { return add(x, 10); }
            workflow "w" { return calc(5); }
        "#);
        assert_eq!(r, Ok(Value::Int(15)));
    }

    #[test]
    fn structs_flow_across_calls() {
        let (_o, r) = run(r#"
            struct P { x: int; y: int; }
            fn total(p: P) <- int { return p.x + p.y; }
            workflow "w" { let p: P = P { x: 3, y: 4 }; return total(p); }
        "#);
        assert_eq!(r, Ok(Value::Int(7)));
    }

    #[test]
    fn enums_flow_across_calls() {
        let (out, r) = run(r#"
            enum Color { Red; Green; }
            fn pick() <- Color { return Color::Green; }
            workflow "w" {
                let c: Color = pick();
                if (c == Color::Green) { print("green"); }
                return 0;
            }
        "#);
        assert_eq!(out, vec!["green"]);
        assert_eq!(r, Ok(Value::Int(0)));
    }

    #[test]
    fn recursion_works_to_a_bound() {
        let (_o, r) = run(r#"
            fn fact(n: int) <- int {
                if (n < 2) { return 1; }
                return n * fact(n - 1);
            }
            workflow "w" { return fact(5); }
        "#);
        assert_eq!(r, Ok(Value::Int(120)));
    }

    #[test]
    fn unbounded_recursion_fails_with_stack_overflow() {
        let (_o, r) = run(r#"
            fn loop_forever(x: int) <- int { return loop_forever(x); }
            workflow "w" { return loop_forever(1); }
        "#);
        match r {
            Err(e) => assert!(e.contains("call stack overflow"), "got: {e}"),
            Ok(v) => panic!("expected failure, got {v:?}"),
        }
    }

    #[test]
    fn arg_count_mismatch_fails_clearly() {
        let (_o, r) = run(r#"
            fn f(a: int) <- int { return a; }
            workflow "w" { return f(1, 2); }
        "#);
        match r {
            Err(e) => assert!(e.contains("expects 1 argument"), "got: {e}"),
            Ok(v) => panic!("expected failure, got {v:?}"),
        }
    }

    #[test]
    fn callee_runtime_error_propagates() {
        let (_o, r) = run(r#"
            fn risky(n: int) <- int { return n / 0; }
            workflow "w" { return risky(10); }
        "#);
        match r {
            Err(e) => assert!(e.contains("division by zero"), "got: {e}"),
            Ok(v) => panic!("expected failure, got {v:?}"),
        }
    }

    #[test]
    fn function_falling_off_end_returns_unit() {
        let (out, r) = run(r#"
            fn noop() { print("ran"); }
            workflow "w" { noop(); return 0; }
        "#);
        assert_eq!(out, vec!["ran"]);
        assert_eq!(r, Ok(Value::Int(0)));
    }

    #[test]
    fn caller_locals_survive_a_call() {
        // A local in the workflow must be intact after a helper that uses
        // its own (separate) locals runs.
        let (_o, r) = run(r#"
            fn bump(n: int) <- int { let tmp: int = n + 100; return tmp; }
            workflow "w" {
                let base: int = 7;
                let other: int = bump(base);
                return base + other;
            }
        "#);
        // base (7) preserved + bump(7)=107 => 114
        assert_eq!(r, Ok(Value::Int(114)));
    }
}
