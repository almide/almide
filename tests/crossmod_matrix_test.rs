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

fn stash[T](v: T) -> T = {
  count = count + 1
  v
}

fn retitle(s: String) -> Unit = {
  title = s
}

fn add_num(n: Int) -> Unit = {
  nums = nums + [n]
}

type Pigment: Codec = { r: Int, g: Int, b: Int }

type Twin = { label: String, score: Int }

pub fn mk_twin(l: String) -> Twin = Twin { label: l, score: 1 }

pub fn read_twin(t: Twin) -> String = t.label

type Wrap = { v: Int }

pub fn mk_wrap(v: Int) -> Wrap = Wrap { v: v }

pub fn wrap_value(w: Wrap) -> Int = w.v

@inline_rust("{ let w = {w}; Wrap { v: w.v + 1 } }")
pub fn bump_wrap(w: Wrap) -> Wrap = Wrap { v: w.v + 1 }

type Node = { tag: String, children: List[Node] }

pub fn node(tag: String, children: List[Node]) -> Node = Node { tag: tag, children: children }

pub fn show(n: Node) -> String = {
  let kids = n.children |> list.map((c) => show(c)) |> list.join("")
  if list.is_empty(n.children) then "<${n.tag}/>"
  else "<${n.tag}>${kids}</${n.tag}>"
}

fn parse_flag(s: String) -> Result[String, String] =
  if s == "yes" then ok("y") else err("n")

