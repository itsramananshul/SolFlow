//! SOL bytecode VM — browser-safe variant.
//!
//! Vendored from the upstream sibling workspace's `vm.rs`
//! (snapshot 2026-05-27). See `../UPSTREAM.md` for the catalog
//! of edits. Behavior is canonical SOL semantics; only browser-
//! hostile sites (println, raw TCP, panics on common errors)
//! were replaced.

use crate::error::RunError;
use solflow_compiler::bytecode::Inst;
use solflow_compiler::parser::{Ast, Type};
use std::collections::HashMap;

const DEFAULT_STEP_LIMIT: usize = 1_000_000;

#[derive(Debug, Clone)]
pub enum HeapObject {
    String(String),
    Struct(Vec<u64>),
    Array(Vec<u64>),
}

struct Frame {
    return_ptr: usize,
    old_fp: usize,
}

pub struct VM {
    stack: Vec<u64>,
    heap: Vec<HeapObject>,
    call_stack: Vec<Frame>,
    inst_ptr: usize,
    fp: usize,
    program: Vec<Inst>,
    done: bool,
    pub fn_entries: HashMap<String, usize>,

    /// Captured `print` output, in canonical order. Each push is
    /// what the upstream VM would have written via `println!`,
    /// minus the trailing newline (the consumer typically renders
    /// each entry as its own row).
    pub output: Vec<String>,

    /// Maximum number of `step()` calls before `run()` aborts with
    /// `RunError::StepLimit`. Browser-safety guard; canonical CLI
    /// VM has no such limit. Configure with `with_step_limit`.
    pub step_limit: usize,

    /// Number of `step()` invocations made on this VM since
    /// construction. Reset doesn't exist on purpose — runs are
    /// always one-shot from a fresh VM.
    pub steps: usize,
}

impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}

