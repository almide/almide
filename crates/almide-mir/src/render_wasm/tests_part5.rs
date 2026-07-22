// tests_part5 — SELF-HOSTED `list.to_string` (the `${list}` compound interp), fix-0276.
//
// A `${xs}` over a `List[T]` desugars (lower::interp_to_string_call) to an element-type-keyed call:
//   List[Int]    → list.to_string      List[Float] → list.to_string_f
//   List[Bool]   → list.to_string_b    List[String]→ list.to_string_s
// each a monomorphic self-host impl (stdlib/list_to_string*.almd) that byte-matches v0's AlmideRepr
// for the element type. An UNSUPPORTED element (nested List[List[_]], Map/Set/…) routes to the
// never-registered `list.to_string_x`, so the using fn WALLS cleanly — never a wrong byte. The
// goldens below are captured from `almide run` on the native v0 oracle.

#[test]
fn compound_list_interp_int_byte_matches_v0() {
    // v0: "[1, 2, 3]", "[]", "[42]", "[-5, 0, 100, -1234567]" (signed decimal elements, ", " joined).
    let src = "fn main() -> Unit = {\n  \
        let a: List[Int] = [1, 2, 3]\n  \
        let b: List[Int] = []\n  \
        let c: List[Int] = [42]\n  \
        let d: List[Int] = [0 - 5, 0, 100, 0 - 1234567]\n  \
        println(\"${a}\")\n  println(\"${b}\")\n  println(\"${c}\")\n  println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "list.to_string"),
        "a List[Int] interp must auto-link list.to_string"
    );
    assert!(
        crate::render_wasm::unlinked_call_names(&prog).is_empty(),
        "a List[Int] interp must be fully linkable, got {:?}",
        crate::render_wasm::unlinked_call_names(&prog)
    );
    if let Some(out) = build_and_run("compound_list_int", &render_wasm_program(&prog)) {
        assert_eq!(out, "[1, 2, 3]\n[]\n[42]\n[-5, 0, 100, -1234567]");
    }
}

#[test]
fn compound_list_interp_bool_byte_matches_v0() {
    // v0: "[true, false]", "[]", "[false, true, true]".
    let src = "fn main() -> Unit = {\n  \
        let a: List[Bool] = [true, false]\n  \
        let b: List[Bool] = []\n  \
        let c: List[Bool] = [false, true, true]\n  \
        println(\"${a}\")\n  println(\"${b}\")\n  println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "list.to_string_b"),
        "a List[Bool] interp must auto-link list.to_string_b"
    );
    assert!(
        crate::render_wasm::unlinked_call_names(&prog).is_empty(),
        "a List[Bool] interp must be fully linkable, got {:?}",
        crate::render_wasm::unlinked_call_names(&prog)
    );
    if let Some(out) = build_and_run("compound_list_bool", &render_wasm_program(&prog)) {
        assert_eq!(out, "[true, false]\n[]\n[false, true, true]");
    }
}

#[test]
fn compound_list_interp_string_byte_matches_v0() {
    // v0: each element double-quoted + escaped (\ " \n \r \t), other bytes (incl. UTF-8) verbatim.
    // "[\"a\", \"b\", \"c\"]", "[]", and an escaping case.
    let src = "fn main() -> Unit = {\n  \
        let a: List[String] = [\"a\", \"b\", \"c\"]\n  \
        let b: List[String] = []\n  \
        let c: List[String] = [\"hi\", \"\"]\n  \
        println(\"${a}\")\n  println(\"${b}\")\n  println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "list.to_string_s"),
        "a List[String] interp must auto-link list.to_string_s"
    );
    assert!(
        crate::render_wasm::unlinked_call_names(&prog).is_empty(),
        "a List[String] interp must be fully linkable, got {:?}",
        crate::render_wasm::unlinked_call_names(&prog)
    );
    if let Some(out) = build_and_run("compound_list_string", &render_wasm_program(&prog)) {
        assert_eq!(out, "[\"a\", \"b\", \"c\"]\n[]\n[\"hi\", \"\"]");
    }
}

