use crate::{
    analyzer::{Symbol, TypeTable},
    diagnostic::{codes, DiagnosticPhase, SolDiagnostic, SourceSpan},
    lexer::{Token, TokenKind},
    parser::{Ast, Program, Type},
};
use std::collections::{HashMap, HashSet};

struct Scope {
    variables: Vec<(String, Type)>, 
}

#[derive(Debug, Clone)]
pub enum Inst {
    PushConst(Ast), 
    StoreLocal(isize), 
    LoadLocal(isize), 
    Pop,
    Dup,

    IntAdd, IntSub, IntMul, IntDiv,
    IntEq, IntNeq, IntGt, IntLt, IntGte, IntLte,

    FloatAdd, FloatSub, FloatMul, FloatDiv,
    FloatEq, FloatNeq, FloatGt, FloatLt, FloatGte, FloatLte,

    CharEq, CharNeq, CharGt, CharLt, CharGte, CharLte,

    LogOr, LogAnd, LogNot,

    BitXor, BitAnd, BitOr, BitNeg, BitLShift, BitRShift,

    NewStruct(usize), 
    GetField(usize), 
    SetField(usize), 

    NewArray,
    ArrayLen,
    GetElem,
    SetElem,

    ConcatStr,
    EqStr,

    Jump(usize), 
    JumpFalse(usize), 
    Call(usize, usize), 
    Ret,
    RetVal,

    PrintInt,
    PrintFloat,
    PrintChar,
    PrintString,

    // --- 9. RPC Message Serialization ---
    SerializeRequest(Type),
    SerializeResponse(Type),
    DeserializeRequestName,
    DeserializeRequestArgs,
    DeserializeResponseData,

    // --- 10. External Function Call (RPC) ---
    ExtCall(Vec<Type>, Box<Type>),
}

pub struct Codegen {
    type_tables: Vec<TypeTable>,
    locals: HashMap<String, (usize, Type)>,
    next_slot: usize,
    active_scopes: Vec<Scope>,
    functions: HashMap<String, usize>,
    fn_returns: HashMap<String, Type>,
    struct_layouts: HashMap<String, Vec<(String, Type)>>,
    for_loop_counter: usize,
    pending_calls: Vec<(usize, String)>,
    ext_functions: HashSet<String>,
    ext_endpoints: HashMap<String, String>,
    /// Diagnostics produced during codegen. Replaces the upstream
    /// `eprintln + process::exit(1)` and bare `panic!` sites.
    /// Callers drain this after `gen_bcode()`.
    pub diagnostics: Vec<SolDiagnostic>,
    /// Per-instruction source-span sidecar (B.D c36). Parallel to
    /// the `Vec<Inst>` returned by `gen_bcode`. Index N gives the
    /// approximate source span for instruction N (the span of the
    /// most-recent enclosing AST node that carried one). The
    /// runtime VM exposes its current `inst_ptr`, so consumers
    /// can map a step → source range via `instruction_spans[ip]`.
    /// Entries are `None` for instructions whose surrounding AST
    /// didn't have a span (top-level synthesized calls, etc.).
    pub instruction_spans: Vec<Option<SourceSpan>>,
    /// Most-recent enclosing AST-node span seen during `compile()`.
    /// Save+restored on each recursive call so siblings don't
    /// inherit each other's spans.
    current_span: Option<SourceSpan>,
}

impl Codegen {
    pub fn from(type_tables: Vec<TypeTable>) -> Self {
        Self {
            type_tables,
            locals: HashMap::new(),
            next_slot: 0,
            active_scopes: Vec::new(),
            functions: HashMap::new(),
            fn_returns: HashMap::new(),
            struct_layouts: HashMap::new(),
            for_loop_counter: 0,
            pending_calls: Vec::new(),
            ext_functions: HashSet::new(),
            ext_endpoints: HashMap::new(),
            diagnostics: Vec::new(),
            instruction_spans: Vec::new(),
            current_span: None,
        }
    }

    /// B.D c36 — extract a span from any AST variant that carries
    /// one. Used by `compile()` to update `current_span` before
    /// pushing instructions, so each instruction inherits an
    /// approximate-but-real source location.
    fn ast_span(node: &Ast) -> Option<SourceSpan> {
        match node {
            Ast::DeclFunc { span, .. }
            | Ast::DeclExtFunc { span, .. }
            | Ast::DeclVar { span, .. }
            | Ast::DeclStruct { span, .. }
            | Ast::DeclEnum { span, .. }
            | Ast::Block { span, .. }
            | Ast::StmtImport { span, .. }
            | Ast::StmtIf { span, .. }
            | Ast::StmtWhile { span, .. }
            | Ast::StmtFor { span, .. } => *span,
            _ => None,
        }
    }

