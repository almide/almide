//! #1628 stage 0: `almide build --target wasm --component` packages the
//! wasm artifact as a WASI 0.2 COMPONENT (wit-component + the Cargo-pinned
//! preview1 adapter — no vendored blob). The wrap is packaging, not a
//! rewrite: the observable behavior must be identical to the plain
//! artifact. Covers both legs and the stdin family (the op-35 cursor,
//! #1625) so the component path inherits the sequential-read contract.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn build(src: &Path, out: &Path, component: bool, incumbent: bool) -> String {
    let mut args = vec![
        "build",
        src.to_str().unwrap(),
        "--target",
        "wasm",
        "-o",
        out.to_str().unwrap(),
    ];
    if component {
        args.push("--component");
    }
    let mut cmd = Command::new(almide_bin());
    cmd.args(&args);
    if incumbent {
        cmd.env("ALMIDE_WASM_INCUMBENT", "1");
    }
    let o = cmd.output().expect("spawn almide");
    let stderr = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "build failed:\n{stderr}");
    stderr
}

fn run_wasmtime(module: &Path, stdin: &str) -> String {
    let mut child = Command::new("wasmtime")
        .arg("run")
        .arg(module.to_str().unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wasmtime");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait wasmtime");
    assert!(
        out.status.success(),
        "wasmtime failed on {}:\n{}",
        module.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

const PROGRAM: &str = r#"import io

effect fn main() -> Unit = {
  let b = io.read_byte()
  let l = io.read_line()!
  let rest = io.read_all()
  println("b=${b} l=${l} rest=${rest}")
}
"#;

const STDIN: &str = "Xline-one\nrest-of-it";

#[test]
fn component_wrap_preserves_behavior_on_both_legs() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let dir = std::env::temp_dir().join("almide-component-target");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = dir.join("echo.almd");
    std::fs::write(&src, PROGRAM).expect("write");

    let core = dir.join("echo_core.wasm");
    let comp = dir.join("echo_comp.wasm");
    build(&src, &core, false, false);
    let line = build(&src, &comp, true, false);
    assert!(
        line.contains("WASI 0.2 component"),
        "the build line must name the component form:\n{line}"
    );
    // A component starts with the component layer marker; a core module
    // does not. (Bytes 4..8: version+layer; layer 1 = component.)
    let comp_bytes = std::fs::read(&comp).expect("read component");
    assert_eq!(&comp_bytes[6..8], &[1, 0], "not a component-layer artifact");

    let icomp = dir.join("echo_icomp.wasm");
    build(&src, &icomp, true, true);

    if !wasmtime_available() {
        return;
    }
    let core_out = run_wasmtime(&core, STDIN);
    assert_eq!(core_out, "b=88 l=line-one rest=rest-of-it\n");
    assert_eq!(
        run_wasmtime(&comp, STDIN),
        core_out,
        "structural component diverged from the core module"
    );
    assert_eq!(
        run_wasmtime(&icomp, STDIN),
        core_out,
        "incumbent component diverged from the core module"
    );
}
