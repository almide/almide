
#[test]
fn wasm_reassign_concat_capture_through_closure() {
    // A non-Copy `var` GROWN by reassignment-concat through a closure
    // (`xs = xs + [list.len(xs)]`, reading its own length each step) must behave
    // identically on both targets. On Rust the `xs = xs + [v]` → `xs.push(v)`
    // peephole fired even though `xs` is a `SharedMut`, producing `xs.get().push(v)`
    // — a push onto a discarded clone, so the reassignment was lost (native stayed
    // [0], then panicked on `xs[3]`). The peephole now skips shared cells, leaving a
    // cell-aware `xs.set(xs.get() + [v])`. Three appends -> [0,1,2,3], len 4.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var xs: List[Int] = [0]\n\
         \x20 let app = () => { xs = xs + [list.len(xs)] }\n\
         \x20 app()\n\
         \x20 app()\n\
         \x20 app()\n\
         \x20 println(int.to_string(list.len(xs)))\n\
         }\n",
    );
}

#[test]
fn wasm_module_global_list_mutated_through_closure() {
    // A module-level mutable global (`var g`) mutated through a closure must behave
    // identically on both targets. On Rust the global lowers to a `thread_local!`
    // `ModuleRc`; it was ALSO (wrongly) classified `shared_mut`, so the enclosing
    // read emitted a lowercase `g.get()` that doesn't exist (`error[E0425]`) while
    // the closure body used `G.with(…)`. Globals are now excluded from shared_mut.
    // f() pushes twice -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "var g: List[Int] = []\n\
         effect fn main() -> Unit = {\n\
         \x20 let f = () => { list.push(g, 7) }\n\
         \x20 f()\n\
         \x20 f()\n\
         \x20 println(int.to_string(list.len(g)))\n\
         }\n",
    );
}

