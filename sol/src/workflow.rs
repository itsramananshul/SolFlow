use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::ast::*;
use crate::compiler::Compiler;
use crate::instruction::Chunk;
use crate::value::Value;
use crate::vm::{Vm, NativeFunc, StepResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    pub workflow_id: String,
    pub source: String,
    pub workflow_name: String,
    pub pc: usize,
    pub bindings: HashMap<String, Value>,
    pub step_count: u64,
    pub completed: bool,
    pub pending_call: Option<(String, Value)>,
    pub stack: Vec<Value>,
}

pub struct WorkflowExecutor {
    pub workflow: WorkflowDecl,
    pub pc: usize,
    pub bindings: HashMap<String, Value>,
    pub step_count: u64,
    pub vm: Vm,
    pub completed: bool,
    pub source: String,
    chunk: Chunk,
    locals_names: Vec<String>,
}

impl WorkflowExecutor {
    pub fn new(source: &str, workflow_name: &str) -> Result<Self, String> {
        let mut parser = crate::parser::Parser::new(source);
        let program = parser.parse()?;

        let mut compiler = Compiler::new();
        let chunk = compiler.compile(&program)?;
        let locals_names = chunk.locals_names.clone();

        let workflow = program.items
            .iter()
            .find_map(|item| {
                if let TopLevel::Workflow(w) = item {
                    if w.name == workflow_name {
                        return Some(w.clone());
                    }
                }
                None
            })
            .ok_or_else(|| format!("workflow '{}' not found", workflow_name))?;

        let vm = Vm::new(chunk.clone());

        Ok(Self {
            workflow,
            pc: 0,
            bindings: HashMap::new(),
            step_count: 0,
            vm,
            completed: false,
            source: source.to_string(),
            chunk,
            locals_names,
        })
    }

    pub fn from_state(state: &WorkflowState) -> Result<Self, String> {
        let mut exec = Self::new(&state.source, &state.workflow_name)?;
        exec.pc = state.pc;
        exec.bindings = state.bindings.clone();
        exec.step_count = state.step_count;
        exec.completed = state.completed;
        exec.vm.pc = state.pc;
        exec.vm.stack = state.stack.clone();
        exec.vm.pending_call = state.pending_call.clone();
        exec.vm.completed = state.completed;
        exec.vm.step_count = state.step_count;

        for (name, val) in &state.bindings {
            if let Some(slot) = exec.locals_names.iter().position(|n| n == name) {
                if slot < exec.vm.locals.len() {
                    exec.vm.locals[slot] = val.clone();
                }
            }
        }

        Ok(exec)
    }

    pub fn save(&self) -> WorkflowState {
        let snap = self.vm.save();
        let mut bindings = HashMap::new();
        for (i, name) in self.locals_names.iter().enumerate() {
            if i < snap.locals.len() {
                let val = &snap.locals[i];
                if !matches!(val, Value::Unit) || self.bindings.contains_key(name) {
                    bindings.insert(name.clone(), val.clone());
                }
            }
        }

        WorkflowState {
            workflow_id: String::new(),
            source: self.source.clone(),
            workflow_name: self.workflow.name.clone(),
            pc: snap.pc,
            bindings,
            step_count: snap.pc as u64,
            completed: self.completed || snap.pc >= self.chunk.instructions.len(),
            pending_call: self.vm.pending_call.clone(),
            stack: snap.stack,
        }
    }

    pub fn reset(&mut self) {
        self.pc = 0;
        self.bindings.clear();
        self.step_count = 0;
        self.completed = false;
        self.vm.reset();
    }

    pub fn step(&mut self, budget: u64) -> Result<StepResult, String> {
        let result = self.vm.step(budget)?;
        self.pc = self.vm.pc;
        self.step_count = self.vm.step_count;
        self.completed = self.vm.completed;

        self.bindings.clear();
        for (i, name) in self.locals_names.iter().enumerate() {
            if i < self.vm.locals.len() {
                let val = &self.vm.locals[i];
                if !matches!(val, Value::Unit) {
                    self.bindings.insert(name.clone(), val.clone());
                }
            }
        }

        Ok(result)
    }

    pub fn resolve_remote_call(&mut self, capability: &str, result: Value) -> Result<Value, String> {
        self.vm.resolve_remote_call(capability, result);
        self.pc = self.vm.pc;
        self.step_count = self.vm.step_count;
        Ok(Value::Unit)
    }

    pub fn register_native(&mut self, name: &str, func: NativeFunc) {
        self.vm.register_native(name, func);
    }

    pub fn name(&self) -> &str {
        &self.workflow.name
    }

    pub fn is_completed(&self) -> bool {
        self.completed || self.vm.completed
    }
}
