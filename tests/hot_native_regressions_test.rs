use almide::lexer::{Lexer, TokenType};
use almide::parser::Parser;

#[test]
fn shebang_is_initial_trivia_and_keeps_error_positions() {
    let tokens = Lexer::tokenize("#!/usr/bin/env -S almide run\n\nfn main() -> Unit = {\n  #\n}\n");
    assert_eq!(tokens[0].token_type, TokenType::Comment);
    let unknown = tokens.iter().find(|t| t.value == "#").unwrap();
    assert_eq!((unknown.line, unknown.col), (4, 3));
    for source in [" #!/bin/sh", "\n#!/bin/sh", "#"] {
        assert_ne!(Lexer::tokenize(source)[0].token_type, TokenType::Comment);
    }
}

#[test]
fn shebang_precedes_dialect_and_survives_formatting() {
    for source in [
        "#!/usr/bin/env -S almide run\n\nfn main() -> Unit = println(\"ok\")\n",
        "#!/usr/bin/env -S almide run\n@dialect(1)\nfn main() -> Unit = println(\"ok\")\n",
        "\u{feff}#!/usr/bin/env -S almide run\r\nfn main() -> Unit = println(\"ok\")\r\n",
    ] {
        let program = Parser::new(Lexer::tokenize(source)).parse().unwrap();
        let formatted = almide::fmt::format_program(&program);
        assert!(formatted.starts_with("#!/usr/bin/env -S almide run\n"), "{formatted}");
        assert_eq!(formatted.matches("#!").count(), 1);
        let again = Parser::new(Lexer::tokenize(&formatted)).parse().unwrap();
        assert_eq!(almide::fmt::format_program(&again), formatted);
    }
}

#[test]
fn native_mir_declines_serializing_fan() {
    let block = "effect fn f(n: Int) -> Int = n + 1\neffect fn main() -> Unit = { let r = fan { f(1), f(2) }; println(int.to_string(r.0 + r.1)) }";
    let mapper = "effect fn main() -> Unit = { let r = fan.map([1, 2], (x) => ok(x + 1))!; println(int.to_string(list.len(r))) }";
    for source in [block, mapper] {
        let error = almide_mir::pipeline::try_render_rust_source(source).unwrap_err();
        assert!(format!("{error:?}").contains("fan concurrency"), "{error:?}");
        assert!(matches!(error, almide_mir::lower::LowerError::UnsupportedAt { .. }), "fan wall must retain its source location");
    }
    assert!(almide_mir::pipeline::try_render_rust_source("fn main() -> Unit = println(\"ordinary\")").is_ok());
}
