//! A generic stdlib fn passed by name (`list.map(rows, list.len)`) checks
//! clean, so it must also render on the walker route (`--target rust`, what
//! `almide test` and every program the v1 render walls on use). The eta
//! expansion kept the fn's own type parameter `A`, and ConcretizeTypes then
//! refused `__eta_0: List[A]` as a compiler bug (#4745). The expansion is now
//! typed from the call site, as the written lambda is.
use std::process::Command;

const SOURCES: &[(&str, &str)] = &[
    (
        "issue",
        "fn widest(rows: List[List[String]]) -> Int = rows |> list.map(list.len) |> list.max ?? 0\n\nfn main() -> Unit = println(int.to_string(widest([[\"a\", \"b\"], [\"c\"]])))\n",
    ),
    (
        "reverse",
        "fn main() -> Unit = println(int.to_string(list.len(list.map([[1, 2], [3]], list.reverse))))\n",
    ),
    (
        "is_some",
        "fn main() -> Unit = println(int.to_string(list.len(list.filter(list.map([some(1), none], option.is_some), (b) => b))))\n",
    ),
    (
        "two_params",
        "fn main() -> Unit = println(int.to_string(list.len(list.map([[(1, \"a\")]], list.unzip))))\n",
    ),
];

#[test]
fn a_generic_stdlib_fn_by_name_renders_on_the_walker_route() {
    let dir = tempfile::tempdir().expect("scratch");
    for (name, src) in SOURCES {
        let file = dir.path().join(format!("{name}.almd"));
        std::fs::write(&file, src).expect("source");
        let check = Command::new(env!("CARGO_BIN_EXE_almide")).arg("check").arg(&file).output().expect("check");
        assert!(check.status.success(), "{name}: check: {}", String::from_utf8_lossy(&check.stderr));
        let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&file).args(["--target", "rust"]).output().expect("emit rust");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success() && !stderr.contains("COMPILER BUG"), "{name}: --target rust refused a program that checks clean:\n{stderr}");
    }
}

/// #4747: the by-name form printed `[4, 4]` on wasm for `List[List[Int]]`
/// (the expansion's unresolved `A` read 8-byte elements as 4-byte slots).
/// Every route answers what the written lambda answers.
#[test]
fn a_generic_stdlib_fn_by_name_answers_the_lambda_on_every_route() {
    let by_name = "fn run(xs: List[List[Int]]) -> List[Int] = list.map(xs, list.len)\n\nfn main() -> Unit = {\n  println(\"${run([[3, 1], [1, 2]])}\")\n  println(\"${list.map([map.from_list([(1, 2), (3, 4)])], map.len)}\")\n  println(\"${option.map(some([1, 2, 3]), list.len)}\")\n  println(\"${list.max_by([[1], [1, 2]], list.len)}\")\n}\n";
    let want = "[2, 2]\n[2]\nsome(3)\nsome([1, 2])\n";
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("by_name.almd");
    std::fs::write(&file, by_name).expect("source");
    for wasm in [false, true] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if wasm {
            c.args(["--target", "wasm"]);
        }
        let out = c.output().expect("run");
        assert_eq!(
            (String::from_utf8_lossy(&out.stdout).as_ref(), out.status.code()),
            (want, Some(0)),
            "wasm={wasm}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
