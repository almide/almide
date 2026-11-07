//! An interpolation piece built once at the top level, whose chain result a
//! call inside a loop reads, keeps its scheduled Drop in the native render
//! (`render_native_tail::last_top_level_read`). Released right after that
//! reader, `drop(v)` ran on every pass and rustc refused the second move
//! (E0382): the inspection probe of `value.get_int` hit it once the linked
//! self-host declarations let `main` render on v1.

fn render(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).expect("renders on v1"))
        .expect("spawn")
        .join()
        .expect("join")
}

#[test]
fn a_piece_whose_chain_a_loop_reads_is_not_released_inside_the_loop() {
    let code = render(
        "effect fn main() -> Unit = {\n\
         \x20 let x: Value = value.object([(\"k1\", value.int(1))])\n\
         \x20 let a1: String = \"k${1}\"\n\
         \x20 var i = 0\n\
         \x20 while i < 2 {\n\
         \x20   let r: Option[Int] = value.get_int(x, a1)\n\
         \x20   println(\"${option.is_some(r)}\")\n\
         \x20   i = i + 1\n\
         \x20 }\n\
         }\n",
    );
    let at = code.find("fn main() {").unwrap_or_else(|| panic!("no main in:\n{code}"));
    let main = &code[at..];
    let main = &main[..main.find("\n}\n").map_or(main.len(), |e| e + 2)];
    let lp = main.find("loop {").unwrap_or_else(|| panic!("no loop in:\n{main}"));
    // The loop's body: up to the `}` that closes it (the scope-end comments
    // name the erased releases; a rendered one is a `drop(..);` statement).
    let body = &main[lp..];
    let body = &body[..body.find("\n    }\n").unwrap_or_else(|| panic!("no loop end in:\n{main}"))];
    assert!(
        !body.lines().any(|l| l.trim_start().starts_with("drop(")),
        "a release inside the loop:\n{main}"
    );
    // The key is still built once, before the loop, and read inside it.
    assert!(main[..lp].contains("rt_int_to_string(1i64)"), "the piece moved:\n{main}");
}
