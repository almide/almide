//! #1836: a module type's repr prints its DECLARED name on every leg. The
//! issue's program — `type Cfg = { a: Int, b: Int }` in module `m`, the entry
//! interpolating `m.mk()` — printed `almide_rt_m_Cfg { a: 1, b: 2 }` on native
//! (the post-flatten Rust struct name) and `m.Cfg { a: 1, b: 2 }` on wasm and
//! the interp (the pre-flatten qualified decl name): an I-divergence in which
//! no leg printed the source spelling. `spec/integration/modules/
//! cross_module_repr_test.almd` pins the native and wasm `almide test` lanes;
//! this net runs the issue's program on all FOUR legs — native, the structural
//! wasm emitter, the incumbent (`ALMIDE_WASM_INCUMBENT=1`) and the interp —
//! and demands the one spelling, `Cfg { a: 1, b: 2 }`. A second program adds
//! the shapes the incumbent still walls on (a derived `Repr`, a list of
//! module records): native, structural wasm and the interp must agree there
//! too, including `m.Pt.repr(p) == "${p}"` (C-009: one format).

use std::path::{Path, PathBuf};
use std::process::Command;

const ISSUE_MODULE: &str = "type Cfg = { a: Int, b: Int }\nfn mk() -> Cfg = Cfg { a: 1, b: 2 }\n";
const ISSUE_ENTRY: &str = "import m\neffect fn main() -> Unit = println(\"${m.mk()}\")\n";
const ISSUE_EXPECTED: &str = "Cfg { a: 1, b: 2 }";

