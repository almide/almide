fn main() {
    let prog = "effect fn main() -> Unit = {\n  println(\"hello, wasm\")\n  println(\"second\")\n}\n";
    let ir = almide_spine::s5::lower_to_ir("hello.almd", prog).expect("probe-bin invariant");
    for f in ir.functions.iter().filter(|f| f.name.as_str() == "main") {
        println!("{}", serde_json::to_string_pretty(&f.body).expect("probe-bin invariant").chars().take(2500).collect::<String>());
    }
}
