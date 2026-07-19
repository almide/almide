
// ── Data-driven discovery test ──
// Scans spec/wasm_cross/*.almd, runs each on both targets, compares output.

#[test]
fn wasm_cross_target_spec() {
    // The cross-target observable-equivalence GATE (ratchet). Every program in
    // spec/wasm_cross/ must produce byte-identical (stdout, stderr, exit code) on
    // native and wasm. A `// @xt-allow: <reason + tracking ref>` line marks a
    // KNOWN / intentional divergence: it is exempt from the equality assertion but
    // LOGGED, so a divergence is never silently ignored — and once it is fixed the
    // gate flags the now-stale allow so the entry gets removed. Native is the
    // reference; native==wasm is a hard invariant, not a "target difference".
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() { return; }
    // Needs wasmtime to run the wasm command and capture its stderr + exit code.
    if Command::new("wasmtime").arg("--version").output().is_err() { return; }

    let spec_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross");
    if !spec_dir.exists() { return; }

    let mut entries: Vec<_> = std::fs::read_dir(&spec_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "almd").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.path());
    if entries.is_empty() { return; }

    let mut passed = 0;
    let mut allowed: Vec<String> = Vec::new();
    let mut stale: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();

    for entry in &entries {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let source = std::fs::read_to_string(&path).unwrap();
        let allow = source.lines().find_map(|l| {
            l.trim().strip_prefix("// @xt-allow:").map(|r| r.trim().to_string())
        });

        let (rc, rout, rerr) = run_native_capture(&source);
        let wasm = match std::panic::catch_unwind(|| run_wasm_capture(&source)) {
            Ok(Some(w)) => w,
            Ok(None) => return, // wasmtime unavailable mid-run → skip the gate
            Err(_) => { failed.push(format!("{name}: WASM build/run panicked")); continue; }
        };
        let (wc, wout, werr) = wasm;
        let equal = rc == wc && rout == wout && rerr == werr;

        match (equal, allow) {
            (true, None) => passed += 1,
            (true, Some(r)) => stale.push(format!("{name}: @xt-allow now MATCHES (was: {r}) — remove the directive")),
            (false, Some(r)) => allowed.push(format!("{name}: {r}")),
            (false, None) => failed.push(format!(
                "{name}: cross-target divergence\n  native: exit={rc} stdout={rout:?} stderr={rerr:?}\n  wasm:   exit={wc} stdout={wout:?} stderr={werr:?}")),
        }
    }

    eprintln!(
        "\nwasm_cross_target_spec (gate): {passed} equal, {} tracked-divergence(s), {} stale-allow(s), {} unexpected",
        allowed.len(), stale.len(), failed.len()
    );
    for a in &allowed { eprintln!("  ~ tracked: {a}"); }

    let mut problems = failed;
    problems.extend(stale);
    if !problems.is_empty() {
        panic!("\n{} cross-target gate problem(s):\n\n{}", problems.len(), problems.join("\n\n"));
    }
}

// ── Closure Architecture v2, P0: cross-module closure identity ──
// A submodule function returning a non-capturing lambda, applied across the
// module boundary. Before P0 the WASM emitter correlated a raw Lambda to its
// LambdaInfo by `lambda_id`, which resets to 0 per module — so a module lambda
// collided with a main-program lambda of the same id, AND module lambdas were
// never registered in the WASM closure pre-scan. `lib.neg()(5)` then returned
// main's `add` body on WASM (native `1005 / -5`, WASM `1005 / 1005`), and a
// no-main-lambda variant emitted invalid WASM ("table index out of bounds").
// GlobalizeClosureIdsPass (program-unique ids) + scanning module functions in
// the pre-scan fix both. These must run on WASM and match the native result.

