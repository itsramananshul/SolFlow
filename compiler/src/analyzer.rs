use std::collections::HashMap;

use crate::{
    diagnostic::{codes, DiagnosticPhase, SolDiagnostic},
    lexer::Token,
    parser::{Ast, Program, Type},
    util::type_eq,
};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Symbol {
    Variable {
        kind: Box<Type>,
    },
    Enum {
        variants: HashMap<String, isize>,
    },
    Struct {
        fields: HashMap<String, Box<Type>>,
    },
}
pub type TypeTableId = usize;
pub type TypeTable = HashMap<String, Symbol>;

pub struct Analyzer {
    pub tt_arena: Vec<TypeTable>,
    tts: Vec<TypeTableId>,
    can_break: bool,
    can_return: bool,
    /// Diagnostics produced during analysis. Replaces the upstream
    /// `eprintln! + process::exit(1)` pattern; callers drain this
    /// after `run()` to surface every semantic error in one pass.
    pub diagnostics: Vec<SolDiagnostic>,
}

impl Analyzer {
    pub fn new() -> Self {
        Self {
            tt_arena: Vec::new(),
            tts: Vec::new(),
            can_break: false,
            can_return: false,
            diagnostics: Vec::new(),
        }
    }

    /// Push a semantic-phase error diagnostic onto the buffer.
    /// Callers should then `return None` from `check()` so the
    /// outer `?` propagator stops cascading into derived errors.
    fn emit_error(&mut self, code: &'static str, message: impl Into<String>) {
        self.diagnostics.push(SolDiagnostic::error(
            DiagnosticPhase::Analyzer,
            code,
            message,
        ));
    }

    fn new_table(&mut self) -> TypeTableId {
        let id = self.tt_arena.len();
        self.tt_arena.push(HashMap::new());
        self.tts.push(id);
        id
    }
    fn pop_table(&mut self) {
        self.tts.pop();
    }
    fn add_entry(&mut self, name: String, symbol: Symbol) {
        if self.tt_arena.is_empty() { self.new_table(); }

        let id = self.tts.last().unwrap();
        if self.tt_arena[*id].insert(name.clone(), symbol).is_some() {
            // Redefinition is reported but we keep the newer entry;
            // continuing lets us surface every error in one analyze pass.
            self.emit_error(codes::SEMA_REDEFINITION, format!("redefinition of `{name}`"));
        }
    }
    fn get_entry(&mut self, name: &String) -> Option<&Symbol> {
        let tables = self.tts.iter().map(|i| &self.tt_arena[*i]);
        tables.rev().find_map(|table| table.get(name))
    }

    // fn kindof(&self, node: Ast) -> Type {
    //     Type::
    // }

