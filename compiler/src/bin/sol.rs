//! Minimal CLI consuming solflow_compiler as a library.
//!
//! Phase B.1 — proves the library API links and exercises the
//! pipeline end-to-end against a fixture. Phase B.6 wires this up
//! to print structured diagnostics; for now it just runs the
//! existing pipeline (which still process-exits on errors).
//!
//! Usage:
//!   cargo run --bin sol -- <path-to-.sol-file>
//!   cargo run --bin sol -- --debug-tokens <path>
//!   cargo run --bin sol -- --debug-ast    <path>

use solflow_compiler::lexer::Lexer;
use solflow_compiler::parser::Parser;
use solflow_compiler::analyzer::Analyzer;
use solflow_compiler::bytecode::Codegen;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut debug_tokens = false;
    let mut debug_ast = false;
    let mut path: Option<String> = None;
    for a in args.iter().skip(1) {
        match a.as_str() {
            "--debug-tokens" => debug_tokens = true,
            "--debug-ast"    => debug_ast = true,
            other => {
                if path.is_some() {
                    eprintln!("usage: sol [--debug-tokens] [--debug-ast] <file.sol>");
                    std::process::exit(2);
                }
                path = Some(other.to_string());
            }
        }
    }
    let Some(file) = path else {
        eprintln!("usage: sol [--debug-tokens] [--debug-ast] <file.sol>");
        std::process::exit(2);
    };

    let mut lexer = Lexer::from(&file);
    let tokens = lexer.tokens();
    if debug_tokens {
        eprintln!("{tokens:#?}");
    }

    let mut parser = Parser::from(tokens);
    let mut program = parser.run();
    if debug_ast {
        eprintln!("{program:#?}");
    }

    let mut analyzer = Analyzer::new();
    analyzer.run(&mut program);

    let mut codegen = Codegen::from(analyzer.tt_arena);
    let _bytecode = codegen.gen_bcode(&program);

    println!("OK: parsed + analyzed + compiled {file}");
}