/// Write a multi-file project into a tempdir and assert its `main.almd`
/// produces identical stdout on the Rust and WASM targets.
fn assert_cross_target_project(files: &[(&str, &str)]) {
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    for (rel, content) in files {
        let p = dir.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, content).unwrap();
    }

    // Native: compile + run from inside the project dir (so almide.toml resolves).
    let r = Command::new(&bin)
        .current_dir(dir.path())
        .args(["run", "main.almd"])
        .output()
        .expect("native run");
    assert!(
        r.status.success(),
        "native compile/run failed:\n{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let rust_out = String::from_utf8_lossy(&r.stdout).trim().to_string();

    // WASM: build, then run with wasmtime (skip the run if wasmtime is absent —
    // the native arm and the build still gate, and the byte gate covers wasm32).
    let b = Command::new(&bin)
        .current_dir(dir.path())
        .args(["build", "main.almd", "--target", "wasm", "-o", "out.wasm"])
        .output()
        .expect("wasm build");
    assert!(
        b.status.success(),
        "wasm build failed:\n{}",
        String::from_utf8_lossy(&b.stderr)
    );
    let w = match Command::new("wasmtime")
        .current_dir(dir.path())
        .arg("--dir=/")
        // Env passthrough mirrors `almide run --target wasm` (`-S inherit-env=y`):
        // the env_get fixture (C-133) reads HOME on both targets.
        .arg("-S")
        .arg("inherit-env=y")
        .arg("out.wasm")
        .output()
    {
        Ok(o) if o.status.code() != Some(127) => o,
        _ => return, // wasmtime unavailable
    };
    assert!(
        w.status.success(),
        "wasm execution failed (invalid module?):\n{}",
        String::from_utf8_lossy(&w.stderr)
    );
    let wasm_out = String::from_utf8_lossy(&w.stdout).trim().to_string();

    assert_eq!(
        rust_out, wasm_out,
        "\ncross-module closure mismatch!\nRust: {:?}\nWASM: {:?}",
        rust_out, wasm_out
    );
}

const CROSS_PKG_TOML: &str = "[package]\nname = \"clostest\"\nversion = \"0.1.0\"\n\n[targets]\nwasm = true\n";
const CROSS_PKG_LIB: &str = "pub fn neg() -> (Int) -> Int = (n) => 0 - n\n";

#[test]
fn wasm_cross_module_returned_non_capturing_lambda() {
    assert_cross_target_project(&[
        ("almide.toml", CROSS_PKG_TOML),
        ("src/lib.almd", CROSS_PKG_LIB),
        (
            "main.almd",
            "import self.lib\n\
             fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)\n\
             fn add() -> (Int) -> Int = (n) => n + 1000\n\
             fn main() -> Unit = {\n\
             \x20 println(int.to_string(apply(add(), 5)))\n\
             \x20 println(int.to_string(apply(lib.neg(), 5)))\n\
             }\n",
        ),
    ]);
}

#[test]
fn wasm_cross_module_lambda_without_local_decoy() {
    // No main-program lambda for the module lambda to mis-resolve to: before P0
    // this emitted invalid WASM ("unknown table 0: table index out of bounds").
    assert_cross_target_project(&[
        ("almide.toml", CROSS_PKG_TOML),
        ("src/lib.almd", CROSS_PKG_LIB),
        (
            "main.almd",
            "import self.lib\n\
             fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)\n\
             fn main() -> Unit = {\n\
             \x20 println(int.to_string(apply(lib.neg(), 5)))\n\
             }\n",
        ),
    ]);
}

const MOD_PKG_TOML: &str = "[package]\nname = \"modtest\"\nversion = \"0.1.0\"\n\n[targets]\nwasm = true\n";

#[test]
fn wasm_cross_module_producer_side_variant_ctor() {
    // #631: a producer fn INSIDE the submodule that owns a variant constructs
    // it via the BARE constructor (`Circle(r)`) and returns it. The ctor-call
    // expression's `.ty` must be pinned to the OWNER-qualified `shape.Shape`,
    // or the #433 name-pinning guard aborts BOTH targets at codegen even though
    // `check` passed. Expected `48` on both (Circle(4) → 4*4*3).
    assert_cross_target_project(&[
        ("almide.toml", MOD_PKG_TOML),
        (
            "src/shape.almd",
            "type Shape = Circle(Int) | Square(Int)\n\
             fn make_circle(r: Int) -> Shape = Circle(r)\n\
             fn area(s: Shape) -> Int = match s {\n\
             \x20 Circle(r) => r * r * 3,\n\
             \x20 Square(w) => w * w,\n\
             }\n",
        ),
        (
            "main.almd",
            "import self.shape\n\
             fn main() -> Unit = {\n\
             \x20 let c = shape.make_circle(4)\n\
             \x20 println(int.to_string(shape.area(c)))\n\
             }\n",
        ),
    ]);
}

#[test]
fn wasm_cross_module_global_init_order_direct() {
    // #632: an importing module's top-let reads an imported module's heap
    // global DIRECTLY. On wasm the eager init order must place `cfg.APP_NAME`
    // before `GREETING`, or the read hits a still-zero string header. Expected
    // `modg` on both (was 8 NUL bytes on wasm).
    assert_cross_target_project(&[
        ("almide.toml", MOD_PKG_TOML),
        ("src/cfg.almd", "let APP_NAME = \"modg\"\n"),
        (
            "main.almd",
            "import self.cfg\n\
             let GREETING = cfg.APP_NAME\n\
             fn main() -> Unit = {\n\
             \x20 println(GREETING)\n\
             }\n",
        ),
    ]);
}

