//! The fan v2 SURFACE MATRIX gate (head × form — docs/roadmap/active/fan-v2.md).
//!
//! Every cell of the 6-head × 2-form matrix is machine-pinned here: an
//! implemented cell must TYPE-CHECK, and an intentionally-absent cell must
//! produce its EXACT matrix answer (never a generic parse error — the dojo
//! MSR rounds showed a misrouted hint costs a model its whole retry budget).
//! The API-completeness rule: a surface with a hand-maintained shape drifts;
//! a gated matrix cannot. Cell states:
//!
//!   head     | block form        | mapper form
//!   ---------+-------------------+---------------------------------------
//!   (all)    | compiles          | compiles (fan.map)
//!   settle   | compiles          | compiles
//!   any      | compiles          | compiles
//!   race     | compiles          | DECLARED-UNIMPLEMENTED (Wave 2 waits
//!            |                   | for a real use — fan-v2 反証条件)
//!   bounded  | compiles (body)   | DESIGN-NONE (compose with fan.map)
//!   timeout  | compiles (body)   | DESIGN-NONE (single deadlined body)
//!
//! Changing any cell (implementing the race mapper, adding a bounded mapper)
//! MUST update this table in the same PR — that is the point of the gate.

use std::io::Write;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// Run `almide check` on `src`; return (ok, combined output).
fn check(name: &str, src: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join("almd-fan-matrix");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.almd"));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(src.as_bytes()).unwrap();
    drop(f);
    let out = Command::new(almide()).arg("check").arg(&path).output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let has_error = text.contains("error:") || text.contains("error[");
    (out.status.success() && !has_error, text)
}

const HELPERS: &str = r#"
effect fn ea() -> Int = 1
effect fn eb() -> Int = 2
fn pure_work(n: Int) -> Int = n * 2
"#;

#[test]
fn fan_surface_matrix() {
    // ── Implemented cells: every one must type-check. ──
    let implemented: &[(&str, String)] = &[
        ("all_block", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let (x, y) = fan {{ ea(); eb() }}\n  println(int.to_string(x + y))\n}}\n")),
        ("all_mapper", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let ys = fan.map([1, 2, 3], (x) => ok(pure_work(x)))\n  println(int.to_string(list.len(ys)))\n}}\n")),
        ("settle_block", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let (ra, rb) = fan.settle {{ ea(); eb() }}\n  println(int.to_string((ra ?? -1) + (rb ?? -1)))\n}}\n")),
        ("settle_mapper", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let rs = fan.settle([1, 2], (x) => ok(pure_work(x)))\n  println(int.to_string(list.len(rs)))\n}}\n")),
        ("any_block", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.any {{ ea(); eb() }}\n  println(int.to_string(v))\n}}\n")),
        ("any_mapper", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.any([1, 2], (x) => ok(pure_work(x)))\n  println(int.to_string(v))\n}}\n")),
        ("race_block", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.race(compute.ms(5)) {{ pure_work(1); pure_work(2) }} ?? -1\n  println(int.to_string(v))\n}}\n")),
        ("bounded_body", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.bounded(compute.ms(50)) {{ pure_work(21) }} ?? -1\n  println(int.to_string(v))\n}}\n")),
        ("timeout_body", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.timeout(duration.ms(50)) {{ pure_work(21) }} ?? -1\n  println(int.to_string(v))\n}}\n")),
    ];
    for (name, src) in implemented {
        let (ok, text) = check(name, src);
        assert!(ok, "matrix cell `{name}` must type-check but failed:\n{text}");
    }

    // ── Intentionally-absent cells: the EXACT matrix answer, no drift. ──
    let absent: &[(&str, String, &str)] = &[
        (
            "race_mapper",
            format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.race([1, 2], (x) => ok(pure_work(x)))\n  println(int.to_string(v ?? -1))\n}}\n"),
            "the fan.race mapper form is declared but not implemented",
        ),
        (
            "race_mapper_budget",
            format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.race(compute.ms(5), [1, 2], (x) => ok(pure_work(x)))\n  println(int.to_string(v ?? -1))\n}}\n"),
            "the fan.race mapper form is declared but not implemented",
        ),
        (
            "bounded_mapper",
            format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.bounded(compute.ms(5), (x) => pure_work(x))\n  println(int.to_string(v ?? -1))\n}}\n"),
            "fan.bounded has no mapper form",
        ),
        (
            "timeout_mapper",
            format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.timeout(duration.ms(5), (x) => pure_work(x))\n  println(int.to_string(v ?? -1))\n}}\n"),
            "fan.timeout has no mapper form",
        ),
    ];
    for (name, src, needle) in absent {
        let (ok, text) = check(name, src);
        assert!(!ok, "matrix cell `{name}` is intentionally absent but type-checked");
        assert!(
            text.contains(needle),
            "matrix cell `{name}` must answer with its cell status (`{needle}`), got:\n{text}"
        );
    }
}
