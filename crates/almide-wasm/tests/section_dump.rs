//! Section-dump goldens (#1586, the zig CheckObject lesson): encoding
//! regressions never surface in behavior tests — zig shipped a ULEB bug
//! that turned `i32.const -64` into a different constant, and only a
//! structural dump caught it. Two micro-fixtures pin the full decoded
//! structure (types, imports, tables, globals, exports, elements, data,
//! and EVERY operator with its immediates) against goldens:
//!
//!   leb_consts — i64 constants straddling the 1/2/3-byte (S)LEB128
//!                boundaries, both signs;
//!   fn_values  — the +1-biased funcref table, element segment, and
//!                call_indirect type indices;
//!   exports    — the export section of an entry `pub fn` (#457) and an
//!                `@export(wasm, "sym")` declared export (#2752).
//!
//! Any byte-level encoding change decodes differently and drifts the
//! dump. Ratify deliberately:
//!   ALMIDE_UPDATE_DUMPS=1 cargo test --release -p almide-wasm --test section_dump

use std::fmt::Write as _;
use std::path::PathBuf;

const LEB_CONSTS: &str = r#"fn main() -> Unit = {
  var s = 0
  var i = 0
  while i < 1 {
    s = s + 63 + 64 + 127 + 128 - 64 - 65 - 8192 - 8193
    s = s + 2147483647 - 2147483648 + 4611686018427387904 - 4611686018427387905
    i = i + 1
  }
  if s == 4611686018427370877 then println("leb ok") else println("leb drift")
}
"#;

const FN_VALUES: &str = r#"fn add1(x: Int) -> Int = x + 1
fn add2(x: Int) -> Int = x + 2

fn main() -> Unit = {
  var pick = 1
  let f = if pick == 1 then add1 else add2
  println(int.to_string(f(41)))
}
"#;

const EXPORTS: &str = r#"pub fn double(x: Int) -> Int = x * 2

@export(wasm, "render")
fn draw(t: Int) -> Int = t + 1

fn helper(x: Int) -> Int = x + 3

fn main() -> Unit = {
  println(int.to_string(helper(1)))
}
"#;

fn dump(bytes: &[u8]) -> String {
    use wasmparser::{Parser, Payload};
    let mut out = String::new();
    let w = &mut out;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("valid module") {
            Payload::TypeSection(r) => {
                for (i, t) in r.into_iter_err_on_gc_types().enumerate() {
                    let _ = writeln!(w, "type {i}: {:?}", t.expect("type"));
                }
            }
            Payload::ImportSection(r) => {
                for group in r {
                    for item in group.expect("imports") {
                        let (_, imp) = item.expect("import");
                        let _ = writeln!(w, "import {}.{} {:?}", imp.module, imp.name, imp.ty);
                    }
                }
            }
            Payload::FunctionSection(r) => {
                for (i, ti) in r.into_iter().enumerate() {
                    let _ = writeln!(w, "func {i} -> type {}", ti.expect("func type index"));
                }
            }
            Payload::TableSection(r) => {
                for t in r {
                    let _ = writeln!(w, "table {:?}", t.expect("table").ty);
                }
            }
            Payload::MemorySection(r) => {
                for m in r {
                    let _ = writeln!(w, "memory {:?}", m.expect("memory"));
                }
            }
            Payload::GlobalSection(r) => {
                for g in r {
                    let g = g.expect("global");
                    let init: Vec<String> = g
                        .init_expr
                        .get_operators_reader()
                        .into_iter()
                        .map(|o| format!("{:?}", o.expect("init op")))
                        .collect();
                    let _ = writeln!(w, "global {:?} = {}", g.ty, init.join(" "));
                }
            }
            Payload::ExportSection(r) => {
                for e in r {
                    let e = e.expect("export");
                    let _ = writeln!(w, "export {} {:?} {}", e.name, e.kind, e.index);
                }
            }
            Payload::ElementSection(r) => {
                for e in r {
                    let e = e.expect("element");
                    if let wasmparser::ElementItems::Functions(fs) = e.items {
                        let idx: Vec<String> =
                            fs.into_iter().map(|f| f.expect("func idx").to_string()).collect();
                        let _ = writeln!(w, "element funcs [{}]", idx.join(","));
                    }
                }
            }
            Payload::DataSection(r) => {
                for d in r {
                    let d = d.expect("data");
                    let head: Vec<String> =
                        d.data.iter().take(24).map(|b| format!("{b:02x}")).collect();
                    let _ = writeln!(w, "data len={} head={}", d.data.len(), head.join(""));
                }
            }
            Payload::CodeSectionEntry(body) => {
                let locals: Vec<String> = body
                    .get_locals_reader()
                    .expect("locals")
                    .into_iter()
                    .map(|l| {
                        let (n, t) = l.expect("local");
                        format!("{n}x{t:?}")
                    })
                    .collect();
                let _ = writeln!(w, "code locals=[{}]", locals.join(","));
                for op in body.get_operators_reader().expect("ops") {
                    let _ = writeln!(w, "  {:?}", op.expect("op"));
                }
            }
            _ => {}
        }
    }
    out
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/dumps")
}