#[test]
fn wasm_cross_module_global_init_order_through_call() {
    // #632 (interprocedural): the importing top-let reads the imported global
    // only THROUGH a function call (`cfg.banner()` interpolates `APP_NAME`) and
    // a List heap global via `list.len(cfg.ITEMS)`. The init order must still
    // place the cfg globals first. Expected `modg ok` / `3` on both.
    assert_cross_target_project(&[
        ("almide.toml", MOD_PKG_TOML),
        (
            "src/cfg.almd",
            "let APP_NAME = \"modg\"\n\
             let ITEMS = [10, 20, 30]\n\
             fn banner() -> String = \"${APP_NAME} ok\"\n",
        ),
        (
            "main.almd",
            "import self.cfg\n\
             let BANNER = cfg.banner()\n\
             let FIRST = list.len(cfg.ITEMS)\n\
             fn main() -> Unit = {\n\
             \x20 println(BANNER)\n\
             \x20 println(int.to_string(FIRST))\n\
             }\n",
        ),
    ]);
}

/// Like `assert_cross_target`, but for programs whose `main` is an `effect fn`.
/// Such a main returns `Result[Unit, _]`, and wasmtime's `_start` prints the
/// wrapped return value (a heap pointer) as a trailing line. Compare only the
/// program's own output (the first `rust_out.lines().len()` lines). A trap
/// still surfaces: `run_wasm` panics when wasmtime exits non-zero.
fn assert_cross_target_effect_main(source: &str) {
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() { return; }
    if Command::new("node").arg("--version").output().is_err() { return; }

    let rust_out = run_rust(source);
    let wasm_out = run_wasm(source);
    let rust_lines: Vec<&str> = rust_out.lines().collect();
    let wasm_lines: Vec<&str> = wasm_out.lines().collect();
    assert!(
        wasm_lines.len() >= rust_lines.len(),
        "\nWASM produced fewer lines than Rust (output dropped)!\nRust: {:?}\nWASM: {:?}\nSource:\n{}",
        rust_out, wasm_out, source
    );
    assert_eq!(
        rust_lines.as_slice(),
        &wasm_lines[..rust_lines.len()],
        "\nCross-target mismatch (effect main)!\nRust: {:?}\nWASM: {:?}\nSource:\n{}",
        rust_out, wasm_out, source
    );
}

#[test]
fn wasm_effect_fn_returns_closure_auto_try_binding() {
    // P3-WASM regression: an effect fn returning a closure, bound via auto-`?`.
    // The binding's var type lagged at `Result[Fn, _]` (auto_try runs after
    // lowering), so `add5(10)` mis-resolved to a Named call `add5` instead of a
    // Computed call through the local. WASM then trapped on the unresolved call
    // and Perceus freed the closure before the call. The `!` form always worked;
    // this guards the `?` form (the common case). Expected: 15.
    assert_cross_target_effect_main(
        "effect fn make_adder_e(n: Int) -> (Int) -> Int = (x) => x + n\n\
         effect fn main() -> Unit = {\n\
         \x20 let add5 = make_adder_e(5)\n\
         \x20 println(int.to_string(add5(10)))\n\
         }\n",
    );
}

#[test]
fn wasm_effect_fn_returns_closure_used_twice() {
    // Same auto-`?` closure binding, but the closure is called more than once —
    // exercises Perceus inc/dec balance for the binding across multiple uses.
    assert_cross_target_effect_main(
        "effect fn make_adder_e(n: Int) -> (Int) -> Int = (x) => x + n\n\
         effect fn main() -> Unit = {\n\
         \x20 let add = make_adder_e(100)\n\
         \x20 println(int.to_string(add(1) + add(2)))\n\
         }\n",
    );
}

#[test]
fn rust_process_exec_forwards_bound_list() {
    // Regression (bug report 2026-06-02): forwarding a *bound* List[String] to
    // process.exec emitted `&[String]` at the call site while the runtime shim took
    // `&Vec<String>`, so `almide check` passed but `almide build` failed (E0308) —
    // the worst failure mode (codegen produces invalid Rust). A List *literal*
    // compiled fine, which made it sneaky. `process.*` is native-only, so this is a
    // Rust-target build+run test: `run_rust` compiles via `almide run` and panics if
    // the build fails, then we assert the output.
    // Skip when the `almide` binary isn't available (e.g. the CI Test Rust job runs
    // `cargo test` without building it) — same guard the cross-target tests use.
    if std::process::Command::new(almide_bin()).arg("--version").output().is_err() { return; }
    let out = run_rust(
        "import process\n\
         effect fn run(cmd: String, a: List[String]) -> String =\n\
         \x20 match process.exec(cmd, a) {\n\
         \x20   ok(o) => o,\n\
         \x20   err(_) => \"\",\n\
         \x20 }\n\
         effect fn main() -> Unit = {\n\
         \x20 let out = run(\"echo\", [\"hello\"])\n\
         \x20 println(out)\n\
         }\n",
    );
    assert_eq!(out, "hello");
}

