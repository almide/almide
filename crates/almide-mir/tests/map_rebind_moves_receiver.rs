//! A map write rebound to its own var (`m = map.set(m, k, v)`, `m[k] = v`)
//! moves the var into the runtime call on the native render (#4585). The
//! var is re-initialized by the rebind that follows before any read, so it
//! moves as itself, as every other dying arg does (`render_native_move::
//! take_expr`). Spelled `std::mem::take(&mut m)`, each write built an empty
//! map in its place and dropped it at the rebind: an extra construction, an
//! extra drop and two copies of the map header per write, which v0's plain
//! move does not pay.

fn render(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).expect("renders on v1"))
        .expect("spawn")
        .join()
        .expect("join")
}

fn main_of(code: &str) -> &str {
    let at = code.find("fn main() {").unwrap_or_else(|| panic!("no main in:\n{code}"));
    let main = &code[at..];
    &main[..main.find("\n}\n").map_or(main.len(), |e| e + 2)]
}

#[test]
fn a_rebound_map_write_moves_its_receiver() {
    let code = render(
        "fn main() -> Unit = {\n\
         \x20 var m: Map[Int, Int] = map.new()\n\
         \x20 for i in 0..<30 {\n\
         \x20   m = map.set(m, i % 20, i)\n\
         \x20 }\n\
         \x20 var c: Map[Int, Int] = map.new()\n\
         \x20 for i in 0..<30 {\n\
         \x20   c[i % 7] = i\n\
         \x20 }\n\
         \x20 println(int.to_string(map.len(m) + map.len(c)))\n\
         }\n",
    );
    let main = main_of(&code);
    let writes: Vec<&str> = main.lines().filter(|l| l.contains("almide_rt_map_set(")).collect();
    assert_eq!(writes.len(), 2, "two map writes expected in:\n{main}");
    for w in writes {
        assert!(!w.contains("std::mem::take"), "the receiver is taken, not moved: {w}\n{main}");
    }
}
