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
/// return, `return_pc` is where the caller resumes, and `func` is the
/// caller's name (restored as the current trace function on return).
#[derive(Debug, Clone)]
pub struct Frame {
    pub return_pc: usize,
    pub locals: Vec<Value>,
    pub func: String,
}

/// Maximum call-stack depth. A program that recurses past this fails with
/// a clear "call stack overflow" rather than blowing the host stack.
const MAX_CALL_DEPTH: usize = 256;

/// What an execution-trace entry records.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum TraceKind {
    /// A statement boundary was reached and executed.
    Stmt,
    /// A user-defined function is about to be entered.
    Call,
    /// A user-defined function returned to its caller.
    Return,
    /// An external capability (Action) call is about to be made;
    /// `detail` carries the capability name (`module.function`).
    ExtCall,
    /// An external capability call returned a value to the workflow.
    ExtResult,
    /// An instruction failed; `detail` carries the message.
    Error,
}

/// One execution-trace entry, emitted as the VM runs when tracing is on.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceEvent {
    /// Monotonic index of this event in the trace.
    pub step: u64,
    pub kind: TraceKind,
    /// Workflow or helper function the VM was executing.
    pub function: String,
    /// Byte span `(start, end)` into the source, when mapped.
    pub span: Option<(usize, usize)>,
    /// Call depth at this event (0 = workflow body).
    pub depth: usize,
    /// Extra context: the callee name for Call, the error message for Error.
    pub detail: Option<String>,
}

