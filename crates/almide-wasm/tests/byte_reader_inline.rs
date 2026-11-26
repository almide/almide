//! The typed byte IO readers inline where their Option is taken apart on the
//! structural leg (#4944, src/inline_calls/readers.rs): `bytes.get_*(b, p)
//! ?? d` and `match bytes.get_*(b, p) { some(v) => .., none => .. }` read in
//! place, with no `some` cell.
//!
//! FAMILY RULE: every `get_*` the bytes module declares as one checked
//! position read — `if fits(b, pos, w) then some(bytes.__read_*(b, pos)) else
//! none` — inlines at both site shapes: all widths, both endiannesses, signed
//! and unsigned, bool, f16 / f32 / f64. The matrix is read from the stdlib source, so a new reader of
//! that shape joins it without an edit here.
//!
//! Each case runs on the wasm leg and on the interpreter (the definition) and
//! pins the expected text where it is short, so a bug in both cannot agree.

mod harness;
use harness::run_wasm;

/// Every `get_*` of stdlib/bytes.almd whose body is one checked read.
fn get_family() -> Vec<(String, &'static str)> {
    let src = include_str!("../../../stdlib/bytes.almd");
    let lines: Vec<&str> = src.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter_map(|(k, l)| {
            let rest = l.strip_prefix("fn get_")?;
            let name = format!("get_{}", &rest[..rest.find('(')?]);
            let body = format!("{} {}", l, lines.get(k + 1).unwrap_or(&""));
            if name.ends_with("_array") || !body.contains("then some(bytes.__read_") {
                return None;
            }
            let kind = if l.contains("-> Bool?") {
                "bool"
            } else if l.contains("-> Float?") {
                "float"
            } else {
                "int"
            };
            Some((name, kind))
        })
        .collect()
}

/// The `bytes.<fn>` sites the inliner rewrote in `src` (the module's own
/// `next_*` bodies are reader sites too, spelled without the module).
fn sites(name: &str, src: &str) -> Vec<String> {
    almide_wasm::reader_sites(&ir_of(name, src)).into_iter().filter(|k| k.starts_with("bytes.")).collect()
}

fn ir_of(name: &str, src: &str) -> almide_ir::IrProgram {
    almide_spine::s5::lower_to_ir(&format!("{name}.almd"), src).expect("lowers")
}

fn agree(name: &str, src: &str) -> String {
    let bytes = almide_wasm::emit_program(&ir_of(name, src)).expect("emits");
    let r = run_wasm(&bytes).expect("runs");
    let i = almide_spine::s5::run_file(&format!("{name}.almd"), src).expect("interp runs");
    assert_eq!(r.exit, 0, "{name}: wasm exited {}: {}", r.exit, r.stderr);
    assert_eq!(i.exit, 0, "{name}: oracle exited {}: {}", i.exit, i.stderr);
    assert_eq!(r.stdout, i.stdout, "{name}: wasm and the interpreter disagree");
    r.stdout
}

fn fallback(kind: &str) -> &'static str {
    match kind {
        "bool" => "false",
        "float" => "-7.5",
        _ => "-7",
    }
}

/// Every reader answers, at the window's edges and outside it (one past the
/// last fitting position, negative, past the end, the top of i64 where `pos
/// + w` would wrap), what the call answered — through `??` and through
/// `match` — and every one of those sites reads in place.
#[test]
fn every_reader_reads_its_window_in_place_at_every_edge() {
    let fam = get_family();
    assert!(fam.len() >= 16, "the matrix lost cells: {fam:?}");
    for (name, kind) in fam {
        let d = fallback(kind);
        let mut body = String::new();
        let positions = ["0", "1", "8", "12", "14", "15", "16", "-1", "21", "9223372036854775807"];
        for p in positions {
            body.push_str(&format!(
                "  println(\"{p}: ${{bytes.{name}(b, {p}) ?? {d}}} ${{match bytes.{name}(b, {p}) {{ some(v) => \"${{v}}\", none => \"none\" }}}}\")\n"
            ));
        }
        let src = format!(
            "effect fn main() -> Unit = {{\n  let b = bytes.from_list([1, 2, 3, 4, 128, 255, 127, 0, 63, 240, 0, 0, 192, 127, 0, 124])\n{body}}}\n"
        );
        let case = format!("rd_{name}");
        let out = agree(&case, &src);
        assert!(out.contains("-1: ") && out.contains("none\n"), "{name}: no read answered none:\n{out}");
        assert_eq!(sites(&case, &src).len(), 2 * positions.len(), "{name}: not every site read in place");
    }
}

/// The site keeps the call's order: the arguments evaluate once each, left
/// to right, before the read; the `??` fallback and the none arm only when
/// nothing fits, the some arm only when it does.
#[test]
fn a_rewritten_site_keeps_the_calls_order() {
    let src = r#"effect fn at(p: Int) -> Int = {
  println("pos ${p}")
  p
}

effect fn dflt() -> Int = {
  println("fallback")
  -1
}

effect fn main() -> Unit = {
  let b = bytes.from_list([1, 0, 2, 0, 3, 0])
  let x = bytes.get_u16_le(b, at(2)!) ?? dflt()!
  let y = bytes.get_u16_le(b, at(5)!) ?? dflt()!
  let z = match bytes.get_u16_le(b, at(4)!) {
    some(v) => {
      println("some ${v}")
      v
    },
    none => {
      println("none")
      0
    },
  }
  let w = match bytes.get_u16_le(b, at(-3)!) {
    none => {
      println("none")
      0
    },
    some(v) => {
      println("some ${v}")
      v
    },
  }
  println("${x} ${y} ${z} ${w}")
}
"#;
    let out = agree("rd_order", src);
    assert_eq!(out, "pos 2\npos 5\nfallback\npos 4\nsome 3\npos -3\nnone\n2 -1 3 0\n");
    assert_eq!(sites("rd_order", src).len(), 4);
}

/// Only a module's checked reader is rewritten, and only where its Option is
/// taken apart on the spot: a main-program fn of the same shape (the
/// CONTROL), a reader bound to a `let` and a reader whose `match` has a
/// guard keep their calls — and still answer what they did.
#[test]
fn only_a_module_reader_at_a_taking_apart_site_is_rewritten() {
    let src = r#"fn get_mine(b: Bytes, pos: Int) -> Int? = if pos >= 0 and pos < bytes.len(b) then some(pos * 2) else none

effect fn main() -> Unit = {
  let b = bytes.from_list([1, 2, 3, 4, 5])
  let kept = bytes.get_u32_le(b, 1)
  let g = match bytes.get_u32_le(b, 0) {
    some(v) if v > 100 => v,
    _ => 0,
  }
  println("${get_mine(b, 0) ?? 0} ${kept ?? 0} ${g} ${bytes.get_u32_le(b, 0) ?? 0}")
}
"#;
    let out = agree("rd_control", src);
    assert_eq!(out, "0 84148994 67305985 67305985\n");
    assert_eq!(sites("rd_control", src), vec!["bytes.get_u32_le".to_string()]);
}
