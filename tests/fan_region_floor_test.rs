//! #3673: native runs every metered region shape the wasm leg runs, with the
//! same charges (C-204 / C-205).
//!
//! #3669 linked the v1-rendered regions into the standard native codegen, and
//! that render's subset still refused two kinds of region the wasm leg runs:
//!
//!   1. an arm that calls a stdlib fn the v1 render has no native body for
//!      (`string.split`, `int.to_float`);
//!   2. a region whose signature the two renders spell differently (a record,
//!      a variant, a tuple, `List[String]`, `Option`, a closure).
//!
//! Each program below prints UNIT COUNTS read back at the budget boundary
//! (the first budget that admits the arm, in charge units), race winners at
//! the boundary, and the fallbacks of cut arms — so a charge that moved by one
//! unit on either leg changes a printed line. The two legs must agree byte for
//! byte on stdout and on the exit code.
//!
//! `almide app.almd --target rust` of a metered program builds on its own
//! (item 3). The last test holds the standard codegen's own meter to the
//! whole-program v1 render on the unit-exact boundary fixtures: with
//! `ALMIDE_FORCE_METER_CODEGEN=1` a program that renders whole is metered by
//! the standard codegen instead, and every observable must stay the same.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A native build of one of these programs is a cargo build; a hang is a bug.
const PER_RUN_LIMIT: Duration = Duration::from_secs(600);

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// (exit code, stdout, stderr), the child killed past [`PER_RUN_LIMIT`].
fn run_limited(mut cmd: Command) -> (Option<i32>, String, String) {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("spawn almide");
    let (mut so, mut se) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let to = std::thread::spawn(move || {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut so, &mut s).ok();
        s
    });
    let te = std::thread::spawn(move || {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut se, &mut s).ok();
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().expect("wait") {
            break Some(st);
        }
        if start.elapsed() > PER_RUN_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (out, err) = (to.join().unwrap(), te.join().unwrap());
    match status {
        Some(st) => (st.code(), out, err),
        None => (Some(-999), "HANG".into(), err),
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-fan-region-floor-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run `src` on both targets and require byte-identical stdout and exit code,
/// and a clean exit on both.
fn assert_two_legs_agree(name: &str, src: &str) {
    if !wasmtime_available() {
        eprintln!("SKIP {name}: wasmtime is not installed, the wasm leg cannot run");
        return;
    }
    let dir = scratch(name);
    let path = dir.join(format!("{name}.almd"));
    std::fs::write(&path, src).unwrap();
    let leg = |wasm: bool| {
        let mut cmd = Command::new(almide());
        cmd.current_dir(&dir).arg("run").arg(&path);
        if wasm {
            cmd.args(["--target", "wasm"]);
        }
        run_limited(cmd)
    };
    let (n_code, n_out, n_err) = leg(false);
    let (w_code, w_out, w_err) = leg(true);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(w_code, Some(0), "{name}: the wasm leg failed\nstdout={w_out}\nstderr={w_err}");
    assert_eq!(
        (n_code, &n_out),
        (w_code, &w_out),
        "{name}: native and wasm disagree\n  native exit={n_code:?} stderr:\n{n_err}"
    );
}

/// Item 1: arms that call a stdlib fn outside the v1 native floor. The
/// self-hosted stdlib body the wasm leg links is a pool body and charges
/// nothing; the unit counts below are the user fns' charges alone.
const STDLIB_ARMS: &str = r#"fn spin(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  i
}

fn parts(s: String) -> Int = list.len(string.split(s, ",")) + spin(string.len(s))

fn half(n: Int) -> Float = int.to_float(spin(n)) / 2.0

effect fn parts_units(s: String, ns: Int) -> Result[Int, String] =
  if ns > 3000 then ok(-1)
  else {
    let r = fan.bounded(compute.ns(ns)) { parts(s) } ?? -1
    if r >= 0 then ok(ns / 3) else parts_units(s, ns + 3)!
  }

effect fn half_units(n: Int, ns: Int) -> Result[Int, String] =
  if ns > 3000 then ok(-1)
  else {
    let r = fan.bounded(compute.ns(ns)) { half(n) } ?? -1.0
    if r >= 0.0 then ok(ns / 3) else half_units(n, ns + 3)!
  }

effect fn main() -> Unit = {
  println("parts a,b: ${parts_units("a,b", 0)!}")
  println("parts a,b,c,d,e: ${parts_units("a,b,c,d,e", 0)!}")
  println("half 7: ${half_units(7, 0)!}")
  println(int.to_string(fan.min(compute.ms(100)) { parts("a,b,c,d"), parts("a,b") } ?? -1))
  println(float.to_string(fan.min(compute.ms(100)) { half(9), half(5) } ?? 0.0))
  println(int.to_string(fan.bounded(compute.ns(12)) { parts("a,b,c,d,e,f") } ?? -1))
  println(float.to_string(fan.bounded(compute.ns(12)) { half(40) } ?? -1.0))
}
"#;

/// Item 2: region signatures beyond the scalar boundary, and a cut arm whose
/// callee returns a record or a `List[String]`.
const REGION_SHAPES: &str = r#"type P = { x: Int, y: Int }

type Shape = Circle(Int) | Square(Int, Int)

fn spin(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  i
}

fn mk(n: Int) -> P = { x: spin(n), y: n * 2 }

fn words(n: Int) -> List[String] = if spin(n) > 2 then ["a", "b"] else ["c"]

fn opt(n: Int) -> Option[Int] = if spin(n) > 2 then some(n) else none

fn shape(n: Int) -> Shape = if spin(n) > 2 then Circle(n) else Square(n, n)

fn pair(n: Int) -> (Int, String) = (spin(n), "p${n}")

fn area(p: P) -> Int = spin(p.x) + p.y

fn show(s: Shape) -> String = match s {
  Circle(r) => "circle ${r}",
  Square(a, b) => "square ${a}x${b}",
}

effect fn mk_units(n: Int, ns: Int) -> Result[Int, String] =
  if ns > 3000 then ok(-1)
  else {
    let r = fan.bounded(compute.ns(ns)) { mk(n) } ?? { x: -1, y: -1 }
    if r.x >= 0 then ok(ns / 3) else mk_units(n, ns + 3)!
  }

effect fn main() -> Unit = {
  let p = fan.min(compute.ms(100)) { mk(6), mk(2) } ?? { x: 0, y: 0 }
  println("record: ${p.x} ${p.y}")
  let w = fan.min(compute.ms(100)) { words(6), words(1) } ?? []
  println("words: ${list.join(w, ",")}")
  let o = fan.min(compute.ms(100)) { opt(6), opt(3) } ?? none
  println("option: ${o ?? -1}")
  let s = fan.min(compute.ms(100)) { shape(7), shape(1) } ?? Circle(0)
  println("variant: ${show(s)}")
  let t = fan.min(compute.ms(100)) { pair(9), pair(4) } ?? (0, "none")
  println("tuple: ${t.0} ${t.1}")
  let base = { x: 3, y: 4 }
  let other = { x: 1, y: 1 }
  let a = fan.min(compute.ms(100)) {
    area(base)
    area(other)
  } ?? -1
  println("record param: ${a}")
  println("mk units: ${mk_units(5, 0)!}")
  let cut = fan.bounded(compute.ns(9)) { mk(100) } ?? { x: -7, y: -7 }
  println("cut record: ${cut.x}")
  let cut_words = fan.bounded(compute.ns(9)) { words(100) } ?? ["cut"]
  println("cut words: ${list.join(cut_words, ",")}")
}
"#;

/// The charges a region pays that the v1 floor never reached: a closure
/// called through a value (one unit per hop), a callback written directly
/// at a stdlib combinator (no hop), the size-proportional concat charge, and
/// a closure built outside the region and called inside it.
const CLOSURES_AND_CONCAT: &str = r#"fn spin(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  i
}

fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)

fn hop(n: Int) -> Int = apply((x) => spin(x), n)

fn mapped_var(xs: List[Int]) -> Int = {
  let f = (x) => spin(x)
  list.sum(list.map(xs, f))
}

fn mapped_lit(xs: List[Int]) -> Int = list.sum(list.map(xs, (x) => spin(x)))

fn build(n: Int) -> String = {
  var s = ""
  var i = 0
  while i < n {
    s = s + "0123456789abcdef"
    i = i + 1
  }
  s
}

fn blen(n: Int) -> Int = string.len(build(n))

effect fn units(k: Int, ns: Int) -> Result[Int, String] =
  if ns > 6000 then ok(-1)
  else {
    let r = match k {
      0 => fan.bounded(compute.ns(ns)) { hop(4) } ?? -1,
      1 => fan.bounded(compute.ns(ns)) { mapped_var([1, 2, 3]) } ?? -1,
      2 => fan.bounded(compute.ns(ns)) { mapped_lit([1, 2, 3]) } ?? -1,
      _ => fan.bounded(compute.ns(ns)) { blen(6) } ?? -1,
    }
    if r >= 0 then ok(ns / 3) else units(k, ns + 3)!
  }

effect fn main() -> Unit = {
  for k in 0..<4 {
    println("units ${k}: ${units(k, 0)!}")
  }
  let g = (x) => spin(x) * 2
  println(int.to_string(fan.min(compute.ms(100)) { apply(g, 9), apply(g, 3) } ?? -1))
  println(fan.min(compute.ms(100)) { build(9), build(4) } ?? "none")
  println(fan.bounded(compute.ns(60)) { build(50) } ?? "cut")
}
"#;

