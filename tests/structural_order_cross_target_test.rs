//! #4036: `<` / `<=` / `>` / `>=` over a tuple or a list compiled natively
//! (lexicographic, R-L0-16) but walled the wasm leg with E082
//! (`binop:cmp-…`), so ordering by a composite key had to go through
//! `list.sort_by`. The checker refuses a direct `[1] < [2]`, so the shape
//! that reaches the leg is an `Ord`-bounded generic instantiated at a tuple
//! or list. The wasm leg now orders them with the same type-directed compare
//! `list.sort` uses, and both targets print the same verdicts.

use std::process::Command;

fn almide_bin() -> String {
    env!("CARGO_BIN_EXE_almide").to_string()
}

const PROGRAM: &str = r#"fn lt[T: Ord](a: T, b: T) -> Bool = a < b
fn le[T: Ord](a: T, b: T) -> Bool = a <= b
fn gt[T: Ord](a: T, b: T) -> Bool = a > b
fn ge[T: Ord](a: T, b: T) -> Bool = a >= b

effect fn main() -> Unit = {
  println("${lt((1, "b"), (1, "c"))} ${gt((1, "b"), (1, "c"))} ${ge((2, "a"), (1, "z"))} ${le((1, "a"), (1, "a"))}")
  println("${lt([1, 2], [1, 3])} ${lt([1, 2], [1, 2, 0])} ${le([1, 2, 0], [1, 2])} ${lt([], [0])}")
  println("${gt(["b"], ["a", "z"])} ${lt([[1], [2]], [[1], [1, 5]])}")
  println("${ge((2, [1, 2]), (2, [1]))} ${lt((some(1), 3), (none, 4))}")
  println("${list.sort([(2, "a"), (1, "z"), (1, "b")])}")
  println("${list.min([(2, "a"), (1, "z")])}")
}
"#;

fn run(dir: &std::path::Path, target: Option<&str>) -> String {
    let src = dir.join("structural_order.almd");
    std::fs::write(&src, PROGRAM).expect("write program");
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(&src);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("spawn almide run");
    assert!(
        out.status.success(),
        "almide run ({target:?}) failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn tuple_and_list_ordering_compiles_on_wasm_and_agrees_with_native() {
    let dir = std::env::temp_dir().join(format!("almide-4036-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let native = run(&dir, None);
    let wasm = run(&dir, Some("wasm"));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        native,
        "true false true true\n\
         true true false true\n\
         true false\n\
         true false\n\
         [(1, \"b\"), (1, \"z\"), (2, \"a\")]\n\
         some((1, \"z\"))\n",
        "native output"
    );
    assert_eq!(wasm, native, "wasm and native disagree");
}
