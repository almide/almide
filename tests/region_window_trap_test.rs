//! #2186 step 1, island (a): the region window's `AlmideRgn` deref is
//! `unsafe { &*ptr }` over an arena the window has rewound, and rustc cannot
//! see a handle that outlives its window — a wrong region-pure verdict in
//! `pass_region_window.rs` is a SILENT use-after-rewind.
//!
//! Two nets. `ALMIDE_REGION_TRAP_STALE=1` (read at BUILD time, the twin of
//! `ALMIDE_RC_TRAP_DOUBLE_FREE`) emits a generation-checked handle: a stale
//! read panics naming the pass. The first test drives the prelude directly
//! with rustc and pins both the trap and the silence it replaces. The second
//! is the differential lane: a corpus of region-window-shaped programs built
//! armed on native and compared byte-for-byte with the wasm leg — a verdict
//! the trap refuses or an answer the legs disagree on is the finding.
//!
//! CI arms the lane as an ordinary root test (`cargo test --test
//! region_window_trap_test`); the trap is per-build, so nothing here changes
//! the shipped prelude or the perf ratchet's region-window rows.

use std::process::Command;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn rustc_bin() -> String {
    std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string())
}

/// A handle allocated inside a window and read after it — the escape the
/// recogniser exists to refuse. `live` (allocated outside any window) must
/// keep reading across the inner window: the check is generation-exact, not
/// "any rewind happened".
const PROBE_POISON: &str = r#"
fn main() {
    let live = almide_rgn_alloc(3i64);
    let stale = almide_region_window(|| almide_rgn_alloc(7i64));
    let inner = almide_region_window(|| { let h = almide_rgn_alloc(1i64); *h });
    let ok = *live + inner;
    let v = *stale;
    println!("{ok} {v}");
}
"#;

/// The ABA shape: a later window re-uses the slot and the stale handle is
/// read while that window is open — the header is a fresh generation, not
/// the poison, and the old bytes are already gone.
const PROBE_ABA: &str = r#"
fn main() {
    let stale = almide_region_window(|| almide_rgn_alloc(7i64));
    let v = almide_region_window(|| { let h = almide_rgn_alloc(9i64); *h + *stale });
    println!("{v}");
}
"#;