impl VM {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(512),
            heap: Vec::with_capacity(128),
            call_stack: Vec::with_capacity(64),
            inst_ptr: 0,
            fp: 0,
            program: Vec::new(),
            done: false,
            fn_entries: HashMap::new(),
            output: Vec::new(),
            step_limit: DEFAULT_STEP_LIMIT,
            steps: 0,
        }
    }

    pub fn from(program: &[Inst]) -> Self {
        Self {
            program: program.to_vec(),
            ..Self::new()
        }
    }

    pub fn with_step_limit(mut self, limit: usize) -> Self {
        self.step_limit = limit;
        self
    }

    pub fn heap_push_string(&mut self, s: String) -> u64 {
        let idx = self.heap.len();
        self.heap.push(HeapObject::String(s));
        idx as u64
    }

    /// Run to completion, returning the top-of-stack value or
    /// the first `RunError` encountered.
    pub fn run(&mut self) -> Result<u64, RunError> {
        loop {
            if self.steps >= self.step_limit {
                return Err(RunError::StepLimit { limit: self.step_limit });
            }
            self.steps += 1;
            match self.step()? {
                Some(v) => return Ok(v),
                None => continue,
            }
        }
    }

    fn pop(&mut self) -> Result<u64, RunError> {
        self.stack.pop().ok_or(RunError::StackUnderflow)
    }

    fn push(&mut self, val: u64) {
        self.stack.push(val);
    }

    pub fn step(&mut self) -> Result<Option<u64>, RunError> {
        if self.done {
            return Ok(None);
        }

        if self.inst_ptr >= self.program.len() {
            self.done = true;
            // Natural end-of-program: return whatever's on top of
            // stack (or 0 if empty).
            return Ok(Some(self.stack.pop().unwrap_or(0)));
        }

        let inst = self.program[self.inst_ptr].clone();
        self.inst_ptr += 1;
        match inst {
            // --- 1. Data Transport & Storage ---
            Inst::PushConst(ast_node) => {
                let bits = match ast_node {
                    Ast::ExprInteger(v) => v as u64,
                    Ast::ExprFloat(v) => v.to_bits(),
                    Ast::ExprChar(v) => v as u64,
                    Ast::ExprBool(v) => if v { 1 } else { 0 },
                    Ast::ExprUndefined => 0,
                    Ast::ExprString(s) => {
                        self.heap.push(HeapObject::String(s.clone()));
                        (self.heap.len() - 1) as u64
                    }
                    // Compiler-bug class: bytecode shouldn't contain
                    // non-literal AST in PushConst. Stay as panic so
                    // the WASM boundary surfaces it as ICE.
                    _ => panic!("Runtime Error: Invalid constant AST node passed to VM"),
                };
                self.push(bits);
            }

            Inst::LoadLocal(offset) => {
                let idx = (self.fp as isize + offset) as usize;
                let val = self.stack[idx];
                self.push(val);
            }

            Inst::StoreLocal(offset) => {
                let val = self.pop()?;
                let idx = (self.fp as isize + offset) as usize;
                while self.stack.len() <= idx {
                    self.stack.push(0);
                }
                self.stack[idx] = val;
            }

            Inst::Pop => {
                self.pop()?;
            }

            Inst::Dup => {
                let val = *self.stack.last().ok_or(RunError::StackUnderflow)?;
                self.push(val);
            }

            // --- 2. Integer Math & Comparisons ---
            Inst::IntAdd => { let b = self.pop()? as i64; let a = self.pop()? as i64; self.push((a + b) as u64); }
            Inst::IntSub => { let b = self.pop()? as i64; let a = self.pop()? as i64; self.push((a - b) as u64); }
            Inst::IntMul => { let b = self.pop()? as i64; let a = self.pop()? as i64; self.push((a * b) as u64); }
            Inst::IntDiv => {
                let b = self.pop()? as i64;
                let a = self.pop()? as i64;
                if b == 0 {
                    return Err(RunError::DivByZero);
                }
                self.push((a / b) as u64);
            }

            Inst::IntEq  => { let b = self.pop()? as i64; let a = self.pop()? as i64; self.push(if a == b { 1 } else { 0 }); }
            Inst::IntNeq => { let b = self.pop()? as i64; let a = self.pop()? as i64; self.push(if a != b { 1 } else { 0 }); }
            Inst::IntGt  => { let b = self.pop()? as i64; let a = self.pop()? as i64; self.push(if a > b { 1 } else { 0 }); }
            Inst::IntGte => { let b = self.pop()? as i64; let a = self.pop()? as i64; self.push(if a >= b { 1 } else { 0 }); }
            Inst::IntLt  => { let b = self.pop()? as i64; let a = self.pop()? as i64; self.push(if a < b { 1 } else { 0 }); }
            Inst::IntLte => { let b = self.pop()? as i64; let a = self.pop()? as i64; self.push(if a <= b { 1 } else { 0 }); }

            // --- 3. Floating-Point Math & Comparisons ---
            Inst::FloatAdd => { let b = f64::from_bits(self.pop()?); let a = f64::from_bits(self.pop()?); self.push((a + b).to_bits()); }
            Inst::FloatSub => { let b = f64::from_bits(self.pop()?); let a = f64::from_bits(self.pop()?); self.push((a - b).to_bits()); }
            Inst::FloatMul => { let b = f64::from_bits(self.pop()?); let a = f64::from_bits(self.pop()?); self.push((a * b).to_bits()); }
            Inst::FloatDiv => { let b = f64::from_bits(self.pop()?); let a = f64::from_bits(self.pop()?); self.push((a / b).to_bits()); }

            Inst::FloatEq  => { let b = f64::from_bits(self.pop()?); let a = f64::from_bits(self.pop()?); self.push(if a == b { 1 } else { 0 }); }
            Inst::FloatNeq => { let b = f64::from_bits(self.pop()?); let a = f64::from_bits(self.pop()?); self.push(if a != b { 1 } else { 0 }); }
            Inst::FloatGt  => { let b = f64::from_bits(self.pop()?); let a = f64::from_bits(self.pop()?); self.push(if a > b { 1 } else { 0 }); }
            Inst::FloatGte => { let b = f64::from_bits(self.pop()?); let a = f64::from_bits(self.pop()?); self.push(if a >= b { 1 } else { 0 }); }
            Inst::FloatLt  => { let b = f64::from_bits(self.pop()?); let a = f64::from_bits(self.pop()?); self.push(if a < b { 1 } else { 0 }); }
            Inst::FloatLte => { let b = f64::from_bits(self.pop()?); let a = f64::from_bits(self.pop()?); self.push(if a <= b { 1 } else { 0 }); }

            // --- 4. Char Comparisons ---
            Inst::CharEq  => { let b = self.pop()?; let a = self.pop()?; self.push(if a == b { 1 } else { 0 }); }
            Inst::CharNeq => { let b = self.pop()?; let a = self.pop()?; self.push(if a != b { 1 } else { 0 }); }
            Inst::CharGt  => { let b = self.pop()?; let a = self.pop()?; self.push(if a > b { 1 } else { 0 }); }
            Inst::CharGte => { let b = self.pop()?; let a = self.pop()?; self.push(if a >= b { 1 } else { 0 }); }
            Inst::CharLt  => { let b = self.pop()?; let a = self.pop()?; self.push(if a < b { 1 } else { 0 }); }
            Inst::CharLte => { let b = self.pop()?; let a = self.pop()?; self.push(if a <= b { 1 } else { 0 }); }

            // --- 5. Logical & Bitwise ---
            Inst::LogOr   => { let b = self.pop()?; let a = self.pop()?; self.push(if a == 1 || b == 1 { 1 } else { 0 }); }
            Inst::LogAnd  => { let b = self.pop()?; let a = self.pop()?; self.push(if a == 1 && b == 1 { 1 } else { 0 }); }
            Inst::LogNot  => { let a = self.pop()?; self.push(if a == 0 { 1 } else { 0 }); }

            Inst::BitXor    => { let b = self.pop()?; let a = self.pop()?; self.push(a ^ b); }
            Inst::BitAnd    => { let b = self.pop()?; let a = self.pop()?; self.push(a & b); }
            Inst::BitOr     => { let b = self.pop()?; let a = self.pop()?; self.push(a | b); }
            Inst::BitNeg    => { let a = self.pop()?; self.push(!a); }
            Inst::BitLShift => { let b = self.pop()?; let a = self.pop()?; self.push(a << b); }
            Inst::BitRShift => { let b = self.pop()?; let a = self.pop()?; self.push(a >> b); }

            // --- 6. Compound Structures (Heap Interaction) ---
            Inst::NewStruct(fields) => {
                let mut elements = vec![0; fields];
                for i in (0..fields).rev() {
                    elements[i] = self.pop()?;
                }
                self.heap.push(HeapObject::Struct(elements));
                self.push((self.heap.len() - 1) as u64);
            }

            Inst::GetField(idx) => {
                let struct_ref = self.pop()? as usize;
                // Two-step lookup: validate heap-shape first, then
                // bounds-check the field index. Both failure modes
                // surface as structured RunErrors (B.11 c32: GetField
                // previously panicked on field-OOB).
                match self.heap.get(struct_ref) {
                    Some(HeapObject::Struct(fields)) => {
                        if idx >= fields.len() {
                            return Err(RunError::IndexOutOfBounds {
                                index: idx,
                                length: fields.len(),
                            });
                        }
                        self.push(fields[idx]);
                    }
                    Some(other) => return Err(RunError::HeapShapeMismatch {
                        expected: "Struct",
                        got: heap_kind(other),
                    }),
                    None => return Err(RunError::HeapShapeMismatch {
                        expected: "Struct",
                        got: "dangling-ref",
                    }),
                }
            }

            Inst::SetField(idx) => {
                let struct_ref = self.pop()? as usize;
                let value = self.pop()?;
                match self.heap.get_mut(struct_ref) {
                    Some(HeapObject::Struct(fields)) => {
                        if idx >= fields.len() {
                            return Err(RunError::IndexOutOfBounds {
                                index: idx,
                                length: fields.len(),
                            });
                        }
                        fields[idx] = value;
                    }
                    Some(other) => return Err(RunError::HeapShapeMismatch {
                        expected: "Struct",
                        got: heap_kind(other),
                    }),
                    None => return Err(RunError::HeapShapeMismatch {
                        expected: "Struct",
                        got: "dangling-ref",
                    }),
                }
                self.push(value);
            }

            Inst::NewArray => {
                let size = self.pop()? as usize;
                self.heap.push(HeapObject::Array(vec![0; size]));
                self.push((self.heap.len() - 1) as u64);
            }

            Inst::ArrayLen => {
                let arr_ref = self.pop()? as usize;
                match self.heap.get(arr_ref) {
                    Some(HeapObject::Array(items)) => self.push(items.len() as u64),
                    Some(other) => return Err(RunError::HeapShapeMismatch {
                        expected: "Array",
                        got: heap_kind(other),
                    }),
                    None => return Err(RunError::HeapShapeMismatch {
                        expected: "Array",
                        got: "dangling-ref",
                    }),
                }
            }

            Inst::GetElem => {
                let idx = self.pop()? as usize;
                let arr_ref = self.pop()? as usize;
                let items = match self.heap.get(arr_ref) {
                    Some(HeapObject::Array(items)) => items,
                    Some(other) => return Err(RunError::HeapShapeMismatch {
                        expected: "Array",
                        got: heap_kind(other),
                    }),
                    None => return Err(RunError::HeapShapeMismatch {
                        expected: "Array",
                        got: "dangling-ref",
                    }),
                };
                if idx >= items.len() {
                    return Err(RunError::IndexOutOfBounds { index: idx, length: items.len() });
                }
                self.push(items[idx]);
            }

            Inst::SetElem => {
                let value = self.pop()?;
                let idx = self.pop()? as usize;
                let arr_ref = self.pop()? as usize;
                let items = match self.heap.get_mut(arr_ref) {
                    Some(HeapObject::Array(items)) => items,
                    Some(other) => return Err(RunError::HeapShapeMismatch {
                        expected: "Array",
                        got: heap_kind(other),
                    }),
                    None => return Err(RunError::HeapShapeMismatch {
                        expected: "Array",
                        got: "dangling-ref",
                    }),
                };
                if idx >= items.len() {
                    return Err(RunError::IndexOutOfBounds { index: idx, length: items.len() });
                }
                items[idx] = value;
                self.push(value);
            }

            Inst::ConcatStr => {
                let idx2 = self.pop()? as usize;
                let idx1 = self.pop()? as usize;
                let s1 = match self.heap.get(idx1) {
                    Some(HeapObject::String(s)) => s.clone(),
                    _ => return Err(RunError::HeapShapeMismatch { expected: "String", got: "other" }),
                };
                let s2 = match self.heap.get(idx2) {
                    Some(HeapObject::String(s)) => s.clone(),
                    _ => return Err(RunError::HeapShapeMismatch { expected: "String", got: "other" }),
                };
                let merged = format!("{}{}", s1, s2);
                self.heap.push(HeapObject::String(merged));
                self.push((self.heap.len() - 1) as u64);
            }

            Inst::EqStr => {
                let idx2 = self.pop()? as usize;
                let idx1 = self.pop()? as usize;
                let s1 = match self.heap.get(idx1) {
                    Some(HeapObject::String(s)) => s.clone(),
                    _ => return Err(RunError::HeapShapeMismatch { expected: "String", got: "other" }),
                };
                let s2 = match self.heap.get(idx2) {
                    Some(HeapObject::String(s)) => s.clone(),
                    _ => return Err(RunError::HeapShapeMismatch { expected: "String", got: "other" }),
                };
                self.push(if s1 == s2 { 1 } else { 0 });
            }

            // --- 7. Control Flow & Jumps ---
            Inst::Jump(target) => {
                self.inst_ptr = target;
            }

            Inst::JumpFalse(target) => {
                if self.pop()? == 0 {
                    self.inst_ptr = target;
                }
            }

            Inst::Call(target, arg_count) => {
                self.call_stack.push(Frame {
                    return_ptr: self.inst_ptr,
                    old_fp: self.fp,
                });
                self.fp = self.stack.len() - arg_count;
                self.inst_ptr = target;
            }

            Inst::Ret => {
                if let Some(frame) = self.call_stack.pop() {
                    self.stack.truncate(self.fp);
                    self.fp = frame.old_fp;
                    self.inst_ptr = frame.return_ptr;
                    self.push(0);
                } else {
                    self.done = true;
                    return Ok(Some(self.pop().unwrap_or(0)));
                }
            }

            Inst::RetVal => {
                let return_value = self.pop()?;
                if let Some(frame) = self.call_stack.pop() {
                    self.stack.truncate(self.fp);
                    self.fp = frame.old_fp;
                    self.inst_ptr = frame.return_ptr;
                    self.push(return_value);
                } else {
                    self.done = true;
                    return Ok(Some(return_value));
                }
            }

            // --- 8. System Outputs ---
            //
            // EDIT vs. upstream: `println!` + `io::stdout().flush()`
            // replaced with capture into `self.output`. Caller
            // surfaces the buffer back to the user.
            Inst::PrintInt => {
                let v = self.pop()? as i64;
                self.output.push(format!("{v}"));
                self.push(0);
            }
            Inst::PrintFloat => {
                let v = f64::from_bits(self.pop()?);
                self.output.push(format!("{v}"));
                self.push(0);
            }
            Inst::PrintChar => {
                let c = char::from_u32(self.pop()? as u32).unwrap_or('?');
                self.output.push(format!("{c}"));
                self.push(0);
            }
            Inst::PrintString => {
                let idx = self.pop()? as usize;
                if let Some(HeapObject::String(s)) = self.heap.get(idx) {
                    self.output.push(s.clone());
                } else {
                    return Err(RunError::HeapShapeMismatch { expected: "String", got: "other" });
                }
                self.push(0);
            }

            // --- 9. RPC Message Serialization (browser-safe) ---
            //
            // These are pure JSON serialization. No network I/O.
            // Vendored verbatim from upstream, only the panics on
            // bad heap shapes are converted to RunError.
            Inst::SerializeRequest(ref elem_type) => {
                let args_ref = self.pop()? as usize;
                let name_ref = self.pop()? as usize;

                let name = match self.heap.get(name_ref) {
                    Some(HeapObject::String(s)) => s.clone(),
                    _ => return Err(RunError::HeapShapeMismatch { expected: "String", got: "other" }),
                };
                let items = match self.heap.get(args_ref) {
                    Some(HeapObject::Array(items)) => items.clone(),
                    _ => return Err(RunError::HeapShapeMismatch { expected: "Array", got: "other" }),
                };

                let args_json: Vec<serde_json::Value> = items
                    .iter()
                    .map(|&val| serialize_value(elem_type, val, &self.heap))
                    .collect();

                let json = serde_json::json!({
                    "type": "request",
                    "name": name,
                    "args": args_json,
                });
                let json_str = serde_json::to_string(&json).unwrap_or_default();
                self.heap.push(HeapObject::String(json_str));
                self.push((self.heap.len() - 1) as u64);
            }

            Inst::SerializeResponse(ref data_type) => {
                let val = self.pop()?;
                let data_value = serialize_value(data_type, val, &self.heap);
                let json = serde_json::json!({
                    "type": "response",
                    "data": data_value,
                });
                let json_str = serde_json::to_string(&json).unwrap_or_default();
                self.heap.push(HeapObject::String(json_str));
                self.push((self.heap.len() - 1) as u64);
            }

            Inst::DeserializeRequestName => {
                let msg_ref = self.pop()? as usize;
                let msg = match self.heap.get(msg_ref) {
                    Some(HeapObject::String(s)) => s.clone(),
                    _ => return Err(RunError::HeapShapeMismatch { expected: "String", got: "other" }),
                };
                let value: serde_json::Value = serde_json::from_str(&msg).unwrap_or(serde_json::Value::Null);
                let name = value["name"].as_str().unwrap_or("").to_string();
                self.heap.push(HeapObject::String(name));
                self.push((self.heap.len() - 1) as u64);
            }

            Inst::DeserializeRequestArgs => {
                let msg_ref = self.pop()? as usize;
                let msg = match self.heap.get(msg_ref) {
                    Some(HeapObject::String(s)) => s.clone(),
                    _ => return Err(RunError::HeapShapeMismatch { expected: "String", got: "other" }),
                };
                let value: serde_json::Value = serde_json::from_str(&msg).unwrap_or(serde_json::Value::Null);
                let args_str = value["args"].to_string();
                self.heap.push(HeapObject::String(args_str));
                self.push((self.heap.len() - 1) as u64);
            }

            Inst::DeserializeResponseData => {
                let msg_ref = self.pop()? as usize;
                let msg = match self.heap.get(msg_ref) {
                    Some(HeapObject::String(s)) => s.clone(),
                    _ => return Err(RunError::HeapShapeMismatch { expected: "String", got: "other" }),
                };
                let value: serde_json::Value = serde_json::from_str(&msg).unwrap_or(serde_json::Value::Null);
                let data_str = serde_json::to_string(&value["data"]).unwrap_or_default();
                self.heap.push(HeapObject::String(data_str));
                self.push((self.heap.len() - 1) as u64);
            }

            // --- 10. External Function Call (browser-blocked) ---
            //
            // EDIT vs. upstream: instead of opening a real TCP
            // socket and speaking HTTP/1.1, we refuse the
            // operation and return a structured error. The
            // editor renders an honest "external call not
            // available in browser simulation" message and
            // execution halts.
            Inst::ExtCall(ref _arg_types, ref _ret_type) => {
                let url_idx = self.pop()? as usize;
                let name_idx = self.pop()? as usize;
                let url = match self.heap.get(url_idx) {
                    Some(HeapObject::String(s)) => s.clone(),
                    _ => "<unknown>".to_string(),
                };
                let function_name = match self.heap.get(name_idx) {
                    Some(HeapObject::String(s)) => s.clone(),
                    _ => "<unknown>".to_string(),
                };
                return Err(RunError::ExtCallBlocked { function_name, url });
            }
        }

        Ok(None)
    }
}

fn heap_kind(h: &HeapObject) -> &'static str {
    match h {
        HeapObject::String(_) => "String",
        HeapObject::Struct(_) => "Struct",
        HeapObject::Array(_) => "Array",
    }
}

/// Convert a `u64` stack value back to a JSON value according
/// to its declared type. Shared by `SerializeRequest` and
/// `SerializeResponse`. Mirrors upstream behavior verbatim.
fn serialize_value(ty: &Type, val: u64, heap: &[HeapObject]) -> serde_json::Value {
    match ty {
        Type::String => match heap.get(val as usize) {
            Some(HeapObject::String(s)) => serde_json::Value::String(s.clone()),
            _ => serde_json::Value::Null,
        },
        Type::Integer => serde_json::Value::Number((val as i64).into()),
        Type::Float => serde_json::Number::from_f64(f64::from_bits(val))
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Type::Bool => serde_json::Value::Bool(val != 0),
        Type::Char => {
            let c = char::from_u32(val as u32).unwrap_or('?');
            serde_json::Value::String(c.to_string())
        }
        _ => serde_json::Value::String(format!("{val:?}")),
    }
}
