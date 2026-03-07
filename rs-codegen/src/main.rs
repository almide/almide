mod ast;
mod emit_rust;

use std::io::Read;

fn main() {
    let mut input = String::new();

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        input = std::fs::read_to_string(&args[1])
            .unwrap_or_else(|e| { eprintln!("Error reading {}: {}", args[1], e); std::process::exit(1); });
    } else {
        std::io::stdin().read_to_string(&mut input)
            .unwrap_or_else(|e| { eprintln!("Error reading stdin: {}", e); std::process::exit(1); });
    }

    let program: ast::Program = serde_json::from_str(&input)
        .unwrap_or_else(|e| { eprintln!("JSON parse error: {}", e); std::process::exit(1); });

    let rust_code = emit_rust::emit(&program);
    print!("{}", rust_code);
}
