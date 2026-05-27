//! Minimal CLI consuming solflow_compiler as a library.
//!
//! Reads a `.sol` file, runs the full compile pipeline, and prints
//! any diagnostics in cargo-style format. Exits non-zero if any
//! error-severity diagnostic is produced.
//!
//! Usage:
//!   cargo run --bin sol -- <path-to-.sol-file>
//!   cargo run --bin sol -- --debug-tokens <path>
//!   cargo run --bin sol -- --debug-ast    <path>

use solflow_compiler::{
    analyze_source, compile_source, format_diagnostic, lex_source, parse_source,
};

fn usage_and_exit() -> ! {
    eprintln!("usage: sol [--debug-tokens] [--debug-ast] <file.sol>");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut debug_tokens = false;
    let mut debug_ast = false;
    let mut path: Option<String> = None;
    for a in args.iter().skip(1) {
        match a.as_str() {
            "--debug-tokens" => debug_tokens = true,
            "--debug-ast" => debug_ast = true,
            other => {
                if path.is_some() {
                    usage_and_exit();
                }
                path = Some(other.to_string());
            }
        }
    }
    let Some(file) = path else { usage_and_exit() };

    let source = match std::fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read {file}: {e}");
            std::process::exit(2);
        }
    };

    if debug_tokens {
        let lex = lex_source(&source);
        eprintln!("{:#?}", lex.value.as_ref().unwrap_or(&Vec::new()));
        print_diagnostics(&lex.diagnostics, &source, &file);
        if lex.has_errors() {
            std::process::exit(1);
        }
    }

    if debug_ast {
        let parse = parse_source(&source);
        eprintln!("{:#?}", parse.value);
        print_diagnostics(&parse.diagnostics, &source, &file);
        if parse.has_errors() {
            std::process::exit(1);
        }
    }

    // Analyze first so semantic errors print under the analyzer
    // banner even when codegen would also flag something downstream.
    let analyze = analyze_source(&source);
    print_diagnostics(&analyze.diagnostics, &source, &file);
    if analyze.has_errors() {
        std::process::exit(1);
    }

    let compile = compile_source(&source);
    print_diagnostics(&compile.diagnostics, &source, &file);
    if compile.has_errors() {
        std::process::exit(1);
    }

    println!("OK: parsed + analyzed + compiled {file}");
}

fn print_diagnostics(
    diags: &[solflow_compiler::SolDiagnostic],
    source: &str,
    file: &str,
) {
    for d in diags {
        eprintln!("{}", format_diagnostic(d, Some(source), Some(file)));
    }
}
