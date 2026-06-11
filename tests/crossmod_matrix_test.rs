//! Cross-module shape matrix gate — completeness-by-construction §2.
//!
//! #484, #486, and the record-literal canonicalization bug all lived for many
//! releases because the spec corpus barely exercised cross-module shapes: a
//! shape the corpus never builds is a shape where a codegen bug lives
//! forever. This gate GENERATES multi-file projects from a cell table
//! (definition site × reference shape × type class × binding position),
//! builds every cell on the Rust target — **any rustc failure on generated
//! code is a compiler bug and fails CI** — runs it, and cross-checks the wasm
//! target byte-for-byte when wasmtime is available.
//!
//! Adding a cell is adding a row. A cell that is red at introduction is
//! declared `KnownBroken` (with the tracking reason); the gate then asserts
//! it KEEPS failing — fixing it without removing the flag fails the suite,
//! so the ledger only shrinks (the @xt-allow ratchet pattern).

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") { return bin; }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() { return cargo_bin.to_str().unwrap().to_string(); }
    "almide".to_string()
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// The shared definition-site module: one of each definable entity class.
const MOD_ALMD: &str = r#"type Emotion = Happy | Sad

type Cfg = { name: String }

type Pigment: Codec = { r: Int, g: Int, b: Int }

let SYSTEM = "hi"

let WORDS = ["a", "b"]

let CFG = Cfg { name: "c" }

let CFGS = [
  Cfg { name: "a" },
  Cfg { name: "b" }
]

let MOOD = Happy

let MAYBE = some(Cfg { name: "opt" })

let N = 7

fn mk() -> Cfg = Cfg { name: "via-fn" }

effect fn estep(n: Int) -> Int = n + 1

var count = 0

var title = "init"

var nums = [1, 2]

let PAIR = ("a", 1)

fn bump() -> Unit = {
  count = count + 1
}

fn retitle(s: String) -> Unit = {
  title = s
}

fn add_num(n: Int) -> Unit = {
  nums = nums + [n]
}
"#;