const SHAPES_MODULE: &str = "type Cfg = { a: Int, b: Int }
type Pt: Repr = { x: Int, y: Int }
type Shape = | Circle(Int) | Rect { w: Int, h: Int } | Dot
type Outer = { name: String, cfg: Cfg }
fn mk() -> Cfg = Cfg { a: 1, b: 2 }
fn mk_pt() -> Pt = Pt { x: 1, y: 2 }
fn circle(r: Int) -> Shape = Circle(r)
fn rect(w: Int, h: Int) -> Shape = Rect { w: w, h: h }
fn dot() -> Shape = Dot
fn outer() -> Outer = Outer { name: \"x\", cfg: mk() }
";
const SHAPES_ENTRY: &str = "import m
effect fn main() -> Unit = {
  println(\"${m.mk()}\")
  println(m.Pt.repr(m.mk_pt()))
  println(\"${m.mk_pt()}\")
  println(\"${m.circle(3)}\")
  println(\"${m.rect(3, 4)}\")
  println(\"${m.dot()}\")
  println(\"${m.outer()}\")
  println(\"${[m.mk(), m.mk()]}\")
}
";
const SHAPES_EXPECTED: &str = "Cfg { a: 1, b: 2 }
Pt { x: 1, y: 2 }
Pt { x: 1, y: 2 }
Circle(3)
Rect { w: 3, h: 4 }
Dot
Outer { name: \"x\", cfg: Cfg { a: 1, b: 2 } }
[Cfg { a: 1, b: 2 }, Cfg { a: 1, b: 2 }]";

/// The interp's cut of the shapes program: the record, the derived `Repr`
/// (`m.Pt.repr(p) == "${p}"`), the nested record and the record list. The
/// variant lines are left out because the interp's ctor registry scans the
/// ROOT decls only — a module-internal `Circle(r)` is an honest
/// `Unsupported` there (a scope limit, not a spelling), and a skip is not a
/// vote.
const INTERP_SHAPES_ENTRY: &str = "import m
effect fn main() -> Unit = {
  println(\"${m.mk()}\")
  println(m.Pt.repr(m.mk_pt()))
  println(\"${m.mk_pt()}\")
  println(\"${m.outer()}\")
  println(\"${[m.mk(), m.mk()]}\")
}
";
const INTERP_SHAPES_EXPECTED: &str = "Cfg { a: 1, b: 2 }
Pt { x: 1, y: 2 }
Pt { x: 1, y: 2 }
Outer { name: \"x\", cfg: Cfg { a: 1, b: 2 } }
[Cfg { a: 1, b: 2 }, Cfg { a: 1, b: 2 }]";

fn almide_bin() -> String {
    env!("CARGO_BIN_EXE_almide").to_string()
}

/// A fresh project dir holding `m/mod.almd` + `main.almd`; the tag keeps the
/// tests in one process from sharing a directory.
fn project(tag: &str, module: &str, entry: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-module-type-repr-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("m")).expect("mkdir");
    std::fs::write(root.join("m/mod.almd"), module).expect("write module");
    std::fs::write(root.join("main.almd"), entry).expect("write entry");
    root
}

fn run(entry: &Path, target: &str, incumbent: bool) -> (bool, String, String) {
    let mut cmd = Command::new(almide_bin());
    cmd.args(["run", entry.to_str().unwrap(), "--target", target]);
    if incumbent {
        cmd.env("ALMIDE_WASM_INCUMBENT", "1");
    }
    let o = cmd.output().expect("spawn almide run");
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).to_string(),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}

fn run_interp(entry: &Path) -> almide_interp::RunOutcome {
    let source = std::fs::read_to_string(entry).expect("read entry");
    let ir = almide::wasm_leg::lower_to_ir(entry.to_str().unwrap(), &source).expect("front failed");
    almide_interp::Interpreter::new(&ir).run_main()
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

#[test]
fn interp_prints_the_declared_name() {
    let root = project("interp", ISSUE_MODULE, ISSUE_ENTRY);
    let out = run_interp(&root.join("main.almd"));
    assert_eq!(out.status, almide_interp::RunStatus::Ok, "interp leg: {}", out.stderr);
    assert_eq!(out.stdout.trim(), ISSUE_EXPECTED, "interp leg");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn interp_derived_repr_and_record_list_print_the_declared_name() {
    let root = project("interp-shapes", SHAPES_MODULE, INTERP_SHAPES_ENTRY);
    let out = run_interp(&root.join("main.almd"));
    assert_eq!(out.status, almide_interp::RunStatus::Ok, "interp leg: {}", out.stderr);
    assert_eq!(out.stdout.trim(), INTERP_SHAPES_EXPECTED, "interp leg");
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn every_compiled_leg_prints_the_declared_name() {
    let root = project("compiled", ISSUE_MODULE, ISSUE_ENTRY);
    let entry = root.join("main.almd");
    let (ok, out, err) = run(&entry, "rust", false);
    assert!(ok, "native run failed:\n{err}");
    assert_eq!(out.trim(), ISSUE_EXPECTED, "native leg");
    if !wasmtime_available() {
        eprintln!("wasmtime not on PATH — the wasm legs are skipped here (CI installs it)");
        return;
    }
    for incumbent in [false, true] {
        let (ok, out, err) = run(&entry, "wasm", incumbent);
        assert!(ok, "wasm run failed (incumbent={incumbent}):\n{err}");
        assert_eq!(out.trim(), ISSUE_EXPECTED, "wasm leg (incumbent={incumbent})");
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// The incumbent is excluded on purpose: it walls on `Pt.repr` and the
/// `__repr_list_rec_*` helper (a pre-existing renderer gap, not a spelling),
/// and a wall is an honest refusal rather than a vote.
#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn derived_repr_and_record_list_agree_on_native_and_structural_wasm() {
    let root = project("compiled-shapes", SHAPES_MODULE, SHAPES_ENTRY);
    let entry = root.join("main.almd");
    let (ok, out, err) = run(&entry, "rust", false);
    assert!(ok, "native run failed:\n{err}");
    assert_eq!(out.trim(), SHAPES_EXPECTED, "native leg");
    if !wasmtime_available() {
        eprintln!("wasmtime not on PATH — the wasm leg is skipped here (CI installs it)");
        return;
    }
    let (ok, out, err) = run(&entry, "wasm", false);
    assert!(ok, "structural wasm run failed:\n{err}");
    assert_eq!(out.trim(), SHAPES_EXPECTED, "structural wasm leg");
    let _ = std::fs::remove_dir_all(&root);
}