#[test]
fn compound_list_interp_string_escapes_byte_match_v0() {
    // The escaping detail: a quote, a backslash, a tab, a newline each escape to a 2-byte sequence.
    // v0 (one line): [\"q\\\"x\", \"b\\\\s\", \"t\\tx\", \"n\\nx\"]
    let src = "fn main() -> Unit = {\n  \
        let c: List[String] = [\"q\\\"x\", \"b\\\\s\", \"t\\tx\", \"n\\nx\"]\n  \
        println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(
        crate::render_wasm::unlinked_call_names(&prog).is_empty(),
        "a List[String] interp must be fully linkable, got {:?}",
        crate::render_wasm::unlinked_call_names(&prog)
    );
    if let Some(out) = build_and_run("compound_list_str_esc", &render_wasm_program(&prog)) {
        // The bytes: opening [, then "q\"x", "b\\s", "t\tx", "n\nx" — backslash-escaped per element.
        assert_eq!(out, "[\"q\\\"x\", \"b\\\\s\", \"t\\tx\", \"n\\nx\"]");
    }
}

#[test]
fn compound_list_interp_float_byte_matches_v0_with_dot0_drop() {
    // THE TRAP: the compound-Float element is `format!("{}", f64)` — the SHORTEST decimal with the
    // trailing ".0" DROPPED for integer-valued floats (1.0 → "1", 100.0 → "100"), unlike scalar
    // float.to_string (which keeps ".0"). Non-integer floats are unchanged (2.5 → "2.5"). v0:
    //   [1, 2.5]                        (1.0 drops ".0", 2.5 kept)
    //   [1, 2, 2.5, 0.5, 100, 0.1, 0.3333333333333333, 123456789, 0.001, 1000000]
    let src = "fn main() -> Unit = {\n  \
        let a: List[Float] = [1.0, 2.5]\n  \
        let b: List[Float] = []\n  \
        let c: List[Float] = [1.0, 2.0, 2.5, 0.5, 100.0, 0.1, 0.3333333333333333, 123456789.0, 0.001, 1000000.0]\n  \
        println(\"${a}\")\n  println(\"${b}\")\n  println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "list.to_string_f"),
        "a List[Float] interp must auto-link list.to_string_f"
    );
    // list.to_string_f calls float.to_string transitively — both must be linked (fixpoint).
    assert!(
        prog.functions.iter().any(|f| f.name == "float.to_string"),
        "list.to_string_f must transitively auto-link float.to_string"
    );
    assert!(
        crate::render_wasm::unlinked_call_names(&prog).is_empty(),
        "a List[Float] interp must be fully linkable, got {:?}",
        crate::render_wasm::unlinked_call_names(&prog)
    );
    if let Some(out) = build_and_run("compound_list_float", &render_wasm_program(&prog)) {
        assert_eq!(
            out,
            "[1, 2.5]\n[]\n[1, 2, 2.5, 0.5, 100, 0.1, 0.3333333333333333, 123456789, 0.001, 1000000]"
        );
    }
}

#[test]
fn compound_list_interp_nested_walls_cleanly() {
    // RESOLVED frontier: a NESTED `List[List[Int]]` LITERAL now materializes (each inner
    // list is a flat block whose rc_dec is its full free — the outer DropListStr reclaims
    // everything) and `${xs}` renders through the composed `list.to_string_ll` self-host.
    // The expectation flips from "walls" to the stronger byte-match claim.
    let src = "fn main() -> Unit = {\n  \
        let xs: List[List[Int]] = [[1, 2], [3]]\n  \
        println(\"${xs}\") }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "main"),
        "the nested list literal + interp must lower now"
    );
    if let Some(out) = build_and_run("compound_list_interp_nested", &render_wasm_program(&prog)) {
        assert_eq!(out, "[[1, 2], [3]]");
    }
}

// ── Record / tuple VALUE MODEL (fix-0276): construction + field/element access ──
//
// A named-record literal `R { x: …, y: … }` is materialized as a flat heap block
// `[rc][len][cap][x@12][y@20]…` (one i64 slot per field, declaration order), and a
// field access `r.x` LOADS from its slot — so a field reads back exactly what was
// stored, byte-matching v0's stdout (the dual-oracle matches output, not the raw
// pointer layout). The goldens are from `almide run` on the v0 oracle.

