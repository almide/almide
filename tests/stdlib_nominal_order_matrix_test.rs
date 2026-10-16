//! Ordering / equality matrix over every stdlib-owned nominal type (#3755).
//!
//! `list.min([path.trusted("/"), …])` type-checked, the wasm leg ran it and
//! ordered the paths by content, and the native build died in rustc
//! (`SafePath: Ord is not satisfied`). The native leg renders an opaque
//! `mod type X = String` as a newtype that derived only `PartialEq`, and a
//! runtime-backed type (`http.HttpRequest`, `json.JsonPath`) has no
//! declaration, so the checker's orderability walk found nothing to refuse.
//!
//! The rule this pins: for each type and each order- or equality-sensitive
//! operation, either `almide check` refuses the program, or the program
//! builds and runs on native. A program that also passes
//! `almide check --target wasm` must run on wasm and print the same as native.
//! "Check passed, then the build failed" is the one outcome that is not allowed.
//!
//! The types are enumerated from their declarations, not from a hand list:
//! the `STDLIB_OWNED_TYPES` registry (the builtin `Value`, the
//! `RUNTIME_BACKED_TYPES` rows, and every bundled `type` declaration, which
//! `runtime_backed_types_matrix` pins exactly), plus every `type` a bundled
//! module declares, `mod type` newtypes included. Each type is also wrapped
//! in an entry-file `local type` newtype. That makes sure the native derive
//! a newtype gets follows its target: a newtype over an unordered type must
//! not derive `Ord`.
use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use almide_lang::ast::{Decl, Visibility};

/// `(module, type name, is an opaque newtype)` for every stdlib-owned type.
fn stdlib_types() -> BTreeMap<(String, String), bool> {
    let mut out: BTreeMap<(String, String), bool> = BTreeMap::new();
    for (m, t) in almide_lang::stdlib_info::STDLIB_OWNED_TYPES {
        out.insert((m.to_string(), t.to_string()), false);
    }
    for module in almide_lang::stdlib_info::BUNDLED_MODULES {
        let Some(source) = almide_lang::stdlib_info::bundled_source(module) else { continue };
        let Some(program) = almide_lang::parse_cached(source) else { continue };
        for decl in &program.decls {
            if let Decl::Type { name, visibility, generics, .. } = decl {
                // A generic declaration has no single element type to probe.
                if generics.as_ref().is_some_and(|g| !g.is_empty()) {
                    continue;
                }
                let opaque = matches!(visibility, Visibility::Mod | Visibility::Local);
                out.insert((module.to_string(), name.as_str().to_string()), opaque);
            }
        }
    }
    out
}

/// One operation over `xs: List[T]`, as a `Bool`-returning body.
const OPS: &[(&str, &str)] = &[
    ("min", "option.is_some(list.min(xs))"),
    ("max", "option.is_some(list.max(xs))"),
    ("sort", "list.len(list.sort(xs)) > 0"),
    ("sort_by", "list.len(list.sort_by(xs, (x) => x)) > 0"),
    ("lt", "list.any(list.zip(xs, xs), ((a, b)) => a < b)"),
    ("eq", "list.any(list.zip(xs, xs), ((a, b)) => a == b)"),
    ("set", "list.any(xs, (x) => set.contains(set.from_list(xs), x))"),
];

struct Cell {
    label: String,
    import: String,
    /// Entry-file declarations the cell needs (the wrapping newtype).
    decls: String,
    /// The element type as spelled in the probe.
    elem: String,
    body: String,
}

fn cells() -> Vec<Cell> {
    let mut cells = Vec::new();
    for (i, ((module, ty), _)) in stdlib_types().iter().enumerate() {
        let qualified = format!("{module}.{ty}");
        for (op, body) in OPS {
            cells.push(Cell {
                label: format!("{qualified} {op}"),
                import: format!("import {module}"),
                decls: String::new(),
                elem: qualified.clone(),
                body: body.to_string(),
            });
            let wrapper = format!("W{i}");
            cells.push(Cell {
                label: format!("local newtype over {qualified} {op}"),
                import: format!("import {module}"),
                decls: format!("local type {wrapper} = {qualified}"),
                elem: wrapper,
                body: body.to_string(),
            });
        }
    }
    cells
}

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

/// The program for a set of cells: one probe fn each, all called from `main`.
fn program(cells: &[&Cell]) -> String {
    let imports: BTreeSet<&str> = cells.iter().map(|c| c.import.as_str()).collect();
    let decls: BTreeSet<&str> = cells.iter().map(|c| c.decls.as_str()).filter(|d| !d.is_empty()).collect();
    let mut src = String::new();
    for i in &imports {
        src.push_str(i);
        src.push('\n');
    }
    for d in &decls {
        src.push_str(d);
        src.push('\n');
    }
    for (i, c) in cells.iter().enumerate() {
        src.push_str(&format!("fn p{i}(xs: List[{}]) -> Bool = {}\n", c.elem, c.body));
    }
    src.push_str("fn main() -> Unit = {\n");
    for i in 0..cells.len() {
        src.push_str(&format!("  println(\"p{i} ${{p{i}([])}}\")\n"));
    }
    src.push_str("}\n");
    src
}

