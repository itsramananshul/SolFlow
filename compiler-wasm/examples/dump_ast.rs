//! Maintainer-aide: dump the AST JSON for a SOL source file.
//!
//! Usage:
//!   cargo run -p solflow_compiler_wasm --example dump_ast -- path/to/file.sol
//!   cargo run -p solflow_compiler_wasm --example dump_ast            # built-in sample
//!
//! Used to (re)generate the importer test fixtures under
//! `src/graph/import/__fixtures__/`. Pretty-prints with serde_json
//! so the JSON is diff-friendly.

const SAMPLE: &str = r#"
struct Point { x: int, y: int }
enum Status { Active, Inactive }
import "io" as io;
function add(a: int, b: int) -> int {
    return a + b;
}
function start() -> int {
    let p: int = 0;
    if (p == 0) {
        print("zero");
    } else {
        print("nonzero");
    }
    while (p < 10) {
        p = p + 1;
    }
    for x in [1, 2, 3] {
        print(x);
    }
    return add(1, 2);
}
"#;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let source = if args.len() > 1 {
        std::fs::read_to_string(&args[1])
            .unwrap_or_else(|e| panic!("read {}: {e}", args[1]))
    } else {
        SAMPLE.to_string()
    };

    let result = solflow_compiler::parse_source(&source);
    if result.has_errors() {
        for d in &result.diagnostics {
            eprintln!("{}", solflow_compiler::format_diagnostic(d, Some(&source), None));
        }
        std::process::exit(1);
    }
    let json = serde_json::to_string_pretty(&result.value).expect("serialize");
    println!("{json}");
}
