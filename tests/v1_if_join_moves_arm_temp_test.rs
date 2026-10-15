//! The v1 native render moves an if-as-value arm's own temporary into the
//! join instead of copying it a second time.
//!
//! A heap-valued `if` whose arms read live lists rendered as
//!
//! ```text
//! if c { let mut v14 = v0.clone(); v13 = v14.clone(); } else { … }
//! ```
//!
//! The arm's `Dup` (`v14`) is the arm's own value and dies with the arm, so
//! the join's second `.clone()` copied the whole list for nothing: two full
//! copies per selection (#3532's argument form). A value that outlives the
//! arm (defined before it) is still copied into the join.
use std::process::Command;

const SRC: &str = r#"effect fn main() -> Unit = {
  let x: List[Int] = list.range(0, 2000)
  let x2: List[Int] = list.range(0, 3000)
  var acc = 0
  var i = 0
  while i < 4 {
    acc = acc + list.len(if i % 2 == 0 then x else x2)
    i = i + 1
  }
  println("${acc + list.len(x) + list.len(x2)}")
}
"#;

fn main_body(rust: &str) -> &str {
    let at = rust.find("\nfn main()").expect("the render has a main");
    let rest = &rust[at..];
    &rest[..rest.find("\n}\n").map(|i| i + 3).unwrap_or(rest.len())]
}

#[test]
fn an_arm_temporary_moves_into_the_join() {
    let rust = almide_mir::pipeline::try_render_rust_source(SRC).expect("the program renders on v1");
    let main = main_body(&rust);
    // one copy per arm (the arm's own Dup of the live list), none at the join
    assert_eq!(main.matches(".clone()").count(), 2, "one copy per arm, none at the join:\n{main}");
}

#[test]
fn the_program_prints_the_same_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("join.almd");
    std::fs::write(&file, SRC).expect("source");
    for target in [None, Some("wasm")] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let out = c.output().expect("run");
        assert!(out.status.success(), "{target:?}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "15000\n", "{target:?}");
    }
}
