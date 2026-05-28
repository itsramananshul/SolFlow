//! SOL bytecode VM — browser-safe variant.
//!
//! Vendored from the upstream sibling workspace's `vm.rs`
//! (snapshot 2026-05-27). See `../UPSTREAM.md` for the catalog
//! of edits. Behavior is canonical SOL semantics; only browser-
//! hostile sites (println, raw TCP, panics on common errors)
//! were replaced.

use crate::error::RunError;
use crate::extcall::{
    try_ext_call_type, ExtCallContext, ExtCallError, ExtCallHandlerArc,
    ExtCallType, ExtCallValue,
};
use solflow_compiler::bytecode::Inst;
use solflow_compiler::parser::{Ast, Type};
use std::collections::HashMap;
use std::sync::Arc;

const DEFAULT_STEP_LIMIT: usize = 1_000_000;

/// Type alias for the `Inst::Print*` callback. Receives the line
/// just pushed to `self.output` plus the `inst_ptr` of the
/// Print instruction (so the host can look up the source span
/// via its `instruction_spans` sidecar).
pub type PrintCallback = Arc<dyn Fn(&str, usize) + Send + Sync>;
/// Default cap on captured trace entries when tracing is enabled.
/// SOL programs can run millions of instructions; bounded memory
/// matters more than full history for the editor UX (truncated
/// trace surfaces a `trace_truncated: true` flag in the envelope).
pub const DEFAULT_TRACE_LIMIT: usize = 10_000;

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

    // -------- Optional execution trace (B.D2 / c42) --------

    /// When tracing is enabled, the inst_ptr of each executed
    /// instruction is appended here. Adjacent-equal entries are
    /// NOT de-duplicated (a tight loop produces many entries with
    /// the same inst_ptr) — the bridge does span-level de-dup
    /// for the UX surface. Bounded by `trace_limit` to keep
    /// memory predictable.
    pub trace: Vec<usize>,

    /// When the trace cap is hit, recording silently stops and
    /// this flag flips. The bridge surfaces it so the UI can
    /// render "trace truncated at N steps."
    pub trace_truncated: bool,

    /// Tracing is opt-in. Default `None` means no recording (no
    /// per-step push, no memory cost). `Some(limit)` records up
    /// to `limit` entries.
    pub trace_limit: Option<usize>,

    /// When a run ends in error, the inst_ptr of the offending
    /// instruction is captured here. The bridge uses it to attach
    /// a source span to the runtime error so the editor can
    /// scroll the source pane to the failure site.
    pub error_inst_ptr: Option<usize>,

    /// Optional `Inst::ExtCall` handler (Phase C C.4 c76). When
    /// `None`, ExtCall returns `RunError::ExtCallBlocked` as it
    /// has since C.1. When `Some`, the VM marshals args + invokes
    /// the handler + pushes the typed return value.
    pub ext_call_handler: Option<ExtCallHandlerArc>,

    /// Optional callback fired EVERY time a Print-family
    /// instruction (`PrintInt` / `PrintFloat` / `PrintChar` /
    /// `PrintString`) appends a line to `self.output`. The
    /// callback receives the line that was just pushed plus the
    /// `inst_ptr` of the Print instruction (lets the controller
    /// look up the source span via `instruction_spans`).
    ///
    /// When `None` (browser-sim path), no overhead per print.
    /// When `Some`, the controller installs a callback that
    /// turns each line into a real-time `RunEvent::Print`
    /// (Phase C C.5 c82).
    pub print_callback: Option<Arc<dyn Fn(&str, usize) + Send + Sync>>,
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
            trace: Vec::new(),
            trace_truncated: false,
            trace_limit: None,
            error_inst_ptr: None,
            ext_call_handler: None,
            print_callback: None,
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

    /// Enable execution-trace recording. Pass `None` for the
    /// default cap (`DEFAULT_TRACE_LIMIT`); pass `Some(n)` for
    /// a custom one. With no call to this, no trace is recorded
    /// (zero overhead).
    pub fn with_trace(mut self, limit: Option<usize>) -> Self {
        self.trace_limit = Some(limit.unwrap_or(DEFAULT_TRACE_LIMIT));
        // Best-effort reserve so trace pushes don't realloc.
        self.trace.reserve(self.trace_limit.unwrap_or(0));
        self
    }

    /// Fire the print callback if installed. Called by every
    /// Print-family instruction just before appending the line
    /// to `self.output`. Cheap when no callback is installed.
    fn emit_print(&self, line: &str) {
        if let Some(cb) = &self.print_callback {
            cb(line, self.inst_ptr);
        }
    }

    pub fn heap_push_string(&mut self, s: String) -> u64 {
        let idx = self.heap.len();
        self.heap.push(HeapObject::String(s));
        idx as u64
    }

    /// Run to completion, returning the top-of-stack value or
    /// the first `RunError` encountered. When tracing is enabled
    /// (`with_trace`), the inst_ptr of each executed instruction
    /// is appended to `self.trace` before `step()` runs.
    pub fn run(&mut self) -> Result<u64, RunError> {
        loop {
            if self.steps >= self.step_limit {
                return Err(RunError::StepLimit { limit: self.step_limit });
            }
            self.steps += 1;
            // Capture trace BEFORE step() advances inst_ptr — we
            // want the IP of the instruction about to execute.
            // EOF / done is detected inside step(); the trace push
            // is harmless if the program is about to end.
            if let Some(limit) = self.trace_limit {
                if self.trace.len() < limit {
                    self.trace.push(self.inst_ptr);
                } else if !self.trace_truncated {
                    self.trace_truncated = true;
                }
            }
            // The inst_ptr that's about to execute — captured for
            // error-attribution if step() returns an error.
            let pre_step_ip = self.inst_ptr;
            match self.step() {
                Ok(Some(v)) => return Ok(v),
                Ok(None) => continue,
                Err(e) => {
                    self.error_inst_ptr = Some(pre_step_ip);
                    return Err(e);
                }
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
                let line = format!("{v}");
                self.emit_print(&line);
                self.output.push(line);
                self.push(0);
            }
            Inst::PrintFloat => {
                let v = f64::from_bits(self.pop()?);
                let line = format!("{v}");
                self.emit_print(&line);
                self.output.push(line);
                self.push(0);
            }
            Inst::PrintChar => {
                let c = char::from_u32(self.pop()? as u32).unwrap_or('?');
                let line = format!("{c}");
                self.emit_print(&line);
                self.output.push(line);
                self.push(0);
            }
            Inst::PrintString => {
                let idx = self.pop()? as usize;
                let line = match self.heap.get(idx) {
                    Some(HeapObject::String(s)) => s.clone(),
                    _ => {
                        return Err(RunError::HeapShapeMismatch { expected: "String", got: "other" });
                    }
                };
                self.emit_print(&line);
                self.output.push(line);
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

            // --- 10. External Function Call ---
            //
            // EDIT vs. upstream: instead of speaking HTTP/1.1
            // directly, the VM dispatches to an optional
            // `ExtCallHandler`. The browser-sim path installs no
            // handler → falls back to `ExtCallBlocked` exactly
            // like before. The controller installs a handler that
            // dispatches to its connector registry (Phase C C.4).
            //
            // The stack on entry (top first) is:
            //   url, function_name, arg_{n-1}, ..., arg_0
            //
            // For args we decode each raw u64 according to the
            // compile-time `arg_types` slot. Compound types are
            // not yet supported at this boundary — they surface
            // as `ExtCallFailed { … unsupported … }`.
            Inst::ExtCall(ref arg_types, ref ret_type) => {
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

                let Some(handler) = self.ext_call_handler.clone() else {
                    // C.1 / browser-sim path — pop args off the
                    // stack so we leave a clean state, then return
                    // the blocked error.
                    for _ in 0..arg_types.len() {
                        let _ = self.pop()?;
                    }
                    return Err(RunError::ExtCallBlocked { function_name, url });
                };

                // Decode args in REVERSE pop order so the handler
                // receives them in compile-time order.
                let mut popped: Vec<u64> = Vec::with_capacity(arg_types.len());
                for _ in 0..arg_types.len() {
                    popped.push(self.pop()?);
                }
                popped.reverse();
                let mut args: Vec<ExtCallValue> = Vec::with_capacity(arg_types.len());
                for (raw, ty) in popped.iter().zip(arg_types.iter()) {
                    match decode_extcall_arg(*raw, ty, &self.heap) {
                        Ok(v) => args.push(v),
                        Err(e) => {
                            return Err(RunError::from(map_extcall_err(
                                e,
                                &function_name,
                            )));
                        }
                    }
                }
                let ret_ty = match try_ext_call_type(ret_type) {
                    Ok(t) => t,
                    Err(e) => {
                        return Err(RunError::from(map_extcall_err(e, &function_name)));
                    }
                };

                let outcome = handler.handle(ExtCallContext {
                    function_name: &function_name,
                    url: &url,
                    args: &args,
                    ret_type: ret_ty,
                });
                let value = match outcome {
                    Ok(v) => v,
                    Err(e) => {
                        return Err(RunError::from(map_extcall_err(e, &function_name)));
                    }
                };

                // Encode return value back onto the VM stack /
                // heap as appropriate for its type.
                match (value, ret_ty) {
                    (ExtCallValue::Void, ExtCallType::Void) => {
                        // No push.
                    }
                    (ExtCallValue::Int(n), ExtCallType::Int) => self.push(n as u64),
                    (ExtCallValue::Float(f), ExtCallType::Float) => self.push(f.to_bits()),
                    (ExtCallValue::Bool(b), ExtCallType::Bool) => {
                        self.push(if b { 1 } else { 0 });
                    }
                    (ExtCallValue::String(s), ExtCallType::String) => {
                        let idx = self.heap_push_string(s);
                        self.push(idx);
                    }
                    (got, expected) => {
                        return Err(RunError::from(map_extcall_err(
                            ExtCallError::TypeMismatch {
                                expected,
                                got: got.ty(),
                            },
                            &function_name,
                        )));
                    }
                }
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

/// Decode a raw u64 stack value back into a typed `ExtCallValue`
/// for handler dispatch (C.4 c76).
fn decode_extcall_arg(
    raw: u64,
    ty: &Type,
    heap: &[HeapObject],
) -> Result<ExtCallValue, ExtCallError> {
    match try_ext_call_type(ty)? {
        ExtCallType::Int => Ok(ExtCallValue::Int(raw as i64)),
        ExtCallType::Float => Ok(ExtCallValue::Float(f64::from_bits(raw))),
        ExtCallType::Bool => Ok(ExtCallValue::Bool(raw != 0)),
        ExtCallType::String => {
            let s = match heap.get(raw as usize) {
                Some(HeapObject::String(s)) => s.clone(),
                Some(other) => {
                    return Err(ExtCallError::Unsupported {
                        reason: format!(
                            "expected String on heap for ExtCall arg, got {}",
                            heap_kind(other)
                        ),
                    });
                }
                None => {
                    return Err(ExtCallError::Unsupported {
                        reason: format!("invalid heap index {raw} for ExtCall string arg"),
                    });
                }
            };
            Ok(ExtCallValue::String(s))
        }
        ExtCallType::Void => Err(ExtCallError::Unsupported {
            reason: "void cannot appear in ExtCall arg position".into(),
        }),
    }
}

/// Patch the connector / function_name fields of an
/// `ExtCallError::Unsupported` or `TypeMismatch` so the eventual
/// `RunError::ExtCallFailed` carries the actual SOL function name
/// rather than `(unknown)`.
fn map_extcall_err(e: ExtCallError, fn_name: &str) -> ExtCallError {
    match e {
        ExtCallError::Unsupported { reason } => ExtCallError::Failed {
            connector: "(runtime)".into(),
            fn_name: fn_name.to_string(),
            message: reason,
        },
        ExtCallError::TypeMismatch { expected, got } => ExtCallError::Failed {
            connector: "(runtime)".into(),
            fn_name: fn_name.to_string(),
            message: format!(
                "ExtCall return type mismatch: expected {}, got {}",
                expected.name(),
                got.name()
            ),
        },
        other => other,
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
