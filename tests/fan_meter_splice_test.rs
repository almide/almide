//! #3669: a metered program (`fan.bounded` / `fan.min` / `fan.timeout`) that
//! walls the whole-program native v1 render is built by the standard codegen
//! with the v1-rendered regions linked in. Two claims:
//!
//!   1. The splice route meters exactly like the whole-program route. With
//!      `ALMIDE_FORCE_METER_SPLICE=1` a program that renders whole takes the
//!      splice route anyway, and every observable (stdout, stderr, exit code)
//!      of the unit-exact boundary fixtures must be unchanged: a spend that
//!      moved by one unit moves a printed boundary.
//!   2. A region that is itself outside the v1 subset is refused by name —
//!      building it without its meter would change which arm wins.
//!
//! The cross-target gate (`spec/wasm_cross/fan_race_walled_program.almd`) is
//! the native == wasm evidence for a program that takes the splice route on
//! its own.

use std::path::{Path, PathBuf};
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

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("spec/wasm_cross/{name}.almd"))
}

/// The announcement an inherited route switch prints — proof the forced run
/// read the switch, i.e. reached the metered route decision.
const FORCED: &str = "ALMIDE_FORCE_METER_SPLICE is set";

/// (exit code, stdout, stderr without the route announcement, announced?)
fn run_native(path: &Path, force_splice: bool) -> (Option<i32>, String, String, bool) {
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(path);
    if force_splice {
        cmd.env("ALMIDE_FORCE_METER_SPLICE", "1");
    }
    let out = cmd.output().expect("failed to spawn almide");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let announced = stderr.contains(FORCED);
    let stderr: Vec<&str> = stderr.lines().filter(|l| !l.contains(FORCED)).collect();
    (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned(), stderr.join("\n"), announced)
}

#[test]
fn the_region_splice_meters_like_the_whole_program_render() {
    let mut failures = Vec::new();
    for name in [
        "fuel_race_boundary",
        "fuel_bounded_boundary",
        "fuel_dyn_charge",
        "fuel_divergence_cut",
        "fuel_trap_window",
        "fuel_single_call_args",
    ] {
        let path = fixture(name);
        let (code, out, err, _) = run_native(&path, false);
        let (s_code, s_out, s_err, announced) = run_native(&path, true);
        if !announced {
            failures.push(format!("{name}: the forced run never reached the metered route decision"));
        }
        if (code, &out, &err) != (s_code, &s_out, &s_err) {
            failures.push(format!(
                "{name}: the splice route changed an observable\n  whole:  exit={code:?} stdout={out:?} stderr={err:?}\n  splice: exit={s_code:?} stdout={s_out:?} stderr={s_err:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// #3673: a region outside the v1 subset is no longer refused — the standard
/// codegen meters it itself — and the verbose note says which meter ran and
/// why the splice could not (fan_region_floor_test.rs holds its charges to
/// the wasm leg's).
#[test]
fn a_region_outside_the_v1_subset_is_metered_by_the_standard_codegen() {
    let dir = std::env::temp_dir().join(format!("almide-fan-meter-splice-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("split_in_arm.almd");
    std::fs::write(
        &path,
        "fn parts(s: String) -> Int = list.len(list.reverse(string.split(s, \",\")))\n\
         effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts(\"a,b\"), parts(\"c\") } ?? 0))\n",
    )
    .unwrap();
    let out = Command::new(almide_bin()).arg("run").arg(&path).env("ALMIDE_VERBOSE", "1").output().expect("spawn almide");
    let _ = std::fs::remove_dir_all(&dir);
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "stdout={stdout:?} stderr={stderr:?}");
    assert_eq!(stdout, "2\n");
    assert!(
        // `string.split` joined the declared native floor (#4006), so the
        // region walls on the string-list reverse it feeds instead.
        stderr.contains("its regions metered by the standard codegen") && stderr.contains("list.reverse"),
        "the note must name the meter that ran, and why the region splice could not: {stderr}"
    );
}

/// #3790 (ADR-0028 E): a single-call body's argument runs inside the region,
/// so an argument outside the v1 subset puts the region on the standard
/// codegen meter, and that meter charges the argument's work: 1002 units for
/// `heavy(1000)` admit the body at 3006 ns and cut it at 3003 ns, as on wasm.
#[test]
fn a_single_call_bodys_argument_is_charged_by_the_standard_codegen_meter() {
    let dir = std::env::temp_dir().join(format!("almide-fan-meter-args-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("args_in_codegen_meter.almd");
    let body = |ns: u32, n: u32| {
        format!("  println(int.to_string(fan.bounded(compute.ns({ns})) {{ pick(heavy({n}), list.len(list.reverse(string.split(\"a,b\", \",\")))) }} ?? -1))\n")
    };
    let src = format!(
        "fn heavy(n: Int) -> Int = {{\n  var i = 0\n  var acc = 0\n  while i < n {{\n    acc = (acc + i * 7) % 999983\n    i = i + 1\n  }}\n  acc\n}}\n\
         fn pick(a: Int, b: Int) -> Int = a + b\n\
         effect fn main() -> Unit = {{\n{}{}{}}}\n",
        body(30, 100000),
        body(3006, 1000),
        body(3003, 1000),
    );
    std::fs::write(&path, src).unwrap();
    let out = Command::new(almide_bin()).arg("run").arg(&path).env("ALMIDE_VERBOSE", "1").output().expect("spawn almide");
    let _ = std::fs::remove_dir_all(&dir);
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "stdout={stdout:?} stderr={stderr:?}");
    assert_eq!(stdout, "-1\n496553\n-1\n");
    assert!(
        stderr.contains("its regions metered by the standard codegen"),
        "the codegen meter must be the one that ran: {stderr}"
    );
}
