//! #4006 no-perf-loss: a tail-recursion loop whose exit flag an arm raises as
//! its last act breaks right there in the native render
//! (`render_native_slots::flag_exits`). With the flag left to the loop's
//! next test, LLVM kept the loop running where it folds the standard
//! codegen's counted loop: `effect_tco.almd` read 32M instructions against
//! v0's 7k.

fn render(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).expect("renders on v1"))
        .expect("spawn")
        .join()
        .expect("join")
}

/// The body of the rendered fn `name`.
fn body<'a>(code: &'a str, name: &str) -> &'a str {
    let at = code.find(&format!("fn almd_{name}(")).unwrap_or_else(|| panic!("no fn {name} in:\n{code}"));
    let rest = &code[at..];
    &rest[..rest.find("\n}\n").map_or(rest.len(), |e| e + 2)]
}

#[test]
fn a_tail_recursion_loop_breaks_where_its_flag_is_raised() {
    let code = render(
        "fn sum_to(n: Int, acc: Int) -> Int = if n <= 0 then acc else sum_to(n - 1, acc + n)\n\
         fn main() -> Unit = println(int.to_string(sum_to(1000, 0)))\n",
    );
    let f = body(&code, "sum_to");
    assert!(f.contains("loop {"), "no loop:\n{f}");
    assert!(f.contains("break;\n"), "the flag arm does not break:\n{f}");
}
