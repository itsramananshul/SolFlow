//! Canonical pretty-printer: AST back to SOL source. Output re-parses to an
//! equal AST (verified by roundtrip tests), so the editor's graph-to-source
//! view can rely on it instead of a hand-rolled emitter.
use crate::ast::*;
use crate::parser::Parser;

pub fn format_source(src: &str) -> Result<String, String> {
    let prog = Parser::new(src).parse()?;
    Ok(format_program(&prog))
}

pub fn format_program(p: &Program) -> String {
    let mut out = String::new();
    for (i, item) in p.items.iter().enumerate() {
        if i > 0 { out.push('\n'); }
        fmt_toplevel(item, &mut out);
    }
    out
}

fn pad(n: usize) -> String { "    ".repeat(n) }
fn esc(s: &str) -> String { s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t") }

fn fmt_type(t: &Type) -> String {
    match t {
        Type::Bool => "bool".into(), Type::Int => "int".into(), Type::Float => "float".into(),
        Type::Char => "char".into(), Type::Str => "str".into(),
        Type::Array(inner) => format!("[]{}", fmt_type(inner)),
        Type::Named(n) => n.clone(),
    }
}

fn fmt_binop(op: &BinOp) -> &'static str {
    match op { BinOp::Add=>"+", BinOp::Sub=>"-", BinOp::Mul=>"*", BinOp::Div=>"/", BinOp::Eq=>"==", BinOp::Ne=>"!=", BinOp::Lt=>"<", BinOp::Gt=>">", BinOp::Le=>"<=", BinOp::Ge=>">=", BinOp::And=>"&&", BinOp::Or=>"||" }
}

fn fmt_expr(e: &Expr) -> String {
    match e {
        Expr::Int(i) => i.to_string(),
        Expr::Float(f) => { let s = f.to_string(); if s.contains('.') || s.contains('e') || s.contains('E') || s.contains("inf") || s.contains("NaN") { s } else { format!("{s}.0") } }
        Expr::Bool(b) => b.to_string(),
        Expr::Char(c) => format!("'{}'", match c { '\n'=>"\\n".into(), '\t'=>"\\t".into(), '\\'=>"\\\\".into(), '\''=>"\\'".into(), other=>other.to_string() }),
        Expr::Str(s) => format!("\"{}\"", esc(s)),
        Expr::Array(items) => format!("[{}]", items.iter().map(fmt_expr).collect::<Vec<_>>().join(", ")),
        Expr::StructInstance { name, fields } => {
            let body = fields.iter().map(|(k, v)| format!("{k}: {}", fmt_expr(v))).collect::<Vec<_>>().join(", ");
            if name.is_empty() { if body.is_empty() { "{}".into() } else { format!("{{ {body} }}") } }
            else if body.is_empty() { format!("{name} {{}}") } else { format!("{name} {{ {body} }}") }
        }
        Expr::EnumVariant { enum_name, variant } => format!("{enum_name}::{variant}"),
        Expr::Ident(n) => n.clone(),
        Expr::MemberAccess(b, f) => format!("{}.{f}", fmt_expr(b)),
        Expr::Index(b, i) => format!("{}[{}]", fmt_expr(b), fmt_expr(i)),
        Expr::BinOp(l, op, r) => format!("({} {} {})", fmt_expr(l), fmt_binop(op), fmt_expr(r)),
        Expr::UnaryOp(x, op) => format!("({}{})", match op { UnaryOp::Neg=>"-", UnaryOp::Not=>"!" }, fmt_expr(x)),
        Expr::Call(callee, args) => format!("{}({})", fmt_expr(callee), args.iter().map(fmt_expr).collect::<Vec<_>>().join(", ")),
        Expr::WorkflowCall { capability_expr, params } => format!("call({}, {})", fmt_expr(capability_expr), fmt_expr(params)),
        Expr::NamespaceCall { namespace, name, args } => format!("{}::{name}({})", fmt_expr(namespace), args.iter().map(fmt_expr).collect::<Vec<_>>().join(", ")),
    }
}

fn fmt_target(t: &Target) -> String {
    match t {
        Target::Ident(n) => n.clone(),
        Target::MemberAccess(b, f) => format!("{}.{f}", fmt_target(b)),
        Target::Index(b, i) => format!("{}[{}]", fmt_target(b), fmt_expr(i)),
    }
}

fn fmt_block(b: &Block, depth: usize, out: &mut String) {
    for s in &b.stmts { fmt_stmt(s, depth, out); }
}

fn fmt_stmt(s: &Stmt, depth: usize, out: &mut String) {
    let p = pad(depth);
    match s {
        Stmt::Let { name, type_, value } => out.push_str(&format!("{p}let {name}: {} = {};\n", fmt_type(type_), fmt_expr(value))),
        Stmt::Assign { target, value } => out.push_str(&format!("{p}{} = {};\n", fmt_target(target), fmt_expr(value))),
        Stmt::If { condition, then, else_ } => {
            out.push_str(&format!("{p}if ({}) {{\n", fmt_expr(condition)));
            fmt_block(then, depth + 1, out);
            if let Some(e) = else_ { out.push_str(&format!("{p}}} else {{\n")); fmt_block(e, depth + 1, out); }
            out.push_str(&format!("{p}}}\n"));
        }
        Stmt::While { condition, body } => {
            out.push_str(&format!("{p}while ({}) {{\n", fmt_expr(condition)));
            fmt_block(body, depth + 1, out);
            out.push_str(&format!("{p}}}\n"));
        }
        Stmt::For { item, iter, body } => {
            out.push_str(&format!("{p}for {item} in {} {{\n", fmt_expr(iter)));
            fmt_block(body, depth + 1, out);
            out.push_str(&format!("{p}}}\n"));
        }
        Stmt::Return(Some(e)) => out.push_str(&format!("{p}return {};\n", fmt_expr(e))),
        Stmt::Return(None) => out.push_str(&format!("{p}return;\n")),
        Stmt::Expr(e) => out.push_str(&format!("{p}{};\n", fmt_expr(e))),
        Stmt::Emit(name) => out.push_str(&format!("{p}emit \"{}\";\n", esc(name))),
    }
}

fn fmt_toplevel(t: &TopLevel, out: &mut String) {
    match t {
        TopLevel::Import(d) => match &d.spec {
            ImportSpec::Module(m) => out.push_str(&format!("import {m};\n")),
            ImportSpec::Named { name, module } => out.push_str(&format!("import \"{name}\" from {module};\n")),
        },
        TopLevel::Struct(s) => {
            out.push_str(&format!("struct {} {{\n", s.name));
            for f in &s.fields { out.push_str(&format!("    {}: {};\n", f.name, fmt_type(&f.type_))); }
            out.push_str("}\n");
        }
        TopLevel::Enum(e) => {
            out.push_str(&format!("enum {} {{\n", e.name));
            for v in &e.variants { out.push_str(&format!("    {v};\n")); }
            out.push_str("}\n");
        }
        TopLevel::Function(f) => {
            let params = f.params.iter().map(|p| format!("{}: {}", p.name, fmt_type(&p.type_))).collect::<Vec<_>>().join(", ");
            out.push_str(&format!("fn {}({params})", f.name));
            if let Some(rt) = &f.return_type { out.push_str(&format!(" <- {}", fmt_type(rt))); }
            out.push_str(" {\n");
            fmt_block(&f.body, 1, out);
            out.push_str("}\n");
        }
        TopLevel::Workflow(w) => {
            out.push_str(&format!("workflow \"{}\" {{\n", w.name));
            fmt_block(&w.body, 1, out);
            out.push_str("}\n");
        }
    }
}
