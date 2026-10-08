//! #2113: do not ship components that will refuse an unnamed host operation.
use std::process::Command;

#[test]
fn unsupported_component_args_are_named_before_writing_an_artifact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("args.almd");
    let artifact = dir.path().join("args.wasm");
    std::fs::write(&source, "import args\neffect fn main() -> Unit = println(args.option(\"x\")! ?? \"-\")\n").expect("source");
    // The p2 component refuses argv and env.set; the p3 one serves both —
    // argv over wasi:cli/environment (ADR-0023 step 3), env.set through the
    // guest-side overlay env.get reads first (#3223).
    let env_set_source = dir.path().join("env_set.almd");
    std::fs::write(&env_set_source, "import env\neffect fn main() -> Unit = env.set(\"ALMIDE_X\", \"1\")\n").expect("source");
    let component = |source: &std::path::Path, p3: bool| {
        std::fs::write(&artifact, b"existing artifact").expect("sentinel");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
        cmd.args(["build", source.to_str().expect("path"), "--target", "wasm", "--component", "-o", artifact.to_str().expect("path")])
            .env_remove("ALMIDE_COMPONENT_ADAPTER")
            .env_remove("ALMIDE_FUEL_PROBE")
            .env_remove("ALMIDE_COMPONENT_P3");
        if p3 { cmd.env("ALMIDE_COMPONENT_P3", "1"); }
        cmd.output().expect("build")
    };
    for (src, p3, op, name) in [(&source, false, 29, "args.option"), (&env_set_source, false, 37, "env.set")] {
        let out = component(src, p3);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{stderr}");
        let host_op = format!("host op {op}");
        for expected in ["E081", name, host_op.as_str(), if p3 { "WASI 0.3" } else { "WASI 0.2" }] {
            assert!(stderr.contains(expected), "missing {expected}: {stderr}");
        }
        assert_eq!(std::fs::read(&artifact).expect("artifact"), b"existing artifact");
    }
    for (src, what) in [(&source, "argv"), (&env_set_source, "env.set")] {
        let out = component(src, true);
        assert!(out.status.success(), "the p3 component serves {what}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(&std::fs::read(&artifact).expect("artifact")[..4], b"\0asm");
    }

    // Preview1 serves argv: the component-specific refusal must not leak here.
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", source.to_str().expect("path"), "--target", "wasm", "-o", artifact.to_str().expect("path")])
        .env_remove("ALMIDE_COMPONENT_P3")
        .output().expect("core build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(std::fs::read(&artifact).expect("artifact").starts_with(b"\0asm"));
}
