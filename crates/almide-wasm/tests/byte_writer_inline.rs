//! The typed byte IO writers inline at their call sites on the structural leg
//! (#4550, src/inline_calls.rs `writer_body` / `Rewriter::splice_writers`,
//! and the set arm's skipped room test, src/bytes_rw.rs `proven_room`).
//!
//! FAMILY RULE: every `put_*` the bytes module declares is one checked
//! in-place write — `if fits(b, pos, w) then bytes.__set_*(b, pos, x) else
//! put_out_of_range(..)` — so EVERY one inlines at a write-back site whose
//! buffer is a bare variable: all widths (1, 2, 4, 8), both endiannesses,
//! signed and unsigned, bool, f16 / f32 / f64. The matrix below is read from
//! the stdlib source, so a new `put_*` joins it without an edit here. The
//! `get_*` readers answer an Option and stay calls (outside the inliner's
//! scalar subset); they are exercised here as the read half of each cell.
//!
//! Each case runs on the wasm leg and on the interpreter (the definition) and
//! pins the expected text where it is short, so a bug in both cannot agree.

mod harness;
use harness::run_wasm;

const LEN: i64 = 16;

/// (name, width, kind) of every `put_*` in stdlib/bytes.almd.
fn put_family() -> Vec<(String, i64, &'static str)> {
    let src = include_str!("../../../stdlib/bytes.almd");
    src.lines()
        .filter_map(|l| l.strip_prefix("fn put_"))
        .map(|rest| {
            let name = format!("put_{}", &rest[..rest.find('(').expect("a param list")]);
            let w = match name.as_str() {
                n if n.contains("64") => 8,
                n if n.contains("32") => 4,
                n if n.contains("16") => 2,
                _ => 1,
            };
            let kind = match name.as_str() {
                "put_bool" => "bool",
                n if n.starts_with("put_f") => "float",
                _ => "int",
            };
            (name, w, kind)
        })
        .collect()
}

fn ir_of(name: &str, src: &str) -> almide_ir::IrProgram {
    almide_spine::s5::lower_to_ir(&format!("{name}.almd"), src).expect("lowers")
}

/// (exit, stdout, stderr) on the wasm leg and on the interpreter.
fn both(name: &str, src: &str) -> ((i32, String, String), (i32, String, String)) {
    let bytes = almide_wasm::emit_program(&ir_of(name, src)).expect("emits");
    let r = run_wasm(&bytes).expect("runs");
    let i = almide_spine::s5::run_file(&format!("{name}.almd"), src).expect("interp runs");
    ((r.exit, r.stdout, r.stderr), (i.exit, i.stdout, i.stderr))
}

fn agree(name: &str, src: &str) -> String {
    let ((exit, out, err), (iexit, iout, ierr)) = both(name, src);
    assert_eq!(exit, 0, "{name}: wasm exited {exit}: {err}");
    assert_eq!(iexit, 0, "{name}: oracle exited {iexit}: {ierr}");
    assert_eq!(out, iout, "{name}: wasm and the interpreter disagree");
    out
}

/// The values a cell writes: ints that wrap at every width and sign, both
/// bools, and the floats whose bits are the edge (-0.0, NaN payloads of both
/// signs, the infinities, an f32 denormal, f16's largest finite).
fn values(kind: &str) -> Vec<&'static str> {
    match kind {
        "bool" => vec!["true", "false"],
        "float" => vec![
            "1.5",
            "-0.0",
            "float.from_bits(0x7FF0000000000001)",
            "-float.from_bits(0x7FF8000000000123)",
            "1.0 / 0.0",
            "-1.0 / 0.0",
            "1.0e-45",
            "65504.0",
        ],
        _ => vec!["-1", "0x0102030405060708", "128", "-32769", "255"],
    }
}

/// The reader of a writer's cell, printing its value or `none`.
fn reader(name: &str) -> String {
    let get = name.replacen("put_", "get_", 1);
    format!("fn rd(b: Bytes, p: Int) -> String = match bytes.{get}(b, p) {{\n  some(v) => \"${{v}}\",\n  none => \"none\",\n}}\n")
}

/// Every in-range write lands where the call would have put it — at the
/// first and the last position that fits — and every read at the boundary
/// offsets (0, len - w, len - w + 1, -1, past the end) answers what it did.
#[test]
fn every_writer_writes_its_window_at_both_edges() {
    for (name, w, kind) in put_family() {
        let last = LEN - w;
        let mut body = String::new();
        for v in values(kind) {
            body.push_str(&format!(
                "  bytes.{name}(b, 0, {v})\n  bytes.{name}(b, {last}, {v})\n  println(\"${{bytes.to_list(b)}}\")\n"
            ));
        }
        for p in [0, last, last + 1, -1, LEN + 5] {
            body.push_str(&format!("  println(\"{p}: ${{rd(b, {p})}}\")\n"));
        }
        let src = format!("{}\neffect fn main() -> Unit = {{\n  var b = bytes.zeros({LEN})\n{body}}}\n", reader(&name));
        let out = agree(&format!("wr_{name}"), &src);
        assert!(out.contains(&format!("{}: none", last + 1)), "{name}: the read past the window answered:\n{out}");
        assert_eq!(almide_wasm::writer_sites(&ir_of(&format!("wr_{name}"), &src)).len(), 2 * values(kind).len(), "{name}");
    }
}