    fn emit_error(&mut self, code: &'static str, message: impl Into<String>) {
        self.diagnostics.push(SolDiagnostic::error(
            DiagnosticPhase::Codegen,
            code,
            message,
        ));
    }

    pub fn with_ext_endpoints(mut self, endpoints: HashMap<String, String>) -> Self {
        self.ext_endpoints = endpoints;
        self
    }

    pub fn function_entries(&self) -> &HashMap<String, usize> {
        &self.functions
    }

    fn scope_from(&self, scope_id: usize) -> Scope {
        let mut scope = Scope { variables: Vec::new() };
        if scope_id < self.type_tables.len() {
            for (name, sym) in self.type_tables[scope_id].clone() {
                match sym {
                    Symbol::Variable { kind } => scope.variables.push((name.to_owned(), *kind)),
                    _ => continue,
                }
            }
        }
        scope
    }

    pub fn gen_bcode(&mut self, program: &Program) -> Vec<Inst> {
        // Register built-in RPC function return types
        self.fn_returns.insert("rpc_request".into(), Type::String);
        self.fn_returns.insert("rpc_response".into(), Type::String);
        self.fn_returns.insert("rpc_name".into(), Type::String);
        self.fn_returns.insert("rpc_args".into(), Type::String);
        self.fn_returns.insert("rpc_data".into(), Type::String);

        for node in program {
            match node {
                Ast::DeclStruct { name, fields, .. } => {
                    let mut sorted_fields: Vec<(String, Type)> = fields
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    sorted_fields.sort_by(|a, b| a.0.cmp(&b.0));
                    self.struct_layouts.insert(name.clone(), sorted_fields);
                }
                Ast::DeclExtFunc { name, ret, .. } => {
                    self.ext_functions.insert(name.clone());
                    self.fn_returns.insert(name.clone(), ret.clone());
                }
                _ => {}
            }
        }

        let mut insts = Vec::new();
        for node in program {
            let is_expr = self.is_expression_node(node);
            self.compile(&mut insts, node.clone());
            
            if is_expr {
                insts.push(Inst::Pop); 
            }
        }
        
        for (inst_idx, name) in &self.pending_calls {
            if let Some(&addr) = self.functions.get(name) {
                if let Inst::Call(_, count) = insts[*inst_idx] {
                    insts[*inst_idx] = Inst::Call(addr, count);
                }
            }
        }

        if let Some(&start_addr) = self.functions.get("start") {
            insts.push(Inst::Call(start_addr, 0));
        }

        // B.D c36 — final size-match. The trailing synthetic
        // `Call(start)` (and any other emits outside the per-decl
        // compile path) gets a `None` span: it doesn't correspond
        // to any source location.
        while self.instruction_spans.len() < insts.len() {
            self.instruction_spans.push(None);
        }

        insts
    }

    fn is_expression_node(&self, node: &Ast) -> bool {
        matches!(node, 
            Ast::ExprBinary { .. } | Ast::ExprUnary { .. } | 
            Ast::ExprFuncCall { .. } | Ast::ExprMemAcc { .. } | 
            Ast::ExprArrAcc { .. } | Ast::ExprInteger(_) | 
            Ast::ExprFloat(_) | Ast::ExprString(_) | 
            Ast::ExprChar(_) | Ast::ExprBool(_) | 
            Ast::ExprVar(_) | Ast::ExprStructInit { .. } | 
            Ast::ExprArrayInit { .. } | Ast::ExprEnumVar { .. } | 
            Ast::ExprReturn { .. } | Ast::ExprUndefined
        )
    }