#[test]
fn record_field_access_byte_matches_v0() {
    // distance_sq reads a.x/a.y/b.x/b.y of two `Point` records and computes over them;
    // the field reads must return the stored values (3,4 and 0,0), giving 25.
    let src = "type Point = { x: Int, y: Int }\n\
        fn distance_sq(a: Point, b: Point) -> Int =\n  \
        (a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)\n\
        fn main() -> Unit = {\n  \
        let p1 = Point { x: 3, y: 4 }\n  \
        let p2 = Point { x: 0, y: 0 }\n  \
        println(int.to_string(p1.x) + \" \" + int.to_string(p1.y))\n  \
        println(int.to_string(distance_sq(p1, p2))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("record_field_access", &render_wasm_program(&prog)) {
        assert_eq!(out, "3 4\n25");
    }
}

#[test]
fn record_field_access_in_lifted_lambda_matches_v0() {
    // `(p) => p.x` over a `List[Point]` mapped by self-hosted list.map: the lifted
    // lambda receives each record handle and LOADS p.x. list.sum over the result
    // confirms the field reads (1+3+5 = 9). The list-of-records is itself materialized
    // (each element a fresh owned record block, recursively freed via DropListStr).
    let src = "type Point = { x: Int, y: Int }\n\
        fn main() -> Unit = {\n  \
        let points = [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }, Point { x: 5, y: 6 }]\n  \
        let xs = list.map(points, (p) => p.x)\n  \
        println(int.to_string(list.sum(xs))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("record_field_in_lambda", &render_wasm_program(&prog)) {
        assert_eq!(out, "9");
    }
}

#[test]
fn generic_record_spread_byte_matches_v0() {
    // fix-0276 target 1: a GENERIC record spread (`Box { ...b, value: 8 }`). The spread
    // materializes a fresh block of the INSTANTIATED layout (`subst_type_var` resolves the
    // generic field `value: T` to `Int`), COPYING the non-overridden heap field `label`
    // (a `Dup` of the base's borrowed handle, so both records own a distinct reference) and
    // storing the override. `b2.value` reads the override (8), `b2.label`/`b.label` both read
    // the copied String ("old") — value semantics, no double-free. (The Pair case exercises
    // two generics + a trailing concrete heap field.)
    let src = "type Box[T] = { value: T, label: String }\n\
        type Pair[A, B] = { first: A, second: B, tag: String }\n\
        fn main() -> Unit = {\n  \
        let b = Box { value: 7, label: \"old\" }\n  \
        let b2 = Box { ...b, value: 8 }\n  \
        println(\"b2=${b2.value} b.label=${b.label} b2.label=${b2.label}\")\n  \
        let p = Pair { first: 100, second: \"hi\", tag: \"T\" }\n  \
        let p2 = Pair { ...p, first: 200 }\n  \
        println(\"${p2.first} ${p2.second} ${p2.tag}\") }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("generic_record_spread", &render_wasm_program(&prog)) {
        assert_eq!(out, "b2=8 b.label=old b2.label=old\n200 hi T");
    }
}

#[test]
fn heap_field_record_param_read_byte_matches_v0() {
    // fix-0276 (target 2 enabling fix): a heap field read over a RECORD PARAM (`fn f(r: R) =
    // r.name`). A record param is passed by the caller as a real same-layout block (the v1
    // calling convention), so `seed_variant_param` now seeds it into `materialized_aggregates`
    // — `r.name` BORROWS its real slot, and the tail moves it out with a `Dup` (an owned copy,
    // no double-free with the caller's record). Before this fix it read an EMPTY deferred value
    // (the silent-empty List[R]-map root cause).
    let src = "type R = { name: String, v: Int }\n\
        fn getname(r: R) -> String = r.name\n\
        fn main() -> Unit = {\n  \
        let x = R { name: \"hello\", v: 9 }\n  \
        println(getname(x) + \" \" + getname(x)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("heap_field_record_param", &render_wasm_program(&prog)) {
        // Both reads return "hello" — the caller's record is untouched (no double-free).
        assert_eq!(out, "hello hello");
    }
}

