//! Tree-walking interpreter for the Sol language (DEPRECATED).
//!
//! This module is deprecated in favour of the bytecode VM (`vm`, `compiler`,
//! `instruction` modules). The tree-walker is kept for reference but not
//! used by `WorkflowExecutor` or any production code.
//!
//! The [`Interpreter`] evaluates Sol programs by walking the AST produced by
//! the parser. It manages:
//! - Function call frames with lexical scoping
//! - Control flow (if/else, while, for)
//! - Capability call interception for workflow execution
//! - Native function registration
//! - Module imports
//!
//! ## Workflow call interception
//!
//! When the interpreter encounters a `call("cap.name", params)` expression or
//! an import-qualified call like `module.function(params)`, it does **not**
//! execute the call itself. Instead it stores the pending call in
//! [`pending_call`](Interpreter::pending_call) and returns an error sentinel
//! (`"__workflow_call__"`). The owner of the [`WorkflowExecutor`](crate::workflow::WorkflowExecutor)
//! intercepts this, resolves the capability via the controller, and feeds the
//! result back via [`pending_call_result`](Interpreter::pending_call_result).

use std::collections::{HashMap, HashSet};
use crate::ast::*;
use crate::value::Value;

/// Type alias for a native (Rust-side) function that Sol can call.
pub type NativeFunc = Box<dyn Fn(&[Value]) -> Result<Value, String> + Send>;

/// Sentinel string returned by the interpreter when a capability call is
/// intercepted during workflow execution.
const WORKFLOW_CALL_SENTINEL: &str = "__workflow_call__";

/// A tree-walking interpreter for the Sol language.
///
/// Evaluates expressions and statements, manages scoped environments,
/// and intercepts capability calls for distributed workflow resolution.
pub struct Interpreter {
    /// User-defined function declarations, keyed by name.
    functions: HashMap<String, FunctionDecl>,
    /// User-defined struct declarations, keyed by name.
    structs: HashMap<String, StructDecl>,
    /// User-defined enum declarations, keyed by name.
    enums: HashMap<String, EnumDecl>,
    /// Registered native (Rust) functions, keyed by name.
    native_funcs: HashMap<String, NativeFunc>,
    /// A pending workflow capability call (capability name, params).
    /// Populated when a `call()` or import-qualified call is intercepted.
    pub pending_call: Option<(String, Value)>,
    /// The result of a resolved pending call, fed back by the executor.
    pub pending_call_result: Option<Value>,
    /// The capability name associated with `pending_call_result`.
    pub pending_call_result_cap: Option<String>,
    /// Index of the call site within the current statement list (used
    /// for resume after a nested call in a loop body).
    pub call_site_index: Option<usize>,
    /// Resume offset within a loop body after resolving a nested call.
    pub resume_body_index: Option<usize>,
    /// Current index in a for-each loop (for resume after nested calls).
    pub for_loop_idx: Option<usize>,
    /// Set of imported module names (from `import module;`).
    modules: HashSet<String>,
    /// Bare capability names imported via `import "name" from module;`.
    bare_caps: HashMap<String, String>,
}

impl Interpreter {
    /// Create a new interpreter with built-in native functions (`print`,
    /// `len`, `to_str`, `type_name`).
    pub fn new() -> Self {
        let mut native = HashMap::new();
        native.insert("print".to_string(), Box::new(print_native) as NativeFunc);
        native.insert("len".to_string(), Box::new(len_native) as NativeFunc);
        native.insert("to_str".to_string(), Box::new(to_str_native) as NativeFunc);
        native.insert("type_name".to_string(), Box::new(type_name_native) as NativeFunc);
        Self {
            functions: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            native_funcs: native,
            pending_call: None,
            pending_call_result: None,
            pending_call_result_cap: None,
            call_site_index: None,
            resume_body_index: None,
            for_loop_idx: None,
            modules: HashSet::new(),
            bare_caps: HashMap::new(),
        }
    }

    /// Register a custom native function that Sol code can call by `name`.
    pub fn register_native(&mut self, name: &str, func: NativeFunc) {
        self.native_funcs.insert(name.to_string(), func);
    }