/// Cap on recorded trace events so a long run can't grow without bound.
const TRACE_CAP: usize = 50_000;

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
    /// Execution trace, populated as the VM runs when `trace_enabled`.
    pub trace: Vec<TraceEvent>,
    trace_enabled: bool,
    /// Name of the function/workflow currently executing (for trace).
    current_func: String,
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
            trace: Vec::new(),
            trace_enabled: false,
            current_func: "workflow".to_string(),
        }
    }

    pub fn register_native(&mut self, name: &str, func: NativeFunc) {
        self.native_funcs.insert(name.to_string(), func);
    }

    /// Bind a top-level input value (e.g. `payload`) so an identifier of
    /// that name resolves at runtime. Workflow code references `payload`
    /// as a free name; without a binding `LoadName` fails with "variable
    /// 'payload' not found". This registers the name + value so manual
    /// runs (and trigger/webhook runs) can inject test event data.
    pub fn bind_input(&mut self, name: &str, value: Value) {
        if let Some(pos) = self.chunk.locals_names.iter().position(|n| n == name) {
            if pos >= self.locals.len() {
                self.locals.resize(pos + 1, Value::Unit);
            }
            self.locals[pos] = value;
        } else {
            self.chunk.locals_names.push(name.to_string());
            self.chunk.locals_count = self.chunk.locals_names.len() as u16;
            self.locals.push(value);
        }
    }

    /// Turn on execution tracing and name the entry (workflow) frame.
    pub fn enable_trace(&mut self, workflow_name: &str) {
        self.trace_enabled = true;
        self.current_func = workflow_name.to_string();
    }

    /// Whether the trace hit its cap and stopped recording further events.
    pub fn trace_truncated(&self) -> bool {
        self.trace.len() >= TRACE_CAP
    }

    /// Record one trace event (capped). No-op when tracing is off.
    fn push_trace(&mut self, kind: TraceKind, span: Option<(usize, usize)>, detail: Option<String>) {
        if !self.trace_enabled || self.trace.len() >= TRACE_CAP {
            return;
        }
        let step = self.trace.len() as u64;
        self.trace.push(TraceEvent {
            step,
            kind,
            function: self.current_func.clone(),
            span,
            depth: self.frames.len(),
            detail,
        });
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
        self.trace.clear();
        self.current_func = "workflow".to_string();
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
                Err(e) => {
                    // Tie the failure to the source of the statement it
                    // happened in so the trace points at the failing step.
                    let span = self.chunk.span_at(self.pc);
                    self.push_trace(TraceKind::Error, span, Some(e.clone()));
                    return Ok(StepResult::Failed(e));
                }
            };
            match result {
                InsResult::Continue => {
                    let boundary_pc = self.pc;
                    self.pc += 1;
                    if is_boundary {
                        if self.ignore_next_boundary {
                            self.ignore_next_boundary = false;
                        } else {
                            let span = self.chunk.span_at(boundary_pc);
                            self.push_trace(TraceKind::Stmt, span, None);
                            stmts_ran += 1;
                        }
                    }
                }
                InsResult::ContinueNoAdvance => {}
                InsResult::Returned(val) => {
                    // A `return` carries no trailing statement boundary (control
                    // leaves first), so record its Stmt event here against the
                    // return statement's source. self.pc still points at the
                    // Return instruction, which maps to that statement's span.
                    let span = self.chunk.span_at(self.pc);
                    self.push_trace(TraceKind::Stmt, span, None);
                    if self.frames.is_empty() {
                        // No caller frame: the workflow itself returned.
                        self.completed = true;
                        return Ok(StepResult::Completed(val));
                    }
                    // Returning from a user-function call. Record the return
                    // while the callee frame is still on the stack (so depth and
                    // function name are the callee's), then unwind: restore the
                    // caller's locals + pc and hand the value back on the stack.
                    self.push_trace(TraceKind::Return, span, Some(self.current_func.clone()));
                    let frame = self.frames.pop().unwrap();
                    self.current_func = frame.func;
                    self.locals = frame.locals;
                    self.pc = frame.return_pc;
                    self.stack.push(val);
                }
                InsResult::RemoteCall(cap, params) => {
                    self.step_count += 1;
                    // Record the external call against its source line before
                    // suspending. self.pc still points at the call instruction.
                    self.push_trace(
                        TraceKind::ExtCall,
                        self.chunk.span_at(self.pc),
                        Some(cap.clone()),
                    );
                    return Ok(StepResult::RemoteCall { capability: cap, params });
                }
                InsResult::CallFunc(name, args) => {
                    if let Some(func) = self.chunk.function(&name).cloned() {
                        // Real user-defined function call.
                        if args.len() != func.param_count as usize {
                            let e = format!(
                                "function '{}' expects {} argument(s), got {}",
                                name, func.param_count, args.len()
                            );
                            self.push_trace(TraceKind::Error, self.chunk.span_at(self.pc), Some(e.clone()));
                            return Ok(StepResult::Failed(e));
                        }
                        if self.frames.len() >= MAX_CALL_DEPTH {
                            let e = format!(
                                "call stack overflow calling '{}' (recursion too deep)",
                                name
                            );
                            self.push_trace(TraceKind::Error, self.chunk.span_at(self.pc), Some(e.clone()));
                            return Ok(StepResult::Failed(e));
                        }
                        // Bind args to the callee's local slots 0..param_count.
                        let mut callee_locals = vec![Value::Unit; func.locals_count as usize];
                        for (i, a) in args.into_iter().enumerate() {
                            callee_locals[i] = a;
                        }
                        // Record the helper call against the caller's source
                        // line, then swap the active function name to the callee
                        // (stashing the caller's name in the new frame).
                        self.push_trace(
                            TraceKind::Call,
                            self.chunk.span_at(self.pc),
                            Some(func.name.clone()),
                        );
                        let caller_locals = std::mem::replace(&mut self.locals, callee_locals);
                        self.frames.push(Frame {
                            return_pc: self.pc + 1,
                            locals: caller_locals,
                            func: std::mem::replace(&mut self.current_func, func.name.clone()),
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

    pub fn resolve_remote_call(&mut self, capability: &str, result: Value) {
        // Record the external call's return before advancing past the call
        // site (self.pc still points at the call instruction here).
        self.push_trace(
            TraceKind::ExtResult,
            self.chunk.span_at(self.pc),
            Some(capability.to_string()),
        );
        self.pending_result = Some(result);
        self.pending_call = None;
        self.pc += 1;
        self.ignore_next_boundary = true;
    }

    /// Record a trace error against the current instruction's source span.
    /// Used by the host when an external call is blocked or fails: the VM
    /// is suspended at the call site, so this ties the failure to the
    /// exact `call(...)` statement.
    pub fn trace_ext_error(&mut self, message: String) {
        self.push_trace(TraceKind::Error, self.chunk.span_at(self.pc), Some(message));
    }

    /// Source span of the instruction the VM is currently at, if mapped.
    /// Lets the host attribute an external-call failure to its call site.
    pub fn current_span(&self) -> Option<(usize, usize)> {
        self.chunk.span_at(self.pc)
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
    fn len_method_returns_array_length() {
        // `.len()` desugars to the `len` builtin with the receiver as arg.
        let (out, r) = run(r#"workflow "x" { let a = [10, 20, 30]; print(a.len()); }"#);
        assert!(r.is_ok(), "run failed: {r:?}");
        assert_eq!(out, vec!["3".to_string()]);
    }

    #[test]
    fn bare_workflow_name_and_upstream_aggregation() {
        // Upstream dialect: a bare-identifier workflow name, an unparenthesized
        // `if`, a `for ... in`, reassignment, `.len()` and division — the
        // load-balanced-ingest pattern, minus the external sensor calls.
        let (out, r) = run(
            r#"workflow aggregate {
                let temps = [20, 30, 10, 40];
                let sum = 0;
                let mx = 0;
                for t in temps {
                    sum = sum + t;
                    if t > mx { mx = t; }
                }
                print(sum / temps.len());
                print(mx);
            }"#,
        );
        assert!(r.is_ok(), "run failed: {r:?}");
        assert_eq!(out, vec!["25".to_string(), "40".to_string()]);
    }

    #[test]
    fn single_helper_call_returns_value() {
        let (_out, r) = run(r#"
            fn dbl(x: int) <- int { return x * 2; }
            workflow "w" { return dbl(21); }
        "#);
        assert_eq!(r, Ok(Value::Int(42)));
    }

    /// Drive a program to completion after binding `payload`.
    fn run_with_payload(src: &str, payload: Value) -> Result<Value, String> {
        let prog = Parser::new(src).parse().expect("parse");
        let chunk = Compiler::new().compile(&prog).expect("compile");
        let mut vm = Vm::new(chunk);
        vm.bind_input("payload", payload);
        let _ = take_output();
        let r = loop {
            match vm.step(1_000_000) {
                Ok(StepResult::Completed(v)) => break Ok(v),
                Ok(StepResult::Yielded(_)) => continue,
                Ok(StepResult::Failed(e)) => break Err(e),
                Err(e) => break Err(e),
                _ => break Err("unexpected".into()),
            }
        };
        let _ = take_output();
        r
    }

    #[test]
    fn unbound_payload_fails_clearly() {
        let (_o, r) = run(r#"workflow "w" { return payload.total; }"#);
        match r {
            Err(e) => assert!(e.contains("variable 'payload' not found"), "got: {e}"),
            Ok(v) => panic!("expected failure, got {v:?}"),
        }
    }

    #[test]
    fn bound_payload_resolves_and_member_access_works() {
        use std::collections::HashMap;
        let mut fields = HashMap::new();
        fields.insert("total".to_string(), Value::Int(1200));
        let r = run_with_payload(
            r#"workflow "w" { return payload.total; }"#,
            Value::Struct(fields),
        );
        assert_eq!(r, Ok(Value::Int(1200)));
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

#[cfg(test)]
mod trace_tests {
    use super::*;
    use crate::compiler::Compiler;
    use crate::parser::Parser;

    /// Run a program to completion with tracing on; return (trace, result).
    fn trace(src: &str) -> (Vec<TraceEvent>, Result<Value, String>) {
        let prog = Parser::new(src).parse().expect("parse");
        let chunk = Compiler::new().compile(&prog).expect("compile");
        let mut vm = Vm::new(chunk);
        vm.enable_trace("w");
        let _ = take_output();
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
        let _ = take_output();
        (vm.trace, result)
    }

    /// The slice of source a span points at.
    fn slice(src: &str, span: Option<(usize, usize)>) -> &str {
        let (s, e) = span.expect("span present");
        &src[s..e]
    }

    #[test]
    fn stmt_events_carry_real_source_spans() {
        let src = r#"workflow "w" { print("a"); let x: int = 1; return x; }"#;
        let (trace, r) = trace(src);
        assert_eq!(r, Ok(Value::Int(1)));
        // Every Stmt event should map to a non-empty slice of the source.
        let stmts: Vec<&TraceEvent> = trace.iter().filter(|e| e.kind == TraceKind::Stmt).collect();
        assert!(stmts.len() >= 3, "expected >=3 stmt events, got {}", stmts.len());
        assert!(slice(src, stmts[0].span).contains("print"));
        assert!(slice(src, stmts[1].span).contains("let x"));
    }

    #[test]
    fn never_empty_for_a_real_run() {
        let (trace, r) = trace(r#"workflow "w" { return 0; }"#);
        assert_eq!(r, Ok(Value::Int(0)));
        assert!(!trace.is_empty(), "trace must not be empty for a real run");
    }

    #[test]
    fn helper_call_emits_call_and_return_events() {
        let src = r#"
            fn dbl(x: int) <- int { return x * 2; }
            workflow "w" { return dbl(21); }
        "#;
        let (trace, r) = trace(src);
        assert_eq!(r, Ok(Value::Int(42)));
        let call = trace.iter().find(|e| e.kind == TraceKind::Call).expect("a Call event");
        // The Call event names the callee and is recorded at workflow depth 0.
        assert_eq!(call.detail.as_deref(), Some("dbl"));
        assert_eq!(call.function, "w");
        assert_eq!(call.depth, 0);
        let ret = trace.iter().find(|e| e.kind == TraceKind::Return).expect("a Return event");
        // The Return event is recorded while still inside the callee (depth 1).
        assert_eq!(ret.function, "dbl");
        assert_eq!(ret.depth, 1);
    }

    #[test]
    fn nested_helpers_nest_call_depth() {
        let src = r#"
            fn add(a: int, b: int) <- int { return a + b; }
            fn calc(x: int) <- int { return add(x, 10); }
            workflow "w" { return calc(5); }
        "#;
        let (trace, r) = trace(src);
        assert_eq!(r, Ok(Value::Int(15)));
        let calls: Vec<&TraceEvent> =
            trace.iter().filter(|e| e.kind == TraceKind::Call).collect();
        // calc called from the workflow (depth 0), add called from calc (depth 1).
        assert_eq!(calls.len(), 2, "expected 2 calls, got {}", calls.len());
        assert_eq!(calls[0].detail.as_deref(), Some("calc"));
        assert_eq!(calls[0].depth, 0);
        assert_eq!(calls[1].detail.as_deref(), Some("add"));
        assert_eq!(calls[1].depth, 1);
        // The deepest statement (inside add) runs at depth 2.
        let max_depth = trace.iter().map(|e| e.depth).max().unwrap();
        assert_eq!(max_depth, 2);
    }

    #[test]
    fn runtime_error_event_points_at_the_failing_statement() {
        let src = r#"
            fn risky(n: int) <- int { return n / 0; }
            workflow "w" { return risky(10); }
        "#;
        let (trace, r) = trace(src);
        assert!(r.is_err());
        let err = trace.iter().find(|e| e.kind == TraceKind::Error).expect("an Error event");
        assert!(err.detail.as_deref().unwrap_or("").contains("division by zero"));
        // The error is attributed to the helper it occurred in.
        assert_eq!(err.function, "risky");
        // and tied to the exact failing statement's source.
        assert!(slice(src, err.span).contains("n / 0"));
    }

    #[test]
    fn step_numbers_are_monotonic() {
        let (trace, _r) = trace(r#"
            fn f(x: int) <- int { return x; }
            workflow "w" { f(1); f(2); return 0; }
        "#);
        for (i, e) in trace.iter().enumerate() {
            assert_eq!(e.step, i as u64, "trace steps must be 0..N in order");
        }
    }

    #[test]
    fn recursion_trace_descends_then_unwinds() {
        let src = r#"
            fn fact(n: int) <- int {
                if (n < 2) { return 1; }
                return n * fact(n - 1);
            }
            workflow "w" { return fact(4); }
        "#;
        let (trace, r) = trace(src);
        assert_eq!(r, Ok(Value::Int(24)));
        let calls = trace.iter().filter(|e| e.kind == TraceKind::Call).count();
        let returns = trace.iter().filter(|e| e.kind == TraceKind::Return).count();
        // fact called 4 times (4,3,2,1); every call returns.
        assert_eq!(calls, 4, "expected 4 calls");
        assert_eq!(returns, 4, "expected 4 returns");
        // Recursion drives depth to 4 (workflow=0, then fact nests to 4).
        let max_depth = trace.iter().map(|e| e.depth).max().unwrap();
        assert_eq!(max_depth, 4);
    }

    /// Drive a program with tracing on, resolving every external call with
    /// `resolve` (capability -> value). Returns the trace.
    fn trace_with_ext(
        src: &str,
        mut resolve: impl FnMut(&str, &Value) -> Value,
    ) -> Vec<TraceEvent> {
        let prog = Parser::new(src).parse().expect("parse");
        let chunk = Compiler::new().compile(&prog).expect("compile");
        let mut vm = Vm::new(chunk);
        vm.enable_trace("w");
        let _ = take_output();
        loop {
            match vm.step(1_000_000) {
                Ok(StepResult::Completed(_)) => break,
                Ok(StepResult::Yielded(_)) => continue,
                Ok(StepResult::Failed(_)) => break,
                Ok(StepResult::RemoteCall { capability, params }) => {
                    let v = resolve(&capability, &params);
                    vm.resolve_remote_call(&capability, v);
                }
                Err(_) => break,
            }
        }
        let _ = take_output();
        vm.trace
    }

    #[test]
    fn external_call_emits_extcall_and_extresult() {
        let src = r#"
            import http;
            workflow "w" {
                let r: int = http.fetch({ url: "x" });
                return r;
            }
        "#;
        let trace = trace_with_ext(src, |_cap, _p| Value::Int(7));
        let call = trace.iter().find(|e| e.kind == TraceKind::ExtCall).expect("ExtCall");
        assert_eq!(call.detail.as_deref(), Some("http.fetch"));
        assert!(call.span.is_some(), "ext call must carry its source span");
        let res = trace.iter().find(|e| e.kind == TraceKind::ExtResult).expect("ExtResult");
        assert_eq!(res.detail.as_deref(), Some("http.fetch"));
        // The result event maps to the same call site as the call.
        assert_eq!(call.span, res.span);
    }

    #[test]
    fn blocked_external_call_error_points_at_call_site() {
        // No resolution path here: simulate the host blocking the call and
        // recording an error at the call site via trace_ext_error.
        let src = r#"
            import http;
            workflow "w" { http.fetch({ url: "x" }); return 0; }
        "#;
        let prog = Parser::new(src).parse().unwrap();
        let chunk = Compiler::new().compile(&prog).unwrap();
        let mut vm = Vm::new(chunk);
        vm.enable_trace("w");
        let _ = take_output();
        let mut blocked_span = None;
        loop {
            match vm.step(1_000_000) {
                Ok(StepResult::RemoteCall { capability, .. }) => {
                    blocked_span = vm.current_span();
                    vm.trace_ext_error(format!("external call '{capability}' is blocked"));
                    break;
                }
                Ok(StepResult::Yielded(_)) => continue,
                _ => break,
            }
        }
        let _ = take_output();
        assert!(blocked_span.is_some(), "current_span must expose the call site");
        let err = vm.trace.iter().find(|e| e.kind == TraceKind::Error).expect("Error event");
        assert!(err.detail.as_deref().unwrap_or("").contains("blocked"));
        assert_eq!(err.span, blocked_span, "error ties to the call site span");
        // The ExtCall was recorded before the block.
        assert!(vm.trace.iter().any(|e| e.kind == TraceKind::ExtCall));
    }

    #[test]
    fn tracing_off_records_nothing() {
        let prog = Parser::new(r#"workflow "w" { return 0; }"#).parse().unwrap();
        let chunk = Compiler::new().compile(&prog).unwrap();
        let mut vm = Vm::new(chunk);
        // No enable_trace() call.
        let _ = take_output();
        while let Ok(StepResult::Yielded(_)) = vm.step(1_000_000) {}
        let _ = take_output();
        assert!(vm.trace.is_empty(), "trace must stay empty when disabled");
    }
}