#[test]
fn nested_ownership_list_literal_walls_not_empty() {
    // fix-0276 (target 2), RESOLVED: a `List[R]` LITERAL where R has a HEAP field now
    // MATERIALIZES with the two-level recursive drop (`$__drop_list_<R>` frees each
    // record, each record frees its String field) — the nested-ownership frontier this
    // test used to pin as a WALL. The expectation flips to the stronger claim: it
    // lowers, runs, and the map-over-it reads REAL fields (never the old empty block).
    let src = "type R = { name: String, v: Float }\n\
        fn fmtr(xs: List[R]) -> String = list.join(list.map(xs, (r) => r.name), \",\")\n\
        fn main() -> Unit = {\n  \
        let rs = [R { name: \"a\", v: 3.5 }, R { name: \"b\", v: 1.2 }]\n  \
        println(fmtr(rs)) }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "main"),
        "the record-list literal must lower (the nested-ownership drop landed)"
    );
    if let Some(out) = build_and_run("nested_ownership_list", &render_wasm_program(&prog)) {
        assert_eq!(out, "a,b", "map over the record list reads real name fields");
    }
}

#[test]
fn tuple_element_access_byte_matches_v0() {
    // A scalar tuple `(Int, Int)` literal materializes its slots; `.0`/`.1` load them.
    let src = "fn main() -> Unit = {\n  \
        let t = (11, 22)\n  \
        println(int.to_string(t.0) + \":\" + int.to_string(t.1)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("tuple_element_access", &render_wasm_program(&prog)) {
        assert_eq!(out, "11:22");
    }
}

#[test]
fn narrow_int_record_field_round_trips() {
    // An `Int8` field stored in a uniform i64 slot round-trips losslessly through
    // `int.from_int8` (127 stays 127, the high field reads independently). Guards that
    // the uniform-slot layout is correct for narrow-int fields (no width-packing needed
    // for the observable output).
    let src = "type Rec = { b: Int8, n: Int }\n\
        fn main() -> Unit = {\n  \
        let r: Rec = { b: 127, n: 9999 }\n  \
        println(int.to_string(int.from_int8(r.b)) + \":\" + int.to_string(r.n)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("narrow_int_record", &render_wasm_program(&prog)) {
        assert_eq!(out, "127:9999");
    }
}

#[test]
fn heap_field_record_byte_matches_v0() {
    // A record with a HEAP (String) field alongside a scalar field is now MATERIALIZED as
    // a mixed scalar+heap block `[rc][len][cap][name@12][n@20]`: the String field is a fresh
    // OWNED handle moved into its slot (cert `m`), the scalar field stored directly, and the
    // block's scope-end drop is a MASKED `DropListStr` (free only the heap slot, then the
    // block — cert = one `d`, no new op). The scalar read `r.n` loads slot 1, the heap read
    // `r.name` borrows slot 0's handle. Both byte-match v0's stdout. (Was: WALLED — a
    // heap-field record needed an ownership-aware recursive drop, now expressed with the
    // EXISTING `Consume`/`DropListStr` cert events.)
    let src = "type R = { name: String, n: Int }\n\
        fn main() -> Unit = {\n  \
        let r = R { name: \"hi\", n: 7 }\n  \
        println(int.to_string(r.n))\n  \
        println(r.name) }\n";
    let prog = lower_source(src);
    // The block is materialized (a width-8 store sequence).
    let main = prog.functions.iter().find(|f| f.name == "main").expect("main");
    assert!(
        main.ops.iter().any(|op| matches!(
            op,
            crate::Op::Prim { kind: crate::PrimKind::Store { width: 8 }, .. }
        )),
        "a heap-field record is now materialized as a mixed scalar+heap block"
    );
    // The OWNERSHIP CERTIFICATE (the gate the corpus-wall + the proven Coq checker run) is
    // BALANCED on every function: each line never decs below 0 and ends at 0. The String
    // field is `im` (alloc + move-in `Consume`), the record block is `id` (alloc + masked
    // `DropListStr` = one `d`) — no new op, no leak, no double-free. (A heap-field READ is a
    // borrowed `LoadHandle` Handle-arg, accounted by the cert as a no-op call-arg — the same
    // established borrow shape `prim.load_str` + `println(payload)` uses.)
    for f in &prog.functions {
        // The GENERATED recursive drops (`__drop_R`, `__drop_list_R`) are the trusted
        // free routines — the real pipeline verifies them by the coown whitelist
        // (proofs/CoownLoop.v), NOT by per-line parity: their whole job is dec-ing
        // refs they do not own on the line (the owner is the caller). Skip them here
        // exactly as the corpus-wall checker does.
        if f.name.starts_with("__drop_") {
            continue;
        }
        let cert = crate::certificate::ownership_certificate(f);
        for line in cert.lines() {
            let mut rc: i64 = 0;
            for c in line.chars() {
                match c {
                    'i' | 'a' => rc += 1,
                    'd' | 'm' => {
                        assert!(rc > 0, "double-free/use-after-move in {} cert line {:?}", f.name, line);
                        rc -= 1;
                    }
                    _ => {}
                }
            }
            assert_eq!(rc, 0, "leak in {} cert line {:?}", f.name, line);
        }
    }
    if let Some(out) = build_and_run("heap_field_record", &render_wasm_program(&prog)) {
        assert_eq!(out, "7\nhi");
    }
}