fn run_probe(armed: bool, probe: &str, label: &str) -> std::process::Output {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("probe.rs");
    let bin = dir.path().join("probe");
    let source = format!("{}\n{probe}", almide::codegen::region_arena_prelude_source(armed));
    std::fs::write(&src, source).expect("write probe");
    let built = Command::new(rustc_bin())
        .arg(&src)
        .args(["--edition", "2021", "-A", "warnings", "-o"])
        .arg(&bin)
        .output()
        .expect("rustc");
    assert!(
        built.status.success(),
        "{label}/armed={armed}: the prelude did not compile:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    Command::new(&bin).output().expect("run probe")
}

#[test]
fn stale_handle_trap_fires_when_armed_and_is_silent_otherwise() {
    for (probe, label, slot) in [
        (PROBE_POISON, "poison", "poisoned by a rewind"),
        (PROBE_ABA, "aba", "re-allocated under generation"),
    ] {
        let armed = run_probe(true, probe, label);
        let stderr = String::from_utf8_lossy(&armed.stderr);
        assert!(!armed.status.success(), "{label}: the armed prelude let a stale read through:\n{stderr}");
        assert!(
            stderr.contains("region window trap (ALMIDE_REGION_TRAP_STALE)")
                && stderr.contains("pass_region_window's region-pure verdict was wrong")
                && stderr.contains(slot),
            "{label}: the trap message must name the knob, the pass and the slot state:\n{stderr}"
        );
        // The island itself: without the trap the same program exits 0 and
        // prints whatever bytes the rewound slot still holds.
        let silent = run_probe(false, probe, label);
        assert!(
            silent.status.success(),
            "{label}: the unarmed prelude is expected to read the stale slot silently — it did not:\n{}",
            String::from_utf8_lossy(&silent.stderr)
        );
    }
}

// ── the differential corpus ──────────────────────────────────────────

/// The wasm leg's own fixture (`crates/almide-wasm/tests/region_window.rs`).
const TREES: &str = r#"type Tree = Leaf | Node(Tree, Tree)

fn make(depth: Int) -> Tree =
  if depth == 0 then Leaf
  else Node(make(depth - 1), make(depth - 1))

fn check(tree: Tree) -> Int = match tree {
  Leaf => 1,
  Node(left, right) => check(left) + check(right) + 1,
}

fn check_trees(iterations: Int, depth: Int) -> Int = {
  var total = 0
  for _ in 0..<iterations {
    total = total + check(make(depth))
  }
  total
}

effect fn main() -> Unit = {
  let keep = make(6)
  println("sum=${check_trees(64, 8)} keep=${check(keep)} one=${check(make(4))}")
}
"#;

/// A window whose scalar argument is itself a window: nested rewinds, and
/// the outer handles must survive the inner one.
const NESTED: &str = r#"type Tree = Leaf | Node(Tree, Tree)

fn make(depth: Int) -> Tree =
  if depth == 0 then Leaf else Node(make(depth - 1), make(depth - 1))

fn check(t: Tree) -> Int = match t {
  Leaf => 1,
  Node(l, r) => check(l) + check(r) + 1,
}

fn depth_of(t: Tree) -> Int = match t {
  Leaf => 0,
  Node(l, r) => {
    let a = depth_of(l)
    let b = depth_of(r)
    if a > b then a + 1 else b + 1
  },
}

effect fn main() -> Unit = {
  let inner = check(make(depth_of(make(3))))
  println("nested=${inner} again=${check(make(check(make(2))))}")
}
"#;

/// Scalar payloads in the cases, a Bool consumer, a `while` driver.
const PAYLOAD: &str = r#"type T = Leaf | Node(Int, T, T)

fn build(d: Int, k: Int) -> T =
  if d == 0 then Leaf else Node(k, build(d - 1, k * 2), build(d - 1, k * 2 + 1))

fn total(t: T) -> Int = match t {
  Leaf => 0,
  Node(v, l, r) => v + total(l) + total(r),
}

fn balanced(t: T) -> Bool = match t {
  Leaf => true,
  Node(_, l, r) => balanced(l) and balanced(r),
}

effect fn main() -> Unit = {
  var i = 0
  var acc = 0
  while i < 5 {
    acc = acc + total(build(i, 1))
    i = i + 1
  }
  println("acc=${acc} bal=${balanced(build(4, 7))}")
}
"#;

/// The consumer takes a second scalar argument and loops over the tree.
const SCALAR_ARGS: &str = r#"type Tree = Leaf | Node(Tree, Tree)

fn make(depth: Int) -> Tree =
  if depth == 0 then Leaf else Node(make(depth - 1), make(depth - 1))

fn check(t: Tree) -> Int = match t {
  Leaf => 1,
  Node(l, r) => check(l) + check(r) + 1,
}

fn count_n(t: Tree, n: Int) -> Int = {
  var s = 0
  for _ in 0..<n {
    s = s + check(t)
  }
  s
}

effect fn main() -> Unit = {
  println("a=${count_n(make(5), 3)} b=${count_n(make(2), 0)}")
}
"#;

/// Float payloads: the two legs must also agree on the number's rendering.
const FLOAT_PAYLOAD: &str = r#"type T = Leaf | Node(Float, T, T)

fn build(d: Int, x: Float) -> T =
  if d == 0 then Leaf else Node(x, build(d - 1, x / 2.0), build(d - 1, x * 1.5))

fn total(t: T) -> Float = match t {
  Leaf => 0.0,
  Node(v, l, r) => v + total(l) + total(r),
}

effect fn main() -> Unit = {
  println("f=${total(build(6, 1.0))} g=${total(build(3, 0.1))}")
}
"#;

/// Both refusals (a global read in the producer, a heap result from the
/// consumer): no window may open, and the legs still agree.
const NO_WINDOW: &str = r#"type Tree = Leaf | Node(Tree, Tree)

let base = 2

fn make(depth: Int) -> Tree =
  if depth == 0 then Leaf
  else Node(make(depth - 1), make(depth - 1))

fn make_g(depth: Int) -> Tree = make(depth + base)

fn check(tree: Tree) -> Int = match tree {
  Leaf => 1,
  Node(left, right) => check(left) + check(right) + 1,
}

fn left_of(tree: Tree) -> Tree = match tree {
  Leaf => Leaf,
  Node(left, _) => left,
}

effect fn main() -> Unit =
  println("g=${check(make_g(3))} l=${check(left_of(make(5)))}")
"#;

/// The generator: `check(make(depth))` repeated `iters` times over a
/// three-case tree, at the depths where the arena stays in its first chunk
/// (0, 1), crosses into a second one (10) and where every node is a `Leaf`.
fn generated(depth: u32, iters: u32) -> String {
    format!(
        r#"type Tree = Leaf | One(Tree) | Node(Tree, Tree)

fn make(depth: Int) -> Tree =
  if depth == 0 then Leaf
  else if depth % 3 == 0 then One(make(depth - 1))
  else Node(make(depth - 1), make(depth - 1))

fn check(t: Tree) -> Int = match t {{
  Leaf => 1,
  One(x) => check(x) + 2,
  Node(l, r) => check(l) + check(r) + 1,
}}

effect fn main() -> Unit = {{
  var total = 0
  for _ in 0..<{iters} {{
    total = total + check(make({depth}))
  }}
  println("d={depth} n={iters} total=${{total}}")
}}
"#
    )
}

/// (label, source, a window is expected to open)
fn corpus() -> Vec<(String, String, bool)> {
    let mut out: Vec<(String, String, bool)> = [
        ("trees", TREES, true),
        ("nested", NESTED, true),
        ("payload", PAYLOAD, true),
        ("scalar_args", SCALAR_ARGS, true),
        ("float_payload", FLOAT_PAYLOAD, true),
        ("no_window", NO_WINDOW, false),
    ]
    .into_iter()
    .map(|(l, s, w)| (l.to_string(), s.to_string(), w))
    .collect();
    for depth in [0u32, 1, 10] {
        for iters in [1u32, 3] {
            out.push((format!("gen_d{depth}_n{iters}"), generated(depth, iters), true));
        }
    }
    out
}

fn build(source: &std::path::Path, target: &str, out: &std::path::Path, armed: bool) -> std::process::Output {
    let mut cmd = Command::new(almide_bin());
    cmd.args(["build", source.to_str().expect("path"), "--target", target, "-o", out.to_str().expect("path")])
        .env_remove("ALMIDE_WASM_INCUMBENT")
        .env_remove("ALMIDE_COMPONENT_P3")
        .env_remove("ALMIDE_REGION_OFF")
        .env("ALMIDE_REGION_DEBUG", "1");
    match armed {
        true => cmd.env("ALMIDE_REGION_TRAP_STALE", "1"),
        false => cmd.env_remove("ALMIDE_REGION_TRAP_STALE"),
    };
    cmd.output().expect("almide build")
}

#[test]
fn region_corpus_agrees_native_armed_vs_wasm() {
    if !wasmtime_available() {
        eprintln!("wasmtime not on PATH — skipping the region-window differential lane");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    for (label, program, expect_window) in corpus() {
        let source = dir.path().join(format!("{label}.almd"));
        std::fs::write(&source, &program).expect("source");
        let native = dir.path().join(format!("{label}.native"));
        let built = build(&source, "rust", &native, true);
        let report = String::from_utf8_lossy(&built.stderr).to_string();
        assert!(built.status.success(), "{label}/native build failed:\n{report}");
        assert_eq!(
            report.contains("[region:native] window"),
            expect_window,
            "{label}: the recogniser's verdict is not the one the corpus expects — the lane would be vacuous:\n{report}"
        );
        let ran = Command::new(&native).output().expect("run native");
        assert!(
            ran.status.success(),
            "{label}/native exited {:?} under the stale-handle trap — a wrong region-pure verdict:\n{}",
            ran.status.code(),
            String::from_utf8_lossy(&ran.stderr)
        );
        let native_out = String::from_utf8_lossy(&ran.stdout).to_string();

        let wasm = dir.path().join(format!("{label}.wasm"));
        let built = build(&source, "wasm", &wasm, false);
        let report = String::from_utf8_lossy(&built.stdout).to_string() + &String::from_utf8_lossy(&built.stderr);
        assert!(built.status.success(), "{label}/wasm build failed:\n{report}");
        assert!(report.contains("structural leg"), "{label}: the wasm build did not take the structural leg:\n{report}");
        let ran = Command::new("wasmtime").arg("run").arg(&wasm).output().expect("wasmtime");
        assert!(ran.status.success(), "{label}/wasm exited {:?}:\n{}", ran.status.code(), String::from_utf8_lossy(&ran.stderr));
        let wasm_out = String::from_utf8_lossy(&ran.stdout).to_string();
        assert_eq!(wasm_out, native_out, "{label}: the wasm leg answered differently from the armed native leg");
    }
}

#[test]
fn the_build_knob_reaches_the_emitted_prelude() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("trees.almd");
    std::fs::write(&source, TREES).expect("source");
    let emit = |armed: bool| {
        let mut cmd = Command::new(almide_bin());
        cmd.arg(source.to_str().expect("path")).args(["--target", "rust"]);
        match armed {
            true => cmd.env("ALMIDE_REGION_TRAP_STALE", "1"),
            false => cmd.env_remove("ALMIDE_REGION_TRAP_STALE"),
        };
        let out = cmd.output().expect("emit");
        assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    assert!(emit(true).contains("almide_rgn_check("), "the armed build must emit the generation check");
    assert!(!emit(false).contains("almide_rgn_check("), "the default build must stay on the unchecked handle");
}
