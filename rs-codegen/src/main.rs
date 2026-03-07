mod ast;
mod emit_rust;
mod lexer;
mod parser;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: almide-rs <file.almd | file.json> [--emit-ast]");
        std::process::exit(1);
    }

    let file = &args[1];
    let emit_ast = args.iter().any(|a| a == "--emit-ast");

    let input = std::fs::read_to_string(file)
        .unwrap_or_else(|e| { eprintln!("Error reading {}: {}", file, e); std::process::exit(1); });

    let program = if file.ends_with(".json") {
        // JSON AST input (legacy mode)
        serde_json::from_str(&input)
            .unwrap_or_else(|e| { eprintln!("JSON parse error: {}", e); std::process::exit(1); })
    } else {
        // .almd source input — lex + parse
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
        let rust_code = emit_rust::emit(&program);
        print!("{}", rust_code);
    }
}