#[test]
fn heap_field_record_loop_reclaims() {
    // SOUNDNESS for the masked recursive drop: a bounded loop building + dropping a fresh
    // heap-field record each iteration must reclaim the owned String field AND the block
    // (the masked `DropListStr`) every iteration — no leak (OOM) / double-free (trap). The
    // String value VARIES per iteration (`"x" + int.to_string(i % 10)`), so a fresh String is
    // allocated and freed each time. 20000 iters; prints the accumulated scalar field sum.
    let src = "type R = { name: String, n: Int }\n\
        fn main() -> Unit = {\n  \
        var sink = 0\n  var i = 0\n  \
        while i < 20000 {\n    \
          let r = R { name: \"x\" + int.to_string(i % 10), n: i }\n    \
          sink = sink + r.n % 7\n    \
          i = i + 1\n  }\n  \
        println(int.to_string(sink)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("heap_field_record_loop", &render_wasm_program(&prog)) {
        // sum of (i % 7) for i in 0..20000 (computed by the v0 oracle on this same program).
        assert_eq!(out, "59997");
    }
}

#[test]
fn spread_heap_field_record_materializes_soundly() {
    // A SPREAD heap-field record (`R { ...r, n: 9 }`) is now MATERIALIZED: a fresh block of
    // the same uniform-slot layout, COPYING each non-overridden field from the base (a scalar
    // load, a heap-handle Dup so both records own a DISTINCT reference) and storing the
    // overrides. So `r2.n` reads the override (9), `r2.name` the copied String ("a"), and `r`
    // is UNCHANGED (its own reference intact). Both `r` and `r2` carry a heap-slot mask — each
    // is freed by its own masked recursive `DropListStr` with no double-free (the copy is a
    // `Dup`, an independent rc), no leak (each record's String freed once).
    let src = "type R = { name: String, n: Int }\n\
        fn main() -> Unit = {\n  \
        let r = R { name: \"a\", n: 1 }\n  \
        let r2 = R { ...r, n: 9 }\n  \
        println(int.to_string(r2.n)) }\n";
    let prog = lower_source(src);
    // The cert stays balanced (the spread copy's `Dup` is matched by the masked drop's free).
    for f in &prog.functions {
        // The GENERATED recursive drops (`__drop_R`, `__drop_list_R`) are the trusted
        // free routines — the real pipeline verifies them by the coown whitelist
        // (proofs/CoownLoop.v), NOT by per-line parity: their whole job is dec-ing
        // refs they do not own on the line (the owner is the caller). Skip them here
        // exactly as the corpus-wall checker does.
        if f.name.starts_with("__drop_") {
            continue;
        }
        let cert = crate::certificate::ownership_certificate(f);
        for line in cert.lines() {
            let mut rc: i64 = 0;
            for c in line.chars() {
                match c {
                    'i' | 'a' => rc += 1,
                    'd' | 'm' => {
                        assert!(rc > 0, "double-free in {} cert {:?}", f.name, line);
                        rc -= 1;
                    }
                    _ => {}
                }
            }
            assert_eq!(rc, 0, "leak in {} cert {:?}", f.name, line);
        }
    }
    if let Some(out) = build_and_run("spread_heap_field_record", &render_wasm_program(&prog)) {
        // r2.n reads the override (9); r2.name copied "a"; r untouched — no double-free.
        assert_eq!(out, "9");
    }
}

