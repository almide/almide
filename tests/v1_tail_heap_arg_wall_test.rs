//! #3550: the v1 native render has a loop conversion only for the shapes
//! `try_tco_rewrite` admits. A self call it leaves as a call, in tail
//! position, with a heap argument, is not a jump: the argument is released
//! after the call (`CallFn`, then `Drop`). Such a function recursed frame by
//! frame and overflowed at a depth wasm runs in constant stack. It now walls
//! onto the standard codegen, which loop-converts it.
use std::process::Command;

const LIST_ARG: &str = "fn go(x: List[Int], i: Int) -> List[Int] = if i <= 0 then x else go([i, 1], i - 1)\n\nfn main() -> Unit = println(int.to_string(list.len(go([1, 2, 3], 1000000))))\n";

const VARIANT_ARG: &str = "type T = | A(Int) | B\n\nfn go(x: T, i: Int) -> T = if i <= 0 then x else go(match x { A(n) => A(n + 1), B => B }, i - 1)\n\nfn main() -> Unit = match go(A(0), 1000000) {\n  A(n) => println(int.to_string(n)),\n  B => println(\"b\"),\n}\n";

/// A heap accumulator the v1 loop conversion DOES take (#3031's corpus
/// program): it must keep rendering on v1, not wall.
const STRING_ACC: &str = "fn go(n: Int, acc: String) -> String =\n  if n == 0 then acc\n  else if n % 2 == 0 then go(n - 1, acc + \"y\")\n  else go(n - 1, acc + \"x\")\n\nfn main() -> Unit = println(int.to_string(string.len(go(1000, \"\"))))\n";

fn run_native(name: &str, src: &str) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("almide run");
    assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_heap_arg_tail_call_the_v1_loop_declines_walls_and_runs_in_constant_stack() {
    for (name, src, expected) in [("list_arg", LIST_ARG, "2\n"), ("variant_arg", VARIANT_ARG, "1000000\n")] {
        let wall = almide_mir::pipeline::try_render_rust_source(src).expect_err(name);
        assert!(format!("{wall:?}").contains("#3550"), "{name}: walled for another reason: {wall:?}");
        assert_eq!(run_native(name, src), expected, "{name}");
    }
}

#[test]
fn a_heap_accumulator_the_v1_loop_takes_still_renders_on_v1() {
    almide_mir::pipeline::try_render_rust_source(STRING_ACC).expect("the v1 TCO loop takes this shape");
}
