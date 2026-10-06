//! #3283: two modules that each declare a record type with the same field set
//! (`a.Size` and `b.Extent`, both `{ w: Float, h: Float }`). The checker types
//! an anonymous record literal structurally, and the native leg named it by a
//! program-wide shape lookup, so every literal in `b` was built as `a`'s struct
//! (`almide_rt_a_Size { .. }` in a fn returning `almide_rt_b_Extent`, rustc
//! E0308) while the wasm leg, structural, ran. `spec/integration/
//! same_shape_records` holds every slot a literal fills (return, `if` arms,
//! list element, `some` / `none`, annotated `let` / `var`, same-module and
//! cross-module argument, record field, variant payload, lambda body, top-level
//! `let`); `spec/integration/modules/same_shape_records_test.almd` holds the
//! two-library shape, which the dependency-shaped leg re-homes as two path
//! dependencies. `almide test` runs one leg, so this net runs both on both
//! legs and demands one output.

use std::path::PathBuf;
use std::process::Command;

const EXPECTED: &str = "7.0 1.0 30.0 3.0
6.0 2.0 -1.0 8.0 8.0
30.0 10.0 3.0 3.0 4.5 0.0
3.5 4.0
Extent { w: 3, h: 6 }
Size { w: 1, h: 1 }";

fn integration() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("spec/integration")
}

fn almide(cwd: PathBuf, args: &[&str]) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("spawn almide");
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).to_string(),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn targets() -> Vec<&'static str> {
    if wasmtime_available() {
        vec!["rust", "wasm"]
    } else {
        eprintln!("wasmtime not on PATH — the wasm leg is skipped here (CI installs it)");
        vec!["rust"]
    }
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn a_record_literal_takes_its_slots_nominal_type_on_every_leg() {
    for target in targets() {
        let (ok, out, err) = almide(integration().join("same_shape_records"), &["run", "src/main.almd", "--target", target]);
        assert!(ok, "{target} run failed:\n{err}");
        assert_eq!(out.trim(), EXPECTED, "{target} leg");
    }
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn two_libraries_with_one_field_set_each_build_their_own_record_on_every_leg() {
    for target in targets() {
        let (ok, out, err) = almide(integration(), &["test", "modules/same_shape_records_test.almd", "--target", target]);
        assert!(ok, "{target} test failed:\n{out}\n{err}");
    }
}
