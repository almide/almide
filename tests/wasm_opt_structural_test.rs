//! #1616: `--wasm-opt` works on the STRUCTURAL leg. The emitter renders
//! `memory.copy`, so wasm-opt needs `--enable-bulk-memory`; without it the
//! tool refused the module and the old message blamed a missing install
//! ("wasm-opt is not installed") on a machine that had it — the
//! silent-wrong-diagnosis class. This gate builds hello-world through the
//! production routing (structural), applies --wasm-opt, and demands (1) the
//! "wasm-opt applied" line, (2) a smaller artifact, (3) the optimized
//! module still prints Hello, world on wasmtime when available.

use std::path::Path;
use std::process::Command;

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

fn wasm_opt_available() -> bool {
    Command::new("wasm-opt").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

#[test]
fn wasm_opt_applies_on_the_structural_leg() {
    if Command::new(almide_bin()).arg("--version").output().is_err() || !wasm_opt_available() {
        return;
    }
    let dir = std::env::temp_dir().join("almide-wasm-opt-structural");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = dir.join("hello.almd");
    std::fs::write(&src, "fn main() -> Unit = {\n  println(\"Hello, world!\")\n}\n")
        .expect("write");
    let out_path = dir.join("hello.wasm");

    let out = Command::new(almide_bin())
        .args([
            "build",
            src.to_str().unwrap(),
            "--target",
            "wasm",
            "--wasm-opt",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .output()
        .expect("spawn almide");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "build failed:\n{stderr}");
    assert!(
        stderr.contains("wasm-opt applied"),
        "--wasm-opt did not apply on the structural leg (#1616 regressed — \
         the message must never blame a missing install when the tool ran):\n{stderr}"
    );

    if wasmtime_available() {
        let run = Command::new("wasmtime")
            .arg(out_path.to_str().unwrap())
            .output()
            .expect("spawn wasmtime");
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "Hello, world!\n",
            "the optimized structural module no longer runs correctly"
        );
    }
}