/// A cut inside a fn that returns a record, a recursive variant, a variant
/// with a payload, `Option`, `List[String]`, a map or a tuple: the cut
/// returns that type's zero, and the region's fallback is what prints. The
/// roomy budget prints the computed values.
const CUT_ZEROS: &str = r#"type P = { x: Int, y: Int }

type Tree = Leaf | Node(Tree, Int, Tree)

type Tag = Named(String) | Anon

fn loop_to(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  i
}

fn rec_of(n: Int) -> P = {
  var i = 0
  while i < n {
    i = i + 1
  }
  { x: i, y: n }
}

fn tree_of(n: Int) -> Tree = {
  var t = Leaf
  for i in 0..<n {
    t = Node(t, i, Leaf)
  }
  t
}

fn tag_of(n: Int) -> Tag = {
  var s = ""
  for _i in 0..<n {
    s = s + "t"
  }
  if n > 0 then Named(s) else Anon
}

fn opt_of(n: Int) -> Option[String] = {
  var s = ""
  for _i in 0..<n {
    s = s + "o"
  }
  if n > 2 then some(s) else none
}

fn words_of(n: Int) -> List[String] = {
  var out: List[String] = []
  for i in 0..<n {
    out = out + ["w${i}"]
  }
  out
}

fn counts_of(n: Int) -> Map[String, Int] = {
  var m: Map[String, Int] = [:]
  for i in 0..<n {
    m = map.set(m, "k${i}", i)
  }
  m
}

fn pair_of(n: Int) -> (Int, Float) = {
  var f = 0.0
  for _i in 0..<n {
    f = f + 0.5
  }
  (loop_to(n), f)
}

fn depth(t: Tree) -> Int = match t {
  Leaf => 0,
  Node(l, _, _) => 1 + depth(l),
}

fn show_tag(t: Tag) -> String = match t {
  Named(s) => "named ${s}",
  Anon => "anon",
}

effect fn main() -> Unit = {
  for ns in [6, 30, 300000] {
    let r = fan.bounded(compute.ns(ns)) { rec_of(20) } ?? { x: -1, y: -1 }
    let t = fan.bounded(compute.ns(ns)) { tree_of(20) } ?? Leaf
    let g = fan.bounded(compute.ns(ns)) { tag_of(20) } ?? Anon
    let o = fan.bounded(compute.ns(ns)) { opt_of(20) } ?? none
    let w = fan.bounded(compute.ns(ns)) { words_of(20) } ?? []
    let c = fan.bounded(compute.ns(ns)) { counts_of(20) } ?? [:]
    let p = fan.bounded(compute.ns(ns)) { pair_of(20) } ?? (-1, -1.0)
    println("${ns}: ${r.x} ${depth(t)} ${show_tag(g)} ${o ?? "none"} ${list.len(w)} ${map.len(c)} ${p.0} ${p.1}")
  }
  let winner = fan.min(compute.ms(100)) {
    tree_of(30)
    tree_of(4)
  } ?? Leaf
  println("race depth ${depth(winner)}")
}
"#;

/// Where a cut returns from. On the wasm leg a lambda written at a stdlib
/// combinator is the combinator's loop body, so a cut at a charge site in
/// that callback leaves the fn that made the call — the later elements, a
/// division by zero among them, never run. A cut inside a CALLEE of the
/// callback returns from the callee only, and the combinator runs on.
const INLINE_CALLBACK_CUT: &str = r#"fn spin(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  i
}

// A cut at the loop head INSIDE a callback written at list.map: the wasm leg
// runs that callback as the map's loop body, so the cut returns from `crunch`
// and the second element (a division by zero) never runs.
fn crunch(xs: List[Int]) -> Int = list.sum(list.map(xs, (x) => {
  let q = 100 / x
  var i = 0
  while i < 50 {
    i = i + 1
  }
  q + i
}))

// A cut inside a CALLEE of the callback returns from the callee only: the
// map runs on, element by element, on both legs.
fn crunch_callee(xs: List[Int]) -> Int = list.sum(list.map(xs, (x) => spin(x * 20) + x))

effect fn main() -> Unit = {
  println(int.to_string(fan.bounded(compute.ns(30)) { crunch([5, 0, 2]) } ?? -1))
  println(int.to_string(fan.bounded(compute.ns(100000)) { crunch([5, 4, 2]) } ?? -1))
  println(int.to_string(fan.bounded(compute.ns(30)) { crunch_callee([5, 3, 2]) } ?? -1))
}
"#;

#[test]
fn an_arm_calling_a_stdlib_fn_outside_the_v1_floor_runs_metered_on_native() {
    assert_two_legs_agree("stdlib_arms", STDLIB_ARMS);
}