    pub fn run(&mut self, program: &mut Program) {
        self.new_table(); // globals

        // Register built-in RPC functions (generic ones handled specially in check())
        let rpc_builtins: Vec<(&str, Vec<Type>, Type)> = vec![
            ("rpc_name", vec![Type::String], Type::String),
            ("rpc_args", vec![Type::String], Type::String),
            ("rpc_data", vec![Type::String], Type::String),
        ];
        for (name, params, ret) in rpc_builtins {
            self.add_entry(name.to_string(), Symbol::Variable {
                kind: Box::new(Type::Function { params, ret: Box::new(ret) }),
            });
        }

        // Pass 1: Register all top-level function declarations so forward references work
        for decl in program.iter() {
            match decl {
                Ast::DeclFunc { name, params, ret, .. } | Ast::DeclExtFunc { name, params, ret } => {
                    let function_type = Box::new(Type::Function {
                        params: params.iter().map(|(_, ty)| ty.to_owned()).collect(),
                        ret: Box::new(ret.clone()),
                    });
                    self.add_entry(name.to_owned(), Symbol::Variable { kind: function_type });
                }
                _ => {}
            }
        }

        // Pass 2: Check all declarations (function names already known)
        for decl in program {
            self.check(decl);
        }

    }
    fn check(&mut self, node: &mut Ast) -> Option<Type> {
        match node {
            Ast::DeclExtFunc { name: _, params: _, ret } => {
                Some(ret.clone())
            }
            Ast::DeclFunc { name: _, params, ret, body, scope } => {
                let function_type = Box::new(Type::Function {
                    params: params.iter().map(|(_, ty)| ty.to_owned()).collect(),
                    ret: Box::new(ret.clone()),
                });
                // Already registered in run() pass 1 — no need for add_entry

                *scope = self.new_table();
                for (name, kind) in params {
                    self.add_entry(name.to_owned(), Symbol::Variable { kind: Box::from(kind.clone()) });
                }

                let old = self.can_return;
                self.can_return = true;
                // TODO: actually do branch checking please
                self.check(body);
                // let ret_type = match self.check(*body) {
                //     Some(ty) => ty,
                //     None => {
                //         eprintln!("function `{name}` has diverging types throughout its branches");
                //         std::process::exit(1);
                //     }
                // };
                // if type_eq(ret_type.clone(), ret.clone()).is_err() {
                //     eprintln!("return type of function `{name}` ({ret:?}) does not match its body ({ret_type:?})");
                //     std::process::exit(1);
                // }
                self.can_return = old;

                self.pop_table();
                Some(*function_type)
            }
            Ast::DeclVar { name, kind, .. } => {
                self.add_entry(name.to_owned(), Symbol::Variable { kind: Box::new(kind.clone()) });
                Some(kind.clone())
            }
            Ast::DeclStruct { name, fields } => {
                self.add_entry(name.to_owned(), Symbol::Struct { fields: fields.iter().map(|(name, ty)| (name.to_owned(), Box::from(ty.clone()))).collect() });
                Some(Type::Ident(name.clone()))
            }
            Ast::DeclEnum { name, variants } => {
                self.add_entry(name.to_owned(), Symbol::Enum { variants: variants.to_owned() });
                Some(Type::Ident(name.clone()))
            }
            Ast::Block { block: stmts, scope } => {
                if stmts.len() == 0 { return Some(Type::Void); }
                *scope = self.new_table();

                let mut last = None;
                for stmt in stmts {
                    let ty = self.check(stmt)?;
                    // if last.is_some() && type_eq(ty.clone(), last.clone().unwrap()).is_err() {
                    //     return None;
                    // }
                    last = Some(ty);
                }

                self.pop_table();
                last
            }
            Ast::StmtImport { alias, .. } => {
                if let Some(a) = alias {
                    self.add_entry(a.to_owned(), Symbol::Variable { kind: Box::from(Type::Void) });
                }
                Some(Type::Void)
            }
            Ast::StmtIf { condition, body, alt } => {
                let cond = self.check(condition)?;
                if type_eq(cond.clone(), Type::Bool).is_err() {
                    self.emit_error(
                        codes::SEMA_WRONG_CONDITION_TYPE,
                        format!("condition of if statement must be of type `bool`, got {cond:?}"),
                    );
                    return None;
                }
                self.check(body);
                match alt {
                    Some(alt_block) => {
                        self.check(alt_block);
                    }
                    None => {}
                }

                Some(Type::Void)
            }
            Ast::StmtWhile { condition, body } => {
                let cond = self.check(&mut *condition)?;
                if type_eq(cond.clone(), Type::Bool).is_err() {
                    self.emit_error(
                        codes::SEMA_WRONG_CONDITION_TYPE,
                        format!("condition of while statement must be of type `bool`, got {cond:?}"),
                    );
                    return None;
                }
                let old = self.can_break;
                self.can_break = true;
                self.check(body);
                self.can_break = old;

                Some(Type::Void)
            }
            Ast::StmtFor { elem_name, array, body } => {
                let arr_type = self.check(array)?;
                let Type::Array { inner, .. } = arr_type else {
                    self.emit_error(
                        codes::SEMA_FOR_IN_NOT_ARRAY,
                        "array in which for loop is iterating over must have the known type `Array`",
                    );
                    return None;
                };

                self.add_entry(elem_name.to_owned(), Symbol::Variable { kind: inner });
                let old = self.can_break;
                self.can_break = true;
                self.check(body);
                self.can_break = old;

                Some(Type::Void)
            }
            Ast::ExprAssign { var_name, value } => {
                let var_type = {
                    let Some(entry) = self.get_entry(&var_name) else {
                        self.emit_error(
                            codes::SEMA_UNDEFINED_NAME,
                            format!("variable `{var_name}` is assigned to before initialization"),
                        );
                        return None;
                    };

                    if let Symbol::Variable { kind: var_type } = entry {
                        var_type.clone()
                    } else {
                        self.emit_error(
                            codes::SEMA_NOT_VARIABLE,
                            format!("`{var_name}` is assigned to, however it is not a variable"),
                        );
                        return None;
                    }
                };

                let rhs_type = self.check(value)?;
                if type_eq(*var_type.clone(), rhs_type.clone()).is_err() {
                    self.emit_error(
                        codes::SEMA_ASSIGN_TYPE_MISMATCH,
                        format!("variable `{var_name}` of type {:?} cannot be assigned a value of type {rhs_type:?}", *var_type),
                    );
                    return None;
                }
                Some(rhs_type)
            }
            Ast::ExprBinary { lhs, rhs, op } => {
                let lhs_type = self.check(lhs)?;
                let rhs_type = self.check(rhs)?;

                match op {
                    // Arithmetic Operations
                    Token::Plus | Token::Dash | Token::Star | Token::Slash => {
                        if type_eq(lhs_type.clone(), rhs_type.clone()).is_err() {
                            self.emit_error(
                                codes::SEMA_ARITH_TYPE_MISMATCH,
                                format!("mismatched types in arithmetic: {lhs_type:?} {op:?} {rhs_type:?}"),
                            );
                            return None;
                        }
                        // Arithmetic usually only works on numeric types
                        match lhs_type {
                            Type::Integer | Type::Float => Some(lhs_type),
                            _ => {
                                self.emit_error(
                                    codes::SEMA_ARITH_BAD_TYPE,
                                    format!("arithmetic operation {op:?} not supported for type {lhs_type:?}"),
                                );
                                None
                            }
                        }
                    }

                    // Equality and Comparison (Always returns Boolean)
                    Token::EqEq | Token::BangEq |
                    Token::MoreThan | Token::LessThan |
                    Token::MoreEq | Token::LessEq => {
                        if type_eq(lhs_type.clone(), rhs_type.clone()).is_err() {
                            self.emit_error(
                                codes::SEMA_COMPARE_TYPE_MISMATCH,
                                format!("cannot compare mismatched types: {lhs_type:?} {op:?} {rhs_type:?}"),
                            );
                            return None;
                        }
                        Some(Type::Bool)
                    }

                    // Logical Operations (Requires Booleans)
                    Token::AmpAmp | Token::PipePipe => {
                        if !matches!(lhs_type, Type::Bool) || !matches!(rhs_type, Type::Bool) {
                            self.emit_error(
                                codes::SEMA_LOGIC_NEEDS_BOOL,
                                format!("logical operation {op:?} requires boolean operands"),
                            );
                            return None;
                        }
                        Some(Type::Bool)
                    }

                    // Bitwise Operations (Usually requires Integers)
                    Token::Ampersand | Token::Pipe | Token::Caret | Token::LShift | Token::RShift => {
                        if !matches!(lhs_type, Type::Integer) || !matches!(rhs_type, Type::Integer) {
                            self.emit_error(
                                codes::SEMA_BITWISE_NEEDS_INT,
                                format!("bitwise operation {op:?} requires integer operands"),
                            );
                            return None;
                        }
                        Some(Type::Integer)
                    }

                    Token::Eq => {
                        if type_eq(lhs_type.clone(), rhs_type.clone()).is_err() {
                            self.emit_error(
                                codes::SEMA_ASSIGN_TYPE_MISMATCH,
                                format!("cannot assign mismatched types: {lhs_type:?} {op:?} {rhs_type:?}"),
                            );
                            return None;
                        }
                        Some(lhs_type)
                    }

                    _ => {
                        self.emit_error(
                            codes::SEMA_UNSUPPORTED_BINOP,
                            format!("unsupported binary operator: {op:?}"),
                        );
                        None
                    }
                }
            }
            Ast::ExprUnary { child, op } => {
                let child_type = self.check(child)?;

                match op {
                    Token::Dash => {
                        if type_eq(child_type.clone(), Type::Integer).is_err() && type_eq(child_type.clone(), Type::Float).is_err() {
                            self.emit_error(
                                codes::SEMA_NEGATE_NEEDS_NUMBER,
                                format!("cannot negate a non-number type: {child_type:?}"),
                            );
                            None
                        } else {
                            Some(child_type)
                        }
                    }
                    Token::Bang => {
                        if type_eq(child_type.clone(), Type::Integer).is_err() && type_eq(child_type.clone(), Type::Float).is_err() && type_eq(child_type.clone(), Type::Bool).is_err() {
                            self.emit_error(
                                codes::SEMA_BANG_BAD_TYPE,
                                format!("cannot apply `!` to this type: {child_type:?}"),
                            );
                            None
                        } else {
                            Some(child_type)
                        }
                    }
                    Token::Tilde => {
                        if type_eq(child_type.clone(), Type::Integer).is_err() {
                            self.emit_error(
                                codes::SEMA_TILDE_NEEDS_INT,
                                format!("cannot bitwise invert a non-integer type: {child_type:?}"),
                            );
                            None
                        } else {
                            Some(child_type)
                        }
                    }
                    _ => {
                        self.emit_error(
                            codes::SEMA_UNSUPPORTED_UNOP,
                            format!("unsupported unary operator: {op:?}"),
                        );
                        None
                    }
                }
            }
            Ast::ExprFuncCall { name, args } => {
                if name == "print" {
                    for arg in args {
                        self.check(arg)?;
                    }
                    return Some(Type::Void);
                }
                if name == "rpc_request" {
                    if args.len() != 2 {
                        self.emit_error(
                            codes::SEMA_RPC_WRONG_ARITY,
                            format!("rpc_request expects 2 arguments, got {}", args.len()),
                        );
                        return None;
                    }
                    let name_ty = self.check(&mut args[0])?;
                    if type_eq(name_ty, Type::String).is_err() {
                        self.emit_error(
                            codes::SEMA_RPC_BAD_SHAPE,
                            "rpc_request: first argument must be str",
                        );
                        return None;
                    }
                    let args_ty = self.check(&mut args[1])?;
                    match args_ty {
                        Type::Array { .. } => {}
                        _ => {
                            self.emit_error(
                                codes::SEMA_RPC_BAD_SHAPE,
                                "rpc_request: second argument must be an array",
                            );
                            return None;
                        }
                    }
                    return Some(Type::String);
                }
                if name == "rpc_response" {
                    if args.len() != 1 {
                        self.emit_error(
                            codes::SEMA_RPC_WRONG_ARITY,
                            format!("rpc_response expects 1 argument, got {}", args.len()),
                        );
                        return None;
                    }
                    self.check(&mut args[0])?;
                    return Some(Type::String);
                }
                // 1. Fetch and clone the signature in a temporary scope
                let (params, ret) = {
                    let Some(entry) = self.get_entry(&name) else {
                        self.emit_error(
                            codes::SEMA_CALL_UNDEFINED,
                            format!("attempting to make a function call on an undefined name `{name}`"),
                        );
                        return None;
                    };

                    if let Symbol::Variable { kind } = entry && let Type::Function { params, ret } = *kind.to_owned() {
                        // Clone the params and ret to release the borrow on self
                        (params.clone(), ret.clone())
                    } else {
                        self.emit_error(
                            codes::SEMA_CALL_NOT_FUNCTION,
                            format!("attempting to make a function call on a non-function type: `{name}`"),
                        );
                        return None;
                    }
                }; // Borrow of self ends here

                // 2. Validate argument count
                if args.len() != params.len() {
                    self.emit_error(
                        codes::SEMA_CALL_WRONG_ARITY,
                        format!("function `{name}` expects {} arguments but received {}", params.len(), args.len()),
                    );
                    return None;
                }

                // 3. Check each argument (safe to borrow self mutably now)
                for (i, (arg, param)) in args.iter_mut().zip(params.iter()).enumerate() {
                    let arg_type = self.check(arg)?;

                    if type_eq(arg_type.clone(), param.clone()).is_err() {
                        self.emit_error(
                            codes::SEMA_CALL_WRONG_ARG_TYPE,
                            format!("function `{name}` expected {param:?} in position {i} but was passed {arg_type:?}"),
                        );
                        return None;
                    }
                }

                // 4. Return the return type
                Some(*ret)
            }
            Ast::ExprMemAcc { lhs, member } => {
                let lhs_type = self.check(lhs)?;
                let Type::Ident(sname) = lhs_type else {
                    self.emit_error(
                        codes::SEMA_MEMBER_NOT_STRUCT,
                        format!("{lhs_type:?} is not a struct with members"),
                    );
                    return None;
                };

                let mem_type = {
                    let Some(entry) = self.get_entry(&sname) else {
                        self.emit_error(
                            codes::SEMA_UNKNOWN_STRUCT,
                            format!("could not find struct `{sname}` in scope"),
                        );
                        return None;
                    };

                    let Symbol::Struct { fields } = entry else {
                        self.emit_error(
                            codes::SEMA_NOT_A_STRUCT,
                            format!("`{sname}` is not a struct"),
                        );
                        return None;
                    };

                    let Some(mem) = fields.get(member) else {
                        self.emit_error(
                            codes::SEMA_NO_SUCH_FIELD,
                            format!("`{sname}` has no member `{member}`"),
                        );
                        return None;
                    };

                    mem.clone()
                };

                Some(*mem_type)
            }
            Ast::ExprEnumVar { name, var } => {
                let Some(entry) = self.get_entry(&name) else {
                    self.emit_error(
                        codes::SEMA_UNKNOWN_STRUCT,
                        format!("could not find type `{name}` in scope"),
                    );
                    return None;
                };

                let Symbol::Enum { variants } = entry else {
                    self.emit_error(
                        codes::SEMA_NOT_AN_ENUM,
                        format!("`{name}` is not an enum"),
                    );
                    return None;
                };

                if variants.get(var).is_none() {
                    self.emit_error(
                        codes::SEMA_NO_SUCH_VARIANT,
                        format!("`{name}` has no variant `{var}`"),
                    );
                    return None;
                };

                Some(Type::Ident(name.clone()))
            }
            Ast::ExprArrAcc { lhs, index } => {
                let lhs_type = self.check(lhs)?;
                let index_type = self.check(index)?;

                if !matches!(index_type, Type::Integer) && !matches!(index_type, Type::Float) {
                    self.emit_error(
                        codes::SEMA_BAD_INDEX_TYPE,
                        "array index must be an integer or float",
                    );
                    return None;
                }

                match lhs_type {
                    Type::Array { inner, .. } => Some(*inner),
                    _ => {
                        self.emit_error(
                            codes::SEMA_INDEX_NOT_ARRAY,
                            "cannot index into a non-array type",
                        );
                        None
                    }
                }
            }
            Ast::ExprReturn { val } => {
                if !self.can_return {
                    self.emit_error(
                        codes::SEMA_ILLEGAL_RETURN,
                        "illegal return statement",
                    );
                    return None;
                }
                match val {
                    Some(v) => Some(self.check(&mut *v)?),
                    None => Some(Type::Void),
                }
            }
            Ast::ExprInteger(_) => Some(Type::Integer),
            Ast::ExprFloat(_) => Some(Type::Float),
            Ast::ExprString(_) => Some(Type::String),
            Ast::ExprChar(_) => Some(Type::Char),
            Ast::ExprBool(_) => Some(Type::Bool),
            Ast::ExprVar(name) => {
                let var_type = {
                    let Some(entry) = self.get_entry(&name) else {
                        self.emit_error(
                            codes::SEMA_UNDEFINED_NAME,
                            format!("variable `{name}` could not be found in the current scope"),
                        );
                        return None;
                    };

                    if let Symbol::Variable { kind: var_type } = entry {
                        var_type.clone()
                    } else {
                        self.emit_error(
                            codes::SEMA_NOT_VARIABLE,
                            format!("`{name}` is not a variable"),
                        );
                        return None;
                    }
                };
                Some(*var_type)
            }
            // Ast::ExprStructInit { name, fields } => {}
            x => {
                // The analyzer has no rule for this AST shape yet.
                // Report as an ICE so editor-generated AST that hits
                // an unfinished arm doesn't abort the test runner
                // (or, in a browser, the WASM worker).
                self.diagnostics.push(SolDiagnostic::internal(
                    codes::ICE_UNHANDLED_AST,
                    format!("analyzer has no rule for AST node: {x:?}"),
                ));
                None
            }
        }
    }
}
