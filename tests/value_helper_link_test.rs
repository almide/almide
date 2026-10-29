//! The wasm link loads the self-host helper a natively lowered `value.*`
//! surface calls, from the surface alone.
//!
//! The emitter lowers value.object / merge / pick / omit itself and calls a
//! helper of stdlib/value_utils.almd by name (`__vu_merge_pairs`, ...) that no
//! IR names. Once the link scanned only the fns a program reaches (#4317), a
//! program whose only `value.*` call was `value.merge` never loaded
//! value_utils and walled `value:unlinked:__vu_merge_pairs` on every wasm leg.
//! Each program here calls ONE surface, so nothing else can load the helper by
//! accident; wasm must run it and print what native prints.
use std::process::Command;

fn run(src: &str, target: Option<&str>) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("v.almd");
    std::fs::write(&file, src).expect("source");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(&file);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("run");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{target:?}: {stderr}");
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn a_lone_value_surface_links_its_helper_on_wasm() {
    for (call, want) in [
        ("value.merge(value.int(1), value.bool(true))", "true\n"),
        ("value.pick(value.int(1), [\"x\"])", "{}\n"),
        ("value.omit(value.int(1), [\"x\"])", "1\n"),
    ] {
        let src = format!("fn main() -> Unit = {{\n  let v = {call}\n  println(\"${{v}}\")\n}}\n");
        let native = run(&src, None);
        assert_eq!(native, want, "{call}: native");
        assert_eq!(run(&src, Some("wasm")), native, "{call}: native and wasm differ");
    }
}

#[test]
fn object_merge_pick_omit_agree_on_both_legs() {
    let src = r#"fn main() -> Unit = {
  let a = value.object([("x", value.int(1)), ("y", value.int(2))])
  let b = value.object([("y", value.int(3)), ("z", value.int(4))])
  println("${value.merge(a, b)} ${value.pick(a, ["y"])} ${value.omit(b, ["y"])}")
}
"#;
    let native = run(src, None);
    assert_eq!(native, "{\"x\":1,\"y\":3,\"z\":4} {\"y\":2} {\"z\":4}\n");
    assert_eq!(run(src, Some("wasm")), native);
}