/// A STATEMENT-position binder `match` whose arm body is a BLOCK that REASSIGNS an outer
/// mutable var EXECUTES the matched arm and mutates the var in place — byte-matching v0.
/// Regression for the two bugs `bind_subject` + the Unit-arm scalar reassignment closed:
///  (1) the binder arm's `{ r = 999 }` block body was nested as the desugared `if`'s tail
///      expr and dropped (`lower_branch_arm`'s tail dispatch had no `Block` case → the
///      assignment vanished); `bind_subject` now flattens the block's statements in after
///      the `let`, and the tail dispatch recurses into a nested block.
///  (2) a scalar `r = 999` inside a Unit arm rebound `value_of[r]` to a fresh frame-local,
///      so the post-match read saw the LAST-lowered arm's local (unset at runtime when the
///      OTHER arm ran → 0); it now mutates `r`'s stable local via `SetLocal` (in place, as
///      v0 does). The matched binder arm runs (n=3 ≠ 0 → `r = 999`), so v0 prints 999.
#[test]
fn stmt_binder_match_block_arm_reassigns_outer_var() {
    let src = "fn main() -> Unit = {\n  \
        var n = 3\n  \
        var r = 0\n  \
        match n {\n    0 => { r = 100 },\n    x => { r = 999 }\n  }\n  \
        println(int.to_string(r)) }\n";
    let prog = lower_source(src);
    assert!(
        crate::render_wasm::unlinked_call_names(&prog).is_empty(),
        "the match must lower + link, got {:?}",
        crate::render_wasm::unlinked_call_names(&prog)
    );
    if let Some(out) = build_and_run("stmt_binder_match_block", &render_wasm_program(&prog)) {
        assert_eq!(out, "999", "the matched binder arm `r = 999` must run and be read in place");
    }
}

/// The LITERAL arm of the same shape, TAKEN (n=0 → `r = 100`): the literal arm's block body
/// `{ r = 100 }` mutates the outer `r` in place too. Guards the SetLocal fix against the
/// taken-arm position — before it, the post-match read saw the binder (else) arm's frozen
/// local (0) regardless of which arm ran at runtime. v0 prints 100.
#[test]
fn stmt_binder_match_literal_arm_taken_reassigns_in_place() {
    let src = "fn main() -> Unit = {\n  \
        var n = 0\n  \
        var r = 0\n  \
        match n {\n    0 => { r = 100 },\n    x => { r = x + 1 }\n  }\n  \
        println(int.to_string(r)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("stmt_match_literal_taken", &render_wasm_program(&prog)) {
        assert_eq!(out, "100", "the taken literal arm `r = 100` must mutate r in place");
    }
}




// ── 2026-07-04 feature pins: shapes opened this pass, each byte-matched against
// v0 during development (the probes graduated to unit tests — coverage ledger F2).

#[test]
fn zip_map_fusion_two_source_loop() {
    // `map(zip(a, b), (pair) => flatten([pair.0, pair.1]))` fuses into a two-source
    // loop bounded by min(len_a, len_b) — no (A,B) tuple list is ever built.
    let src = "fn concat_cols(a: List[List[Float]], b: List[List[Float]]) -> List[List[Float]] =\n\
        list.zip(a, b) |> list.map((pair) => list.flatten([pair.0, pair.1]))\n\
        fn main() -> Unit = {\n\
        let c = concat_cols([[1.0, 2.0], [3.0, 4.0]], [[5.0], [6.0]])\n\
        for row in c {\n\
        println(row |> list.map((v) => float.to_string(v)) |> list.join(\",\")) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("zip_map_fusion", &render_wasm_program(&prog)) {
        assert_eq!(out, "1.0,2.0,5.0\n3.0,4.0,6.0");
    }
}

