//! A heap `match` bound in a block-bodied callback inside a generic fn (#4598).
//! The branch lift outlined it into a plain helper whose parameter kept the
//! enclosing fn's type parameter (`oo: O`), which monomorphization never
//! reaches: native failed with an unresolved `TypeVar(O)` and wasm walled.
//! A branch that names a type parameter now stays in place.

use std::process::Command;

const SRC: &str = r#"protocol Order {
  fn cmp(o: Self, a: String, b: String) -> Int
}

type Lexical: Order = {  }

fn Lexical.cmp(o: Lexical, a: String, b: String) -> Int = if a < b then -1 else if a > b then 1 else 0

fn greatest[O: Order](o: O, names: List[String]) -> String? = {
  let r = names |> list.fold(
    (o, none),
    ((oo, best), n) => {
      let next = match best {
        none => some(n),
        some(h) => if oo.cmp(n, h) > 0 then some(n) else some(h),
      }
      (oo, next)
    },
  )
  r.1
}

fn pair[A](x: A, xs: List[Int]) -> (A, List[Int]) = list.fold(xs, (x, []), (acc, n) => {
  let ys = if n > 1 then acc.1 + [n] else acc.1
  (acc.0, ys)
})

fn main() -> Unit = {
  println("${greatest(Lexical {}, ["pear", "fig", "zoo", "apple"])}")
  println("${greatest(Lexical {}, [])}")
  println("${pair("k", [1, 2, 3]).1}")
}
"#;

#[test]
fn a_heap_branch_over_a_type_parameter_runs_on_native_and_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("g.almd");
    std::fs::write(&file, SRC).unwrap();
    for target in [None, Some("wasm")] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
        cmd.arg("run").arg(&file).current_dir(dir.path());
        if let Some(t) = target {
            cmd.args(["--target", t]);
        }
        let out = cmd.output().expect("almide");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{target:?}:\n{stdout}\n{stderr}");
        assert_eq!(stdout, "some(\"zoo\")\nnone\n[2, 3]\n", "{target:?}:\n{stderr}");
    }
}
