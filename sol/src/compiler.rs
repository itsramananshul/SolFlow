use std::collections::HashSet;
use crate::ast::*;
use crate::instruction::{Chunk, FuncInfo, Instruction};
use crate::value::Value;

pub struct Compiler {
    imports: HashSet<String>,
    functions: HashSet<String>,
    /// `(start_pc, end_pc, span)` for each statement: every instruction the
    /// statement compiled to falls in `start_pc..end_pc`. Inner statements
    /// are recorded before the enclosing one, so building the per-instruction
    /// span table first-wins gives each instruction its innermost statement's
    /// source span. This drives the execution trace's source mapping and lets
    /// a runtime error point at the exact failing statement.
    span_ranges: Vec<(usize, usize, (usize, usize))>,
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            imports: HashSet::new(),
            functions: HashSet::new(),
            span_ranges: Vec::new(),
        }
    }

    /// Emit a statement boundary (a budget tick and trace step marker).
    fn push_boundary(&mut self, chunk: &mut Chunk) {
        chunk.instructions.push(Instruction::StmtBoundary);
    }

    /// Compile a block's statements, recording each statement's instruction
    /// range and source span for the trace.
    fn compile_block(
        &mut self,
        block: &Block,
        chunk: &mut Chunk,
        locals: &mut Vec<String>,
    ) -> Result<(), String> {
        for (i, stmt) in block.stmts.iter().enumerate() {
            let span = block.stmt_spans.get(i).copied();
            let start = chunk.instructions.len();
            self.compile_stmt(stmt, chunk, locals)?;
            let end = chunk.instructions.len();
            if let Some(sp) = span {
                self.span_ranges.push((start, end, sp));
            }
        }
        Ok(())
    }

    pub fn compile(&mut self, program: &Program) -> Result<Chunk, String> {
        for item in &program.items {
            match item {
                TopLevel::Import(decl) => {
                    match &decl.spec {
                        ImportSpec::Module(name) => { self.imports.insert(name.clone()); }
                        ImportSpec::Named { name: _, module } => { self.imports.insert(module.clone()); }
                    }
                }
                TopLevel::Function(f) => { self.functions.insert(f.name.clone()); }
                _ => {}
            }
        }

        let workflow = program.items.iter().find_map(|item| {
            if let TopLevel::Workflow(w) = item { Some(w.clone()) } else { None }
        }).ok_or_else(|| "no workflow found in program".to_string())?;

        let mut chunk = Chunk::new();

        // The workflow body is the entry frame, compiled first at pc 0 and
        // terminated with Halt so execution never falls through into the
        // function bodies that follow.
        let mut wf_locals: Vec<String> = Vec::new();
        self.compile_block(&workflow.body, &mut chunk, &mut wf_locals)?;
        chunk.instructions.push(Instruction::Halt);
        chunk.locals_count = wf_locals.len() as u16;
        chunk.locals_names = wf_locals;

        // Compile every top-level `fn` into the same instruction stream,
        // each at its own entry_pc with its own local namespace (params
        // first, then `let` bindings). A `Call` to one of these names is a
        // real call resolved by the VM via this table. Functions are
        // reached only through Call, so order does not matter and forward
        // references / mutual recursion work.
        for item in &program.items {
            if let TopLevel::Function(f) = item {
                let entry_pc = chunk.instructions.len();
                let mut fn_locals: Vec<String> =
                    f.params.iter().map(|p| p.name.clone()).collect();
                let param_count = fn_locals.len() as u16;
                self.compile_block(&f.body, &mut chunk, &mut fn_locals)?;
                // Implicit `return ()` so a function that falls off the end
                // returns Unit and never runs into the next function.
                chunk.instructions.push(Instruction::PushUnit);
                chunk.instructions.push(Instruction::Return);
                chunk.functions.push(FuncInfo {
                    name: f.name.clone(),
                    entry_pc,
                    param_count,
                    locals_count: fn_locals.len() as u16,
                });
            }
        }

        // Build the per-instruction span table. Statements were recorded
        // inner-before-outer, so assigning first-wins gives each instruction
        // its innermost enclosing statement's span. Instructions outside any
        // statement (e.g. a function's implicit trailing return) stay None.
        let mut spans = vec![None; chunk.instructions.len()];
        for (start, end, sp) in &self.span_ranges {
            for pc in *start..*end {
                if pc < spans.len() && spans[pc].is_none() {
                    spans[pc] = Some(*sp);
                }
            }
        }
        chunk.instruction_spans = spans;

        Ok(chunk)
    }

    fn compile_stmt(&mut self, stmt: &Stmt, chunk: &mut Chunk, locals: &mut Vec<String>) -> Result<(), String> {
        match stmt {
            Stmt::Let { name, value, .. } => {
                self.compile_expr(value, chunk, locals)?;
                let slot = self.get_or_add_local(name, locals);
                chunk.instructions.push(Instruction::StoreLocal(slot));
                self.push_boundary(chunk);
                Ok(())
            }
            Stmt::Assign { target, value } => {
                self.compile_expr(value, chunk, locals)?;
                match target {
                    Target::Ident(name) => {
                        if let Ok(slot) = self.find_local(name, locals) {
                            chunk.instructions.push(Instruction::StoreLocal(slot));
                        } else {
                            return Err(format!("variable '{}' not found for assignment", name));
                        }
                    }
                    Target::MemberAccess(obj, field) => {
                        self.compile_target(obj, chunk, locals)?;
                        let field_idx = chunk.add_constant(Value::Str(field.clone()));
                        chunk.instructions.push(Instruction::StoreField(field_idx));
                        // StoreField pushes the modified struct back; store to root local
                        let root = self.root_target(obj);
                        if let Some(root_name) = root {
                            if let Ok(slot) = self.find_local(&root_name, locals) {
                                chunk.instructions.push(Instruction::StoreLocal(slot));
                            }
                        }
                    }
                    Target::Index(_, _) => {
                        return Err("index assignment not supported".into());
                    }
                }
                self.push_boundary(chunk);
                Ok(())
            }
            Stmt::If { condition, then, else_ } => {
                self.compile_expr(condition, chunk, locals)?;
                let else_jump = chunk.instructions.len();
                chunk.instructions.push(Instruction::JumpIfFalse(0));

                self.compile_block(then, chunk, locals)?;

                let end_jump = if else_.is_some() {
                    let pos = chunk.instructions.len();
                    chunk.instructions.push(Instruction::Jump(0));
                    Some(pos)
                } else {
                    None
                };

                let else_offset = chunk.instructions.len() as u32;
                if let Instruction::JumpIfFalse(ref mut offset) = chunk.instructions[else_jump] {
                    *offset = else_offset;
                }

                if let Some(block) = else_ {
                    self.compile_block(block, chunk, locals)?;
                    if let Some(jump_pos) = end_jump {
                        let end_offset = chunk.instructions.len() as u32;
                        if let Instruction::Jump(ref mut offset) = chunk.instructions[jump_pos] {
                            *offset = end_offset;
                        }
                    }
                }

                self.push_boundary(chunk);
                Ok(())
            }
            Stmt::While { condition, body } => {
                let loop_start = chunk.instructions.len() as u32;

                self.compile_expr(condition, chunk, locals)?;
                let exit_jump = chunk.instructions.len();
                chunk.instructions.push(Instruction::JumpIfFalse(0));

                self.compile_block(body, chunk, locals)?;

                chunk.instructions.push(Instruction::Jump(loop_start));

                let exit_offset = chunk.instructions.len() as u32;
                if let Instruction::JumpIfFalse(ref mut offset) = chunk.instructions[exit_jump] {
                    *offset = exit_offset;
                }

                self.push_boundary(chunk);
                Ok(())
            }
            Stmt::For { item, iter, body } => {
                self.compile_expr(iter, chunk, locals)?;
                let iter_slot = locals.len() as u16;
                locals.push(format!("__for_iter_{}", item));
                chunk.instructions.push(Instruction::StoreLocal(iter_slot));

                let idx_slot = locals.len() as u16;
                locals.push(format!("__for_idx_{}", item));
                chunk.instructions.push(Instruction::PushInt(0));
                chunk.instructions.push(Instruction::StoreLocal(idx_slot));

                let loop_start = chunk.instructions.len() as u32;

                chunk.instructions.push(Instruction::LoadLocal(idx_slot));
                chunk.instructions.push(Instruction::LoadLocal(iter_slot));
                chunk.instructions.push(Instruction::Len);
                chunk.instructions.push(Instruction::Lt);
                let exit_jump = chunk.instructions.len();
                chunk.instructions.push(Instruction::JumpIfFalse(0));

                chunk.instructions.push(Instruction::LoadLocal(iter_slot));
                chunk.instructions.push(Instruction::LoadLocal(idx_slot));
                chunk.instructions.push(Instruction::Index);
                let item_slot = self.get_or_add_local(item, locals);
                chunk.instructions.push(Instruction::StoreLocal(item_slot));

                self.compile_block(body, chunk, locals)?;

                chunk.instructions.push(Instruction::LoadLocal(idx_slot));
                chunk.instructions.push(Instruction::PushInt(1));
                chunk.instructions.push(Instruction::Add);
                chunk.instructions.push(Instruction::StoreLocal(idx_slot));
                chunk.instructions.push(Instruction::Jump(loop_start));

                let exit_offset = chunk.instructions.len() as u32;
                if let Instruction::JumpIfFalse(ref mut offset) = chunk.instructions[exit_jump] {
                    *offset = exit_offset;
                }

                self.push_boundary(chunk);
                Ok(())
            }
            Stmt::Return(val) => {
                match val {
                    Some(expr) => self.compile_expr(expr, chunk, locals)?,
                    None => chunk.instructions.push(Instruction::PushUnit),
                }
                chunk.instructions.push(Instruction::Return);
                Ok(())
            }
            Stmt::Expr(expr) => {
                self.compile_expr(expr, chunk, locals)?;
                chunk.instructions.push(Instruction::Pop);
                self.push_boundary(chunk);
                Ok(())
            }
            Stmt::Emit(_) => {
                chunk.instructions.push(Instruction::PushUnit);
                self.push_boundary(chunk);
                Ok(())
            }
        }
    }

    /// Push exactly one params value for a positional capability call so the
    /// `WorkflowCall` instruction (which pops `[capability, params]`) is always
    /// balanced. Zero args become an empty struct, one arg passes through, and
    /// several args fold into an array.
    fn compile_call_params(
        &mut self,
        args: &[Expr],
        chunk: &mut Chunk,
        locals: &[String],
    ) -> Result<(), String> {
        match args.len() {
            0 => chunk.instructions.push(Instruction::MakeStruct(0)),
            1 => self.compile_expr(&args[0], chunk, locals)?,
            _ => {
                for arg in args {
                    self.compile_expr(arg, chunk, locals)?;
                }
                chunk.instructions.push(Instruction::MakeArray(args.len() as u16));
            }
        }
        Ok(())
    }

    fn compile_expr(&mut self, expr: &Expr, chunk: &mut Chunk, locals: &[String]) -> Result<(), String> {
        match expr {
            Expr::Int(n) => {
                chunk.instructions.push(Instruction::PushInt(*n));
            }
            Expr::Float(n) => {
                chunk.instructions.push(Instruction::PushFloat(*n));
            }
            Expr::Bool(b) => {
                chunk.instructions.push(Instruction::PushBool(*b));
            }
            Expr::Char(c) => {
                chunk.instructions.push(Instruction::PushChar(*c));
            }
            Expr::Str(s) => {
                let idx = chunk.add_constant(Value::Str(s.clone()));
                chunk.instructions.push(Instruction::PushStr(idx));
            }
            Expr::Array(elements) => {
                for elem in elements {
                    self.compile_expr(elem, chunk, locals)?;
                }
                chunk.instructions.push(Instruction::MakeArray(elements.len() as u16));
            }
            Expr::StructInstance { fields, .. } => {
                for (k, v) in fields {
                    let key_idx = chunk.add_constant(Value::Str(k.clone()));
                    chunk.instructions.push(Instruction::PushStr(key_idx));
                    self.compile_expr(v, chunk, locals)?;
                }
                chunk.instructions.push(Instruction::MakeStruct(fields.len() as u16));
            }
            Expr::EnumVariant { enum_name, variant } => {
                let enum_idx = chunk.add_constant(Value::Str(enum_name.clone()));
                let var_idx = chunk.add_constant(Value::Str(variant.clone()));
                chunk.instructions.push(Instruction::MakeEnum(enum_idx, var_idx));
            }
            Expr::Ident(name) => {
                if let Ok(slot) = self.find_local(name, locals) {
                    chunk.instructions.push(Instruction::LoadLocal(slot));
                } else {
                    let idx = chunk.add_constant(Value::Str(name.clone()));
                    chunk.instructions.push(Instruction::LoadName(idx));
                }
            }
            Expr::MemberAccess(obj, field) => {
                self.compile_expr(obj, chunk, locals)?;
                let idx = chunk.add_constant(Value::Str(field.clone()));
                chunk.instructions.push(Instruction::MemberAccess(idx));
            }
            Expr::Index(arr, idx) => {
                self.compile_expr(arr, chunk, locals)?;
                self.compile_expr(idx, chunk, locals)?;
                chunk.instructions.push(Instruction::Index);
            }
            Expr::BinOp(left, op, right) => {
                self.compile_expr(left, chunk, locals)?;
                self.compile_expr(right, chunk, locals)?;
                let instr = match op {
                    BinOp::Add => Instruction::Add,
                    BinOp::Sub => Instruction::Sub,
                    BinOp::Mul => Instruction::Mul,
                    BinOp::Div => Instruction::Div,
                    BinOp::Eq => Instruction::Eq,
                    BinOp::Ne => Instruction::Ne,
                    BinOp::Lt => Instruction::Lt,
                    BinOp::Gt => Instruction::Gt,
                    BinOp::Le => Instruction::Le,
                    BinOp::Ge => Instruction::Ge,
                    BinOp::And => Instruction::And,
                    BinOp::Or => Instruction::Or,
                };
                chunk.instructions.push(instr);
            }
            Expr::UnaryOp(operand, op) => {
                self.compile_expr(operand, chunk, locals)?;
                match op {
                    UnaryOp::Neg => chunk.instructions.push(Instruction::Neg),
                    UnaryOp::Not => chunk.instructions.push(Instruction::Not),
                }
            }
            Expr::Call(callee, args) => {
                let is_import_call = matches!(callee.as_ref(),
                    Expr::MemberAccess(obj, _) if matches!(obj.as_ref(), Expr::Ident(m) if self.imports.contains(m))
                );
                let is_sleep = matches!(callee.as_ref(), Expr::Ident(n) if n == "sleep");

                if is_import_call {
                    if let Expr::MemberAccess(obj, field) = callee.as_ref() {
                        if let Expr::Ident(module) = obj.as_ref() {
                            let cap = format!("{}.{}", module, field);
                            let idx = chunk.add_constant(Value::Str(cap));
                            chunk.instructions.push(Instruction::PushStr(idx));
                        }
                    }
                    // `WorkflowCall` consumes exactly [capability, params]. An
                    // import-qualified call carries its args positionally, so
                    // collapse them into a single params value: no args -> an
                    // empty struct (the OpenPrem agents expect a JSON object),
                    // one arg -> that value, many -> an array. Without this,
                    // a zero-arg call like `numbers.get()` underflows the stack
                    // and a multi-arg call mis-binds.
                    self.compile_call_params(args, chunk, locals)?;
                    chunk.instructions.push(Instruction::WorkflowCall);
                } else if is_sleep {
                    let idx = chunk.add_constant(Value::Str("__system.sleep".into()));
                    chunk.instructions.push(Instruction::PushStr(idx));
                    self.compile_call_params(args, chunk, locals)?;
                    chunk.instructions.push(Instruction::WorkflowCall);
                } else if let Expr::MemberAccess(obj, method) = callee.as_ref() {
                    // Method-style builtin call on a value the importer did not
                    // flag as a module: `arr.len()` -> `len(arr)`,
                    // `s.to_str()` -> `to_str(s)`. The receiver becomes the
                    // first argument. Upstream examples use `.len()` this way.
                    self.compile_expr(obj, chunk, locals)?;
                    for arg in args {
                        self.compile_expr(arg, chunk, locals)?;
                    }
                    let name_idx = chunk.add_constant(Value::Str(method.clone()));
                    chunk
                        .instructions
                        .push(Instruction::Call(name_idx, (args.len() + 1) as u8));
                } else {
                    for arg in args {
                        self.compile_expr(arg, chunk, locals)?;
                    }
                    let name = match callee.as_ref() {
                        Expr::Ident(n) => n.clone(),
                        _ => "__invalid_call_target".into(),
                    };
                    let name_idx = chunk.add_constant(Value::Str(name));
                    chunk.instructions.push(Instruction::Call(name_idx, args.len() as u8));
                }
            }
            Expr::WorkflowCall { capability_expr, params } => {
                self.compile_expr(capability_expr, chunk, locals)?;
                self.compile_expr(params, chunk, locals)?;
                chunk.instructions.push(Instruction::WorkflowCall);
            }
            Expr::NamespaceCall { namespace, name, args } => {
                self.compile_expr(namespace, chunk, locals)?;
                for arg in args {
                    self.compile_expr(arg, chunk, locals)?;
                }
                let name_idx = chunk.add_constant(Value::Str(name.clone()));
                chunk.instructions.push(Instruction::ModuleCall(name_idx));
            }
        }
        Ok(())
    }

    fn root_target(&self, target: &Target) -> Option<String> {
        match target {
            Target::Ident(name) => Some(name.clone()),
            Target::MemberAccess(obj, _) => self.root_target(obj),
            Target::Index(obj, _) => self.root_target(obj),
        }
    }

    fn compile_target(&mut self, target: &Target, chunk: &mut Chunk, locals: &[String]) -> Result<(), String> {
        match target {
            Target::Ident(name) => {
                let slot = self.find_local(name, locals)?;
                chunk.instructions.push(Instruction::LoadLocal(slot));
            }
            Target::MemberAccess(obj, _field) => {
                self.compile_target(obj, chunk, locals)?;
            }
            Target::Index(_, _) => {
                return Err("complex target in assignment not supported".into());
            }
        }
        Ok(())
    }

    fn get_or_add_local(&mut self, name: &str, locals: &mut Vec<String>) -> u16 {
        if let Some(pos) = locals.iter().position(|n| n == name) {
            return pos as u16;
        }
        let slot = locals.len();
        locals.push(name.to_string());
        slot as u16
    }

    fn find_local(&self, name: &str, locals: &[String]) -> Result<u16, String> {
        locals.iter().position(|n| n == name)
            .map(|p| p as u16)
            .ok_or_else(|| format!("variable '{}' not found", name))
    }
}