effect fn confirm(tag: String) -> Result[Unit, String] = {
  println("confirmed:" + tag)
  ok(())
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
            name: "generic_fn_mutates_module_var",
            // #788: a MONOMORPHIZED copy of a generic module fn alpha-renamed
            // its free reference to the module-level `var` (a fresh VarId the
            // storage annotation does not key), so native rendered a bare
            // local `count = …` and rustc E0425'd. Two distinct instantiations
            // force two specializations; both must route through the
            // thread_local static.
            main: r#"import self as m
effect fn main() -> Unit = {
  let a = m.stash(41)
  let s = m.stash("x")
  println(int.to_string(m.count))
}
"#,
            expected: "2",
            status: Status::Works,
        },
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
            status: Status::KnownBroken("#782 v1 gap: match over an UNTRACKED subject with a call-bearing arm cannot take the both-arms linearization (would run the untaken arm's effects)"),
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
        // #609 / C-098: a derived Codec whose subject + element types live in
        // ANOTHER module. The call site carries only `Type.method` (no module);
        // the WASM dispatcher used to ICE on the unflattened dotted name and on
        // the `__decode_list_m.Pigment` helper name. Covers direct / List / Option
        // fields. `almide test` masked this via native fallback — this gate runs
        // the wasm leg explicitly and asserts byte-identical.
        Cell {
            name: "cross_module_derived_codec_roundtrip",
            main: r#"import self as m
type Wrapped: Codec = {
  id: Int,
  payload: m.Pigment,
  palette: List[m.Pigment],
  accent: Option[m.Pigment],
}
effect fn main() -> Unit = {
  let w = Wrapped { id: 1, payload: m.Pigment { r: 1, g: 2, b: 3 }, palette: [m.Pigment { r: 4, g: 5, b: 6 }], accent: some(m.Pigment { r: 7, g: 8, b: 9 }) }
  match Wrapped.decode(Wrapped.encode(w)) {
    ok(v) => println("b=" + int.to_string(v.payload.b)),
    err(e) => println("err: " + e),
  }
}
"#,
            expected: "b=3",
            status: Status::Works,
        },
        // STRUCTURAL TWINS: the checker unifies same-BASE-NAME same-SHAPE record
        // decls across modules (values flow both directions freely), so codegen
        // merges them into one canonical struct (flatten twin-merge). Before,
        // whichever sites resolved to the "other" twin's name failed as
        // generated-Rust E0308 (`expected almide_rt_m_Twin, found Twin`) —
        // the almai root-vs-provider LLMResponse class.
        Cell {
            name: "structural_twin_records_flow_both_directions",
            main: r#"import self as m
type Twin = { label: String, score: Int }
fn local_read(t: Twin) -> String = t.label
effect fn main() -> Unit = {
  // module → root direction
  println(local_read(m.mk_twin("from-mod")))
  // root → module direction (a root literal into the module's reader)
  println(m.read_twin(Twin { label: "from-root", score: 2 }))
}
"#,
            expected: "from-mod\nfrom-root",
            status: Status::Works,
        },
        // RECURSIVE record type across the module boundary: unifying `El`
        // with its module twin `lib.El` expands both to record form and
        // recurses into `children: List[El]` — without the equi-recursive
        // pair guard in unify_structural this re-reached El×lib.El forever
        // (compiler stack overflow; the svg `render(group(..))` shape).
        Cell {
            name: "recursive_record_type_cross_module",
            main: r#"import self as m
effect fn main() -> Unit = {
  let g = m.node("g", [m.node("rect", []), m.node("g", [m.node("circle", [])])])
  println(m.show(g))
}
"#,
            expected: "<g><rect/><g><circle/></g></g>",
            status: Status::Works,
        },
        // A user package's `@inline_rust` fn with a REAL Almide body: native
        // pastes the template (whose bare struct tokens must survive the
        // flatten mangle — requalified by StdlibLowering), wasm compiles the
        // body (the attr is a native-only optimization). Both used to fail
        // cross-module: E0422 on the unmangled struct name / `no WASM
        // dispatch` ICE (the aes cfb8_encrypt shape).
        Cell {
            name: "inline_rust_with_fallback_body_cross_module",
            main: r#"import self as m
effect fn main() -> Unit = {
  let w = m.bump_wrap(m.mk_wrap(4))
  println(int.to_string(m.wrap_value(w)))
}
"#,
            expected: "5",
            status: Status::Works,
        },
        // ResultPropagation Phase 2b: an `effect fn main() -> Result[..]` whose
        // body tail is a `match` must NOT Ok-wrap an arm that calls a
        // Result-DECLARED effect fn (never sig-lifted, so not in `lifted_fns`,
        // but its call ty IS already the Result) — the porta `__almide_main`
        // double-wrap (E0308 `Result<Result<..>>` on native).
        Cell {
            name: "effect_main_match_tail_calls_result_effect_fn",
            main: r#"import self as m
effect fn main() -> Result[Unit, String] = {
  let cmd = "go"
  match cmd {
    "go" => m.confirm("go"),
    _ => ok(()),
  }
}
"#,
            expected: "confirmed:go",
            status: Status::Works,
        },
        // auto-? skip set: a `match r { ok/err }` sitting BEHIND a value wrapper
        // (here the `ok(...)` tail) must still keep `r`'s binding a Result — the
        // old checker/lowering walk stopped at Match/Block/If, so the binding was
        // auto-?'d out from under the match (porta mcp handle_builtin_exec).
        Cell {
            name: "result_match_behind_ok_wrapper",
            main: r#"import self as m
effect fn main() -> Result[Unit, String] = {
  let parsed = m.parse_flag("yes")
  ok(match parsed {
    ok(v) => println("ok:" + v),
    err(e) => println("err:" + e),
  })
}
"#,
            expected: "ok:y",
            status: Status::KnownBroken("#782 v1 gap: heap-result ResultOk cannot be faithfully returned (would move out an empty deferred heap value)"),
        },
        // auto-? skip set, depth: the binding + Result-match live INSIDE another
        // match arm. The skip set used to apply only to top-level binds, so the
        // nested `let parsed` was unconditionally auto-?'d while the match kept
        // its ok/err arms (porta mcp handle_tools_call, E0308 on native).
        Cell {
            name: "result_match_bind_inside_match_arm",
            main: r#"import self as m
effect fn main() -> Result[Unit, String] = {
  let sel = some("yes")
  match sel {
    some(s) => {
      let parsed = m.parse_flag(s)
      let msg = match parsed {
        ok(v) => "ok:" + v,
        err(e) => "err:" + e,
      }
      println(msg)
      ok(())
    },
    none => ok(()),
  }
}
"#,
            expected: "ok:y",
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

/// `@extern(rs, "bridge", "fn")` declared in an IMPORTED module: the definition
/// is emitted as a `use bridge::fn as <name>;` alias, and every cross-module
/// call site renders the flatten prefix `almide_rt_<module>_<fn>` — the alias
/// must carry that same prefixed name (porta wasm_rt: 28× E0425 when the alias
/// kept the bare name). Needs a native/ dir + [native-deps], so it can't be a
/// matrix cell (run_cell writes no native assets). Native-only: @extern(rs) has
/// no wasm leg.
#[test]
fn extern_rs_fn_in_module_native() {
    if Command::new(almide_bin()).arg("--version").output().is_err() { return; }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let native = dir.path().join("native");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&native).unwrap();
    // A [native-deps] entry forces the self-contained cargo build with
    // source_root set — the path that injects native/*.rs into the crate.
    std::fs::write(
        dir.path().join("almide.toml"),
        "[package]\nname = \"externpkg\"\nversion = \"0.1.0\"\n\n[native-deps]\nonce_cell = \"1\"\n",
    ).unwrap();
    std::fs::write(
        native.join("bridge.rs"),
        "pub fn shout(s: impl AsRef<str>) -> String { format!(\"{}!\", s.as_ref()) }\n",
    ).unwrap();
    std::fs::write(
        src.join("util.almd"),
        "@extern(rs, \"bridge\", \"shout\")\nfn shout(s: String) -> String\n",
    ).unwrap();
    std::fs::write(
        src.join("main.almd"),
        "import util\n\neffect fn main() -> Unit = println(util.shout(\"hey\"))\n",
    ).unwrap();
    let out_bin = dir.path().join("out");
    let build = Command::new(almide_bin())
        .args(["build", "src/main.almd", "-o", out_bin.to_str().unwrap()])
        .current_dir(dir.path())
        .output().expect("run almide build");
    assert!(
        build.status.success(),
        "native build of a module @extern(rs) fn failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&out_bin).output().expect("run extern binary");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "hey!");
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
