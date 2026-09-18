//! #2110: heap-returning exports must not lose support when main is absent.
use std::process::Command;

#[test]
fn board_library_exports_execute_on_the_structural_route() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("board.almd");
    let wasm = dir.path().join("board.wasm");
    let source = format!(
        "{}\n{}",
        include_str!("fixtures/issue2110/board.almd"),
        r#"
fn board_status() -> Int = match parse_board("{\"errors\":[{\"type\":\"RATE_LIMITED\"}]}") {
  RateLimited => 7,
  _ => 0,
}
fn board_missing() -> Int = match parse_board("{}") { BoardNotFound => 9, _ => 0 }
"#
    );
    std::fs::write(&file, source).expect("source");
    for forced in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_almide"));
        command
            .args([
                "build",
                file.to_str().expect("path"),
                "--target",
                "wasm",
                "-o",
                wasm.to_str().expect("path"),
            ])
            .env_remove("ALMIDE_WASM_STRUCTURAL")
            .env_remove("ALMIDE_WASM_INCUMBENT");
        if forced {
            command.env("ALMIDE_WASM_STRUCTURAL", "1");
        }
        let built = command.output().expect("build");
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        assert!(String::from_utf8_lossy(&built.stderr).contains("structural leg"));
    }
    let bytes = std::fs::read(&wasm).expect("wasm");
    let mut exports = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ExportSection(section) = payload.expect("parse") {
            for export in section {
                exports.push(export.expect("export").name.to_string());
            }
        }
    }
    for name in [
        "_start",
        "memory",
        "parse_board",
        "board_status",
        "board_missing",
    ] {
        assert!(
            exports.iter().any(|export| export == name),
            "missing export {name}: {exports:?}"
        );
    }
    for (name, expected) in [("board_status", "7"), ("board_missing", "9")] {
        let result = Command::new("wasmtime")
            .args(["run", "--invoke", name])
            .arg(&wasm)
            .output()
            .expect("invoke");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), expected);
    }
}

#[test]
fn library_mode_requires_every_public_body_and_program_mode_requires_main() {
    let mut ir =
        almide::wasm_leg::lower_to_ir_with_deps("library.almd", "fn broken() -> Int = 42", &[])
            .expect("lower");
    assert!(
        almide_wasm::emit_program_with_ops(&ir).is_err(),
        "program mode needs main"
    );
    let function = ir
        .functions
        .iter_mut()
        .find(|f| f.name.as_str() == "broken")
        .expect("function");
    function.body.kind = almide_ir::IrExprKind::Hole;
    let error = almide_wasm::emit_library_with_ops(&ir).expect_err("must not omit a public export");
    assert!(
        format!("{error:?}").contains("exported function `broken`"),
        "{error:?}"
    );
}