fn check(name: &str, src: &str) {
    let ir = almide_spine::s5::lower_to_ir(&format!("{name}.almd"), src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let got = dump(&bytes);
    let path = golden_dir().join(format!("{name}.txt"));
    if std::env::var("ALMIDE_UPDATE_DUMPS").is_ok() {
        std::fs::create_dir_all(golden_dir()).expect("dumps dir");
        std::fs::write(&path, &got).expect("write golden");
    }
    let want = std::fs::read_to_string(&path)
        .expect("dump golden — generate with ALMIDE_UPDATE_DUMPS=1");
    if got != want {
        let first = want
            .lines()
            .zip(got.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| format!("line {}: golden `{a}` vs actual `{b}`", i + 1))
            .unwrap_or_else(|| "length differs".into());
        panic!(
            "{name}: section dump drifted — an ENCODING change decodes differently \
             (ratify with ALMIDE_UPDATE_DUMPS=1 after reviewing the diff). First divergence: {first}"
        );
    }
}

#[test]
fn leb_boundary_constants_encode_stably() {
    check("leb_consts", LEB_CONSTS);
}

#[test]
fn fn_value_table_encodes_stably() {
    check("fn_values", FN_VALUES);
}

#[test]
fn export_section_encodes_stably() {
    check("exports", EXPORTS);
}

/// The emitter's refusal for `src`, which must not produce a module.
fn refusal(name: &str, src: &str) -> String {
    let ir = almide_spine::s5::lower_to_ir(&format!("{name}.almd"), src).expect("front");
    match almide_wasm::emit_program(&ir) {
        Ok(_) => panic!("{name}: the module must be refused, it emitted"),
        Err(e) => format!("{e:?}"),
    }
}

#[test]
fn two_exports_of_one_name_are_refused() {
    let err = refusal(
        "dup_export",
        "@export(wasm, \"f\")\nfn a(x: Int) -> Int = x\n\n@export(wasm, \"f\")\nfn b(x: Int) -> Int = x + 1\n\nfn main() -> Unit = println(int.to_string(a(1) + b(1)))\n",
    );
    assert!(err.contains("duplicate wasm export name `f` (fns `a` and `b`)"), "{err}");
}

#[test]
fn an_export_claiming_a_module_owned_name_is_refused() {
    let err = refusal(
        "reserved_export",
        "@export(wasm, \"memory\")\nfn m(x: Int) -> Int = x\n\nfn main() -> Unit = println(int.to_string(m(1)))\n",
    );
    assert!(err.contains("duplicate wasm export name `memory` (fn `m` and the module's own export)"), "{err}");
}

/// A fn whose body does not lower: the typed hole the leg refuses on purpose.
const GROW: &str = "fn grow(n: Int) -> Int = if n > 100 then todo(\"big\") else n + 1\n";

fn export_names(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::ExportSection(r) = payload.expect("valid module") {
            names.extend(r.into_iter().map(|e| e.expect("export").name.to_string()));
        }
    }
    names
}

#[test]
fn an_unlowered_fn_main_reaches_refuses_the_module_at_its_site() {
    let err = refusal("reached_hole", &format!("{GROW}\nfn main() -> Unit = println(int.to_string(grow(1)))\n"));
    assert!(err.contains("Unsupported"), "{err}");
    let site = almide_wasm::decline_site::last().expect("the wall names its site");
    assert_eq!(site.function, "grow", "{site:?}");
}

#[test]
fn an_unlowered_fn_main_does_not_reach_is_not_exported() {
    // #457: an entry fn exports only when its whole call closure lowers;
    // one that does not is left out, never shipped as a trapping stub.
    let ir = almide_spine::s5::lower_to_ir("unreached_hole.almd", &format!("{GROW}\nfn main() -> Unit = println(\"main\")\n"))
        .expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("main never reaches the hole");
    let names = export_names(&bytes);
    assert!(!names.iter().any(|n| n == "grow"), "{names:?}");
}

#[test]
fn a_declared_export_that_does_not_lower_is_refused() {
    let err = refusal("declared_hole", &format!("@export(wasm, \"grow\")\n{GROW}\nfn main() -> Unit = println(\"main\")\n"));
    assert!(err.contains("exported function `grow` cannot be lowered"), "{err}");
    let site = almide_wasm::decline_site::last().expect("the wall names its site");
    assert_eq!(site.function, "grow", "{site:?}");
}

#[test]
fn a_library_export_that_does_not_lower_is_refused() {
    // Library ABI: every public fn must export, so the hole refuses the
    // module where a program would only leave the fn out.
    let ir = almide_spine::s5::lower_to_ir("library_hole.almd", GROW).expect("front");
    let err = format!("{:?}", almide_wasm::emit_library_with_ops(&ir).expect_err("must refuse"));
    assert!(err.contains("exported function `grow` cannot be lowered"), "{err}");
}

#[test]
fn a_program_without_main_is_refused() {
    let err = refusal("no_main", "fn f(x: Int) -> Int = x + 1\n");
    assert!(err.contains("no main function"), "{err}");
}
