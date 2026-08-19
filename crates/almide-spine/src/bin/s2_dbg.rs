fn main() {
    let src = "fn __s2_base() -> Int = 1\nfn __s2_user() -> Int = __s2_base()\n";
    let tokens = almide_syntax::lexer::Lexer::tokenize(src);
    let mut p = almide_syntax::parser::Parser::new(tokens).with_file("dbg.almd");
    let prog = p.parse().unwrap();
    for d in &prog.decls {
        let v = serde_json::to_value(d).unwrap();
        println!("{}", serde_json::to_string(&v).unwrap().chars().take(400).collect::<String>());
    }
}
