//! A position read of a `fan { }` block (`(fan { a(), b() }).0`) reads one
//! element of the tuple the block builds and consumes nothing. The wasm leg
//! named a call result or a tuple literal read this way so the frame released
//! it, but not a fan block: the tuple and its other elements stayed live at
//! exit — with plain arms, with fallible arms, and inside an inlined
//! `fan.map` callback, on the success path and on the first-err path alike.
//! Native drops them; the two legs print the same.
use std::process::Command;

const PLAIN: &str = r#"fn tag(n: Int) -> String = "v" + int.to_string(n)

effect fn main() -> Unit = {
  let s = "pre/" + (fan {
    tag(1)
    tag(2)
  }).0
  println(s)
}
"#;

const FALLIBLE: &str = r#"effect fn ok_s(n: Int) -> Result[String, String] = ok("v" + int.to_string(n))

effect fn one(x: Int) -> Result[String, String] = {
  let s = (fan {
    ok_s(x)!
    ok_s(x + 10)!
  }).0
  ok(s)
}

effect fn main() -> Unit = {
  let r: Result[String, String] = one(1)
  println("${r}")
}
"#;

const IN_CALLBACK: &str = r#"effect fn ok_s(n: Int) -> Result[String, String] = ok("v" + int.to_string(n))
effect fn fail_s(m: String) -> Result[String, String] = err(m + "!")

effect fn run(xs: List[Int]) -> Result[List[String], String] = {
  let pre = "pre" + int.to_string(list.len(xs))
  let out = fan.map(xs, (x) => ok(pre + "/" + (fan {
      ok_s(x)!
      if x == 2 then fail_s("bad")! else ok_s(x + 10)!
    }).0))!
  ok(out)
}

effect fn main() -> Unit = {
  let good: Result[List[String], String] = run([1, 3])
  println("${good}")
  let bad: Result[List[String], String] = run([1, 2, 3])
  println("${bad}")
}
"#;

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

#[test]
fn a_position_read_of_a_fan_block_leaves_nothing_live_on_wasm() {
    let cases = [
        ("plain", PLAIN, "pre/v1\n"),
        ("fallible", FALLIBLE, "ok(\"v1\")\n"),
        ("in_callback", IN_CALLBACK, "ok([\"pre2/v1\", \"pre2/v3\"])\nerr(\"bad!\")\n"),
    ];
    for (name, source, expected) in cases {
        let dir = tempfile::tempdir().expect("scratch");
        let file = dir.path().join(format!("{name}.almd"));
        std::fs::write(&file, source).expect("source");

        let native = almide().arg("run").arg(&file).output().expect("native run");
        assert!(native.status.success(), "[{name}] {}", String::from_utf8_lossy(&native.stderr));
        assert_eq!(String::from_utf8_lossy(&native.stdout), expected, "[{name}] native");

        let wasm = almide()
            .arg("run")
            .arg(&file)
            .args(["--target", "wasm"])
            .env("ALMIDE_WASM_ALLOC_COUNT", "1")
            .output()
            .expect("wasm run");
        assert!(wasm.status.success(), "[{name}] {}", String::from_utf8_lossy(&wasm.stderr));
        assert_eq!(String::from_utf8_lossy(&wasm.stdout), expected, "[{name}] wasm");
        let counts = String::from_utf8_lossy(&wasm.stderr);
        let live: u64 = counts
            .split_whitespace()
            .find_map(|w| w.strip_prefix("live=")?.parse().ok())
            .unwrap_or_else(|| panic!("[{name}] no live= in {counts}"));
        assert_eq!(live, 0, "[{name}] blocks live at exit: {counts}");
    }
}
