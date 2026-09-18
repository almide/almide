//! #2113: do not ship components that will refuse an unnamed host operation.
use std::process::Command;

#[test]
fn unsupported_component_args_are_named_before_writing_an_artifact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("args.almd");
    let artifact = dir.path().join("args.wasm");
    std::fs::write(&source, "import args\neffect fn main() -> Unit = println(args.option(\"x\") ?? \"-\")\n").expect("source");
    for p3 in [false, true] {
        std::fs::write(&artifact, b"existing artifact").expect("sentinel");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
        cmd.args(["build", source.to_str().expect("path"), "--target", "wasm", "--component", "-o", artifact.to_str().expect("path")])
            .env_remove("ALMIDE_COMPONENT_ADAPTER")
            .env_remove("ALMIDE_WASM_INCUMBENT")
            .env_remove("ALMIDE_FUEL_PROBE")
            .env_remove("ALMIDE_COMPONENT_P3");
        if p3 { cmd.env("ALMIDE_COMPONENT_P3", "1"); }
        let out = cmd.output().expect("build");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{stderr}");
        for expected in ["E081", "args.option", "host op 29", if p3 { "WASI 0.3" } else { "WASI 0.2" }] {
            assert!(stderr.contains(expected), "missing {expected}: {stderr}");
        }
        assert_eq!(std::fs::read(&artifact).expect("artifact"), b"existing artifact");
    }

    // Preview1 serves argv: the component-specific refusal must not leak here.
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", source.to_str().expect("path"), "--target", "wasm", "-o", artifact.to_str().expect("path")])
        .env_remove("ALMIDE_COMPONENT_P3")
        .output().expect("core build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(std::fs::read(&artifact).expect("artifact").starts_with(b"\0asm"));
}
