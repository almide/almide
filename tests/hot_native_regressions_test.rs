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
fn native_mir_never_serializes_a_fan() {
    let block = "effect fn f(n: Int) -> Result[Int, String] = ok(n + 1)\neffect fn main() -> Unit = { let r = fan { f(1), f(2) }; println(int.to_string(r.0 + r.1)) }";
    let mapper = "effect fn main() -> Unit = { let r = fan.map([1, 2], (x) => ok(x + 1))!; println(int.to_string(list.len(r))) }";
    // #4006 family H: a `fan { … }` block's arms are lifted and certified, and
    // the block renders on scoped threads.
    let rendered = almide_mir::pipeline::try_render_rust_source(block).expect("a fan block renders threaded");
    assert!(rendered.contains("std::thread::scope"), "the arms run on scoped threads:\n{rendered}");
    // An arm calling an effect fn declared `-> Int` renders too. A body that
    // cannot fail is a plain scalar fn, so its arm has no Err to carry.
    let implicit = block.replace("-> Result[Int, String] = ok(n + 1)", "-> Int = n + 1");
    let rendered = almide_mir::pipeline::try_render_rust_source(&implicit).expect("an infallible implicit arm renders threaded");
    assert!(rendered.contains("std::thread::scope"), "the arms run on scoped threads:\n{rendered}");
    assert!(rendered.contains("fn almd_f(v0: i64) -> i64"), "an infallible effect fn is a scalar fn:\n{rendered}");
    // A body that CAN fail rides the effect ABI's Result through the threaded
    // take, and the join tests the arms in index order so the lowest-index Err
    // is the one that propagates (ADR-0024 D1).
    let fallible = "effect fn g(n: Int) -> Result[Int, String] = if n > 1 then err(\"bad\") else ok(n)\neffect fn f(n: Int) -> Int = g(n)! + 1\neffect fn main() -> Unit = { let r = fan { f(1), f(2) }; println(int.to_string(r.0 + r.1)) }";
    let rendered = almide_mir::pipeline::try_render_rust_source(fallible).expect("a fallible implicit arm renders threaded");
    assert!(rendered.contains("(Result<i64, AlmideString>, Result<i64, AlmideString>) = std::thread::scope"), "each arm hands its Result to the join:\n{rendered}");
    let join = &rendered[rendered.find("std::thread::scope").unwrap()..];
    let (first, second) = (join.find("v0.is_err()"), join.find("v1.is_err()"));
    assert!(matches!((first, second), (Some(a), Some(b)) if a < b), "the join tests arm 0's Err before arm 1's:\n{rendered}");
    // `fan.map` renders through its registered parallel runtime site: the
    // mapper never runs as a sequential loop under the fan's name.
    let rendered = almide_mir::pipeline::try_render_rust_source(mapper).expect("fan.map renders on its parallel site");
    assert!(rendered.contains("almide_rt_fan_map_par("), "fan.map keeps its parallel runtime site:\n{rendered}");
    // A program with a metered region keeps the honest decline, located at the
    // fan: a region's budget is a per-thread counter.
    let metered = "effect fn f(n: Int) -> Result[Int, String] = ok(n + 1)\nfn spin(n: Int) -> Int = n * 2\neffect fn main() -> Unit = { let b = fan.bounded(compute.ns(3006)) { spin(3) } ?? 0; let r = fan { f(1), f(2) }; println(int.to_string(r.0 + r.1 + b)) }";
    let error = almide_mir::pipeline::try_render_rust_source(metered).unwrap_err();
    assert!(format!("{error:?}").contains("fan concurrency"), "{error:?}");
    assert!(matches!(error, almide_mir::lower::LowerError::UnsupportedAt { .. }), "fan wall must retain its source location");
    assert!(almide_mir::pipeline::try_render_rust_source("fn main() -> Unit = println(\"ordinary\")").is_ok());
}