fn write(dir: &std::path::Path, name: &str, src: &str) -> std::path::PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, src).expect("write probe");
    file
}

/// `almide check` on one cell, optionally with `--target wasm`.
fn checks(dir: &std::path::Path, idx: usize, cell: &Cell, wasm: bool) -> bool {
    let file = write(dir, &format!("c{idx}_{}.almd", if wasm { "w" } else { "n" }), &program(&[cell]));
    let mut c = almide();
    c.arg("check").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    c.output().expect("almide check").status.success()
}

fn run(file: &std::path::Path, wasm: bool) -> (bool, String, String) {
    let mut c = almide();
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn every_stdlib_nominal_type_is_refused_or_builds_on_both_legs() {
    let types = stdlib_types();
    // The enumeration must reach the shapes this bug lives in.
    assert!(types.values().any(|opaque| *opaque), "no opaque `mod type` newtype enumerated");
    assert!(
        !almide_lang::stdlib_info::RUNTIME_BACKED_TYPES.is_empty(),
        "no runtime-backed type enumerated"
    );

    let dir = tempfile::tempdir().expect("scratch");
    let cells = cells();
    // Check every cell on both targets, in parallel.
    let verdicts: Vec<(bool, bool)> = std::thread::scope(|s| {
        let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
        let chunk = cells.len().div_ceil(workers);
        let handles: Vec<_> = cells
            .chunks(chunk)
            .enumerate()
            .map(|(k, part)| {
                let dir = dir.path();
                s.spawn(move || {
                    part.iter()
                        .enumerate()
                        .map(|(j, c)| {
                            let idx = k * chunk + j;
                            let native = checks(dir, idx, c, false);
                            let wasm = native && checks(dir, idx, c, true);
                            (native, wasm)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("worker")).collect()
    });

    let accepted: Vec<&Cell> = cells.iter().zip(&verdicts).filter(|(_, v)| v.0).map(|(c, _)| c).collect();
    let wasm_ok: Vec<&Cell> = cells.iter().zip(&verdicts).filter(|(_, v)| v.1).map(|(c, _)| c).collect();
    assert!(!accepted.is_empty(), "every cell refused — the probe programs are malformed");
    assert!(!wasm_ok.is_empty(), "no cell passes `check --target wasm` — the cross-leg half compares nothing");

    // Every check-accepted cell must build and run on native. Run them all as
    // one program. On failure, bisect to the cells rustc refused, so the
    // report names them.
    let all = write(dir.path(), "native_all.almd", &program(&accepted));
    let (ok, _, _) = run(&all, false);
    if !ok {
        let failing: Vec<&str> = accepted
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                let f = write(dir.path(), &format!("solo{i}.almd"), &program(&[**c]));
                !run(&f, false).0
            })
            .map(|(_, c)| c.label.as_str())
            .collect();
        panic!(
            "{} cell(s) pass `almide check` but fail the native build \
             (refuse them at check time, or give native the order wasm uses): {failing:#?}",
            failing.len()
        );
    }

    // The wasm-accepted cells run on wasm and print what native prints.
    let w = write(dir.path(), "wasm_all.almd", &program(&wasm_ok));
    let (nok, nout, nerr) = run(&w, false);
    assert!(nok, "native run of the wasm-accepted cells failed: {nerr}");
    let (wok, wout, werr) = run(&w, true);
    assert!(wok, "wasm run of the wasm-accepted cells failed: {werr}");
    assert_eq!(nout, wout, "native and wasm disagree on the wasm-accepted cells");
}

/// The fuzz repro, and the order the two legs now share: an opaque
/// String newtype orders by its content (byte order), on both legs.
#[test]
fn opaque_string_newtypes_order_by_content_on_both_legs() {
    const SOURCE: &str = r#"import path
import html
fn main() -> Unit = {
  let p = list.min([path.trusted("/"), path.trusted("é.d/f"), path.trusted("")])
  println("[${path.to_string(p ?? path.trusted("x"))}]")
  let ps = list.sort([path.trusted("é"), path.trusted("b/a"), path.trusted("B"), path.trusted("b")])
  println("${list.map(ps, (q) => path.to_string(q))}")
  let hs = [html.raw("<p>"), html.raw("<a>"), html.raw("")]
  println("${list.map(list.sort(hs), (h) => html.to_string(h))}")
  println("${list.map(list.max(hs) |> option.to_list, (h) => html.to_string(h))}")
}
"#;
    const EXPECTED: &str = "[]\n[\"B\", \"b\", \"b/a\", \"é\"]\n[\"\", \"<a>\", \"<p>\"]\n[\"<p>\"]\n";
    let dir = tempfile::tempdir().expect("scratch");
    let file = write(dir.path(), "order.almd", SOURCE);
    for wasm in [false, true] {
        let (ok, out, err) = run(&file, wasm);
        assert!(ok, "wasm={wasm}: {err}");
        assert_eq!(out, EXPECTED, "wasm={wasm}");
    }
}
