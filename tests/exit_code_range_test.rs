//! C-350: `process.exit(code)` passes the exit status 0..=255 through, any
//! other code is a defined abort, and a WASI preview-1 build walls 126..=255.
//!
//! The `spec/wasm_cross` fixtures carry the promise across the stock WASI
//! runtime. This test carries the values a fixture cannot: one program per
//! code, over both boundaries, on every leg —
//!
//! | code | native, embedded host | preview-1 build on a stock runtime |
//! |---|---|---|
//! | 0..=125 | the code | the code |
//! | 126..=255 | the code (#2780) | the wall line, exit 1 |
//! | 256, -1 | the domain line, exit 1 | the domain line, exit 1 |
//!
//! Before C-350, 256 answered 0 natively and -1 answered 255: POSIX keeps the
//! low 8 bits, so a failure read as success. 0.63 then narrowed every target
//! to 0..=125, which took 126..=255 away from native wrappers (#2780).
//!
//! The code is read through an `effect fn` so the folder cannot see it: a
//! constant takes a different path (#1117), and then the test would pass with
//! the rule absent.
use std::process::Command;

const DOMAIN: &str = "Error: exit code must be in 0..=255";
const WALL: &str = "Error: a WASI preview-1 build cannot exit with a code in 126..=255";

fn almide() -> String {
    std::env::var("ALMIDE_BIN")
        .unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn program(code: i64) -> String {
    format!(
        "import process\n\n\
         effect fn chosen() -> Int = {code}\n\n\
         effect fn main() -> Unit = {{\n  \
           println(\"before-exit\")\n  \
           let c = chosen()!\n  \
           process.exit(c)\n\
         }}\n"
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Leg {
    Native,
    /// `almide run --target wasm`: the structural module on the embedded host.
    Embedded,
    /// `almide build --target wasm`, run on the stock `wasmtime` CLI.
    StockP1,
}

fn has_wasmtime() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok()
}

/// (exit code, stdout, stderr) on one leg.
fn run(dir: &std::path::Path, src: &str, leg: Leg) -> (i32, String, String) {
    let file = dir.join("main.almd");
    std::fs::write(&file, src).unwrap();
    let out = match leg {
        Leg::Native => Command::new(almide()).args(["run", file.to_str().unwrap()]).output(),
        Leg::Embedded => Command::new(almide())
            .args(["run", file.to_str().unwrap(), "--target", "wasm"])
            .output(),
        Leg::StockP1 => {
            let wasm = dir.join("main.wasm");
            let built = Command::new(almide())
                .args(["build", file.to_str().unwrap(), "--target", "wasm", "-o"])
                .arg(&wasm)
                .output()
                .unwrap();
            assert!(built.status.success(), "build failed: {}", String::from_utf8_lossy(&built.stderr));
            Command::new("wasmtime").arg(&wasm).output()
        }
    }
    .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        stderr.trim().to_string(),
    )
}

fn expect(code: i64, legs: &[Leg], want_exit: i32, want_err: &str) {
    let dir = tempfile::tempdir().unwrap();
    let src = program(code);
    for &leg in legs {
        if leg != Leg::Native && !has_wasmtime() {
            eprintln!("skip {leg:?}: wasmtime unavailable");
            continue;
        }
        assert_eq!(
            run(dir.path(), &src, leg),
            (want_exit, "before-exit".to_string(), want_err.to_string()),
            "process.exit({code}) on {leg:?}"
        );
    }
}

const EVERY_LEG: &[Leg] = &[Leg::Native, Leg::Embedded, Leg::StockP1];

#[test]
fn a_code_every_build_delivers_is_the_exit_code_on_every_leg() {
    for code in [0, 1, 3, 124, 125] {
        expect(code, EVERY_LEG, code as i32, "");
    }
}

/// The issue's codes: 126 (not executable), 127 (not found), 130 and 137
/// (killed by SIGINT and SIGKILL), and 255, the top of the status.
#[test]
fn an_exit_status_above_125_passes_through_natively_and_on_the_embedded_host() {
    for code in [126, 127, 128, 130, 137, 200, 255] {
        expect(code, &[Leg::Native, Leg::Embedded], code as i32, "");
    }
}

#[test]
fn a_preview1_build_walls_126_to_255_with_its_own_line() {
    for code in [126, 127, 130, 255] {
        expect(code, &[Leg::StockP1], 1, WALL);
    }
}

/// 256 and -1 are the two POSIX answers wrongly (0 and 255) instead of
/// refusing; 1000 is a code no leg could carry.
#[test]
fn a_code_outside_the_exit_status_aborts_identically_on_every_leg() {
    for code in [256, 1000, -1] {
        expect(code, EVERY_LEG, 1, DOMAIN);
    }
}

/// The abort is reached through the ordinary termination convention: stdout
/// written before the call still arrives, and nothing after it runs.
#[test]
fn the_abort_keeps_the_output_written_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let src = "import process\n\n\
               effect fn chosen() -> Int = 300\n\n\
               effect fn main() -> Unit = {\n  \
                 println(\"one\")\n  \
                 println(\"two\")\n  \
                 let c = chosen()!\n  \
                 process.exit(c)\n  \
               }\n";
    let (exit, out, err) = run(dir.path(), src, Leg::Native);
    assert_eq!((exit, out.as_str(), err.as_str()), (1, "one\ntwo", DOMAIN));
}

/// The two places that spell the preview-1 wall — the IR rewrite the incumbent
/// renderer takes and the `to_wasi` exit shim — print the same line, and the
/// constants above are the ones the compiler uses.
#[test]
fn the_wall_and_domain_lines_are_spelled_once() {
    assert_eq!(almide_ir::exit_code::OUT_OF_RANGE_MSG, DOMAIN);
    assert_eq!(almide_ir::exit_code::PREVIEW1_WALL_MSG, WALL);
    assert_eq!(almide_wasi::EXIT_WALL_MSG, format!("{WALL}\n").as_bytes());
    assert_eq!(
        i64::from(almide_wasi::EXIT_WALL_FROM),
        almide_ir::exit_code::PREVIEW1_MAX_EXIT_CODE + 1
    );
}