#[test]
fn a_region_with_a_record_variant_tuple_list_or_option_signature_runs_metered_on_native() {
    assert_two_legs_agree("region_shapes", REGION_SHAPES);
}

#[test]
fn closure_hops_and_concat_charges_match_the_wasm_leg() {
    assert_two_legs_agree("closures_and_concat", CLOSURES_AND_CONCAT);
}

#[test]
fn a_cut_returns_the_zero_of_any_return_type() {
    assert_two_legs_agree("cut_zeros", CUT_ZEROS);
}

#[test]
fn a_cut_in_an_inline_stdlib_callback_leaves_the_calling_fn() {
    assert_two_legs_agree("inline_callback_cut", INLINE_CALLBACK_CUT);
}

fn rustc_available() -> bool {
    Command::new("rustc").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Item 3: `almide app.almd --target rust` stands alone for a metered
/// program — it carries the codegen's meter and its runtime instead of
/// region delegates into a module only the CLI appends. The emitted Rust
/// compiles warning-free under plain rustc (`unused_comparisons` is the
/// `compute.ns` clamp in `main`, emitted for every budget, metered or not),
/// and the binary prints what the wasm leg prints.
#[test]
fn the_emitted_rust_of_a_metered_program_builds_and_runs_on_its_own() {
    if !wasmtime_available() || !rustc_available() {
        eprintln!("SKIP: rustc or wasmtime is not installed");
        return;
    }
    for (name, src) in [("emit_stdlib_arms", STDLIB_ARMS), ("emit_region_shapes", REGION_SHAPES)] {
        let dir = scratch(name);
        let almd = dir.join("main.almd");
        std::fs::write(&almd, src).unwrap();
        let mut emit = Command::new(almide());
        emit.current_dir(&dir).arg(&almd).args(["--target", "rust"]);
        let (code, rust, err) = run_limited(emit);
        assert_eq!(code, Some(0), "{name}: emit failed\n{err}");
        let rs = dir.join("main.rs");
        std::fs::write(&rs, &rust).unwrap();
        let bin = dir.join("main");
        let mut rustc = Command::new("rustc");
        rustc.args(["--edition", "2021", "-O", "-D", "warnings", "-A", "non_snake_case", "-A", "unused_macros", "-A", "unused_comparisons", "-o"]).arg(&bin).arg(&rs);
        let (code, _, err) = run_limited(rustc);
        assert_eq!(code, Some(0), "{name}: the emitted Rust does not compile warning-free on its own:\n{err}");
        let (b_code, b_out, b_err) = run_limited(Command::new(&bin));
        let mut wasm = Command::new(almide());
        wasm.current_dir(&dir).arg("run").arg(&almd).args(["--target", "wasm"]);
        let (w_code, w_out, _) = run_limited(wasm);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!((b_code, &b_out), (w_code, &w_out), "{name}: the standalone binary and wasm disagree\n{b_err}");
    }
}

/// The announcement an inherited route switch prints.
const FORCED: &str = "ALMIDE_FORCE_METER_CODEGEN is set";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("spec/wasm_cross/{name}.almd"))
}

/// (exit code, stdout, stderr without the route announcement, announced?)
fn run_native(path: &Path, force_codegen_meter: bool) -> (Option<i32>, String, String, bool) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(path);
    if force_codegen_meter {
        cmd.env("ALMIDE_FORCE_METER_CODEGEN", "1");
    }
    let (code, out, err) = run_limited(cmd);
    let announced = err.contains(FORCED);
    let err: Vec<&str> = err.lines().filter(|l| !l.contains(FORCED)).collect();
    (code, out, err.join("\n"), announced)
}

#[test]
fn the_codegen_meter_charges_like_the_whole_program_render() {
    let mut failures = Vec::new();
    for name in [
        "fuel_race_boundary",
        "fuel_bounded_boundary",
        "fuel_dyn_charge",
        "fuel_divergence_cut",
        "fuel_trap_window",
        "fuel_trap_cut",
        "fuel_cut_in_arm_loop",
        "fuel_cut_releases_frame",
        "fuel_block_body",
        "fuel_race_err_skip",
        "fuel_var_budget",
        "fuel_zero_budget_inline",
    ] {
        let path = fixture(name);
        let (code, out, err, _) = run_native(&path, false);
        let (m_code, m_out, m_err, announced) = run_native(&path, true);
        if !announced {
            failures.push(format!("{name}: the forced run never reached the metered route decision"));
        }
        if (code, &out, &err) != (m_code, &m_out, &m_err) {
            failures.push(format!(
                "{name}: the codegen meter changed an observable\n  whole: exit={code:?} stdout={out:?} stderr={err:?}\n  codegen: exit={m_code:?} stdout={m_out:?} stderr={m_err:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