/// A write whose window leaves the buffer aborts with the call's message and
/// exit code: one past the last fitting position, negative, past the end, and
/// at the top of i64 (where `pos + w` would wrap).
#[test]
fn every_writer_aborts_outside_its_window() {
    for (name, w, kind) in put_family() {
        let v = values(kind)[0];
        for p in [LEN - w + 1, -1, LEN + 3, i64::MAX] {
            let src = format!(
                "effect fn main() -> Unit = {{\n  var b = bytes.zeros({LEN})\n  bytes.{name}(b, {p}, {v})\n  println(\"unreachable\")\n}}\n"
            );
            let case = format!("wr_abort_{name}");
            let ((exit, out, err), (iexit, _, ierr)) = both(&case, &src);
            let msg = format!("bytes.{name}: {w}-byte write at {p} is out of range for length {LEN}");
            assert_ne!(exit, 0, "{case} at {p}: wasm did not abort");
            assert_eq!(exit, iexit, "{case} at {p}: exit codes differ ({err} / {ierr})");
            assert!(out.is_empty(), "{case} at {p}: wasm ran past the abort: {out}");
            assert!(err.contains(&msg), "{case} at {p}: wasm said {err:?}, not {msg:?}");
            assert!(ierr.contains(&msg), "{case} at {p}: the oracle said {ierr:?}");
            assert_eq!(almide_wasm::writer_sites(&ir_of(&case, &src)), vec![format!("bytes.{name}")], "{case}");
        }
    }
}

/// The family rule, as one program: one site per `put_*` inlines, and
/// nothing else does — not the readers, not a main-program fn of the same
/// shape (the CONTROL: only a module's declaration is a checked writer), not
/// an append.
#[test]
fn the_whole_family_and_nothing_else_inlines() {
    let fam = put_family();
    assert!(fam.len() >= 19, "the matrix lost cells: {fam:?}");
    let mut body = String::new();
    for (name, _, kind) in &fam {
        body.push_str(&format!("  bytes.{name}(b, 8, {})\n", values(kind)[0]));
    }
    let src = format!(
        r#"fn put_mine(mut b: Bytes, pos: Int, x: Int) -> Unit = if pos >= 0 then bytes.put_u8(b, pos, x) else ()

effect fn main() -> Unit = {{
  var b = bytes.zeros({LEN})
{body}  put_mine(b, 1, 7)
  bytes.append_u8(b, 9)
  println("${{bytes.to_list(b)}} ${{bytes.get_u8(b, 1) ?? -1}}")
}}
"#
    );
    agree("wr_family", &src);
    let mut sites = almide_wasm::writer_sites(&ir_of("wr_family", &src));
    sites.sort();
    // put_mine's own `bytes.put_u8` is a site; its call is not (a main-
    // program fn is never a checked writer).
    let mut want: Vec<String> = fam.iter().map(|(n, _, _)| format!("bytes.{n}")).chain(["bytes.put_u8".to_string()]).collect();
    want.sort();
    assert_eq!(sites, want);
}

/// The buffer keeps value semantics through the inlined write: an alias
/// taken before it keeps the old bytes (the set's COW gate still runs), a
/// module-level `var` and a `mut` param are written in place and seen by
/// the caller.
#[test]
fn inlined_writes_keep_value_semantics() {
    let src = r#"var g = bytes.zeros(4)

fn fill(mut b: Bytes, x: Int) -> Unit = {
  bytes.put_u16_le(b, 0, x)
  bytes.put_u16_be(b, 2, x)
}

effect fn main() -> Unit = {
  var b = bytes.zeros(4)
  let snap = b
  bytes.put_u32_le(b, 0, 0x01020304)
  fill(b, 0x0506)
  bytes.put_i8(g, 3, -2)
  let gs = g
  bytes.put_u8(g, 0, 1)
  println("${bytes.to_list(snap)} ${bytes.to_list(b)} ${bytes.to_list(gs)} ${bytes.to_list(g)}")
}
"#;
    let out = agree("wr_alias", src);
    assert_eq!(out, "[0, 0, 0, 0] [6, 5, 5, 6] [0, 0, 0, 254] [1, 0, 0, 254]\n");
    assert_eq!(almide_wasm::writer_sites(&ir_of("wr_alias", src)).len(), 5);
}

/// The room test the set skips is only the one its guard already asked: a
/// guard for a NARROWER window over the same variables keeps the set's own
/// test (the CONTROL for `proven_room`), so a 4-byte write with room for 2
/// stays a no-op, as C-229 says.
#[test]
fn a_narrower_guard_keeps_the_sets_own_room_test() {
    let src = r#"fn w4(mut b: Bytes, p: Int) -> Unit = if p >= 0 and p <= bytes.len(b) - 2 then bytes.__set_u32_le(b, p, 0x01020304) else ()

effect fn main() -> Unit = {
  var b = bytes.zeros(6)
  w4(b, 3)
  w4(b, 2)
  println("${bytes.to_list(b)}")
}
"#;
    assert_eq!(agree("wr_narrow", src), "[0, 0, 4, 3, 2, 1]\n");
}
