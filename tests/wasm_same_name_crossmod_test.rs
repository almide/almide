//! Lock for #692: the main program and an imported module of the same package
//! each define a fn with the SAME bare name (different arity/result) — the
//! module-internal call (and specifically its TAIL call) must bind the
//! module-local function, not the main program's.
//!
//! Before the fix, `emit_tail_call`'s Named arm looked up func_map by BARE name
//! only, so an intra-module tail call `route(x, 100)` inside `m.route_line`
//! bound the bridge's 0-arg f64 `route` and emitted an invalid `return_call`
//! ("current function requires result type [i64] but callee returns [f64]"),
//! failing structural validation while `almide check` stayed green. The fix
//! shares one resolver (current-module qualified → bare → any-module
//! qualified) between emit_call and emit_tail_call.
//!
//! Builds the two-file package and invokes the export with `wasmtime --invoke`;
//! skips cleanly when `almide` or `wasmtime` is unavailable.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let release = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if release.exists() {
        return release.to_str().unwrap().to_string();
    }
    let debug = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/almide");
    if debug.exists() {
        return debug.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tools_available() -> bool {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return false;
    }
    Command::new("wasmtime").arg("--version").output().is_ok()
}

const MOD_ALMD: &str = r#"fn route(a: Int, b: Int) -> Int = a + b
fn route_line(x: Int) -> Int = route(x, 100)
"#;

const BRIDGE_ALMD: &str = r#"import self as m
import float
import int
@export(wasm, "route")
fn route() -> Float = int.to_float(m.route_line(5))
effect fn main() -> Unit = {
  let _ = route()
  ()
}
"#;

#[test]
fn same_bare_fn_name_across_main_and_module_resolves_locally() {
    if !tools_available() {
        eprintln!("skipping: almide or wasmtime unavailable");
        return;
    }
    let dir = std::env::temp_dir().join(format!("almide-692-gate-{}", std::process::id()));
    let src = dir.join("src");
    std::fs::create_dir_all(&src).expect("mk temp dirs");
    std::fs::write(
        dir.join("almide.toml"),
        "[package]\nname = \"i692\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(src.join("mod.almd"), MOD_ALMD).unwrap();
    std::fs::write(src.join("bridge.almd"), BRIDGE_ALMD).unwrap();
    let wasm = dir.join("bridge.wasm");

    let build = Command::new(almide_bin())
        .current_dir(&dir)
        .args([
            "build",
            "src/bridge.almd",
            "--target",
            "wasm",
            "-o",
            wasm.to_str().unwrap(),
        ])
        .output()
        .expect("spawn almide build");
    assert!(
        build.status.success(),
        "wasm build failed on a check-green same-bare-name package (#692 class):\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    // route() = to_float(m.route_line(5)) = to_float(m.route(5, 100)) = 105 —
    // the intra-module tail call must bind m.route (2-arg), not the bridge's.
    let run = Command::new("wasmtime")
        .args(["--invoke", "route", wasm.to_str().unwrap()])
        .output()
        .expect("spawn wasmtime");
    assert!(
        run.status.success(),
        "invoking export trapped:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let out = String::from_utf8_lossy(&run.stdout);
    assert!(
        out.trim().ends_with("105"),
        "expected 105 from the module-local route(5, 100), got: {out}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