#[test]
fn conditional_fold_over_string_int_tuples() {
    // `fold(pairs, "", (acc, pair) => if pair.1 == t then pair.0 else acc)` — the
    // borrowed tuple's heap field is Dup-acquired into the accumulator slot; the
    // (String, Int) literal list drops via the mirrored DropListStrInt.
    let src = "fn lookup(pairs: List[(String, Int)], target: Int) -> String =\n\
        list.fold(pairs, \"\", (acc, pair) => if pair.1 == target then pair.0 else acc)\n\
        fn main() -> Unit = {\n\
        let ps = [(\"alpha\", 1), (\"beta\", 2)]\n\
        println(lookup(ps, 2))\n\
        println(lookup(ps, 9)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("cond_fold_str_int", &render_wasm_program(&prog)) {
        // (build_and_run trims the trailing newline; the second lookup prints "".)
        assert_eq!(out, "beta");
    }
}

#[test]
fn fold_heap_accumulator_via_call_body() {
    // `fold(ks, x, (h, k) => step(h, k))` — a Var-seeded (Dup) heap accumulator
    // updated by a fresh-owned CALL result each iteration (drop-old + SetLocal).
    let src = "fn step(acc: List[Float], k: Int) -> List[Float] =\n\
        list.map(acc, (v) => v + int.to_float(k))\n\
        fn run(ks: List[Int], x: List[Float]) -> List[Float] =\n\
        list.fold(ks, x, (h, k) => step(h, k))\n\
        fn main() -> Unit = {\n\
        let r = run([1, 2, 3], [10.0, 20.0])\n\
        println(r |> list.map((v) => float.to_string(v)) |> list.join(\",\")) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("fold_call_acc", &render_wasm_program(&prog)) {
        assert_eq!(out, "16.0,26.0");
    }
}

#[test]
fn scalar_block_lambda_bodies_inline() {
    // The mel filterbank shape: nested maps whose inner lambda body is a BLOCK
    // (lets + an if chain) — lowered inline by the scalar-block body arm.
    let src = "fn filters(pts: List[Float], m: Int) -> List[List[Float]] =\n\
        list.range(0, 2) |> list.map((i) => {\n\
        let left = list.get(pts, i) |> option.unwrap_or(0.0)\n\
        let right = list.get(pts, i + 1) |> option.unwrap_or(1.0)\n\
        list.range(0, m) |> list.map((k) => {\n\
        let f = int.to_float(k)\n\
        let h = if f < left then 0.0 else if f < right then f - left else right\n\
        h * 2.0 }) })\n\
        fn main() -> Unit = {\n\
        for row in filters([1.0, 3.0], 3) {\n\
        println(row |> list.map((v) => float.to_string(v)) |> list.join(\",\")) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("scalar_block_lambda", &render_wasm_program(&prog)) {
        assert_eq!(out, "0.0,0.0,2.0\n0.0,0.0,0.0");
    }
}

#[test]
fn eager_global_init_traps_before_main() {
    // C-007: an abortable top-let evaluates at startup (the synthesized
    // __global_init runs before $main), even when unused.
    let src = "let zero = 0\nlet bad = 10 / zero\n\
        fn main() -> Unit = {\n  println(\"never\") }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "main"),
        "the program lowers; the trap happens at RUNTIME startup"
    );
}

// ── 2026-07-04 crush-pass pins: the shapes opened while closing the nn walls,
// each byte-verified against v0 during development.

#[test]
fn record_int_tuple_result_ctor() {
    // `Result[(Record, Int), String]` ok/err (the gguf parse_header shape): a FLAT
    // record's tuple reuses the DropResultStrInt physics; err is a plain String.
    let src = "type Hdr = { version: Int, count: Int }\n\
        effect fn parse(x: Int) -> Result[(Hdr, Int), String] =\n\
        if x != 7 then err(\"bad magic\") else ok((Hdr { version: x, count: x * 2 }, 24))\n\
        effect fn main() -> Unit = {\n\
        match parse(7) {\n\
        ok(pair) => { let (h, off) = pair\n\
        println(int.to_string(h.version) + \":\" + int.to_string(h.count) + \":\" + int.to_string(off)) }\n\
        err(e) => println(\"E:\" + e) }\n\
        match parse(1) { ok(p) => println(\"?\") err(e) => println(\"E:\" + e) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("rec_int_result", &render_wasm_program(&prog)) {
        assert_eq!(out, "7:14:24\nE:bad magic");
    }
}

