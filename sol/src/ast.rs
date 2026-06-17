//! Abstract syntax tree (AST) types for the Sol language.
//!
//! These types represent parsed Sol programs. They are produced by the
//! [`Parser`](crate::parser::Parser) and consumed by the
//! [`Interpreter`](crate::interpreter::Interpreter) and
//! analysis passes.

/// A Sol type annotation.
use serde::Serialize;
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Type {
    /// `bool`
    Bool,
    /// `int`
    Int,
    /// `float`
    Float,
    /// `char`
    Char,
    /// `str`
    Str,
    /// `[T]` — array of `T`
    Array(Box<Type>),
    /// A named type (struct or enum), e.g. `MyStruct`
    Named(String),
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum BinOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `&&`
    And,
    /// `||`
    Or,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum UnaryOp {
    /// `-expr` (numeric negation)
    Neg,
    /// `!expr` (logical not)
    Not,
}

/// An expression in the Sol language.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Expr {
    /// Integer literal: `42`
    Int(i64),
    /// Floating-point literal: `3.14`
    Float(f64),
    /// Boolean literal: `true`, `false`
    Bool(bool),
    /// Character literal: `'x'`
    Char(char),
    /// String literal: `"hello"`
    Str(String),
    /// Array literal: `[1, 2, 3]`
    Array(Vec<Expr>),
    /// Struct instance: `{ key: value, … }` (anonymous) or `Name { … }` (named)
    StructInstance {
        /// The struct name (empty for anonymous structs).
        name: String,
        /// Field name/value pairs.
        fields: Vec<(String, Expr)>,
    },
    /// Enum variant: `MyEnum::Variant`
    EnumVariant {
        /// The enum name.
        enum_name: String,
        /// The variant name.
        variant: String,
    },
    /// Variable reference: `my_var`
    Ident(String),
    /// Member access: `obj.field`
    MemberAccess(Box<Expr>, String),
    /// Index access: `arr[idx]`
    Index(Box<Expr>, Box<Expr>),
    /// Binary operation: `left + right`
    BinOp(Box<Expr>, BinOp, Box<Expr>),
    /// Unary operation: `-expr` or `!expr`
    UnaryOp(Box<Expr>, UnaryOp),
    /// Function call: `callee(arg1, arg2, …)`
    /// Resolves to either a local function or a workflow capability call.
    Call(Box<Expr>, Vec<Expr>),
    /// Explicit workflow capability call: `call("cap.name", params)`
    /// or `call(expr, params)` where `expr` evaluates to a capability name string.
    WorkflowCall {
        /// Expression that evaluates to the capability name string.
        capability_expr: Box<Expr>,
        /// Parameters to pass to the capability.
        params: Box<Expr>,
    },
    /// Namespace-qualified call: `module::rpc(args)` where `module` is
    /// any expression (typically a `Value::Module` or a known import).
    NamespaceCall {
        /// Expression that evaluates to a module/namespace.
        namespace: Box<Expr>,
        /// The RPC function name.
        name: String,
        /// Arguments to the RPC call.
        args: Vec<Expr>,
    },
}

/// A block statement — a sequence of statements delimited by `{ }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Block {
    /// The statements in this block.
    pub stmts: Vec<Stmt>,
}

/// A statement in the Sol language.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Stmt {
    /// Variable declaration: `let name: Type = value;`
    Let {
        /// Variable name.
        name: String,
        /// Type annotation.
        type_: Type,
        /// Initialiser expression.
        value: Expr,
    },
    /// Assignment: `target = value;`
    Assign {
        /// The assignment target (identifier, field, or index).
        target: Target,
        /// The value to assign.
        value: Expr,
    },
    /// Conditional: `if (condition) { … } else { … }`
    If {
        /// The condition expression.
        condition: Expr,
        /// The `then` branch.
        then: Block,
        /// The optional `else` branch.
        else_: Option<Block>,
    },
    /// While loop: `while (condition) { … }`
    While {
        /// The loop condition.
        condition: Expr,
        /// The loop body.
        body: Block,
    },
    /// For-in loop: `for item in iter { … }`
    For {
        /// The loop variable name.
        item: String,
        /// The iterable expression (currently must be an array).
        iter: Expr,
        /// The loop body.
        body: Block,
    },
    /// Return statement: `return;` or `return value;`
    Return(Option<Expr>),
    /// Expression statement — an expression evaluated for side effects.
    Expr(Expr),
    /// Emit an event string: `emit "event_name";`
    Emit(String),
}

/// An assignment target — the left-hand side of an `=` expression.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Target {
    /// Simple identifier: `x = …`
    Ident(String),
    /// Field assignment: `obj.field = …`
    MemberAccess(Box<Target>, String),
    /// Index assignment: `arr[idx] = …`
    Index(Box<Target>, Box<Expr>),
}

/// A function parameter.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Param {
    /// The parameter name.
    pub name: String,
    /// The parameter type annotation.
    pub type_: Type,
}

/// A function declaration: `fn name(params) -> ReturnType { … }`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FunctionDecl {
    /// The function name.
    pub name: String,
    /// The parameter list.
    pub params: Vec<Param>,
    /// The optional return type.
    pub return_type: Option<Type>,
    /// The function body block.
    pub body: Block,
}

/// A workflow declaration: `workflow "name" { … }`
///
/// Workflows are the top-level executable unit in OpenPrem.
/// They contain imperative Sol code that may call capabilities
/// via `call("cap.name", params)` or `module.function(params)`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkflowDecl {
    /// The workflow name (a string literal).
    pub name: String,
    /// The workflow body block.
    pub body: Block,
}

/// A named field in a struct declaration.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Field {
    /// The field name.
    pub name: String,
    /// The field type.
    pub type_: Type,
}

/// A struct declaration: `struct Name { field: Type; … }`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StructDecl {
    /// The struct name.
    pub name: String,
    /// The struct fields.
    pub fields: Vec<Field>,
}

/// An enum declaration: `enum Name { Variant1; Variant2; … }`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnumDecl {
    /// The enum name.
    pub name: String,
    /// The variant names.
    pub variants: Vec<String>,
}

/// An import specifier.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ImportSpec {
    /// `import module;` — imports all capabilities from `module`.
    Module(String),
    /// `import "name" from module;` — imports a single named capability.
    Named { name: String, module: String },
}

/// An import declaration.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportDecl {
    /// The import specification.
    pub spec: ImportSpec,
}

/// A top-level declaration in a Sol program.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum TopLevel {
    /// A function declaration.
    Function(FunctionDecl),
    /// A struct declaration.
    Struct(StructDecl),
    /// An enum declaration.
    Enum(EnumDecl),
    /// A workflow declaration.
    Workflow(WorkflowDecl),
    /// An import declaration.
    Import(ImportDecl),
}

/// A complete Sol program — a sequence of top-level items.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Program {
    /// The top-level items in the program.
    pub items: Vec<TopLevel>,
}
