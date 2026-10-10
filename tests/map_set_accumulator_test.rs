//! #2393: accumulating into a Map by functional update — `m = map.set(m, k, v)`
//! in a `for` or `while`, and the `list.fold` callback form — must move the
//! receiver into `almide_rt_map_set`, never clone it. v0.66.0 emitted
//! `almide_rt_map_set(big.clone(), …)`, so every step copied the whole map:
//! 32,000 string-keyed entries took 7.88 s of user time on native (macOS
//! arm64) against 0.00 s on v0.67.0. The emitted call is the pin; the run
//! checks the result stays right.
use std::process::Command;

const SOURCE: &str = r#"fn by_fold(xs: List[Int]) -> Map[String, Int] =
  xs |> list.fold(map.new(), (m, n) => map.set(m, "k${int.to_string(n)}", n))

effect fn main() -> Unit = {
  var big: Map[String, Int] = [:]
  for n in list.range(0, 2000) {
    big = map.set(big, "k${int.to_string(n)}", n)
  }
  var w: Map[Int, Int] = map.new()
  var i = 0
  while i < 2000 {
    w = map.set(w, i, i * 2)
    i = i + 1
  }
  let f = by_fold(list.range(0, 2000))
  println("${map.len(big)} ${map.len(w)} ${map.len(f)} ${map.get(w, 1999) ?? -1} ${map.get(f, "k1999") ?? -1}")
}
"#;

#[test]
fn a_map_accumulated_by_functional_update_moves_into_map_set() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("acc.almd");
    std::fs::write(&source, SOURCE).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg(&source)
        .args(["--target", "rust"])
        .output()
        .expect("emit");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8(out.stdout).expect("utf-8");
    for (receiver, form) in [("big", "for"), ("w", "while"), ("m", "fold callback")] {
        assert!(
            rust.contains(&format!("almide_rt_map_set({receiver},")),
            "the {form} form no longer moves `{receiver}` into map.set"
        );
        assert!(
            !rust.contains(&format!("almide_rt_map_set({receiver}.clone()")),
            "the {form} form clones `{receiver}` on every step (#2393)"
        );
    }

    let bin = dir.path().join("acc");
    let build = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("build");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let run = Command::new(&bin).output().expect("run");
    assert!(run.status.success());
    assert_eq!(String::from_utf8(run.stdout).expect("utf-8"), "2000 2000 2000 3998 1999\n");
}