    fn compile(&mut self, insts: &mut Vec<Inst>, node: Ast) {
        // B.D c36 — span attribution shell. Tracks the most-recent
        // enclosing AST-node span via save+restore; backfills
        // `instruction_spans` for any instructions emitted by the
        // inner body. The "innermost compile call with a span" wins
        // because deeper recursion fills the sidecar first and
        // outer fills are no-ops (length already matches).
        let saved_span = self.current_span;
        if let Some(s) = Self::ast_span(&node) {
            self.current_span = Some(s);
        }
        let before = insts.len();
        self.compile_inner(insts, node);
        let after = insts.len();
        let span_for_range = self.current_span;
        while self.instruction_spans.len() < before {
            // Catch up gaps from earlier emits (shouldn't happen in
            // well-formed compile flow but is defensive).
            self.instruction_spans.push(None);
        }
        while self.instruction_spans.len() < after {
            self.instruction_spans.push(span_for_range);
        }
        self.current_span = saved_span;
    }

    fn compile_inner(&mut self, insts: &mut Vec<Inst>, node: Ast) {
        match node {
            // --- 1. Primitive Constants ---
            Ast::ExprInteger(v) => insts.push(Inst::PushConst(Ast::ExprInteger(v))),
            Ast::ExprFloat(v)   => insts.push(Inst::PushConst(Ast::ExprFloat(v))),
            Ast::ExprChar(v)    => insts.push(Inst::PushConst(Ast::ExprChar(v))),
            Ast::ExprString(v)  => insts.push(Inst::PushConst(Ast::ExprString(v))),
            Ast::ExprBool(v)    => insts.push(Inst::PushConst(Ast::ExprBool(v))),
            Ast::ExprUndefined  => insts.push(Inst::PushConst(Ast::ExprUndefined)),

            // --- 2. Variables & Declarations ---
            Ast::ExprVar(name) => {
                let offset = self.find_local_offset(&name);
                insts.push(Inst::LoadLocal(offset));
            }
            Ast::DeclVar { name, kind, value, .. } => {
                if let Some(val_node) = value {
                    self.compile(insts, *val_node);
                    let offset = self.get_or_create_local(&name, kind);
                    insts.push(Inst::StoreLocal(offset));
                } else {
                    self.get_or_create_local(&name, kind);
                }
            }
            Ast::ExprAssign { var_name, value } => {
                self.compile(insts, *value);
                insts.push(Inst::Dup); 
                let offset = self.find_local_offset(&var_name);
                insts.push(Inst::StoreLocal(offset));
            }

            // --- 3. Blocks & Local Statements ---
            Ast::Block { block, scope: scope_id, .. } => {
                if scope_id < self.type_tables.len() {
                    self.active_scopes.push(self.scope_from(scope_id));
                }

                let saved_next = self.next_slot;

                for stmt in block {
                    let is_expr = self.is_expression_node(&stmt);
                    self.compile(insts, stmt);
                    if is_expr {
                        insts.push(Inst::Pop); 
                    }
                }

                self.locals.retain(|_, (slot, _)| *slot < saved_next);
                self.next_slot = saved_next;

                if scope_id < self.type_tables.len() {
                    self.active_scopes.pop();
                }
            }

            // --- 4. Control Flow ---
            Ast::StmtIf { condition, body, alt, .. } => {
                self.compile(insts, *condition);
                let jump_false_idx = insts.len();
                insts.push(Inst::JumpFalse(0)); 

                self.compile(insts, *body);

                if let Some(else_branch) = alt {
                    let jump_end_idx = insts.len();
                    insts.push(Inst::Jump(0)); 

                    let else_start = insts.len();
                    insts[jump_false_idx] = Inst::JumpFalse(else_start);

                    self.compile(insts, *else_branch);
                    let end_idx = insts.len();
                    insts[jump_end_idx] = Inst::Jump(end_idx);
                } else {
                    let end_idx = insts.len();
                    insts[jump_false_idx] = Inst::JumpFalse(end_idx);
                }
            }

            Ast::StmtWhile { condition, body, .. } => {
                let loop_start = insts.len();
                self.compile(insts, *condition);

                let jump_false_idx = insts.len();
                insts.push(Inst::JumpFalse(0)); 

                self.compile(insts, *body);
                insts.push(Inst::Jump(loop_start));

                let loop_end = insts.len();
                insts[jump_false_idx] = Inst::JumpFalse(loop_end);
            }

            Ast::StmtFor { elem_name, array, body, .. } => {
                let saved_next = self.next_slot;

                let array_type = self.infer_type(&array);
                let elem_type = match array_type {
                    Type::Array { inner, .. } => *inner,
                    _ => Type::Integer,
                };
                self.get_or_create_local(&elem_name, elem_type);

                let counter = self.for_loop_counter;
                self.for_loop_counter += 1;
                let arr_slot = format!("__for_arr_{}", counter);
                let idx_slot = format!("__for_idx_{}", counter);
                let len_slot = format!("__for_len_{}", counter);

                self.compile(insts, *array);
                self.get_or_create_local(&arr_slot, Type::Integer);
                insts.push(Inst::StoreLocal(self.find_local_offset(&arr_slot)));

                insts.push(Inst::LoadLocal(self.find_local_offset(&arr_slot)));
                insts.push(Inst::ArrayLen);
                self.get_or_create_local(&len_slot, Type::Integer);
                insts.push(Inst::StoreLocal(self.find_local_offset(&len_slot)));

                insts.push(Inst::PushConst(Ast::ExprInteger(0)));
                self.get_or_create_local(&idx_slot, Type::Integer);
                insts.push(Inst::StoreLocal(self.find_local_offset(&idx_slot)));

                let loop_start = insts.len();

                insts.push(Inst::LoadLocal(self.find_local_offset(&idx_slot)));
                insts.push(Inst::LoadLocal(self.find_local_offset(&len_slot)));
                insts.push(Inst::IntLt);
                let jump_false_idx = insts.len();
                insts.push(Inst::JumpFalse(0));

                insts.push(Inst::LoadLocal(self.find_local_offset(&arr_slot)));
                insts.push(Inst::LoadLocal(self.find_local_offset(&idx_slot)));
                insts.push(Inst::GetElem);
                insts.push(Inst::StoreLocal(self.find_local_offset(&elem_name)));

                self.compile(insts, *body);

                insts.push(Inst::LoadLocal(self.find_local_offset(&idx_slot)));
                insts.push(Inst::PushConst(Ast::ExprInteger(1)));
                insts.push(Inst::IntAdd);
                insts.push(Inst::StoreLocal(self.find_local_offset(&idx_slot)));

                insts.push(Inst::Jump(loop_start));

                let loop_end = insts.len();
                insts[jump_false_idx] = Inst::JumpFalse(loop_end);

                self.locals.retain(|_, (slot, _)| *slot < saved_next);
                self.next_slot = saved_next;
            }

            // --- 5. Operations & Intercepted Assignments ---
            Ast::ExprBinary { lhs, rhs, op } => {
                if op.get_kind() == TokenKind::Eq {
                    match *lhs {
                        Ast::ExprVar(name) => {
                            self.compile(insts, *rhs);
                            insts.push(Inst::Dup); 
                            let offset = self.find_local_offset(&name);
                            insts.push(Inst::StoreLocal(offset));
                        }
                        Ast::ExprArrAcc { lhs: arr, index } => {
                            self.compile(insts, *arr);
                            self.compile(insts, *index);
                            self.compile(insts, *rhs);
                            insts.push(Inst::SetElem); 
                        }
                        Ast::ExprMemAcc { lhs: obj, member } => {
                            self.compile(insts, *rhs);
                            insts.push(Inst::Dup);
                            
                            let obj_type = self.infer_type(&obj);
                            self.compile(insts, *obj);
                            
                            let mut field_idx = 0;
                            if let Type::Ident(struct_name) = obj_type {
                                if let Some(layout) = self.struct_layouts.get(&struct_name) {
                                    if let Some(pos) = layout.iter().position(|(n, _)| n == &member) {
                                        field_idx = pos;
                                    }
                                }
                            }
                            insts.push(Inst::SetField(field_idx)); 
                        }
                        _ => {
                            self.emit_error(
                                codes::CODEGEN_BAD_LHS,
                                "invalid left-hand side in assignment",
                            );
                        }
                    }
                } else {
                    self.compile(insts, *lhs.clone());
                    self.compile(insts, *rhs.clone());
                    let ty = self.infer_type(&Ast::ExprBinary { lhs, rhs, op: op.clone() });
                    self.emit_binary_op(insts, op, ty);
                }
            }
            
            Ast::ExprUnary { child, op } => {
                self.compile(insts, *child.clone());
                let op_kind = op.get_kind();
                match op_kind {
                    TokenKind::Bang => insts.push(Inst::LogNot),
                    TokenKind::Tilde => insts.push(Inst::BitNeg),
                    TokenKind::Dash => {
                        if let Type::Float = self.infer_type(&child) {
                            insts.push(Inst::PushConst(Ast::ExprFloat(-1.0)));
                            insts.push(Inst::FloatMul);
                        } else {
                            insts.push(Inst::PushConst(Ast::ExprInteger(-1)));
                            insts.push(Inst::IntMul);
                        }
                    }
                    _ => {}
                }
            }

            // --- 6. Functions & Calls ---
            Ast::DeclFunc { name, params, ret, body, scope, .. } => {
                let jump_over_idx = insts.len();
                insts.push(Inst::Jump(0)); 

                let func_entry = insts.len();
                self.functions.insert(name.clone(), func_entry);
                self.fn_returns.insert(name, ret); 

                self.locals.clear();
                self.next_slot = 0;

                for (param_name, param_type) in params {
                    self.locals.insert(param_name, (self.next_slot, param_type));
                    self.next_slot += 1;
                }

                if scope < self.type_tables.len() {
                    self.active_scopes.push(self.scope_from(scope));
                }

                self.compile(insts, *body);
                insts.push(Inst::Ret); 

                if scope < self.type_tables.len() {
                    self.active_scopes.pop();
                }

                let end_idx = insts.len();
                insts[jump_over_idx] = Inst::Jump(end_idx);
            }
            Ast::ExprFuncCall { name, args } => {
                if name == "print" && !args.is_empty() {
                    self.compile(insts, args[0].clone());
                    match self.display_type(&args[0]) {
                        Type::Integer | Type::Bool => insts.push(Inst::PrintInt),
                        Type::Float => insts.push(Inst::PrintFloat),
                        Type::Char => insts.push(Inst::PrintChar),
                        Type::String => insts.push(Inst::PrintString),
                        _ => insts.push(Inst::PrintInt),
                    }
                } else if name == "rpc_request" {
                    let elem_type = match self.infer_type(&args[1]) {
                        Type::Array { inner, .. } => *inner,
                        _ => Type::String,
                    };
                    self.compile(insts, args[0].clone());
                    self.compile(insts, args[1].clone());
                    insts.push(Inst::SerializeRequest(elem_type));
                } else if name == "rpc_response" {
                    let data_type = self.infer_type(&args[0]);
                    self.compile(insts, args[0].clone());
                    insts.push(Inst::SerializeResponse(data_type));
                } else if name == "rpc_name" {
                    self.compile(insts, args[0].clone());
                    insts.push(Inst::DeserializeRequestName);
                } else if name == "rpc_args" {
                    self.compile(insts, args[0].clone());
                    insts.push(Inst::DeserializeRequestArgs);
                } else if name == "rpc_data" {
                    self.compile(insts, args[0].clone());
                    insts.push(Inst::DeserializeResponseData);
                } else if self.ext_functions.contains(&name) {
                    let ret = self.fn_returns.get(&name).cloned().unwrap_or(Type::String);
                    let arg_types: Vec<Type> = args.iter().map(|a| self.infer_type(a)).collect();
                    let url = match self.ext_endpoints.get(&name).cloned() {
                        Some(u) => u,
                        None => {
                            self.emit_error(
                                codes::CODEGEN_MISSING_EXT_ENDPOINT,
                                format!("no endpoint configured for ext function `{name}`"),
                            );
                            // Skip emitting ExtCall bytecode — caller's
                            // CompileResult will surface the error and the
                            // bytecode won't be executed.
                            return;
                        }
                    };
                    for arg in args {
                        self.compile(insts, arg);
                    }
                    insts.push(Inst::PushConst(Ast::ExprString(name.clone())));
                    insts.push(Inst::PushConst(Ast::ExprString(url)));
                    insts.push(Inst::ExtCall(arg_types, Box::new(ret)));
                } else if let Some(&target_address) = self.functions.get(&name) {
                    let count = args.len();
                    for arg in args {
                        self.compile(insts, arg);
                    }
                    insts.push(Inst::Call(target_address, count));
                } else {
                    let count = args.len();
                    for arg in args {
                        self.compile(insts, arg);
                    }
                    let inst_idx = insts.len();
                    insts.push(Inst::Call(0, count));
                    self.pending_calls.push((inst_idx, name));
                }
            }
            Ast::ExprReturn { val } => {
                if let Some(ret_node) = val {
                    self.compile(insts, *ret_node);
                    insts.push(Inst::RetVal);
                } else {
                    insts.push(Inst::Ret);
                }
            }

            // --- 7. Compounds (Structs, Arrays, Enums) ---
            // FIX: Added .cloned() to safely dissociate the map reference from the recursive call loops
            Ast::ExprStructInit { name, fields } => {
                if let Some(layout) = self.struct_layouts.get(&name).cloned() {
                    for (f_name, _) in &layout {
                        if let Some((_, f_val)) = fields.iter().find(|(n, _)| n == f_name) {
                            self.compile(insts, f_val.clone());
                        } else {
                            insts.push(Inst::PushConst(Ast::ExprUndefined));
                        }
                    }
                    insts.push(Inst::NewStruct(layout.len()));
                } else {
                    insts.push(Inst::NewStruct(0));
                }
            }
            Ast::ExprMemAcc { lhs, member } => {
                let lhs_type = self.infer_type(&lhs);
                self.compile(insts, *lhs);
                
                let mut field_idx = 0;
                if let Type::Ident(struct_name) = lhs_type {
                    if let Some(layout) = self.struct_layouts.get(&struct_name) {
                        if let Some(pos) = layout.iter().position(|(n, _)| n == &member) {
                            field_idx = pos;
                        }
                    }
                }
                insts.push(Inst::GetField(field_idx)); 
            }
            Ast::ExprArrayInit { values } => {
                insts.push(Inst::PushConst(Ast::ExprInteger(values.len() as i128)));
                insts.push(Inst::NewArray);
                for (i, val) in values.into_iter().enumerate() {
                    insts.push(Inst::Dup);
                    insts.push(Inst::PushConst(Ast::ExprInteger(i as i128)));
                    self.compile(insts, val);
                    insts.push(Inst::SetElem);
                    insts.push(Inst::Pop); 
                }
            }
            Ast::ExprArrAcc { lhs, index } => {
                self.compile(insts, *lhs);
                self.compile(insts, *index);
                insts.push(Inst::GetElem);
            }
            Ast::ExprEnumVar { var, .. } => {
                let variant_hash = var.chars().next().unwrap_or('A') as i128 % 10;
                insts.push(Inst::PushConst(Ast::ExprInteger(variant_hash)));
            }

            // --- Compile-time Meta Nodes (No Op) ---
            Ast::DeclStruct { .. } | Ast::DeclEnum { .. } | Ast::DeclExtFunc { .. } | Ast::StmtImport { .. } => {}
        }
    }