enum Status {
    Works,
    /// Red at introduction — the gate asserts it KEEPS failing until fixed
    /// (then the flag must be removed). The string names why / the tracker.
    KnownBroken(&'static str),
}

struct Cell {
    name: &'static str,
    main: &'static str,
    expected: &'static str,
    status: Status,
}

fn cells() -> Vec<Cell> {
    vec![
        Cell {
            name: "tuple_variant_payload_type",
            main: r#"import self as m
type Tag = | PolicyTag(m.Emotion) | Empty
fn describe(t: Tag) -> String = match t { PolicyTag(_) => "tag", Empty => "empty" }
effect fn main() -> Unit = println(describe(PolicyTag(m.Happy)))
"#,
            expected: "tag",
            status: Status::Works,
        },
        // #609: a derived Codec method on a type defined in ANOTHER module
        // (`m.Pigment.encode/decode`) ICE'd the WASM emitter (it built the
        // runtime name from the TYPE name, not the owning module) while native
        // worked — and `almide test` hid it behind the native fallback. This
        // cell is the both-target regression lock; it covers the scalar method
        // AND the synthesized `__decode_list_<mod>.<Type>` helper.
        Cell {
            name: "cross_module_codec_roundtrip",
            main: r#"import self as m
type Bundle: Codec = { lead: m.Pigment, palette: List[m.Pigment] }
effect fn main() -> Unit = {
  let p = m.Pigment { r: 1, g: 2, b: 3 }
  let one = match m.Pigment.decode(m.Pigment.encode(p)) { ok(v) => v.b, _ => -1 }
  let bd = Bundle { lead: p, palette: [m.Pigment { r: 4, g: 5, b: 6 }, m.Pigment { r: 7, g: 8, b: 9 }] }
  let lst = match Bundle.decode(Bundle.encode(bd)) { ok(v) => list.len(v.palette), _ => -1 }
  println(int.to_string(one) + " " + int.to_string(lst))
}
"#,
            expected: "3 2",
            status: Status::Works,
        },
        Cell {
            name: "record_variant_payload_field",
            main: r#"import self as m
type Tag = | SetP { p: m.Emotion } | Empty
fn describe(t: Tag) -> String = match t { SetP { .. } => "set", Empty => "empty" }
effect fn main() -> Unit = {
  let t = SetP { p: m.Sad }
  println(describe(t))
}
"#,
            expected: "set",
            status: Status::Works,
        },
        Cell {
            name: "generic_variant_payload",
            main: r#"import self as m
type Tag = | Wrapped(List[m.Emotion]) | Empty
fn describe(t: Tag) -> String = match t {
  Wrapped(es) => "wrap:" + int.to_string(list.len(es)),
  Empty => "empty",
}
effect fn main() -> Unit = println(describe(Wrapped([m.Happy, m.Sad])))
"#,
            expected: "wrap:2",
            status: Status::Works,
        },
        Cell {
            name: "string_toplet_by_value",
            main: r#"import self as m
fn shout(s: String) -> String = s + "!"
effect fn main() -> Unit = println(shout(m.SYSTEM))
"#,
            expected: "hi!",
            status: Status::Works,
        },
        Cell {
            name: "list_toplet_by_value",
            main: r#"import self as m
fn first(xs: List[String]) -> String = list.get(xs, 0) ?? "?"
effect fn main() -> Unit = println(first(m.WORDS))
"#,
            expected: "a",
            status: Status::Works,
        },
        Cell {
            name: "record_toplet_member",
            main: r#"import self as m
effect fn main() -> Unit = println(m.CFG.name)
"#,
            expected: "c",
            status: Status::Works,
        },
        Cell {
            name: "record_toplet_unused",
            main: r#"import self as m
effect fn main() -> Unit = println("ok")
"#,
            expected: "ok",
            status: Status::Works,
        },
        Cell {
            name: "record_list_toplet_len",
            main: r#"import self as m
effect fn main() -> Unit = println(int.to_string(list.len(m.CFGS)))
"#,
            expected: "2",
            status: Status::Works,
        },
        Cell {
            name: "fn_returned_record_member",
            main: r#"import self as m
effect fn main() -> Unit = println(m.mk().name)
"#,
            expected: "via-fn",
            status: Status::Works,
        },
        Cell {
            name: "variant_toplet_match",
            main: r#"import self as m
effect fn main() -> Unit =
  match m.MOOD { m.Happy => println("happy"), m.Sad => println("sad") }
"#,
            expected: "happy",
            status: Status::Works,
        },
        Cell {
            name: "int_toplet_arith",
            main: r#"import self as m
effect fn main() -> Unit = println(int.to_string(m.N + 1))
"#,
            expected: "8",
            status: Status::Works,
        },
        Cell {
            name: "option_record_toplet",
            main: r#"import self as m
effect fn main() -> Unit =
  match m.MAYBE { some(c) => println(c.name), none => println("?") }
"#,
            expected: "opt",
            status: Status::Works,
        },
        Cell {
            name: "crossmod_brace_construction",
            main: r#"import self as m
effect fn main() -> Unit = {
  let c = m.Cfg { name: "x" }
  println(c.name)
}
"#,
            expected: "x",
            status: Status::Works,
        },
        Cell {
            name: "crossmod_paren_named_construction",
            main: r#"import self as m
effect fn main() -> Unit = {
  let c = m.Cfg(name: "y")
  println(c.name)
}
"#,
            expected: "y",
            status: Status::Works,
        },
        Cell {
            name: "toplet_through_closure",
            main: r#"import self as m
effect fn main() -> Unit = {
  let f = (s: String) => s + "?"
  println(f(m.SYSTEM))
}
"#,
            expected: "hi?",
            status: Status::Works,
        },
        Cell {
            name: "annotated_let_crossmod_type",
            main: r#"import self as m
effect fn main() -> Unit = {
  let c: m.Cfg = m.mk()
  println(c.name)
}
"#,
            expected: "via-fn",
            status: Status::Works,
        },
        Cell {
            name: "fn_sig_param_and_ret",
            main: r#"import self as m
fn id_emotion(e: m.Emotion) -> m.Emotion = e
effect fn main() -> Unit =
  match id_emotion(m.Sad) { m.Happy => println("happy"), m.Sad => println("sad") }
"#,
            expected: "sad",
            status: Status::Works,
        },
        Cell {
            name: "effect_fn_cross_call_auto_try",
            main: r#"import self as m
effect fn main() -> Unit = {
  let x = m.estep(7)
  println(int.to_string(x))
}
"#,
            expected: "8",
            status: Status::Works,
        },
        Cell {
            name: "toplet_borrow_position",
            main: r#"import self as m
effect fn main() -> Unit = println(int.to_string(string.len(m.SYSTEM)))
"#,
            expected: "2",
            status: Status::Works,
        },
        Cell {
            name: "var_scalar_toplet_read",
            main: r#"import self as m
effect fn main() -> Unit = println(int.to_string(m.count))
"#,
            expected: "0",
            status: Status::Works,
        },
        Cell {
            name: "var_scalar_mutate_via_fn",
            main: r#"import self as m
effect fn main() -> Unit = {
  m.bump()
  m.bump()
  println(int.to_string(m.count))
}
"#,
            expected: "2",
            status: Status::Works,
        },
        Cell {
            name: "var_string_toplet_read",
            main: r#"import self as m
effect fn main() -> Unit = println(m.title)
"#,
            expected: "init",
            status: Status::Works,
        },
        Cell {
            name: "var_string_reassign_via_fn",
            main: r#"import self as m
effect fn main() -> Unit = {
  m.retitle("x")
  println(m.title)
}
"#,
            expected: "x",
            status: Status::Works,
        },
        Cell {
            name: "var_list_inplace_via_fn",
            main: r#"import self as m
effect fn main() -> Unit = {
  m.add_num(3)
  println(int.to_string(list.len(m.nums)))
}
"#,
            expected: "3",
            status: Status::Works,
        },
        Cell {
            name: "var_toplet_through_closure",
            main: r#"import self as m
effect fn main() -> Unit = {
  m.bump()
  let f = () => m.count
  println(int.to_string(f()))
}
"#,
            expected: "1",
            status: Status::Works,
        },
        Cell {
            name: "var_direct_crossmod_assign",
            main: r#"import self as m
effect fn main() -> Unit = {
  m.nums = m.nums + [3]
  println(int.to_string(list.len(m.nums)))
}
"#,
            expected: "3",
            status: Status::Works,
        },
        Cell {
            name: "spread_from_toplet_base",
            main: r#"import self as m
effect fn main() -> Unit = {
  let c = { ...m.CFG, name: "z" }
  println(c.name)
}
"#,
            expected: "z",
            status: Status::Works,
        },
        Cell {
            name: "tuple_toplet_destructure",
            main: r#"import self as m
effect fn main() -> Unit = {
  let (s, n) = m.PAIR
  println(s + int.to_string(n))
}
"#,
            expected: "a1",
            status: Status::Works,
        },
        Cell {
            name: "toplet_bind_then_pass",
            main: r#"import self as m
fn shout(s: String) -> String = s + "!"
effect fn main() -> Unit = {
  let x = m.SYSTEM
  println(shout(x))
}
"#,
            expected: "hi!",
            status: Status::Works,
        },
    ]
}

struct CellResult {
    native_ok: bool,
    detail: String,
}

fn run_cell(cell: &Cell, check_wasm: bool) -> CellResult {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"cellpkg\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(src.join("mod.almd"), MOD_ALMD).unwrap();
    std::fs::write(src.join("main.almd"), cell.main).unwrap();

    let out_bin = dir.path().join("out");
    let build = Command::new(almide_bin())
        .args(["build", "src/main.almd", "-o", out_bin.to_str().unwrap()])
        .current_dir(dir.path())
        .output().expect("run almide build");
    if !build.status.success() {
        return CellResult {
            native_ok: false,
            detail: format!("native build failed:\n{}", String::from_utf8_lossy(&build.stderr)),
        };
    }
    let run = Command::new(&out_bin).output().expect("run cell binary");
    let native_out = String::from_utf8_lossy(&run.stdout).trim().to_string();
    if native_out != cell.expected {
        return CellResult {
            native_ok: false,
            detail: format!("native stdout mismatch: expected {:?}, got {:?}", cell.expected, native_out),
        };
    }

    if check_wasm {
        let wasm_path = dir.path().join("out.wasm");
        let wbuild = Command::new(almide_bin())
            .args(["build", "src/main.almd", "--target", "wasm", "-o", wasm_path.to_str().unwrap()])
            .current_dir(dir.path())
            .output().expect("run almide build --target wasm");
        if !wbuild.status.success() {
            return CellResult {
                native_ok: false,
                detail: format!("wasm build failed:\n{}", String::from_utf8_lossy(&wbuild.stderr)),
            };
        }
        let wrun = Command::new("wasmtime").arg(&wasm_path).output().expect("run wasmtime");
        let wasm_out = String::from_utf8_lossy(&wrun.stdout).trim().to_string();
        if !wrun.status.success() || wasm_out != native_out {
            return CellResult {
                native_ok: false,
                detail: format!(
                    "wasm divergence: native {:?}, wasm {:?} (exit {:?})\n{}",
                    native_out, wasm_out, wrun.status.code(),
                    String::from_utf8_lossy(&wrun.stderr)
                ),
            };
        }
    }

    CellResult { native_ok: true, detail: String::new() }
}

#[test]
fn crossmod_shape_matrix() {
    if Command::new(almide_bin()).arg("--version").output().is_err() { return; }
    let check_wasm = wasmtime_available();

    let mut failures: Vec<String> = Vec::new();
    for cell in cells() {
        let result = run_cell(&cell, check_wasm);
        match (&cell.status, result.native_ok) {
            (Status::Works, true) => {}
            (Status::Works, false) => {
                failures.push(format!("cell `{}` FAILED:\n{}", cell.name, result.detail));
            }
            (Status::KnownBroken(reason), false) => {
                eprintln!("cell `{}` still broken as declared ({})", cell.name, reason);
            }
            (Status::KnownBroken(reason), true) => {
                failures.push(format!(
                    "cell `{}` is declared KnownBroken ({}) but now PASSES — remove the flag (the ledger only shrinks)",
                    cell.name, reason
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "cross-module shape matrix: {} cell(s) failed — a rustc error or wasm divergence on \
         generated code is a compiler bug:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