#[test]
fn wasm_module_global_map_mutated_through_closure() {
    // Same, via `map.insert` invoked as an EXPRESSION on a `ModuleRc` global. The
    // Rust walker only special-cased list push/pop/clear, so map/string/bytes
    // mutators on a global mutated a discarded `(**c.borrow()).clone()` (silently
    // wrong); the mutator set is now the shared `is_inplace_mutator`. Inserts two
    // distinct keys -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "var g: Map[String, Int] = map.new()\n\
         effect fn main() -> Unit = {\n\
         \x20 let f = () => { map.insert(g, \"a\", 1) }\n\
         \x20 let h = () => { map.insert(g, \"b\", 2) }\n\
         \x20 f()\n\
         \x20 h()\n\
         \x20 println(int.to_string(map.len(g)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_selected_by_if_branch() {
    // `let f = if c then A else B` where A, B are distinct closures. Native gave
    // E0308 "if and else have incompatible types" (each branch a different
    // anonymous closure). The unified boxing pass boxes both branches to
    // `Rc<dyn Fn>` so the `if` unifies. 2 calls -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let f = if true then (() => { list.push(acc, 1) }) else (() => { list.push(acc, 2) })\n\
         \x20 f()\n\
         \x20 f()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_selected_by_match_arm() {
    // Same join via `match`. Native gave E0308 "match arms have incompatible
    // types". Boxing each arm body unifies them. 1 call -> len 1.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let k = 1\n\
         \x20 let f = match k { 1 => (() => { list.push(acc, 10) }), _ => (() => { list.push(acc, 20) }) }\n\
         \x20 f()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_in_map_from_list() {
    // A closure as the value of a `map.from_list([(k, closure)])` literal. The old
    // boxing covered `map.insert`/`get_or` only; from_list's closures live inside a
    // list-of-tuples literal. The unified pass boxes Fn positions inside a uniform
    // container's tuple elements, so the inner list-of-tuples is handled. len 1.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let m: Map[String, () -> Unit] = map.from_list([(\"a\", () => { list.push(acc, 1) })])\n\
         \x20 let f = map.get_or(m, \"a\", () => {})\n\
         \x20 f()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_pushed_into_list_fn() {
    // `list.push(fs, closure)` onto a `List[() -> Unit]` var. Boxing fired for list
    // LITERALS only; pushing a closure into an existing `List[Fn]` was native
    // E0562 "impl Trait not allowed in paths". The container-mutator rule boxes the
    // pushed closure against the receiver's element type. len 1.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 var fs: List[() -> Unit] = []\n\
         \x20 list.push(fs, () => { list.push(acc, 1) })\n\
         \x20 let f = fs[0]\n\
         \x20 f()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_list_in_record_field() {
    // A list of TWO distinct closures stored in a record field `{ fs: [A, B] }`.
    // The old List[Fn] boxing fired only for a direct `Bind`; here the list is a
    // record field value, so native failed to box (E0308). The unified pass boxes
    // any `List[Fn]` node regardless of parent. 3 calls -> len 3.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let r = { fs: [() => { list.push(acc, 1) }, () => { list.push(acc, 2) }] }\n\
         \x20 (r.fs[0])()\n\
         \x20 (r.fs[1])()\n\
         \x20 (r.fs[0])()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_bytes_set_at_shared_through_closure() {
    // `bytes.set_at` had no WASM dispatch arm -> fell through to an ICE in
    // emit_wasm. Native ran fine. A `set_at` arm (in-place index store, no realloc,
    // Unit return) was added. The Bytes buffer is shared by reference across the
    // writer/reader closures, so after the writer sets index 0 to 99 the reader
    // observes 99. Cross-target = 1, then 99, then 99.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var b: Bytes = bytes.from_list([1, 1, 1])\n\
         \x20 let writer = () => { bytes.set_at(b, 0, 99); bytes.get_or(b, 0, -1) }\n\
         \x20 let reader = () => { bytes.get_or(b, 0, -1) }\n\
         \x20 println(int.to_string(reader()))\n\
         \x20 println(int.to_string(writer()))\n\
         \x20 println(int.to_string(reader()))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_as_variant_payload() {
    // A closure stored as a tuple-variant payload `Run(() -> Unit)`. Native gave
    // E0562 "impl Trait not allowed in field types" — the variant field rendered
    // `impl Fn`. The payload type is now `Rc<dyn Fn>`, the enum derives Clone only,
    // and the constructor boxes the closure. One Run fires, one Noop fires. len 2.
    assert_cross_target_effect_main(
        "type Action = Run(() -> Unit) | Noop\n\
         effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let a = Run(() => { list.push(acc, 1) })\n\
         \x20 match a { Run(f) => f(), Noop => {} }\n\
         \x20 let b: Action = Noop\n\
         \x20 match b { Run(f) => f(), Noop => { list.push(acc, 9) } }\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_with_fn_typed_param() {
    // A closure whose own parameter is function-typed: `(g: () -> Unit) => ...`.
    // Native gave E0562 "impl Trait not allowed in closure parameters". The param
    // is now `Rc<dyn Fn>` and the call site boxes the closure it passes. The inner
    // closure runs 3 times, each adding 10 to a captured var -> p=30.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let run3 = (g: () -> Unit) => { g(); g(); g() }\n\
         \x20 var p = 0\n\
         \x20 run3(() => { p = p + 10 })\n\
         \x20 println(\"p=\" + int.to_string(p))\n\
         }\n",
    );
}

#[test]
fn wasm_global_named_rust_keyword() {
    // A global named `box` (a Rust reserved word). The thread_local static is
    // declared `BOX` (raw name uppercased) but reads/writes used the keyword-
    // escaped `r#box`, whose uppercase `R#BOX` is invalid Rust ("unknown prefix").
    // Reads, writes, and the closure mutation now all route through the raw name.
    assert_cross_target_effect_main(
        "var box: List[Int] = []\n\
         effect fn main() -> Unit = {\n\
         \x20 let r = { run: () => { list.push(box, 42) } }\n\
         \x20 r.run()\n\
         \x20 r.run()\n\
         \x20 r.run()\n\
         \x20 println(int.to_string(list.len(box)))\n\
         }\n",
    );
}

#[test]
fn wasm_global_name_collides_with_stdlib_param() {
    // A mutable global `n: Int` collides by name with the `n` parameter of stdlib
    // numeric helpers (e.g. `int.to_int8_checked(n)`). The storage classifier's
    // by-name fallback misclassified that parameter as the global, so its body
    // read `N.with(...)` (i64) where an f64 was expected -> rustc E0308. The
    // fallback is now restricted to ALMIDE_RT_-prefixed cross-module names.
    assert_cross_target_effect_main(
        "var n: Int = 0\n\
         var xs: List[Int] = []\n\
         var s: String = \"\"\n\
         effect fn main() -> Unit = {\n\
         \x20 let step = () => { n = n + 1; list.push(xs, n); s = s + \"x\" }\n\
         \x20 step(); step(); step()\n\
         \x20 println(int.to_string(n))\n\
         \x20 println(int.to_string(list.len(xs)))\n\
         \x20 println(int.to_string(string.len(s)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_called_as_hof_lambda_param() {
    // A closure that arrives as a higher-order-function lambda PARAMETER, called
    // inside the lambda body: `list.fold(fns, 0, (acc, f) => acc + f(100))`. The
    // call `f(100)` kept the type `fn(Int) -> Int` instead of its return `Int`
    // (the param's concrete type is only fixed by the enclosing fold's
    // unification, after the body was checked), so `acc + f(100)` tripped the IR
    // verifier — a native ICE, and a structurally-broken WASM module. A Computed
    // call's result type is now the callee's Fn return. fns = [+1, *2, -3];
    // fns[0](10) = 11; fold over f(100) = 101 + 200 + 97 = 398.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let fns: List[(Int) -> Int] = [(x) => x + 1, (x) => x * 2, (x) => x - 3]\n\
         \x20 println(int.to_string(fns[0](10)))\n\
         \x20 let total = list.fold(fns, 0, (acc, f) => acc + f(100))\n\
         \x20 println(int.to_string(total))\n\
         }\n",
    );
}

#[test]
fn wasm_variant_payload_parametered_closure() {
    // A variant whose payload is a closure with a non-Unit signature
    // (`Thunk((Int) -> Int)`). The variant field rendered `impl Fn(i64) -> i64`
    // (E0562 in field types) for the parametered closure; it now renders
    // `Rc<dyn Fn>`. Thunk((x) => x*x), matched and called with 9 -> 81.
    assert_cross_target_effect_main(
        "type Node = Leaf(Int) | Thunk((Int) -> Int)\n\
         effect fn main() -> Unit = {\n\
         \x20 let n = Thunk((x) => x * x)\n\
         \x20 let r = match n { Leaf(v) => v, Thunk(f) => f(9) }\n\
         \x20 println(int.to_string(r))\n\
         }\n",
    );
}

#[test]
fn wasm_record_field_list_of_closures() {
    // A struct/record field whose type CONTAINS a closure nested in a container
    // (`stages: List[(Int) -> Int]`). The field type rendered `Vec<impl Fn>`
    // (E0562: impl Trait in field types); `render_type_field_fn` now recurses
    // into List/Map/Tuple, boxing every Fn to `Rc<dyn Fn>`. stages[1] = (x)=>x*3;
    // (stages[1])(10) = 30; len 3.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let r = { stages: [(x) => x + 2, (x) => x * 3], extra: 0 }\n\
         \x20 println(int.to_string((r.stages[1])(10)))\n\
         \x20 println(int.to_string(list.len(r.stages)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_returning_closure_in_list() {
    // Curried closures (`(Int) -> (Int) -> Int`) stored in a list: calling one
    // yields an inner closure, which is then called. The inner returned closure
    // rendered `Box<dyn Fn>` (not Clone → E0599 when cloned at the call site), and
    // its value lacked the trait-object cast (E0271). Nested returned closures are
    // now `Rc<dyn Fn>` with an explicit cast on both the type and the value sides.
    // make_add(10)(5) = 15; make_mul(3)(4) = 12.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let factories: List[(Int) -> (Int) -> Int] = [(n) => (x) => x + n, (n) => (x) => x * n]\n\
         \x20 let make_add = factories[0]\n\
         \x20 let make_mul = factories[1]\n\
         \x20 println(int.to_string((make_add(10))(5)))\n\
         \x20 println(int.to_string((make_mul(3))(4)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_map_from_hof_then_coalesce() {
    // Closures collected into a map by a HOF (`map.from_list(list.map(...))`),
    // then read with `??`. The map values were a CONCRETE closure type (the
    // boxing pass missed HOF-produced closures), so the `??` fallback (boxed)
    // couldn't unify → E0308. `list.map`'s mapper result is now boxed, so the map
    // is uniformly `Rc<dyn Fn>`. greeters["x"]() -> "hi-x".
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let names = [\"x\", \"z\"]\n\
         \x20 let greeters: Map[String, () -> String] = map.from_list(list.map(names, (nm) => (nm, () => \"hi-\" + nm)))\n\
         \x20 let gx = greeters[\"x\"] ?? (() => \"?\")\n\
         \x20 println(gx())\n\
         }\n",
    );
}

#[test]
fn wasm_closure_map_set_then_coalesce() {
    // A `Map[String, (Int) -> Int]` built with `map.set` (immutable update), read
    // with `map.get(...) ?? fallback`. The stored closures and the `??` fallback
    // are now both boxed to `Rc<dyn Fn>` (map.set value boxing + the get_or-as-
    // module-call default boxing), so they unify. add(10)=11, neg(7)=-7.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var m: Map[String, (Int) -> Int] = map.new()\n\
         \x20 m = map.set(m, \"add\", (x) => x + 1)\n\
         \x20 m = map.set(m, \"neg\", (x) => 0 - x)\n\
         \x20 let fa = map.get(m, \"add\") ?? ((x) => x)\n\
         \x20 let fn_ = map.get(m, \"neg\") ?? ((x) => x)\n\
         \x20 println(int.to_string(fa(10)))\n\
         \x20 println(int.to_string(fn_(7)))\n\
         }\n",
    );
}

// ── Cross-target program termination on an unhandled error ──
// `effect fn main` that fails must, on BOTH targets, exit non-zero and print
// `Error: <msg>` to stderr (native via the Display `fn main` wrapper, wasm via
// `__main_runner`'s tag check → fd_write(stderr) + proc_exit(1)).
// Spec: docs/specs/result-option-effect.md §4.


#[test]
fn unhandled_main_error_terminates_consistently() {
    if Command::new(almide_bin()).arg("--version").output().is_err() { return; }
    let src = "effect fn boom() -> Result[Int, String] = err(\"kaboom\")\n\
               effect fn main() -> Unit = {\n\
               \x20 let v = boom()!\n\
               \x20 println(int.to_string(v))\n\
               }\n";
    let (rc, _, rerr) = run_native_capture(src);
    assert_ne!(rc, 0, "native must exit non-zero on an unhandled error");
    assert_eq!(rerr, "Error: kaboom", "native stderr (Display, no Debug quotes)");
    if let Some((wc, _, werr)) = run_wasm_capture(src) {
        assert_ne!(wc, 0, "wasm must exit non-zero on an unhandled error");
        assert_eq!(rc, wc, "exit codes must match cross-target");
        assert_eq!(rerr, werr, "stderr must match cross-target byte-for-byte");
    }
}

#[test]
fn successful_main_exits_zero_both_targets() {
    if Command::new(almide_bin()).arg("--version").output().is_err() { return; }
    // Guards the wasm `__main_runner` tag check: an `Ok` main must NOT be misread
    // as an error and aborted — it must exit 0 with its normal output.
    let src = "effect fn good() -> Result[Int, String] = ok(7)\n\
               effect fn main() -> Unit = {\n\
               \x20 let v = good()!\n\
               \x20 println(int.to_string(v))\n\
               }\n";
    let (rc, rout, _) = run_native_capture(src);
    assert_eq!(rc, 0);
    assert_eq!(rout, "7");
    if let Some((wc, wout, _)) = run_wasm_capture(src) {
        assert_eq!(wc, 0, "an Ok main must exit 0 on wasm");
        assert_eq!(wout, "7");
    }
}

/// #644: an unreachable `local fn` that references a native-only matrix intrinsic
/// must NOT break the WASM build. The reachability prune drops the dead body
/// (and the CLI native-only pre-check, sharing the same reachability, ignores it)
/// so the build succeeds instead of ICEing/refusing. `local` is required because
/// Almide functions are PUBLIC by default — an exported root that is never pruned.
#[test]
fn dead_local_native_only_intrinsic_does_not_break_wasm_build() {
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("dead.almd");
    std::fs::write(
        &src,
        "local fn dead(h: Matrix, w: Bytes, g: List[Int]) -> Matrix = {\n  \
           let (out, _a, _b) = matrix.qwen3_block_q1_0_kv(h, h, h, w, g, g, 0, 4, 2, 8, 16, 10000.0, 0.00001)\n  \
           out\n}\n\
         fn main() -> Unit = println(\"ok\")\n",
    )
    .unwrap();
    let out_wasm = dir.path().join("dead.wasm");
    let r = Command::new(&bin)
        .args([
            "build",
            src.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            out_wasm.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        r.status.success() && out_wasm.exists(),
        "WASM build of a program with a dead native-only intrinsic should succeed (#644).\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
}

// ── #1394: the WASI dirfd is DISCOVERED, not assumed ────────────────────────
//
// The v1 fs floor used to pass `(i32.const 3)` as the dirfd at all 13 WASI
// path-call sites and make a guest-absolute path "fd-3-relative" by stripping
// its leading '/'. That is right only under a host that preopens EXACTLY ONE
// directory and that directory is `/` — which is what every other gate in this
// repo runs (`wasmtime --dir=/`), so nothing here could see the bug. These
// tests are the ONE place that varies the preopen set, because they drive
// wasmtime themselves with two `--dir=` flags. A `spec/wasm_cross` fixture
// cannot reach it: that harness's wasmtime invocation is fixed at one preopen,
// and the native oracle it differs against has no preopen concept at all.
//
// `$path_norm` now scans `fd_prestat_get`/`fd_prestat_dir_name` from fd 3 up
// (once, cached) and returns the LONGEST-PREFIX preopen fd plus the remainder.

/// The probe: write / exists / read-back the ABSOLUTE path given as argv[0].
/// Each line it prints goes through a different WASI path call, so a
/// mis-resolved dirfd shows up whichever syscall drops it.
const PREOPEN_PROBE_SRC: &str = "import fs\n\
     import env\n\
     \n\
     effect fn main() -> Unit = {\n\
     \x20 let p = env.args()[0]\n\
     \x20 match fs.write(p, \"PREOPEN-MULTI\") {\n\
     \x20   ok(_) => println(\"write=OK\"),\n\
     \x20   err(e) => println(\"write=ERR[\" + e + \"]\"),\n\
     \x20 }\n\
     \x20 println(if fs.exists(p) then \"exists=yes\" else \"exists=no\")\n\
     \x20 match fs.read_text(p) {\n\
     \x20   ok(s) => println(\"read=\" + s),\n\
     \x20   err(e) => println(\"read=ERR[\" + e + \"]\"),\n\
     \x20 }\n\
     }\n";

/// Build `PREOPEN_PROBE_SRC` to wasm in `dir`, or `None` when the toolchain is
/// unavailable (the same skip discipline as the rest of this file).
fn build_preopen_probe(dir: &Path) -> Option<std::path::PathBuf> {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return None;
    }
    if Command::new("wasmtime").arg("--version").output().is_err() {
        return None;
    }
    let src_path = dir.join("preopen_probe.almd");
    let wasm_path = dir.join("preopen_probe.wasm");
    std::fs::write(&src_path, PREOPEN_PROBE_SRC).unwrap();
    let build = Command::new(almide_bin())
        .args([
            "build",
            src_path.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            wasm_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to build the preopen probe");
    assert!(
        build.status.success(),
        "preopen probe wasm build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    Some(wasm_path)
}

/// Run `wasm` on wasmtime with the given preopens (IN ORDER — the first is fd 3)
/// and one guest path argument. Returns trimmed stdout.
fn run_with_preopens(wasm: &Path, dirs: &[String], guest_path: &str) -> String {
    let mut cmd = Command::new("wasmtime");
    for d in dirs {
        cmd.arg(format!("--dir={d}"));
    }
    let out = cmd
        .arg(wasm.to_str().unwrap())
        .arg(guest_path)
        .output()
        .expect("failed to run wasmtime");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn wasm_multi_preopen_resolves_to_the_right_root() {
    let dir = tempfile::tempdir().unwrap();
    let Some(wasm) = build_preopen_probe(dir.path()) else { return };
    // `tempdir()` can hand back a symlinked path (macOS `/var` -> `/private/var`);
    // canonicalize so the guest's absolute path is the one wasmtime resolves
    // through the `/` preopen, and so the decoy below is built from the same
    // spelling the guest will send.
    let sandbox = std::fs::canonicalize(dir.path()).unwrap();
    let first = sandbox.join("first");
    let second = sandbox.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();

    // ── Cell B: two preopens, `/` SECOND, and the stripped path does not exist
    // under the first. Pre-#1394 this was a loud `write=ERR[No such file or
    // directory (os error 2)]` — fd 3 was `first/`, which has no `<sandbox>`
    // subtree inside it.
    let target_b = second.join("b.txt");
    let out = run_with_preopens(
        &wasm,
        &[first.to_str().unwrap().to_string(), "/".to_string()],
        target_b.to_str().unwrap(),
    );
    assert_eq!(
        out, "write=OK\nexists=yes\nread=PREOPEN-MULTI",
        "with two preopens the guest must resolve an absolute path against the \
         `/` preopen (fd 4 here), not against whichever directory happens to be fd 3"
    );
    assert!(
        target_b.exists(),
        "the bytes must land at the host path the guest asked for ({})",
        target_b.display()
    );

    // ── Cell C: the SILENT one. Same two preopens, but the stripped path DOES
    // exist under `first/`, so the pre-#1394 floor answered `ok(())` and put the
    // bytes in a completely different host file — no error anywhere. Build that
    // decoy explicitly: `first/` + the guest path minus its leading '/'.
    let target_c = second.join("c.txt");
    let stripped = target_c.to_str().unwrap().trim_start_matches('/');
    let decoy = first.join(stripped);
    std::fs::create_dir_all(decoy.parent().unwrap()).unwrap();
    let out = run_with_preopens(
        &wasm,
        &[first.to_str().unwrap().to_string(), "/".to_string()],
        target_c.to_str().unwrap(),
    );
    assert_eq!(out, "write=OK\nexists=yes\nread=PREOPEN-MULTI");
    assert!(
        target_c.exists(),
        "the bytes must land at {} — the path the guest actually named",
        target_c.display()
    );
    assert!(
        !decoy.exists(),
        "SILENT WRONG FILE: the write landed at {} instead of the requested {}",
        decoy.display(),
        target_c.display()
    );

    // ── Control: ONE preopen, `/` — the configuration every other gate runs.
    // This arm must behave exactly as it did pre-#1394; if the fix changed
    // anything here it broke the whole suite, and this says so directly.
    let target_a = second.join("a.txt");
    let out = run_with_preopens(&wasm, &["/".to_string()], target_a.to_str().unwrap());
    assert_eq!(out, "write=OK\nexists=yes\nread=PREOPEN-MULTI");
    assert!(target_a.exists());
}

#[test]
fn wasm_mapped_preopen_matches_the_guest_prefix() {
    // The exact preopen shape `src/cli/run.rs::wasmtime_fs_args` builds on
    // Windows: `--dir=.`-style launcher cwd (fd 3) plus the host temp dir MAPPED
    // at the guest path `/tmp` (fd 4), with `TMPDIR=/tmp` steering `fs.temp_dir`
    // to produce `/tmp/...` paths. Pre-#1394 such a path stripped to `tmp/...`
    // and resolved against fd 3 — the mapping set up for it was never consulted.
    // Reproduced here on Unix, where the same two-preopen shape is expressible;
    // the Windows CI job never writes through `fs.temp_dir`, which is why this
    // went unreached.
    let dir = tempfile::tempdir().unwrap();
    let Some(wasm) = build_preopen_probe(dir.path()) else { return };
    let sandbox = std::fs::canonicalize(dir.path()).unwrap();
    let cwd = sandbox.join("cwd");
    let tmpmap = sandbox.join("tmpmap");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&tmpmap).unwrap();

    let out = run_with_preopens(
        &wasm,
        &[
            cwd.to_str().unwrap().to_string(),
            format!("{}::/tmp", tmpmap.to_str().unwrap()),
        ],
        "/tmp/mapped.txt",
    );
    assert_eq!(
        out, "write=OK\nexists=yes\nread=PREOPEN-MULTI",
        "`/tmp/...` must resolve against the preopen NAMED `/tmp`, whatever fd it got"
    );
    assert!(
        tmpmap.join("mapped.txt").exists(),
        "the bytes must land inside the directory mapped at /tmp"
    );
    assert!(
        !cwd.join("tmp").exists(),
        "nothing may be created under the fd-3 preopen: the path named /tmp, not ./tmp"
    );
}

#[test]
fn wasm_preopen_prefix_match_stops_at_a_component_boundary() {
    // The longest-prefix rule matches on '/'-separated COMPONENTS, so a preopen
    // named `<sandbox>/pre` must not claim `<sandbox>/prefix/...`. Both dirs
    // exist and `/` is preopened too, so a naive byte-prefix rule would silently
    // write into `pre/` — the same silent-wrong-file class as cell C, one layer
    // down, and the reason the match tests `path[len(name)] == '/'`.
    let dir = tempfile::tempdir().unwrap();
    let Some(wasm) = build_preopen_probe(dir.path()) else { return };
    let sandbox = std::fs::canonicalize(dir.path()).unwrap();
    let pre = sandbox.join("pre");
    let prefix = sandbox.join("prefix");
    std::fs::create_dir_all(&pre).unwrap();
    std::fs::create_dir_all(&prefix).unwrap();

    let target = prefix.join("f.txt");
    let out = run_with_preopens(
        &wasm,
        &[pre.to_str().unwrap().to_string(), "/".to_string()],
        target.to_str().unwrap(),
    );
    assert_eq!(out, "write=OK\nexists=yes\nread=PREOPEN-MULTI");
    assert!(target.exists(), "the bytes must land at {}", target.display());
    assert!(
        std::fs::read_dir(&pre).unwrap().next().is_none(),
        "the `<sandbox>/pre` preopen must not claim `<sandbox>/prefix/...`"
    );
}