    fn get_or_create_local(&mut self, name: &str, ty: Type) -> isize {
        if let Some((slot, _)) = self.locals.get(name) {
            *slot as isize
        } else {
            let slot = self.next_slot;
            self.locals.insert(name.to_string(), (slot, ty));
            self.next_slot += 1;
            slot as isize
        }
    }

    fn find_local_offset(&mut self, name: &str) -> isize {
        if let Some((slot, _)) = self.locals.get(name) {
            *slot as isize
        } else {
            let mut resolved_type = Type::Integer;
            for table in &self.type_tables {
                for (sym_name, sym) in table {
                    if sym_name == name {
                        if let Symbol::Variable { kind } = sym {
                            resolved_type = *kind.clone();
                        }
                    }
                }
            }
            let slot = self.next_slot;
            self.locals.insert(name.to_string(), (slot, resolved_type));
            self.next_slot += 1;
            slot as isize
        }
    }

    fn infer_type(&self, node: &Ast) -> Type {
        match node {
            Ast::ExprInteger(_) => Type::Integer,
            Ast::ExprFloat(_)   => Type::Float,
            Ast::ExprChar(_)    => Type::Char,
            Ast::ExprString(_)  => Type::String,
            Ast::ExprBool(_)    => Type::Bool,
            Ast::ExprVar(name)  => {
                if let Some((_, ty)) = self.locals.get(name) {
                    return ty.clone();
                }
                for table in &self.type_tables {
                    for (sym_name, sym) in table {
                        if sym_name == name {
                            if let Symbol::Variable { kind } = sym {
                                return *kind.clone();
                            }
                        }
                    }
                }
                Type::Integer
            }
            Ast::ExprMemAcc { lhs, member } => {
                let lhs_type = self.infer_type(lhs);
                if let Type::Ident(struct_name) = lhs_type {
                    if let Some(layout) = self.struct_layouts.get(&struct_name) {
                        if let Some((_, f_type)) = layout.iter().find(|(n, _)| n == member) {
                            return f_type.clone();
                        }
                    }
                }
                Type::Integer
            }
            Ast::ExprBinary { lhs, rhs, .. } => {
                let lt = self.infer_type(lhs);
                let rt = self.infer_type(rhs);
                if let Type::Float = lt { Type::Float }
                else if let Type::Float = rt { Type::Float }
                else { lt }
            }
            Ast::ExprUnary { child, .. } => self.infer_type(child),
            Ast::ExprArrAcc { lhs, .. } => {
                match self.infer_type(lhs) {
                    Type::Array { inner, .. } => *inner,
                    _ => Type::Integer,
                }
            }
            Ast::ExprFuncCall { name, .. } => {
                self.fn_returns.get(name).cloned().unwrap_or(Type::Integer)
            }
            _ => Type::Integer,
        }
    }

