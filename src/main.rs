mod ast;
mod emit_rust;
mod emit_ts;
mod lexer;
mod parser;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let files: Vec<&str> = args.iter().skip(1)
        .filter(|a| !a.starts_with("--"))
        .map(|s| s.as_str())
        .collect();

    if files.is_empty() {
        eprintln!("Usage: almide <file.almd> [--target rust|ts] [--emit-ast]");
        std::process::exit(1);
    }

    let file = files[0];
    let emit_ast = args.iter().any(|a| a == "--emit-ast");

    let target = args.iter()
        .position(|a| a == "--target")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("rust");

    let input = std::fs::read_to_string(file)
        .unwrap_or_else(|e| { eprintln!("Error reading {}: {}", file, e); std::process::exit(1); });

    let program = if file.ends_with(".json") {
        serde_json::from_str(&input)
            .unwrap_or_else(|e| { eprintln!("JSON parse error: {}", e); std::process::exit(1); })
    } else {
        let tokens = lexer::Lexer::tokenize(&input);
        let mut parser = parser::Parser::new(tokens);
        parser.parse()
            .unwrap_or_else(|e| { eprintln!("Parse error: {}", e); std::process::exit(1); })
    };

    if emit_ast {
        let json = serde_json::to_string_pretty(&program)
            .unwrap_or_else(|e| { eprintln!("JSON serialize error: {}", e); std::process::exit(1); });
        println!("{}", json);
    } else {
        let code = match target {
            "rust" | "rs" => emit_rust::emit(&program),
            "ts" | "typescript" => emit_ts::emit(&program),
            other => { eprintln!("Unknown target: {}. Use rust or ts.", other); std::process::exit(1); }
        };
        print!("{}", code);
    }
}
