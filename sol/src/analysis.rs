use serde::Serialize;
use crate::ast::*;
use crate::parser::Parser;

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowCallSite {
    pub module: String,
    pub capability: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowAnalysis {
    pub workflow_name: String,
    pub call_graph: Vec<WorkflowCallSite>,
    pub imported_modules: Vec<String>,
    pub capabilities: Vec<String>,
}

/// Parse a SOL source string and return every capability name referenced
/// by `call("cap", …)` expressions or import-qualified calls.
///
/// Returns an empty vec (not an error) when there are no capability refs.
pub fn extract_capabilities(source: &str) -> Result<Vec<String>, String> {
    let analysis = analyze_all(source)?;
    Ok(analysis.capabilities)
}

/// Analyze a specific workflow in a SOL source and return its full analysis
/// including the ordered call graph, imported modules, and capabilities.
pub fn analyze_workflow(source: &str, workflow_name: &str) -> Result<WorkflowAnalysis, String> {
    let mut parser = Parser::new(source);
    let program = parser.parse()?;

    let mut modules: Vec<String> = Vec::new();

    for item in &program.items {
        if let TopLevel::Import(decl) = item {
            if let ImportSpec::Module(m) = &decl.spec {
                if !modules.contains(m) {
                    modules.push(m.clone());
                }
            }
        }
    }

    let mut call_graph: Vec<WorkflowCallSite> = Vec::new();

    for item in &program.items {
        match item {
            TopLevel::Workflow(w) if w.name == workflow_name => {
                collect_calls_from_block(&w.body, &modules, &mut call_graph);
            }
            TopLevel::Function(f) => {
                collect_calls_from_block(&f.body, &modules, &mut call_graph);
            }
            _ => {}
        }
    }

    let (mut caps, _): (Vec<_>, Vec<_>) = call_graph.iter()
        .map(|c| c.capability.clone())
        .partition(|_| true);
    caps.sort();
    caps.dedup();

    Ok(WorkflowAnalysis {
        workflow_name: workflow_name.to_string(),
        call_graph,
        imported_modules: modules,
        capabilities: caps,
    })
}

/// Analyze all workflows and return flat capability list (keeps backwards compat).
fn analyze_all(source: &str) -> Result<WorkflowAnalysis, String> {
    let mut parser = Parser::new(source);
    let program = parser.parse()?;

    let mut modules: Vec<String> = Vec::new();
    let mut caps: Vec<String> = Vec::new();

    for item in &program.items {
        if let TopLevel::Import(decl) = item {
            match &decl.spec {
                ImportSpec::Module(m) => {
                    if !modules.contains(m) {
                        modules.push(m.clone());
                    }
                }
                ImportSpec::Named { name, module: _ } => {
                    if !caps.contains(name) {
                        caps.push(name.clone());
                    }
                }
            }
        }
    }

    let mut call_graph: Vec<WorkflowCallSite> = Vec::new();

    for item in &program.items {
        match item {
            TopLevel::Function(f) => {
                collect_calls_from_block(&f.body, &modules, &mut call_graph);
            }
            TopLevel::Workflow(w) => {
                collect_calls_from_block(&w.body, &modules, &mut call_graph);
            }
            _ => {}
        }
    }

    for site in &call_graph {
        if !caps.contains(&site.capability) {
            caps.push(site.capability.clone());
        }
    }

    caps.sort();
    caps.dedup();

    Ok(WorkflowAnalysis {
        workflow_name: String::new(),
        call_graph,
        imported_modules: modules,
        capabilities: caps,
    })
}

// ── recursive statement walker ──

fn collect_calls_from_block(block: &Block, modules: &[String], calls: &mut Vec<WorkflowCallSite>) {
    for stmt in &block.stmts {
        collect_calls_from_stmt(stmt, modules, calls);
    }
}

fn collect_calls_from_stmt(stmt: &Stmt, modules: &[String], calls: &mut Vec<WorkflowCallSite>) {
    match stmt {
        Stmt::Let { value, .. } => {
            collect_calls_from_expr(value, modules, calls);
        }
        Stmt::Assign { value, .. } => {
            collect_calls_from_expr(value, modules, calls);
        }
        Stmt::If {
            condition,
            then,
            else_,
        } => {
            collect_calls_from_expr(condition, modules, calls);
            collect_calls_from_block(then, modules, calls);
            if let Some(b) = else_ {
                collect_calls_from_block(b, modules, calls);
            }
        }
        Stmt::While { condition, body } => {
            collect_calls_from_expr(condition, modules, calls);
            collect_calls_from_block(body, modules, calls);
        }
        Stmt::For { iter, body, .. } => {
            collect_calls_from_expr(iter, modules, calls);
            collect_calls_from_block(body, modules, calls);
        }
        Stmt::Return(val) => {
            if let Some(e) = val {
                collect_calls_from_expr(e, modules, calls);
            }
        }
        Stmt::Expr(e) => {
            collect_calls_from_expr(e, modules, calls);
        }
        Stmt::Emit(_) => {}
    }
}

// ── recursive expression walker ──

fn collect_calls_from_expr(expr: &Expr, modules: &[String], calls: &mut Vec<WorkflowCallSite>) {
    match expr {
        Expr::WorkflowCall {
            capability_expr,
            params,
        } => {
            // If the capability name is a static string literal, we can extract it.
            // Dynamic expressions (variables, concatenations, etc.) are skipped in
            // static analysis — they'll be resolved at runtime.
            if let Expr::Str(capability_name) = capability_expr.as_ref() {
                let module = capability_name.rsplit_once('.')
                    .map(|(m, _)| m.to_string())
                    .unwrap_or_default();
                calls.push(WorkflowCallSite {
                    module,
                    capability: capability_name.clone(),
                });
            }
            collect_calls_from_expr(params, modules, calls);
        }
        Expr::Call(callee, args) => {
            // module.function()  →  capability "module.function"
            if let Expr::MemberAccess(obj, field) = callee.as_ref() {
                if let Expr::Ident(mod_name) = obj.as_ref() {
                    if modules.contains(mod_name) {
                        let cap = format!("{}.{}", mod_name, field);
                        calls.push(WorkflowCallSite {
                            module: mod_name.clone(),
                            capability: cap,
                        });
                    }
                }
            }
            for arg in args {
                collect_calls_from_expr(arg, modules, calls);
            }
        }
        Expr::BinOp(left, _, right) => {
            collect_calls_from_expr(left, modules, calls);
            collect_calls_from_expr(right, modules, calls);
        }
        Expr::UnaryOp(e, _) => {
            collect_calls_from_expr(e, modules, calls);
        }
        Expr::MemberAccess(obj, _) => {
            collect_calls_from_expr(obj, modules, calls);
        }
        Expr::Index(arr, idx) => {
            collect_calls_from_expr(arr, modules, calls);
            collect_calls_from_expr(idx, modules, calls);
        }
        Expr::Array(elements) => {
            for e in elements {
                collect_calls_from_expr(e, modules, calls);
            }
        }
        Expr::StructInstance { fields, .. } => {
            for (_, v) in fields {
                collect_calls_from_expr(v, modules, calls);
            }
        }
                Expr::NamespaceCall { namespace, name: _, args } => {
                    collect_calls_from_expr(namespace, modules, calls);
                    for arg in args {
                        collect_calls_from_expr(arg, modules, calls);
                    }
                }
                Expr::Int(_)
        | Expr::Float(_)
        | Expr::Bool(_)
        | Expr::Char(_)
        | Expr::Str(_)
        | Expr::Ident(_)
        | Expr::EnumVariant { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_call_caps() {
        let source = r#"
            workflow "test" {
                let data = call("system.cpu", {});
                print(data);
            }
        "#;
        let caps = extract_capabilities(source).unwrap();
        assert_eq!(caps, vec!["system.cpu"]);
    }

    #[test]
    fn test_extract_import_caps() {
        let source = r#"
            import system;
            workflow "test" {
                let data = system.cpu({});
                print(data);
            }
        "#;
        let caps = extract_capabilities(source).unwrap();
        assert_eq!(caps, vec!["system.cpu"]);
    }

    #[test]
    fn test_extract_mixed() {
        let source = r#"
            import numbers;
            workflow "test" {
                let a = call("system.cpu", {});
                let b = numbers.get(42);
                print(a, b);
            }
        "#;
        let caps = extract_capabilities(source).unwrap();
        assert_eq!(caps, vec!["numbers.get", "system.cpu"]);
    }

    #[test]
    fn test_no_caps() {
        let source = r#"
            workflow "empty" {
                print("hello");
            }
        "#;
        let caps = extract_capabilities(source).unwrap();
        let expected: Vec<String> = vec![];
        assert_eq!(caps, expected);
    }

    #[test]
    fn test_named_import() {
        let source = r#"
            import "get" from numbers;
            workflow "test" {
                let x = get(5);
                print(x);
            }
        "#;
        let caps = extract_capabilities(source).unwrap();
        assert_eq!(caps, vec!["get"]);
    }

    #[test]
    fn test_analyze_workflow_call_graph() {
        let source = r#"
            import app_b1;
            workflow "demo" {
                let temp = app_b1.get_temp({ sensor: "rooftop" });
                let alert = app_b1.notify({ msg: "done" });
            }
        "#;
        let analysis = analyze_workflow(source, "demo").unwrap();
        assert_eq!(analysis.call_graph.len(), 2);
        assert_eq!(analysis.call_graph[0].capability, "app_b1.get_temp");
        assert_eq!(analysis.call_graph[0].module, "app_b1");
        assert_eq!(analysis.call_graph[1].capability, "app_b1.notify");
        assert_eq!(analysis.call_graph[1].module, "app_b1");
    }

    #[test]
    fn test_analyze_workflow_ordered() {
        let source = r#"
            import a;
            import b;
            workflow "order" {
                let x = a.foo({});
                let y = b.bar({});
                let z = a.baz({});
            }
        "#;
        let analysis = analyze_workflow(source, "order").unwrap();
        let caps: Vec<&str> = analysis.call_graph.iter().map(|c| c.capability.as_str()).collect();
        assert_eq!(caps, vec!["a.foo", "b.bar", "a.baz"]);
    }

    #[test]
    fn test_analyze_workflow_three_node() {
        let source = r#"
            import app_b1;
            import app_b2;
            import app_c1;

            workflow "demo" {
                let temp = app_b1.get_temp({ sensor: "rooftop" });
                let log_res = app_b2.log({ text: "check" });
                let alert = app_c1.notify({ message: "done" });
            }
        "#;
        let analysis = analyze_workflow(source, "demo").unwrap();
        assert_eq!(analysis.call_graph.len(), 3);
        assert_eq!(analysis.call_graph[0].capability, "app_b1.get_temp");
        assert_eq!(analysis.call_graph[1].capability, "app_b2.log");
        assert_eq!(analysis.call_graph[2].capability, "app_c1.notify");
        assert_eq!(analysis.imported_modules, vec!["app_b1", "app_b2", "app_c1"]);
    }

    #[test]
    fn test_analyze_with_branches_includes_all_paths() {
        let source = r#"
            import x;
            import y;
            workflow "branches" {
                if (true) {
                    let a = x.foo({});
                } else {
                    let b = y.bar({});
                }
            }
        "#;
        let analysis = analyze_workflow(source, "branches").unwrap();
        let caps: Vec<&str> = analysis.call_graph.iter().map(|c| c.capability.as_str()).collect();
        assert!(caps.contains(&"x.foo"));
        assert!(caps.contains(&"y.bar"));
    }
}
