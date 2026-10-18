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
//!   any      | REMOVED (E027)    | REMOVED (E027) — a `??` chain /
//!            |                   | list.find_map, dialect epoch 14 (ADR-0028)
//!   min      | compiles (clock)  | compiles (clock, xs, f — (spend, index)
//!            |                   | lex-min over a dynamic list); a clockless
//!            |                   | spelling is the targeted parse error
//!   bounded  | compiles (body)   | DESIGN-NONE (compose with fan.map)
//!   timeout  | compiles (body)   | DESIGN-NONE (single deadlined body)
//!
//! Changing any cell (adding a bounded mapper, retiring a form)
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
effect fn ra() -> Result[Int, String] = ok(1)
effect fn rb() -> Result[Int, String] = err("no")
fn pure_work(n: Int) -> Int = n * 2
"#;

#[test]
fn fan_surface_matrix() {
    // ── Implemented cells: every one must type-check. ──
    let implemented: &[(&str, String)] = &[
        ("all_block", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let (x, y) = fan {{ ea(), eb() }}\n  println(int.to_string(x + y))\n}}\n")),
        ("all_mapper", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let ys = fan.map([1, 2, 3], (x) => ok(pure_work(x)))!\n  println(int.to_string(list.len(ys)))\n}}\n")),
        ("settle_block", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let (ra, rb) = fan.settle {{ ea(), eb() }}\n  println(int.to_string((ra ?? -1) + (rb ?? -1)))\n}}\n")),
        ("settle_mapper", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let rs = fan.settle([1, 2], (x) => ok(pure_work(x)))\n  println(int.to_string(list.len(rs)))\n}}\n")),
        ("chain_block", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = rb() ?? ra() ?? -1\n  println(int.to_string(v))\n}}\n")),
        ("find_map_mapper", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = list.find_map([1, 2], (x) => if x > 1 then some(pure_work(x)) else none) ?? -1\n  println(int.to_string(v))\n}}\n")),
        ("race_block", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.min(compute.ms(5)) {{ pure_work(1), pure_work(2) }} ?? -1\n  println(int.to_string(v))\n}}\n")),
        ("race_mapper_budget", format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.min(compute.ms(5), [1, 2], (x) => ok(pure_work(x))) ?? -1\n  println(int.to_string(v))\n}}\n")),
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
            "any_block",
            format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.any {{ ra(), rb() }} ?? -1\n  println(int.to_string(v))\n}}\n"),
            "fan.any was removed",
        ),
        (
            "any_mapper",
            format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.any([1, 2], (x) => ra()) ?? -1\n  println(int.to_string(v))\n}}\n"),
            "fan.any was removed",
        ),
        (
            "min_block_noclock",
            format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.min {{ pure_work(1), pure_work(2) }} ?? -1\n  println(int.to_string(v))\n}}\n"),
            "fan.min requires a clock argument",
        ),
        (
            "min_mapper_noclock",
            format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.min([1, 2], (x) => ok(pure_work(x))) ?? -1\n  println(int.to_string(v))\n}}\n"),
            "fan.min requires a clock argument",
        ),
        (
            "race_mapper_nonresult",
            // The mapper-form contract: the mapper must return a Result.
            format!("{HELPERS}\neffect fn main() -> Unit = {{\n  let v = fan.min(compute.ms(100), [1, 2], (x) => pure_work(x)) ?? -1\n  println(int.to_string(v))\n}}\n"),
            "fan.min mapper",
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