    fn display_type(&self, node: &Ast) -> Type {
        match node {
            Ast::ExprBinary { op, .. } => {
                match op.get_kind() {
                    TokenKind::EqEq | TokenKind::BangEq
                    | TokenKind::MoreThan | TokenKind::LessThan
                    | TokenKind::MoreEq | TokenKind::LessEq
                    | TokenKind::PipePipe | TokenKind::AmpAmp
                    => Type::Integer,
                    _ => self.infer_type(node),
                }
            }
            Ast::ExprUnary { op, .. } => {
                match op.get_kind() {
                    TokenKind::Bang => Type::Integer,
                    _ => self.infer_type(node),
                }
            }
            _ => self.infer_type(node),
        }
    }

    fn emit_binary_op(&self, insts: &mut Vec<Inst>, op: Token, ty: Type) {
        let op_kind = op.get_kind();
        match ty {
            Type::Float => match op_kind {
                TokenKind::Plus => insts.push(Inst::FloatAdd),
                TokenKind::Dash => insts.push(Inst::FloatSub),
                TokenKind::Star => insts.push(Inst::FloatMul),
                TokenKind::Slash => insts.push(Inst::FloatDiv),
                TokenKind::EqEq => insts.push(Inst::FloatEq),
                TokenKind::BangEq => insts.push(Inst::FloatNeq),
                TokenKind::MoreThan => insts.push(Inst::FloatGt),
                TokenKind::LessThan => insts.push(Inst::FloatLt),
                TokenKind::MoreEq => insts.push(Inst::FloatGte),
                TokenKind::LessEq => insts.push(Inst::FloatLte),
                _ => {}
            },
            Type::Char => match op_kind {
                TokenKind::EqEq => insts.push(Inst::CharEq),
                TokenKind::BangEq => insts.push(Inst::CharNeq),
                TokenKind::MoreThan => insts.push(Inst::CharGt),
                TokenKind::LessThan => insts.push(Inst::CharLt),
                TokenKind::MoreEq => insts.push(Inst::CharGte),
                TokenKind::LessEq => insts.push(Inst::CharLte),
                _ => {}
            },
            Type::String => match op_kind {
                TokenKind::Plus => insts.push(Inst::ConcatStr),
                TokenKind::EqEq => insts.push(Inst::EqStr),
                TokenKind::BangEq => {
                    insts.push(Inst::EqStr);
                    insts.push(Inst::LogNot);
                }
                _ => {}
            },
            _ => match op_kind {
                TokenKind::Plus => insts.push(Inst::IntAdd),
                TokenKind::Dash => insts.push(Inst::IntSub),
                TokenKind::Star => insts.push(Inst::IntMul),
                TokenKind::Slash => insts.push(Inst::IntDiv),
                TokenKind::EqEq => insts.push(Inst::IntEq),
                TokenKind::BangEq => insts.push(Inst::IntNeq),
                TokenKind::MoreThan => insts.push(Inst::IntGt),
                TokenKind::LessThan => insts.push(Inst::IntLt),
                TokenKind::MoreEq => insts.push(Inst::IntGte),
                TokenKind::LessEq => insts.push(Inst::IntLte),
                TokenKind::PipePipe => insts.push(Inst::LogOr),
                TokenKind::AmpAmp => insts.push(Inst::LogAnd),
                TokenKind::Caret => insts.push(Inst::BitXor),
                TokenKind::Ampersand => insts.push(Inst::BitAnd),
                TokenKind::Pipe => insts.push(Inst::BitOr),
                TokenKind::LShift => insts.push(Inst::BitLShift),
                TokenKind::RShift => insts.push(Inst::BitRShift),
                _ => {}
            }
        }
    }
}