#[test]
fn wasm_non_copy_mutable_capture_through_closure() {
    // Closure v2 P6: mutating a captured non-Copy `var` through a closure must be
    // visible to the enclosing scope — on BOTH targets. Before P6 it silently
    // returned 0: Rust used `RcCow` (copy-on-write clones on a shared mutation),
    // WASM captured the list by value (no shared cell). Copy scalars already worked
    // (P3 shared cell); this is the non-Copy (`SharedMut` / heap-cell) analogue.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let inner = () => { list.push(acc, 1) }\n\
         \x20 inner()\n\
         \x20 inner()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_nested_non_copy_mutable_capture() {
    // A non-Copy `var` bound inside a closure, mutated by a nested closure — the
    // tail read of the shared cell must not outlive it (a Rust borrow-lifetime
    // hazard) and the cell must thread through the WASM env. Cross-target = 3.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let outer = () => {\n\
         \x20   var acc: List[Int] = []\n\
         \x20   let inner = () => { list.push(acc, 1) }\n\
         \x20   inner()\n\
         \x20   inner()\n\
         \x20   inner()\n\
         \x20   list.len(acc)\n\
         \x20 }\n\
         \x20 println(int.to_string(outer()))\n\
         }\n",
    );
}

#[test]
fn wasm_sibling_closures_share_mutable_capture() {
    // Closure v2 P6: two SIBLING closures capture the same non-Copy `var` — one
    // mutates it, the other only reads it. The reader must observe the writer's
    // mutation (they share one cell). On WASM the reader was lifted to a
    // ClosureCreate/EnvLoad that loaded the raw cell ptr and used it as the list,
    // so `list.len` read garbage (a leaked pointer) instead of 0; the fix keeps any
    // shared-cell-capturing lambda raw so both closures share one heap cell.
    // wipe() clears -> reader sees len 0. Cross-target = 0.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var xs: List[Int] = [7, 8, 9]\n\
         \x20 let wipe = () => { list.clear(xs) }\n\
         \x20 let size = () => { list.len(xs) }\n\
         \x20 wipe()\n\
         \x20 println(int.to_string(size()))\n\
         }\n",
    );
}

#[test]
fn wasm_reader_closure_observes_writer_closure() {
    // Same shared-cell sibling-closure case via in-place `list.pop`: the writer
    // drains 3 of 4 elements, a separate reader closure reports the length. The
    // reader must observe the pops through the shared cell. Cross-target = 1.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var xs: List[Int] = [5, 6, 7, 8]\n\
         \x20 let drain = () => { list.pop(xs) }\n\
         \x20 let report = () => { list.len(xs) }\n\
         \x20 drain()\n\
         \x20 drain()\n\
         \x20 drain()\n\
         \x20 println(int.to_string(report()))\n\
         }\n",
    );
}

#[test]
fn wasm_bytes_mutable_capture_through_closure() {
    // Closure v2 P6: a captured `var buf: Bytes` mutated through a closure must
    // become a shared cell, like List/Map/String. `bytes.push` (and the rest of the
    // bytes `&mut` builders) is an in-place mutator, but bytes' stdlib `mut`
    // annotations are incomplete, so the cell-detection keys off the runtime's
    // actual mutation surface. Before the fix WASM lost the appends (len 0). f()
    // pushes twice -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var buf: Bytes = bytes.new(0)\n\
         \x20 let f = () => { bytes.push(buf, 65) }\n\
         \x20 f()\n\
         \x20 f()\n\
         \x20 println(int.to_string(bytes.len(buf)))\n\
         }\n",
    );
}