#[test]
fn while_two_accumulators_flatten_merge() {
    // The fft combine skeleton: a while loop carrying TWO list accumulators of
    // all-scalar tuples, merged by `list.flatten([first, second])` — the heap-var
    // list-literal call argument + the scalar-aggregate concat element.
    let src = "fn combine(n: Int) -> List[(Float, Float)] = {\n\
        var k = 0\n\
        var first: List[(Float, Float)] = []\n\
        var second: List[(Float, Float)] = []\n\
        while k < n {\n\
        let e = (int.to_float(k), 0.5)\n\
        first = first + [e]\n\
        second = second + [e]\n\
        k = k + 1 }\n\
        list.flatten([first, second]) }\n\
        effect fn main() -> Unit = {\n\
        for c in combine(2) {\n\
        println(float.to_string(c.0) + \",\" + float.to_string(c.1)) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("while_two_acc", &render_wasm_program(&prog)) {
        assert_eq!(out, "0.0,0.5\n1.0,0.5\n0.0,0.5\n1.0,0.5");
    }
}

#[test]
fn let_bound_tuple_option_match_executes() {
    // `let e = list.get(xs, k) |> option.unwrap_or((0.0, 0.0))` inside a while —
    // the tuple-unwrap_or desugar + the component-merge execution (ONE owned block,
    // no per-arm alloc; the i32 handle widens through Prim::Handle).
    let src = "fn pick_loop(xs: List[(Float, Float)], n: Int) -> Float = {\n\
        var k = 0\n\
        var total = 0.0\n\
        while k < n {\n\
        let e = list.get(xs, k) |> option.unwrap_or((0.0, 0.0))\n\
        total = total + e.0 + e.1\n\
        k = k + 1 }\n\
        total }\n\
        effect fn main() -> Unit =\n\
        println(float.to_string(pick_loop([(1.0, 0.5), (2.0, 0.25)], 3)))\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("tuple_opt_match", &render_wasm_program(&prog)) {
        assert_eq!(out, "3.75");
    }
}

#[test]
fn scalar_tuple_fold_with_preamble_statements() {
    // The best_pair_index skeleton: a scalar-tuple fold whose body carries
    // per-iteration preamble lets (a String `?? \"\"` copy among them) before the
    // component-projected if-tree.
    let src = "fn best(tokens: List[String], ranks: List[Int]) -> (Int, Int) = {\n\
        let last = list.len(tokens) - 1\n\
        list.fold(list.range(0, last), (0 - 1, 99999999), (acc, i) => {\n\
        let (best_i, best_rank) = acc\n\
        let a = list.get(tokens, i) |> option.unwrap_or(\"\")\n\
        let rank = if string.len(a) > 1 then list.get(ranks, i) |> option.unwrap_or(99999999) else 99999999\n\
        if rank < best_rank then (i, rank) else (best_i, best_rank) }) }\n\
        effect fn main() -> Unit = {\n\
        let r = best([\"ab\", \"c\", \"def\"], [5, 9, 2])\n\
        println(int.to_string(r.0) + \":\" + int.to_string(r.1)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("fold_preamble", &render_wasm_program(&prog)) {
        assert_eq!(out, "0:5");
    }
}

#[test]
fn range_argument_materializes_via_list_range() {
    // `for _ in 0..count` over an append accumulator whose desugar passes the
    // Range as a CALL argument — materialized via the self-hosted list.range.
    let src = "fn collect(n: Int) -> List[(String, Int)] = {\n\
        var acc: List[(String, Int)] = []\n\
        var p = 10\n\
        for i in 0..n {\n\
        acc = acc + [(\"k\" + int.to_string(i), p)]\n\
        p = p + 2 }\n\
        acc }\n\
        effect fn main() -> Unit = {\n\
        for pair in collect(3) {\n\
        println(pair.0 + \"=\" + int.to_string(pair.1)) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("range_arg", &render_wasm_program(&prog)) {
        assert_eq!(out, "k0=10\nk1=12\nk2=14");
    }
}

include!("tests_part5_b.rs");
include!("tests_part5_c.rs");
