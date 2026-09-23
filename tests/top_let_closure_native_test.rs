//! #2537: closure-holding top-level lets on the NATIVE leg, for the shapes the
//! incumbent wasm brick refuses (a tuple cell and a record over a public fn
//! alias read through field access), so they cannot live in spec/wasm_cross
//! without raising its wall ceilings. The list-of-records and variant-payload
//! shapes are pinned cross-target by spec/wasm_cross/top_let_closure_*.almd.
//! Before the fix every one of these failed rustc with E0277 (`Rc<dyn Fn>` is
//! not `Sync`, and the value sat in a `static LazyLock`).

use std::process::Command;

const SRC: &str = r#"type Handler = (Int) -> Int

type Boxed = { h: Handler }

type Step = { name: String, run: (Int) -> Int }

let PAIR = ("p", (x: Int) => x + 3)

let BOXED = Boxed { h: (x) => x - 1 }

let ONE = Step { name: "one", run: (x) => x * 10 }

effect fn main() -> Unit = {
  let (label, f) = PAIR
  println("${label} ${int.to_string(f(4))}")
  println(int.to_string(BOXED.h(1)))
  println("${ONE.name} ${int.to_string(ONE.run(2))}")
}
"#;

#[test]
fn closure_holding_top_lets_build_and_run_natively() {
    let dir = std::env::temp_dir().join(format!("almide-2537-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.almd");
    std::fs::write(&file, SRC).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", file.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("run almide");
    let _ = std::fs::remove_dir_all(&dir);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "native run failed:\n{stderr}");
    assert_eq!(stdout, "p 7\n0\none 20\n", "stderr:\n{stderr}");
}