#[test]
fn wasm_string_push_clear_in_place() {
    // string.push / string.clear had NO WASM dispatch arm — they ICE'd the emitter
    // (native worked). They are `mut s`/in-place mutators in is_inplace_mutator, so
    // a captured String mutated through a closure also relied on this. push appends
    // (via __string_append, write-back like list.push), clear sets len 0. 2 pushes
    // of "ab" -> len 4. Cross-target = 4.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var s: String = \"\"\n\
         \x20 let f = () => { string.push(s, \"ab\") }\n\
         \x20 f()\n\
         \x20 f()\n\
         \x20 println(int.to_string(string.len(s)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_stored_in_tuple() {
    // A capture-mutating closure stored in a tuple, destructured into a `let`, and
    // called. Rust typed the binding `(impl Fn(), i64)` → E0562 (impl Trait in a
    // binding type). The let-type's nested Fn subtree is now erased to `_` so Rust
    // infers the concrete closure type. 2 calls -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let pair = (() => { list.push(acc, 1) }, 0)\n\
         \x20 let (g, _) = pair\n\
         \x20 g()\n\
         \x20 g()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_call_closure_through_list_index() {
    // `fs[0]()` — calling a closure indexed out of a list. The parser read `[0](` as
    // a const-generic type-args call (`fs::<0>()`), emitting an invalid bare-name
    // call (native E0425 / wasm trap). `[...]( ` is now a type-args call only when
    // the brackets name a type; an int index makes it `(fs[0])()`. 2 calls -> len 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let fs = [() => { list.push(acc, 1) }]\n\
         \x20 fs[0]()\n\
         \x20 fs[0]()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_in_anonymous_record() {
    // A closure stored in an ANONYMOUS record field, then called via `r.run()`.
    // Rust gave `E0277`: the generic anon-record struct demanded `T: Clone + Debug
    // + PartialEq`, which a closure fails. The struct now derives Clone only when a
    // field is a closure (like a `type`-declared record's `has_fn_fields` path).
    // A `type`-declared record already worked. f pushes twice -> len 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let r = { run: () => { list.push(acc, 1) } }\n\
         \x20 r.run()\n\
         \x20 r.run()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_indexassign_noncopy_element_through_closure() {
    // IndexAssign of a NON-Copy element through a closure capturing the list:
    // `xs: List[String]; () => { xs[0] = xs[0] + "!" }`. WASM trapped — the
    // captured cell's `RcDec` ran a TYPED rc_dec over the cell ptr as if it were the
    // list, reading cell[0] (the object ptr) as an element count and decref'ing
    // garbage addresses. A plain rc_dec on the cell (matching the plain rc_inc on
    // capture) fixes it. List[Int] (Copy elems) never hit the element-drop loop.
    // Two appends -> "a!!".
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var xs: List[String] = [\"a\", \"b\"]\n\
         \x20 let f = () => { xs[0] = xs[0] + \"!\" }\n\
         \x20 f()\n\
         \x20 f()\n\
         \x20 println(xs[0])\n\
         }\n",
    );
}

#[test]
fn wasm_closures_stored_in_map() {
    // Two DIFFERENT closures stored in a `Map[String, () -> Unit]`, then one
    // extracted via get_or and called. Rust gave `E0308` — the map's erased `_`
    // value type was inferred from the first closure, so the second (and the get_or
    // default) couldn't unify. The closures going into `map.insert` / `map.get_or`
    // are now `RcWrap`'d to `Rc<dyn Fn>`, so `_` infers one uniform boxed type (as
    // a List[Fn] literal already does). f then h push -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 var m: Map[String, () -> Unit] = map.new()\n\
         \x20 map.insert(m, \"a\", () => { list.push(acc, 1) })\n\
         \x20 map.insert(m, \"b\", () => { list.push(acc, 1) })\n\
         \x20 let f = map.get_or(m, \"a\", () => {})\n\
         \x20 let h = map.get_or(m, \"b\", () => {})\n\
         \x20 f()\n\
         \x20 h()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_typed_param_closure_capturing_mutable() {
    // A closure with a TYPED param `(k: String) => …` that captures a mutated var
    // (so it stays a raw, capture-clone-wrapped closure) dropped the param type in
    // Rust codegen → `move |k| …` → E0282. The bind site now annotates the tail
    // lambda's params through the capture-clone block. Word-count -> 3,2,2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var m: Map[String, Int] = map.new()\n\
         \x20 let bump = (k: String) => {\n\
         \x20   let cur: Int = map.get_or(m, k, 0)\n\
         \x20   map.insert(m, k, cur + 1)\n\
         \x20 }\n\
         \x20 for w in [\"a\", \"b\", \"a\", \"a\", \"b\"] { bump(w) }\n\
         \x20 println(int.to_string(map.get_or(m, \"a\", -1)))\n\
         \x20 println(int.to_string(map.get_or(m, \"b\", -1)))\n\
         \x20 println(int.to_string(map.len(m)))\n\
         }\n",
    );
}
