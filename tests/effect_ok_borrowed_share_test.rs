//! `ok(x)` where an effect fn's RAW type is expected is transparent: the
//! value IS `x`, while every consumer reads the node as a fresh construction.
//! Over a BORROWED `x` (a parameter, a local still read after) the node takes
//! the share a construction would (data.rs `lower_err_raise`); without it the
//! callee's epilogue released the parameter the caller received, and the
//! wasm leg trapped (`unreachable`) once the block was reused (#4936). The node is
//! certified: the share is the carrier's share-out, so the program runs
//! without `--allow-incomplete-verification`.

use std::process::Command;

fn run(src: &std::path::Path, target: Option<&str>) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(src);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("almide");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

const PROGRAM: &str = r#"
effect fn part(s: String) -> String = {
  if string.len(s) > 0 then ok(s) else err("empty")
}

effect fn keep(xs: List[String]) -> List[String] = {
  let ys = xs
  if list.len(ys) > 0 then ok(ys) else err("none")
}

effect fn main() -> Unit = {
  let x = "ab" + int.to_string(3)
  let xs = [x, "c" + int.to_string(4)]
  var acc = ""
  for i in 0..<2000 {
    let y = part(x)!
    let zs = keep(xs)!
    acc = y + "q" + int.to_string(i) + list.join(zs, ",")
  }
  println(acc)
  println(x)
  println(list.join(xs, ","))
}
"#;

#[test]
fn a_borrowed_ok_payload_is_shared_and_certified_on_wasm() {
    let dir = std::env::temp_dir().join(format!("almide-ok-borrowed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("main.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let want = "ab3q1999ab3,c4\nab3\nab3,c4\n";
    for target in [None, Some("wasm")] {
        let (ok, stdout, stderr) = run(&src, target);
        assert!(ok, "{target:?} failed:\n{stderr}");
        assert_eq!(stdout, want, "{target:?}:\n{stderr}");
        assert!(!stderr.contains("INCOMPLETE") && !stderr.contains("UNCERTIFIED"), "{target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