    /// Load a parsed [`Program`] into the interpreter, registering all
    /// functions, structs, enums, and imports.
    pub fn load_program(&mut self, program: &Program) -> Result<(), String> {
        for item in &program.items {
            match item {
                TopLevel::Function(f) => { self.functions.insert(f.name.clone(), f.clone()); }
                TopLevel::Struct(s) => { self.structs.insert(s.name.clone(), s.clone()); }
                TopLevel::Enum(e) => { self.enums.insert(e.name.clone(), e.clone()); }
                TopLevel::Workflow(_) => {}
                TopLevel::Import(decl) => {
                    match &decl.spec {
                        ImportSpec::Module(name) => { self.modules.insert(name.clone()); }
                        ImportSpec::Named { name, module: full } => {
                            self.bare_caps.insert(name.clone(), full.clone());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Call a user-defined function by name with the given arguments.
    fn call_function(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        let body = self.functions.get(name)
            .ok_or_else(|| format!("function '{}' not found", name))?
            .body.clone();
        let params = self.functions.get(name)
            .ok_or_else(|| format!("function '{}' not found", name))?
            .params.clone();
        let mut local_env = Env::new();
        for (param, arg) in params.iter().zip(args.iter()) {
            local_env.insert(param.name.clone(), arg.clone());
        }
        self.exec_block(&body, &mut local_env)
    }

    /// Execute a single statement in the given environment.
    ///
    /// Returns [`Value::Unit`] for most statements, or a value for `return`.
    pub fn exec_stmt(&mut self, stmt: &Stmt, env: &mut Env) -> Result<Value, String> {
        match stmt {
            Stmt::Let { name, type_: _, value } => {
                let val = self.eval_expr(value, env)?;
                env.insert(name.clone(), val);
                Ok(Value::Unit)
            }
            Stmt::Expr(expr) => {
                self.eval_expr(expr, env)?;
                Ok(Value::Unit)
            }
            Stmt::If { condition, then, else_ } => {
                let cond = self.eval_expr(condition, env)?;
                if self.is_truthy(&cond)? {
                    self.exec_block(then, env)
                } else if let Some(else_block) = else_ {
                    self.exec_block(else_block, env)
                } else {
                    Ok(Value::Unit)
                }
            }
            Stmt::While { condition, body } => {
                loop {
                    self.call_site_index = None;
                    let cond = self.eval_expr(condition, env)?;
                    if !self.is_truthy(&cond)? { break; }
                    let start = self.resume_body_index.take().unwrap_or(0);
                    let mut iter = body.stmts[start..].iter().enumerate();
                    while let Some((offset, stmt)) = iter.next() {
                        let idx = start + offset;
                        self.call_site_index = Some(idx);
                        let ret = self.exec_stmt(stmt, env)?;
                        self.call_site_index = None;
                        if !matches!(ret, Value::Unit) {
                            return Ok(ret);
                        }
                    }
                }
                self.resume_body_index = None;
                Ok(Value::Unit)
            }
            Stmt::For { item, iter, body } => {
                let iter_val = self.eval_expr(iter, env)?;
                match iter_val {
                    Value::Array(arr) => {
                        let skip = self.for_loop_idx.take().unwrap_or(0);
                        for (elem_idx, elem) in arr.iter().enumerate().skip(skip) {
                            self.for_loop_idx = Some(elem_idx);
                            env.insert(item.clone(), elem.clone());
                            let start = self.resume_body_index.take().unwrap_or(0);
                            let mut iter = body.stmts[start..].iter().enumerate();
                            while let Some((offset, stmt)) = iter.next() {
                                let idx = start + offset;
                                self.call_site_index = Some(idx);
                                let ret = self.exec_stmt(stmt, env)?;
                                self.call_site_index = None;
                                if !matches!(ret, Value::Unit) {
                                    return Ok(ret);
                                }
                            }
                        }
                    }
                    _ => return Err("for loop requires an array".into()),
                }
                self.resume_body_index = None;
                self.for_loop_idx = None;
                Ok(Value::Unit)
            }
            Stmt::Return(val) => {
                match val {
                    Some(expr) => self.eval_expr(expr, env),
                    None => Ok(Value::Unit),
                }
            }
            Stmt::Assign { target, value } => {
                let val = self.eval_expr(value, env)?;
                self.assign_target(target, val, env)?;
                Ok(Value::Unit)
            }
            Stmt::Emit(_) => {
                Ok(Value::Unit)
            }
        }
    }

    /// Execute a sequence of statements in a block, returning early on `return`.
    fn exec_block(&mut self, block: &Block, env: &mut Env) -> Result<Value, String> {
        for stmt in &block.stmts {
            let val = self.exec_stmt(stmt, env)?;
            if !matches!(val, Value::Unit) {
                return Ok(val);
            }
        }
        Ok(Value::Unit)
    }

    /// Evaluate an expression in the given environment.
    pub fn eval_expr(&mut self, expr: &Expr, env: &Env) -> Result<Value, String> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Float(n) => Ok(Value::Float(*n)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Char(c) => Ok(Value::Char(*c)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Array(elements) => {
                let mut vals = Vec::new();
                for e in elements {
                    vals.push(self.eval_expr(e, env)?);
                }
                Ok(Value::Array(vals))
            }
            Expr::StructInstance { name: _, fields } => {
                let mut map = HashMap::new();
                for (k, v) in fields {
                    map.insert(k.clone(), self.eval_expr(v, env)?);
                }
                Ok(Value::Struct(map))
            }
            Expr::EnumVariant { enum_name, variant } => {
                Ok(Value::Enum(enum_name.clone(), variant.clone()))
            }
            Expr::Ident(name) => {
                env.get(name).cloned()
                    .ok_or_else(|| format!("variable '{}' not found", name))
            }
            Expr::MemberAccess(obj, field) => {
                let obj_val = self.eval_expr(obj, env)?;
                match obj_val {
                    Value::Struct(map) => map.get(field.as_str()).cloned()
                        .ok_or_else(|| format!("field '{}' not found", field)),
                    _ => Err(format!("cannot access field '{}' on {}", field, obj_val)),
                }
            }
            Expr::Index(arr, index) => {
                let arr_val = self.eval_expr(arr, env)?;
                let idx_val = self.eval_expr(index, env)?;
                let arr_str = format!("{}", arr_val);
                let idx_str = format!("{}", idx_val);
                match (arr_val, idx_val) {
                    (Value::Array(items), Value::Int(i)) => {
                        let i = i as usize;
                        items.get(i).cloned()
                            .ok_or_else(|| format!("index {} out of bounds", i))
                    }
                    _ => Err(format!("cannot index {} with {}", arr_str, idx_str)),
                }
            }
            Expr::BinOp(left, op, right) => {
                let l = self.eval_expr(left, env)?;
                let r = self.eval_expr(right, env)?;
                self.eval_binop(&l, op, &r)
            }
            Expr::UnaryOp(operand, op) => {
                let val = self.eval_expr(operand, env)?;
                match op {
                    UnaryOp::Neg => match val {
                        Value::Int(n) => Ok(Value::Int(-n)),
                        _ => Err(format!("cannot negate {}", val)),
                    },
                    UnaryOp::Not => match val {
                        Value::Bool(b) => Ok(Value::Bool(!b)),
                        _ => Err(format!("cannot apply 'not' to {}", val)),
                    },
                }
            }
            Expr::Call(callee, args) => {
                let mut evaluated_args = Vec::new();
                for arg in args {
                    evaluated_args.push(self.eval_expr(arg, env)?);
                }
                match self.resolve_imported_call(callee)? {
                    Some(cap) => {
                        return self.do_workflow_call(&cap, evaluated_args);
                    }
                    None => {}
                }
                match callee.as_ref() {
                    Expr::Ident(name) => {
                        if name == "sleep" {
                            return self.do_sleep(evaluated_args);
                        }
                        if self.functions.contains_key(name) {
                            self.call_function(name, &evaluated_args)
                        } else if let Some(func) = self.native_funcs.get(name) {
                            (func)(&evaluated_args)
                        } else {
                            Err(format!("function '{}' not found", name))
                        }
                    }
                    _ => {
                        let hint = if self.modules.is_empty() {
                            ". Try using `import module_name;` at the top of your file"
                        } else {
                            ""
                        };
                        Err(format!("cannot call {:?}{}", callee, hint))
                    }
                }
            }
            Expr::NamespaceCall { namespace: _, name: _, args: _ } => {
                Err("NamespaceCall not supported in deprecated tree-walker interpreter (use VM)".into())
            }
            Expr::WorkflowCall { capability_expr, params } => {
                let cap_name = match self.eval_expr(capability_expr, env)? {
                    Value::Str(s) => s,
                    other => return Err(format!("capability name must evaluate to a string, got {}", other)),
                };
                let matches = self.pending_call_result.is_some()
                    && self.pending_call_result_cap.as_deref() == Some(&cap_name);
                if matches {
                    self.pending_call_result_cap = None;
                    return Ok(self.pending_call_result.take().unwrap());
                }
                let params_val = self.eval_expr(params, env)?;
                self.pending_call = Some((cap_name, params_val));
                Err("__workflow_call__".into())
            }
        }
    }

    /// Evaluate a binary operator on two runtime values.
    fn eval_binop(&self, left: &Value, op: &BinOp, right: &Value) -> Result<Value, String> {
        match op {
            BinOp::Add => {
                match (left, right) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                    (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
                    (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
                    (Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
                    _ => Err(format!("cannot add {} and {}", left, right)),
                }
            }
            BinOp::Sub => {
                match (left, right) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
                    (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
                    (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - *b as f64)),
                    _ => Err(format!("cannot subtract {} and {}", left, right)),
                }
            }
            BinOp::Mul => {
                match (left, right) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
                    (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
                    (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * *b as f64)),
                    _ => Err(format!("cannot multiply {} and {}", left, right)),
                }
            }
            BinOp::Div => {
                match (left, right) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
                    (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 / b)),
                    (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / *b as f64)),
                    _ => Err(format!("cannot divide {} and {}", left, right)),
                }
            }
            BinOp::Eq => Ok(Value::Bool(left == right)),
            BinOp::Ne => Ok(Value::Bool(left != right)),
            BinOp::Lt => cmp_binop(left, right, "<"),
            BinOp::Gt => cmp_binop(left, right, ">"),
            BinOp::Le => cmp_binop(left, right, "<="),
            BinOp::Ge => cmp_binop(left, right, ">="),
            BinOp::And => bool_binop(left, right, |a, b| a && b, "and"),
            BinOp::Or => bool_binop(left, right, |a, b| a || b, "or"),
        }
    }

    /// Determine the truthiness of a value for use as a condition.
    fn is_truthy(&self, val: &Value) -> Result<bool, String> {
        match val {
            Value::Bool(b) => Ok(*b),
            Value::Int(n) => Ok(*n != 0),
            _ => Err(format!("cannot use {} as condition", val)),
        }
    }

    /// Resolve an imported call expression to a capability string.
    ///
    /// For `module.function(params)`, returns `Some("module.function")` if
    /// `module` is a known import. For bare identifiers, checks `bare_caps`.
    fn resolve_imported_call(&self, callee: &Expr) -> Result<Option<String>, String> {
        match callee {
            Expr::Ident(name) => {
                Ok(self.bare_caps.get(name).cloned())
            }
            Expr::MemberAccess(obj, field) => {
                if let Expr::Ident(module) = obj.as_ref() {
                    if self.modules.contains(module) {
                        return Ok(Some(format!("{}.{}", module, field)));
                    }
                    if self.modules.is_empty() {
                        return Err(format!(
                            "'{}' is not a known function or import. Did you mean to add `import {};` at the top?",
                            format!("{}.{}", module, field),
                            module,
                        ));
                    }
                    return Err(format!(
                        "'{}' is not imported. Available imports: {}. Add `import {};` at the top of your file.",
                        module,
                        self.modules.iter().map(|m| format!("'{}'", m)).collect::<Vec<_>>().join(", "),
                        module,
                    ));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Intercept a capability call during workflow execution.
    ///
    /// Stores the pending call and returns the workflow call sentinel error
    /// so the [`WorkflowExecutor`](crate::workflow::WorkflowExecutor) can
    /// resolve the capability remotely.
    fn do_sleep(&mut self, args: Vec<Value>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("sleep() takes exactly 1 argument (milliseconds)".into());
        }
        let ms = match &args[0] {
            Value::Int(n) => *n,
            _ => return Err("sleep() requires an integer argument".into()),
        };
        let params = Value::Struct([("milliseconds".to_string(), Value::Int(ms))].into());
        self.do_workflow_call("__system.sleep", vec![params])
    }

    fn do_workflow_call(&mut self, capability: &str, args: Vec<Value>) -> Result<Value, String> {
        let matches = self.pending_call_result.is_some()
            && self.pending_call_result_cap.as_deref() == Some(capability);
        if matches {
            self.pending_call_result_cap = None;
            return Ok(self.pending_call_result.take().unwrap());
        }
        let params_val = args.into_iter().next().unwrap_or(Value::Struct(HashMap::new()));
        self.pending_call = Some((capability.to_string(), params_val));
        Err(WORKFLOW_CALL_SENTINEL.to_string())
    }

    /// Assign a value to an assignment target in the given environment.
    fn assign_target(&mut self, target: &Target, val: Value, env: &mut Env) -> Result<(), String> {
        match target {
            Target::Ident(name) => {
                env.insert(name.clone(), val);
                Ok(())
            }
            Target::MemberAccess(obj, field) => {
                match obj.as_ref() {
                    Target::Ident(name) => {
                        if let Some(Value::Struct(ref mut map)) = env.get_mut(name) {
                            map.insert(field.clone(), val);
                            Ok(())
                        } else {
                            Err(format!("cannot assign to field '{}' of non-struct", field))
                        }
                    }
                    _ => Err("complex assignment target not supported".into()),
                }
            }
            Target::Index(_, _) => Err("index assignment not supported".into()),
        }
    }
}

/// Type alias for a local environment (variable name → value mapping).
type Env = HashMap<String, Value>;

/// Compare two numeric values and return the boolean result.
fn cmp_val(left: &Value, right: &Value, _op: &str) -> Result<bool, String> {
    let result = match (left, right) {
        (Value::Int(a), Value::Int(b)) => {
            match _op {
                "<"  => a < b,  ">"  => a > b,
                "<=" => a <= b, ">=" => a >= b,
                _ => return Err(format!("unknown operator '{}'", _op)),
            }
        }
        (Value::Float(a), Value::Float(b)) => {
            match _op {
                "<"  => *a < *b,  ">"  => *a > *b,
                "<=" => *a <= *b, ">=" => *a >= *b,
                _ => return Err(format!("unknown operator '{}'", _op)),
            }
        }
        (Value::Int(a), Value::Float(b)) => {
            let a = *a as f64;
            match _op {
                "<"  => a < *b,  ">"  => a > *b,
                "<=" => a <= *b, ">=" => a >= *b,
                _ => return Err(format!("unknown operator '{}'", _op)),
            }
        }
        (Value::Float(a), Value::Int(b)) => {
            let b = *b as f64;
            match _op {
                "<"  => *a < b,  ">"  => *a > b,
                "<=" => *a <= b, ">=" => *a >= b,
                _ => return Err(format!("unknown operator '{}'", _op)),
            }
        }
        _ => return Err(format!("cannot compare {} and {}", left, right)),
    };
    Ok(result)
}

/// Comparison helper that wraps `cmp_val` in a `Value::Bool`.
fn cmp_binop(left: &Value, right: &Value, _op: &str) -> Result<Value, String> {
    cmp_val(left, right, _op).map(Value::Bool)
}

/// Boolean logic helper for `&&` and `||`.
fn bool_binop(left: &Value, right: &Value, f: fn(bool, bool) -> bool, op: &str) -> Result<Value, String> {
    match (left, right) {
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(f(*a, *b))),
        _ => Err(format!("cannot {} {} and {}", op, left, right)),
    }
}

/// Native function `print(...)` — prints values to stdout separated by spaces.
fn print_native(args: &[Value]) -> Result<Value, String> {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 { print!(" "); }
        print!("{}", arg);
    }
    println!();
    Ok(Value::Unit)
}

/// Native function `len(value)` — returns the length of a string or array.
fn len_native(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("len() takes exactly 1 argument".into());
    }
    match &args[0] {
        Value::Str(s) => Ok(Value::Int(s.len() as i64)),
        Value::Array(arr) => Ok(Value::Int(arr.len() as i64)),
        _ => Err(format!("cannot take length of {}", args[0])),
    }
}

/// Native function `to_str(value)` — converts any value to its string representation.
fn to_str_native(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("to_str() takes exactly 1 argument".into());
    }
    Ok(Value::Str(format!("{}", args[0])))
}

/// Native function `type_name(value)` — returns the type name as a string.
fn type_name_native(args: &[Value]) -> Result<Value, String> {
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
