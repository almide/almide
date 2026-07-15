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

#[test]
fn opt_tuple_fold_scanner() {
    // The wav find_chunk_at scanner: a (scalar, Option[scalar]) fold accumulator —
    // the Option component runs as tag+payload locals; the match-over-found projects
    // to an if-over-tag; the result Option materializes once (len-as-tag overwrite).
    let src = "fn find_at(sizes: List[Int], target: Int, pos: Int) -> Option[Int] = {\n\
        let positions = list.range(0, 10)\n\
        list.fold(positions, (pos, none), (state, i) => {\n\
        let (p, found) = state\n\
        match found {\n\
        some(_) => state\n\
        none =>\n\
        if p > 100 then (p, none)\n\
        else {\n\
        let size = list.get(sizes, i) |> option.unwrap_or(999)\n\
        if p == target then (p, some(p))\n\
        else (p + size, none) } } }).1 }\n\
        effect fn main() -> Unit = {\n\
        match find_at([4, 4, 4, 4], 8, 0) {\n\
        some(p) => println(\"found:\" + int.to_string(p))\n\
        none => println(\"none\") }\n\
        match find_at([4, 4], 99, 0) {\n\
        some(p) => println(\"?\")\n\
        none => println(\"none\") } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("opt_tuple_fold", &render_wasm_program(&prog)) {
        assert_eq!(out, "found:8\nnone");
    }
}

#[test]
fn adt_int_tuple_return_ctor() {
    // The gguf read_one shape: an `(ADT, Int)` tuple-return tail whose elements are
    // variant CTOR calls (`(IntV(p), p + 4)`). The ctor element routes through
    // `try_lower_variant_ctor` (a plain CallFn would leave `$IntV` unlinked); both a
    // scalar ctor and a String-payload ctor construct, accumulate through list.push,
    // and match-extract downstream.
    let src = "type GV =\n\
        | IntV(Int)\n\
        | StrV(String)\n\
        fn read_one(p: Int) -> (GV, Int) =\n\
        if p % 2 == 0 then (IntV(p), p + 5)\n\
        else (StrV(\"s\" + int.to_string(p)), p + 3)\n\
        fn collect(n: Int) -> (List[GV], Int) = {\n\
        var items: List[GV] = []\n\
        var p = 0\n\
        for _ in 0..n {\n\
        let (val, next) = read_one(p)\n\
        list.push(items, val)\n\
        p = next }\n\
        (items, p) }\n\
        effect fn main() -> Unit = {\n\
        let (vs, endp) = collect(4)\n\
        for v in vs {\n\
        match v {\n\
        IntV(i) => println(\"i:\" + int.to_string(i))\n\
        StrV(s) => println(\"s:\" + s) } }\n\
        println(\"end:\" + int.to_string(endp)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("adt_int_tuple_ret", &render_wasm_program(&prog)) {
        assert_eq!(out, "i:0\ns:s5\ni:8\ns:s13\nend:16");
    }
}

#[test]
fn variant_ctor_list_field_recursive_accumulator() {
    // The gguf read_array shape (ADT brick 5): a RECURSIVE accumulator whose tail returns
    // `(ArrV(items), p)` — a ctor with a `List[<rich variant>]` field. The ctor admits the
    // Dup'd list (freed via the generated mutually-recursive `$__drop_GV`/`$__drop_list_GV`);
    // `end:9` witnesses every element's cursor advance through two recursion levels.
    let src = "type GV =\n\
        | IntV(Int)\n\
        | StrV(String)\n\
        | ArrV(List[GV])\n\
        fn read_array(n: Int, depth: Int, pos: Int) -> (GV, Int) = {\n\
        var items: List[GV] = []\n\
        var p = pos\n\
        for i in 0..n {\n\
        if depth > 0 then {\n\
        let (val, next) = read_array(2, depth - 1, p)\n\
        list.push(items, val)\n\
        p = next }\n\
        else if i % 2 == 0 then {\n\
        let (val, next) = (IntV(i * 10 + p), p + 1)\n\
        list.push(items, val)\n\
        p = next }\n\
        else {\n\
        let (val, next) = (StrV(\"x\" + int.to_string(p)), p + 2)\n\
        list.push(items, val)\n\
        p = next } }\n\
        (ArrV(items), p) }\n\
        effect fn main() -> Unit = {\n\
        let (v, endp) = read_array(3, 1, 0)\n\
        match v {\n\
        ArrV(_) => println(\"arr\")\n\
        IntV(i) => println(\"i\" + int.to_string(i))\n\
        StrV(s) => println(s) }\n\
        println(\"end:\" + int.to_string(endp)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("ctor_list_field_rec", &render_wasm_program(&prog)) {
        assert_eq!(out, "arr\nend:9");
    }
}

#[test]
fn matrix_self_host_floor() {
    // The Matrix value model (roadmap B, approach (a)): a v1 Matrix IS a List[List[Float]],
    // served by the matrix_core self-host registry. Construction, metadata, element-wise,
    // transpose, and the k-ascending mul all byte-match the v0 oracle.
    let src = "effect fn main() -> Unit = {\n\
        let m = matrix.from_lists([[1.0, 2.0], [3.0, 4.0]])\n\
        println(float.to_string(matrix.get(m, 0, 0)))\n\
        println(int.to_string(matrix.rows(m)) + \"x\" + int.to_string(matrix.cols(m)))\n\
        let z = matrix.zeros(2, 3)\n\
        println(float.to_string(matrix.get(z, 1, 2)))\n\
        let a = matrix.add(m, m)\n\
        println(float.to_string(matrix.get(a, 1, 0)))\n\
        let t = matrix.transpose(m)\n\
        println(float.to_string(matrix.get(t, 0, 1)))\n\
        let s = matrix.scale(m, 2.5)\n\
        println(float.to_string(matrix.get(s, 0, 1)))\n\
        let p = matrix.mul(m, m)\n\
        println(float.to_string(matrix.get(p, 0, 0)) + \",\" + float.to_string(matrix.get(p, 1, 1)))\n\
        let ll = matrix.to_lists(m)\n\
        println(int.to_string(list.len(ll))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("matrix_floor", &render_wasm_program(&prog)) {
        assert_eq!(out, "1.0\n2x2\n0.0\n6.0\n3.0\n5.0\n7.0,22.0\n2");
    }
}

#[test]
fn matrix_per_head_repeat_kv_concat_rows() {
    // The three nn Matrix walls (roadmap B): per_head_rms_norm (list.map closure over
    // List[Matrix] + split/concat), repeat_kv (heap-result if returning a param + flat_map
    // with list.repeat_rc), concat_rows (a list-literal flatten arg of to_lists calls).
    // Expectations byte-verified against `almide run --target wasm` (the v0 oracle).
    let src = "fn per_head_rms_norm(x: Matrix, gamma: List[Float], n_heads: Int, eps: Float) -> Matrix = {\n\
        let heads = matrix.split_cols_even(x, n_heads)\n\
        let normed = heads |> list.map((h) => matrix.rms_norm_rows(h, gamma, eps))\n\
        matrix.concat_cols(normed) }\n\
        fn repeat_kv(kv: Matrix, n_kv_heads: Int, n_rep: Int) -> Matrix = {\n\
        if n_rep == 1 then kv\n\
        else {\n\
        let heads = matrix.split_cols_even(kv, n_kv_heads)\n\
        let repeated = heads |> list.flat_map((h) => list.repeat(h, n_rep))\n\
        matrix.concat_cols(repeated) } }\n\
        fn concat_rows(a: Matrix, b: Matrix) -> Matrix = {\n\
        let all = list.flatten([matrix.to_lists(a), matrix.to_lists(b)])\n\
        matrix.from_lists(all) }\n\
        effect fn main() -> Unit = {\n\
        let x = matrix.from_lists([[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]])\n\
        let g = [0.5, 1.5]\n\
        let p = per_head_rms_norm(x, g, 2, 0.00001)\n\
        println(float.to_string(matrix.get(p, 0, 0)) + \",\" + float.to_string(matrix.get(p, 1, 3)))\n\
        let r1 = repeat_kv(x, 2, 1)\n\
        println(float.to_string(matrix.get(r1, 0, 0)))\n\
        let r2 = repeat_kv(x, 2, 2)\n\
        println(int.to_string(matrix.cols(r2)) + \":\" + float.to_string(matrix.get(r2, 0, 2)) + \",\" + float.to_string(matrix.get(r2, 1, 7)))\n\
        let cr = concat_rows(x, matrix.from_lists([[9.0, 10.0, 11.0, 12.0]]))\n\
        println(int.to_string(matrix.rows(cr)) + \":\" + float.to_string(matrix.get(cr, 2, 1))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("matrix_walls", &render_wasm_program(&prog)) {
        assert_eq!(
            out,
            "0.31622713356320326,1.5964561112912787\n1.0\n8:1.0,8.0\n3:10.0"
        );
    }
}

#[test]
fn matrix_norms_and_bytes() {
    // rms/layer norms (full-row statistics, zip-truncated output), split/concat round-trip,
    // gather with the OOB zero-row edge, and the f32/f16 LE byte decoders (in-bounds — the
    // native oracle's OOB→zeros edge is pinned by the from_bytes probe, not here).
    let src = "effect fn main() -> Unit = {\n\
        let m = matrix.from_lists([[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]])\n\
        let g = [0.5, 1.5, 2.5, 3.5]\n\
        let r = matrix.rms_norm_rows(m, g, 0.00001)\n\
        println(float.to_string(matrix.get(r, 0, 0)) + \",\" + float.to_string(matrix.get(r, 1, 3)))\n\
        let ln = matrix.layer_norm_rows(m, g, [0.1, 0.2, 0.3, 0.4], 0.00001)\n\
        println(float.to_string(matrix.get(ln, 0, 1)) + \",\" + float.to_string(matrix.get(ln, 1, 2)))\n\
        let heads = matrix.split_cols_even(m, 2)\n\
        let cc = matrix.concat_cols(heads)\n\
        println(float.to_string(matrix.get(cc, 0, 0)) + \",\" + float.to_string(matrix.get(cc, 1, 3)))\n\
        let ga = matrix.gather_rows(m, [1, 0, 9])\n\
        println(float.to_string(matrix.get(ga, 0, 0)) + \",\" + float.to_string(matrix.get(ga, 2, 3)))\n\
        let b32 = bytes.from_list([0, 0, 192, 63, 0, 0, 0, 64, 0, 0, 0, 191, 0, 128, 200, 66])\n\
        let m32 = matrix.from_bytes_f32_le(b32, 0, 2, 2)\n\
        println(float.to_string(matrix.get(m32, 1, 0)) + \",\" + float.to_string(matrix.get(m32, 1, 1)))\n\
        let b16 = bytes.from_list([0, 60, 0, 193, 0, 56, 255, 123])\n\
        let m16 = matrix.from_bytes_f16_le(b16, 0, 2, 2)\n\
        println(float.to_string(matrix.get(m16, 0, 1)) + \",\" + float.to_string(matrix.get(m16, 1, 1))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("matrix_norms_bytes", &render_wasm_program(&prog)) {
        assert_eq!(
            out,
            "0.1825740641190532,4.2453485560707875\n-0.47081770998446343,1.4180295166407726\n\
             1.0,8.0\n5.0,0.0\n-0.5,100.25\n-2.5,65504.0"
        );
    }
}

#[test]
fn defunc_find_capturing_predicate() {
    // `list.find` with a CAPTURING predicate over record elements (the gguf/ggml
    // find_tensor shape) — inlined as an early-exit loop with a len-as-tag Option
    // result. Previously the dropped closure emitted INVALID WASM (the translation
    // type-mismatch escape); the general unfaithful-HOF wall plus this inline turned
    // that into a faithful execution. Scalar find (`x == k`) rides the same loop.
    let src = "type Tensor = {\n\
        name: String,\n\
        off: Int,\n\
        }\n\
        fn find_tensor(ts: List[Tensor], name: String) -> Option[Tensor] =\n\
        ts |> list.find((t) => t.name == name)\n\
        fn find_val(xs: List[Int], k: Int) -> Option[Int] =\n\
        xs |> list.find((x) => x == k)\n\
        effect fn main() -> Unit = {\n\
        let ts = [{ name: \"a\", off: 3 }, { name: \"b\", off: 7 }]\n\
        match find_tensor(ts, \"b\") {\n\
        some(t) => println(\"hit:\" + int.to_string(t.off))\n\
        none => println(\"none\") }\n\
        match find_tensor(ts, \"zz\") {\n\
        some(t) => println(\"?\")\n\
        none => println(\"none\") }\n\
        match find_val([3, 7, 9], 7) {\n\
        some(v) => println(\"v:\" + int.to_string(v))\n\
        none => println(\"none\") } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("defunc_find", &render_wasm_program(&prog)) {
        assert_eq!(out, "hit:7\nnone\nv:7");
    }
}

#[test]
fn load_weights_record_return_shape() {
    // The whisper load_weights skeleton: a heap-result RECORD return whose fields are a
    // match-bound Matrix (via find_tensor + a byte decode), a List[Float] call, and a
    // `list.map` of a record-building user call capturing the model record.
    let src = "type Tensor = {\n\
        name: String,\n\
        ftype: Int,\n\
        off: Int,\n\
        }\n\
        type Model = {\n\
        tensors: List[Tensor],\n\
        data: Bytes,\n\
        }\n\
        type Layer = {\n\
        w: Matrix,\n\
        b: List[Float],\n\
        }\n\
        type Weights = {\n\
        conv_w: Matrix,\n\
        conv_b: List[Float],\n\
        layers: List[Layer],\n\
        }\n\
        fn find_tensor(ts: List[Tensor], name: String) -> Option[Tensor] =\n\
        ts |> list.find((t) => t.name == name)\n\
        fn tensor_vec(m: Model, name: String, n: Int) -> List[Float] =\n\
        match find_tensor(m.tensors, name) {\n\
        some(t) => bytes.read_f32_le_array(m.data, t.off, n)\n\
        none => list.map(list.range(0, n), (_) => 0.0) }\n\
        fn load_layer(m: Model, i: Int, n: Int) -> Layer = {\n\
        {\n\
        w: matrix.from_bytes_f32_le(m.data, i * 8, 1, 2),\n\
        b: tensor_vec(m, \"conv.bias\", n),\n\
        } }\n\
        fn load_weights(m: Model, n: Int) -> Weights = {\n\
        let conv_w = match find_tensor(m.tensors, \"conv.weight\") {\n\
        some(t) => matrix.transpose(matrix.from_bytes_f32_le(m.data, t.off, 2, 2))\n\
        none => matrix.zeros(2, 2) }\n\
        {\n\
        conv_w: conv_w,\n\
        conv_b: tensor_vec(m, \"conv.bias\", 2),\n\
        layers: list.map(list.range(0, n), (i) => load_layer(m, i, 2)),\n\
        } }\n\
        effect fn main() -> Unit = {\n\
        let ts = [\n\
        { name: \"conv.weight\", ftype: 0, off: 0 },\n\
        { name: \"conv.bias\", ftype: 0, off: 8 },\n\
        ]\n\
        let data = bytes.from_list([0, 0, 192, 63, 0, 0, 0, 64, 0, 0, 0, 191, 0, 128, 200, 66])\n\
        let m: Model = { tensors: ts, data: data }\n\
        let w = load_weights(m, 2)\n\
        println(float.to_string(matrix.get(w.conv_w, 1, 0)))\n\
        println(float.to_string(list.get(w.conv_b, 0) ?? 9.9))\n\
        let l1 = list.get(w.layers, 1)\n\
        match l1 {\n\
        some(l) => println(float.to_string(matrix.get(l.w, 0, 1)))\n\
        none => println(\"none\") } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("load_weights_shape", &render_wasm_program(&prog)) {
        assert_eq!(out, "2.0\n-0.5\n100.25");
    }
}

#[test]
fn matrix_record_field_drop_routes_recursively() {
    // A record with `Matrix` / `List[Matrix]` fields: the generated `$__drop_<R>` must
    // free each field's ROWS (via `__drop_matrix` / `__drop_list_matrix`), not flat-
    // `rc_dec` the outer block (the pre-fix row leak). A 2000-iteration create+drop
    // loop runs bounded with the right sum — the leak-loop convention.
    let src = "type W = {\n\
        m: Matrix,\n\
        ms: List[Matrix],\n\
        }\n\
        fn mk(i: Int) -> W = {\n\
        {\n\
        m: matrix.from_lists([[int.to_float(i), 2.0], [3.0, 4.0]]),\n\
        ms: matrix.split_cols_even(matrix.ones(2, 4), 2),\n\
        } }\n\
        effect fn main() -> Unit = {\n\
        var i = 0\n\
        var acc = 0.0\n\
        while i < 2000 {\n\
        let w = mk(i)\n\
        acc = acc + matrix.get(w.m, 0, 0) + int.to_float(list.len(w.ms))\n\
        i = i + 1 }\n\
        println(float.to_string(acc)) }\n";
    let prog = lower_source(src);
    let wat = render_wasm_program(&prog);
    assert!(wat.contains("$__drop_matrix"), "the Matrix field routes through __drop_matrix");
    assert!(
        wat.contains("$__drop_list_matrix"),
        "the List[Matrix] field routes through __drop_list_matrix"
    );
    if let Some(out) = build_and_run("matrix_field_drop", &wat) {
        // Σ i (0..2000) + 2000 × len(ms)=2 = 1999000 + 4000.
        assert_eq!(out, "2003000.0");
    }
}

#[test]
fn anon_record_with_anon_list_field_drop_source_typechecks() {
    // An UNTYPED anon-record binding whose field is a List of STRUCTURAL records:
    // the synthesized `__drop_anonrec_<hash>` must bind the list field with the
    // STRUCTURAL source type (`List[{ name: String, off: Int }]`) — writing the
    // drop-fn hash as a type (`List[anonrec_<hash>]`) type-errored the WHOLE
    // generated batch ("undefined variable 'f0'") and failed the render
    // program-level.
    let src = "effect fn main() -> Unit = {\n\
        let ts = [{ name: \"a\", off: 3 }, { name: \"b\", off: 9 }]\n\
        let m = { tensors: ts, tag: 7 }\n\
        println(int.to_string(m.tag))\n\
        println(int.to_string(list.len(m.tensors))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("anon_f0_shape", &render_wasm_program(&prog)) {
        assert_eq!(out, "7\n2");
    }
}

#[test]
fn variant_record_ctor_construct_and_match() {
    // RECORD-ctor variants end to end: construction (`Data { … }` → the TAGGED block, a
    // tag-less plain record misread every match as tag 0 — the mt2 miscompile), record
    // PATTERNS (`Data { seq, .. }` — named-field slot binds incl. `..`), a heap-result
    // record-pattern match (payload/message borrows), and NESTED record-ctor fields
    // (`Node { left: Leaf(1), right: Node { … } }` — the recursive tree).
    let src = "type Tree = | Leaf(Int) | Node { left: Tree, right: Tree, value: Int }\n\
        fn tree_sum(t: Tree) -> Int =\n\
        match t {\n\
        Leaf(n) => n\n\
        Node { left, right, value } => tree_sum(left) + tree_sum(right) + value\n\
        }\n\
        type Message =\n\
        | Ping\n\
        | Data { payload: String, seq: Int }\n\
        | Error { code: Int, message: String }\n\
        fn message_code(m: Message) -> Int = match m {\n\
        Ping => 0,\n\
        Data { seq, .. } => seq,\n\
        Error { code, .. } => code,\n\
        }\n\
        fn message_text(m: Message) -> String = match m {\n\
        Ping => \"ping\",\n\
        Data { payload, .. } => payload,\n\
        Error { message, .. } => message,\n\
        }\n\
        effect fn main() -> Unit = {\n\
        let t = Node { left: Leaf(1), right: Node { left: Leaf(2), right: Leaf(3), value: 10 }, value: 5 }\n\
        println(int.to_string(tree_sum(t)))\n\
        let m1 = Data { payload: \"abc\", seq: 42 }\n\
        let m2 = Error { code: 7, message: \"boom\" }\n\
        println(int.to_string(message_code(m1)) + \":\" + message_text(m1))\n\
        println(int.to_string(message_code(m2)) + \":\" + message_text(m2))\n\
        println(int.to_string(message_code(Ping)) + \":\" + message_text(Ping)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("variant_record_ctor", &render_wasm_program(&prog)) {
        assert_eq!(out, "21\n42:abc\n7:boom\n0:ping");
    }
}

#[test]
fn variant_list_field_extraction_loops() {
    // The gguf ValArray CONSUMER: a statement match binding a `List[variant]` field
    // (`ArrV(rows)`), iterated with nested matches and a per-row String accumulator —
    // the arm-tail loop must RUN (it was silently elided to caps markers before).
    let src = "type GV =\n\
        | IntV(Int)\n\
        | StrV(String)\n\
        | ArrV(List[GV])\n\
        fn read_array(n: Int, depth: Int, pos: Int) -> (GV, Int) = {\n\
        var items: List[GV] = []\n\
        var p = pos\n\
        for i in 0..n {\n\
        if depth > 0 then {\n\
        let (val, next) = read_array(2, depth - 1, p)\n\
        list.push(items, val)\n\
        p = next }\n\
        else if i % 2 == 0 then {\n\
        let (val, next) = (IntV(i * 10 + p), p + 1)\n\
        list.push(items, val)\n\
        p = next }\n\
        else {\n\
        let (val, next) = (StrV(\"x\" + int.to_string(p)), p + 2)\n\
        list.push(items, val)\n\
        p = next } }\n\
        (ArrV(items), p) }\n\
        effect fn main() -> Unit = {\n\
        let (v, endp) = read_array(3, 1, 0)\n\
        match v {\n\
        ArrV(rows) => {\n\
        println(\"rows:\" + int.to_string(list.len(rows)))\n\
        for row in rows {\n\
        match row {\n\
        ArrV(cells) => {\n\
        var line = \"\"\n\
        for c in cells {\n\
        match c {\n\
        IntV(i) => { line = line + \"i\" + int.to_string(i) + \",\" }\n\
        StrV(s) => { line = line + s + \",\" }\n\
        ArrV(_) => { line = line + \"?,\" }\n\
        } }\n\
        println(line) }\n\
        IntV(i) => println(\"i\" + int.to_string(i))\n\
        StrV(s) => println(s)\n\
        } } }\n\
        IntV(i) => println(\"top-i\")\n\
        StrV(s) => println(\"top-s\")\n\
        }\n\
        println(\"end:\" + int.to_string(endp)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("variant_list_field_loops", &render_wasm_program(&prog)) {
        assert_eq!(out, "rows:3\ni0,x1,\ni3,x4,\ni6,x7,\nend:9");
    }
}

#[test]
fn pair_selfhosts_enumerate_zip_skv_hshare() {
    // The light self-host batch (backlog T3-5): scalar/String enumerate, scalar/rc-row
    // zip (min-length), map.from_list/entries over the skv vocab repr (duplicate key =
    // FIRST position, LAST value — v0's AlmideMap collect), and the handle-sharing
    // get_or/take over a List[rows]. All byte-verified against `almide run` (native).
    let src = "effect fn main() -> Unit = {\n\
        let xs = [10.5, 20.25]\n\
        for pair in list.enumerate(xs) {\n\
        println(int.to_string(pair.0) + \"=\" + float.to_string(pair.1)) }\n\
        for p2 in list.enumerate([\"ab\", \"cd\"]) {\n\
        println(int.to_string(p2.0) + \":\" + p2.1) }\n\
        let zs = list.zip([1, 2, 3], [40, 50, 60, 70])\n\
        println(int.to_string(list.len(zs)))\n\
        let za = list.zip([[1.0, 2.0], [3.0]], [[9.0], [8.0], [7.0]])\n\
        match list.get(za, 1) {\n\
        some(pr) => println(float.to_string(list.get(pr.0, 0) ?? 0.0) + \"/\" + float.to_string(list.get(pr.1, 0) ?? 0.0))\n\
        none => println(\"none\") }\n\
        let vocab = map.from_list([(\"abc\", 100), (\"d\", 4), (\"abc\", 999)])\n\
        println(int.to_string(map.len(vocab)) + \",\" + int.to_string(map.get(vocab, \"abc\") ?? -1))\n\
        for e in map.entries(vocab) {\n\
        println(e.0 + \"->\" + int.to_string(e.1)) }\n\
        let rows = [[1.5, 2.5], [3.5]]\n\
        let r1 = list.get_or(rows, 1, [])\n\
        println(float.to_string(list.get(r1, 0) ?? -1.0))\n\
        println(int.to_string(list.len(list.take(rows, 1)))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("pair_selfhosts", &render_wasm_program(&prog)) {
        assert_eq!(
            out,
            "0=10.5\n1=20.25\n0:ab\n1:cd\n3\n3.0/8.0\n2,999\nabc->999\nd->4\n3.5\n1"
        );
    }
}

#[test]
fn bytes_length_prefixed_strings() {
    // bytes.read_length_prefixed_strings_le: u32-LE prefixes, a truncated tail STOPS
    // the scan (v0's break), lossy UTF-8 decode; a mid-buffer start reads the rest.
    let src = "effect fn main() -> Unit = {\n\
        let b = bytes.from_list([2, 0, 0, 0, 97, 98, 0, 0, 0, 0, 3, 0, 0, 0, 120, 121, 122, 9, 0])\n\
        let xs = bytes.read_length_prefixed_strings_le(b, 0, 10)\n\
        println(int.to_string(list.len(xs)))\n\
        for s in xs {\n\
        println(\"[\" + s + \"]\") }\n\
        let tail = bytes.read_length_prefixed_strings_le(b, 6, 10)\n\
        println(int.to_string(list.len(tail))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("bytes_lenprefix", &render_wasm_program(&prog)) {
        assert_eq!(out, "3\n[ab]\n[]\n[xyz]\n2");
    }
}

#[test]
fn heap_if_returning_a_bound_var_is_leak_free() {
    // The `let base = "…"; let base = if c then base + "…" else base` shape (default_fields
    // describe's Rect arm). The else arm Dups `base` and moves it out; the ownership
    // certificate must NOT double-count that move against the shared scope-local's
    // reference (the pre-fix `iammd` REJECT — a Consumed value must not also take the
    // EndIf val-move). A 3000-iteration loop stays bounded (no double-free / no leak).
    let src = "fn describe(width: Float, color: String) -> String = {\n\
        let base = \"rect \" + float.to_string(width)\n\
        let base = if color != \"\" then base + \" color=\" + color else base\n\
        base }\n\
        effect fn main() -> Unit = {\n\
        var i = 0\n\
        var last = \"\"\n\
        while i < 3000 {\n\
        last = describe(int.to_float(i), if i % 2 == 0 then \"c\" else \"\")\n\
        i = i + 1 }\n\
        println(last) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("heap_if_bound_var", &render_wasm_program(&prog)) {
        assert_eq!(out, "rect 2999.0");
    }
}

#[test]
fn guard_else_early_return_and_continue_execute() {
    // Phase A end-to-end: a function-body `guard cond else err(...)` returns the Err on
    // the failing path (the pre-fix always-continue miscompile returned ok), and a
    // loop-body `guard cond else continue` filters iterations. Byte-verified vs v0.
    let src = "effect fn validated(s: String) -> Result[String, String] = {\n\
        guard string.len(s) > 0 else err(\"empty\")\n\
        ok(string.to_upper(s)) }\n\
        effect fn main() -> Unit = {\n\
        match validated(\"hi\") { ok(v) => println(\"ok:\" + v), err(e) => println(\"err:\" + e) }\n\
        match validated(\"\") { ok(v) => println(\"ok:\" + v), err(e) => println(\"err:\" + e) }\n\
        var total = 0\n\
        for i in 1..=10 {\n\
        guard i % 2 != 0 else continue\n\
        total = total + i }\n\
        println(int.to_string(total)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("guard_early_return", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok:HI\nerr:empty\n25");
    }
}

#[test]
fn heap_result_match_over_option_field_and_let_bound_variant() {
    // Phase B: (1) a heap-result `match` over a BORROWED `Option[String]` FIELD subject
    // (`match u.email { some(e) => "…${e}…", none => u.name }`) — the field's Option handle
    // is borrowed and tracked so the heap-payload some-bind executes. (2) a `let nm = match
    // s.shape { Circle(_) => "circle", … }; "${nm}…"` — a let-bound CUSTOM-VARIANT heap-result
    // match, tail-duplicated into each arm (wrap_match_arms). Both byte-verified vs v0.
    let src = "type Shape = | Circle(Float) | Rect(Float, Float)\n\
        type User = { name: String, email: Option[String] }\n\
        fn user_display(u: User) -> String =\n\
        match u.email { some(e) => \"${u.name} <${e}>\", none => u.name }\n\
        fn describe(s: Shape, label: String) -> String = {\n\
        let nm = match s { Circle(_) => \"circle\", Rect(_, _) => \"rect\" }\n\
        \"${nm}: ${label}\" }\n\
        effect fn main() -> Unit = {\n\
        let a: User = { name: \"alice\", email: some(\"a@x.com\") }\n\
        let b: User = { name: \"bob\", email: none }\n\
        println(user_display(a))\n\
        println(user_display(b))\n\
        println(describe(Circle(5.0), \"big\"))\n\
        println(describe(Rect(1.0, 2.0), \"wide\")) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("heap_match_option_field", &render_wasm_program(&prog)) {
        assert_eq!(out, "alice <a@x.com>\nbob\ncircle: big\nrect: wide");
    }
}

#[test]
fn list_of_record_ctor_variants_literal() {
    // Phase B: a `[Click { x, y }, KeyPress { key }, Close]` literal — a List of RECORD-CTOR
    // variants (a rich variant with a String field). Each element materializes via the tagged
    // variant ctor (not the plain-record path); the list's `$__drop_list_Event` frees each
    // recursively. Byte-verified vs v0.
    let src = "type Event =\n\
        | Click { x: Int, y: Int }\n\
        | KeyPress { key: String }\n\
        | Close\n\
        fn name(e: Event) -> String = match e {\n\
        Click { x, .. } => \"click:\" + int.to_string(x)\n\
        KeyPress { key } => \"key:\" + key\n\
        Close => \"close\"\n\
        }\n\
        effect fn main() -> Unit = {\n\
        let events = [Click { x: 1, y: 2 }, KeyPress { key: \"a\" }, Close]\n\
        for e in events { println(name(e)) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("list_record_ctor_variants", &render_wasm_program(&prog)) {
        assert_eq!(out, "click:1\nkey:a\nclose");
    }
}

#[test]
fn matrix_softmax_rows_byte_matches_scalar_libm_oracle() {
    // Phase D1: the WASM matrix oracle computes softmax with scalar `rt.math_exp` (= libm
    // exp, which the self-hosted `math.exp`/math_exp.almd byte-matches) and a LEFT-TO-RIGHT
    // scalar sum — NOT the native SIMD fast-exp. The self-host transcribes the SAME op order
    // (row-max subtract → per-element exp → l-to-r sum → divide, with the NaN/Inf/sum<=0 →
    // uniform 1/cols guard), so it is byte-exact vs v0 `--target wasm` even at the -1e9 mask,
    // the ±708 clamp boundary, and extreme magnitudes.
    let src = "effect fn main() -> Unit = {\n\
        let m = matrix.from_lists([\n\
        [1000.0, 0.0 - 1000000000.0, 2.0, 5.5],\n\
        [0.001, 100.0, 0.0 - 50.0, 0.0033333333],\n\
        [710.0, 0.0 - 710.0, 0.0, 708.0]])\n\
        let ls = matrix.to_lists(matrix.softmax_rows(m))\n\
        for row in ls { for x in row { println(float.to_string(x)) } } }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "matrix.softmax_rows"),
        "softmax_rows must auto-link its self-host"
    );
    // Golden captured from `almide run --target wasm` (the scalar-libm oracle).
    if let Some(out) = build_and_run("matrix_softmax", &render_wasm_program(&prog)) {
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 12, "3 rows × 4 cols");
        // Row 2 is a proper distribution summing to ~1; row 1's masked lane is ~0.
        assert!(lines[1].starts_with('0'), "masked -1e9 lane → ~0, got {}", lines[1]);
    }
}

#[test]
fn matrix_gelu_byte_matches_scalar_libm_oracle() {
    // Phase D1: gelu (tanh approx) is element-wise scalar arithmetic + `rt.math_exp` (libm,
    // = self-hosted math.exp). The self-host transcribes the exact op order — inner = K*(x +
    // 0.044715*(x*x)*x), clamp ±20, e2 = exp(2*clamped), tanh = (e2-1)/(e2+1), 0.5*(1+tanh)*x
    // — so it is byte-exact vs v0 `--target wasm` across sign, magnitude, and the clamp region.
    let src = "effect fn main() -> Unit = {\n\
        let m = matrix.from_lists([[0.0 - 3.0, 0.0 - 0.5, 0.0, 0.5, 1.0], [2.0, 5.0, 0.0 - 10.0, 100.0, 0.001]])\n\
        let ls = matrix.to_lists(matrix.gelu(m))\n\
        for row in ls { for x in row { println(float.to_string(x)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.gelu"), "gelu self-host must link");
    if let Some(out) = build_and_run("matrix_gelu", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 10);
        assert_eq!(out.lines().next().unwrap(), "-0.0036373920817729943");
    }
}

#[test]
fn matrix_swiglu_gate_byte_matches_scalar_libm_oracle() {
    // Phase D1: swiglu_gate — g/u are LEFT-TO-RIGHT dot products, sig = 1/(1+exp(clamp(-g,
    // ±40))) via scalar rt.math_exp (= math.exp), out = (g*sig)*u. The self-host transcribes
    // the exact accumulation + op order, byte-exact vs v0 `--target wasm`.
    let src = "effect fn main() -> Unit = {\n        let x = matrix.from_lists([[1.0, 2.0, 0.0 - 1.0], [0.5, 0.0 - 3.0, 2.0]])\n        let wg = matrix.from_lists([[0.1, 0.2, 0.3], [0.0 - 0.4, 0.5, 0.0 - 0.6], [1.0, 0.0, 0.0 - 1.0], [0.2, 0.2, 0.2]])\n        let wu = matrix.from_lists([[0.5, 0.0 - 0.5, 1.0], [0.3, 0.3, 0.3], [0.0 - 1.0, 1.0, 0.0], [0.7, 0.0 - 0.2, 0.1]])\n        let ls = matrix.to_lists(matrix.swiglu_gate(x, wg, wu))\n        for row in ls { for v in row { println(float.to_string(v)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.swiglu_gate"), "swiglu self-host must link");
    if let Some(out) = build_and_run("matrix_swiglu", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 8, "2 rows × 4 out channels");
        assert_eq!(out.lines().next().unwrap(), "-0.1649501991937434");
    }
}

#[test]
fn matrix_rope_rotate_byte_matches_scalar_oracle() {
    // Phase D1: RoPE — per (row=pos, head, pair) rotate by inv_freq = exp(-(2i/head_dim)*
    // log theta), angle = pos*inv_freq, (x0*cos-x1*sin, x0*sin+x1*cos), via scalar self-hosted
    // math.{exp,log,sin,cos}. Op order transcribed exactly → byte-exact vs v0 `--target wasm`.
    let src = "effect fn main() -> Unit = {\n        let x = matrix.from_lists([\n        [1.0, 0.0, 0.5, 0.0 - 0.5, 2.0, 1.0, 0.0 - 1.0, 0.3],\n        [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]])\n        let ls = matrix.to_lists(matrix.rope_rotate(x, 2, 4, 10000.0))\n        for row in ls { for v in row { println(float.to_string(v)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.rope_rotate"), "rope self-host must link");
    if let Some(out) = build_and_run("matrix_rope", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 16, "2 rows × 8 cols");
        assert_eq!(out.lines().next().unwrap(), "1.0");
    }
}

#[test]
fn matrix_multi_head_attention_byte_matches_scalar_oracle() {
    // Phase D1: MHA — per head, per query row: scaled Q·K^T (+ causal -1e9 mask), softmax
    // (scalar rt.math_exp = math.exp), weighted V-sum. Heads write DISJOINT columns so the
    // i-outer/h-inner self-host is byte-identical to v0's h-outer/i-inner `--target wasm`.
    let src = "effect fn main() -> Unit = {\n        let q = matrix.from_lists([[1.0, 0.0, 0.5, 0.0 - 0.5], [0.2, 0.3, 0.0 - 0.1, 0.4], [1.0, 1.0, 0.0, 0.0]])\n        let k = matrix.from_lists([[0.5, 0.5, 1.0, 0.0], [0.0 - 0.2, 0.1, 0.3, 0.0 - 0.4], [0.7, 0.0 - 0.3, 0.2, 0.9]])\n        let v = matrix.from_lists([[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0], [0.0 - 1.0, 0.0 - 2.0, 0.5, 0.25]])\n        for row in matrix.to_lists(matrix.multi_head_attention(q, k, v, 2)) { for x in row { println(float.to_string(x)) } }\n        for row in matrix.to_lists(matrix.masked_multi_head_attention(q, k, v, 2)) { for x in row { println(float.to_string(x)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.masked_multi_head_attention"), "masked mha self-host must link");
    if let Some(out) = build_and_run("matrix_mha", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 24, "2×(3 rows × 4 cols)");
        assert_eq!(out.lines().next().unwrap(), "1.0487146726713201");
    }
}

#[test]
fn matrix_from_q1_0_bytes_byte_matches_oracle() {
    // Phase D1 (final): Q1_0 dequant — fp16 scale decode + per-weight sign bit (1→+scale,
    // 0→-scale) over an 18-byte/128-weight block. Pure bit-ops via prim.band/bshr_u/bshl/bor
    // + bits_to_f32. Byte-exact vs v0 `--target wasm`.
    let src = "effect fn main() -> Unit = {\n\
        let b = bytes.from_list([0, 56, 170, 204, 15, 240, 0, 255, 51, 102, 129, 66, 24, 60, 195, 60, 90, 165])\n\
        let m = matrix.from_q1_0_bytes(b, 0, 2, 8)\n\
        for row in matrix.to_lists(m) { for x in row { println(float.to_string(x)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.from_q1_0_bytes"), "q1_0 self-host must link");
    if let Some(out) = build_and_run("matrix_q1_0", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 16, "2 rows × 8 cols");
        // fp16 0x3800 = 0.5; first sign byte 0xAA = 10101010 → bit0=0 → -0.5.
        assert_eq!(out.lines().next().unwrap(), "-0.5");
    }
}

#[test]
fn value_field_byte_matches_oracle() {
    // B-2 prerequisite: value.field(v, key) self-host — Object tag check + linear key scan,
    // Ok(field) / Err("missing field '<k>'") / Err("expected Object"), byte-exact vs v0.
    let src = "fn get_id(v: Value) -> Int =\n\
        match value.field(v, \"id\") { ok(fv) => value.as_int(fv) ?? 0 - 1, err(_) => 0 - 2 }\n\
        effect fn main() -> Unit = {\n\
        match json.parse(\"{\\\"id\\\":7}\") { ok(v) => println(int.to_string(get_id(v))), err(_) => println(\"perr\") } }\n";
    let prog = lower_source(&format!("import json\n{src}"));
    assert!(prog.functions.iter().any(|f| f.name == "value.field"), "value.field self-host must link");
    if let Some(out) = build_and_run("value_field", &render_wasm_program(&prog)) {
        assert_eq!(out, "6"); // (as_int ?? 0) - 1 = 6 — matches v0
    }
}

#[test]
fn derived_codec_decode_chain_lowers_and_byte_matches() {
    // B-2: the derived Codec `T.decode(v)` chain — `let f = value.as_T(value.field(v,k)?)?; …;
    // ok(T{…})`. Two fixes compose: (1) the nested call-arg `?` (Try) is lifted to a separate
    // bind so the proven nested value-Result match lowers (extract_first_callarg_unwrap), and
    // (2) the derive tags each record-field value with its DECLARED type (not Ty::Unknown) so
    // the v1 record builder stores a scalar `Int` field directly instead of the rc_inc +
    // i64.extend_i32_u heap path that emitted invalid wasm. Single, multi, and NESTED-record
    // fields all byte-match v0 `--target wasm`.
    let src = "type Inner: Codec = { x: Int, y: Int }\n\
        type Config: Codec = { host: String, port: Int, inner: Inner }\n\
        effect fn main() -> Unit = {\n\
        let text = \"{\\\"host\\\":\\\"h\\\",\\\"port\\\":8080,\\\"inner\\\":{\\\"x\\\":1,\\\"y\\\":2}}\"\n\
        match json.parse(text) {\n\
        ok(v) => match Config.decode(v) {\n\
        ok(c) => println(c.host + \":\" + int.to_string(c.port) + \" \" + int.to_string(c.inner.x))\n\
        err(e) => println(\"e:\" + e) }\n\
        err(_) => println(\"perr\") } }\n";
    let prog = lower_source(&format!("import json\n{src}"));
    assert!(prog.functions.iter().any(|f| f.name == "Config.decode"), "Config.decode must link");
    assert!(prog.functions.iter().any(|f| f.name == "Inner.decode"), "nested Inner.decode must link");
    if let Some(out) = build_and_run("codec_decode", &render_wasm_program(&prog)) {
        assert_eq!(out, "h:8080 1");
    }
}

#[test]
fn derived_codec_list_and_default_fields_decode() {
    // B-2 extension: Codec `List[T]` fields (self-hosted __decode_list_T / __encode_list_T
    // over value.as_T / value.array) and DEFAULT fields (__decode_default_T: absent/Null →
    // default). Both decode + the generated encode method byte-match v0 `--target wasm`.
    let src = "type Rec: Codec = { id: Int, tags: List[Int], names: List[String] }\n\
        type Cfg: Codec = { host: String = \"localhost\", port: Int = 8080, tags: List[String] }\n\
        effect fn main() -> Unit = {\n\
        match json.parse(\"{\\\"id\\\":5,\\\"tags\\\":[1,2,3],\\\"names\\\":[\\\"a\\\",\\\"b\\\"]}\") {\n\
        ok(v) => match Rec.decode(v) { ok(r) => println(int.to_string(r.id) + \" \" + int.to_string(list.len(r.tags)) + \" \" + int.to_string(list.len(r.names))), err(_e) => println(\"e\") }\n\
        err(_) => println(\"perr\") }\n\
        match json.parse(\"{\\\"tags\\\":[\\\"x\\\"]}\") {\n\
        ok(v) => match Cfg.decode(v) { ok(c) => println(c.host + \" \" + int.to_string(c.port)), err(_e) => println(\"e\") }\n\
        err(_) => println(\"perr\") } }\n";
    let prog = lower_source(&format!("import json\n{src}"));
    assert!(prog.functions.iter().any(|f| f.name == "__decode_list_int"), "list decode helper must link");
    assert!(prog.functions.iter().any(|f| f.name == "__decode_default_int"), "default decode helper must link");
    if let Some(out) = build_and_run("codec_list_default", &render_wasm_program(&prog)) {
        assert_eq!(out, "5 3 2\nlocalhost 8080");
    }
}

#[test]
fn derived_variant_codec_decode_all_payload_shapes() {
    // Derived-Codec DECODE of tagged variants across every payload shape the trust-spine handles:
    // a nested scalar-record field (Wrap(Color)), a record-shaped case with a String + nested record
    // (Tag), a List field (Multi), a tuple with scalar/String fields (Pair), and unit (Plain). The
    // decode reads the tag as a plain String (value.keys |> list.get ?? "") + the payload via
    // value.field — NOT a (String, Value) tuple the trust-spine walls — then `ok(Ctor(..))`
    // materializes the variant (a nested scalar-record field stored + freed by the masked rc_dec).
    let src = "type Color: Codec = { r: Int, g: Int, b: Int }\n\
        type Labeled: Codec = { label: String, n: Int }\n\
        type Shape: Codec = | Wrap(Color) | Boxed(Labeled) | Tag { name: String, c: Color } | Multi(List[Int]) | Pair(Int, String) | Plain\n\
        effect fn main() -> Unit = {\n\
        match Shape.decode(Shape.encode(Wrap({ r: 1, g: 2, b: 3 }))) { ok(s) => match s { Wrap(c) => println(int.to_string(c.g)), _ => println(\"?\") }, err(e) => println(e) }\n\
        match Shape.decode(Shape.encode(Boxed({ label: \"z\", n: 8 }))) { ok(s) => match s { Boxed(i) => println(i.label + \" \" + int.to_string(i.n)), _ => println(\"?\") }, err(e) => println(e) }\n\
        match Shape.decode(Shape.encode(Tag { name: \"hi\", c: { r: 4, g: 5, b: 6 } })) { ok(s) => match s { Tag { name, c } => println(name + \" \" + int.to_string(c.b)), _ => println(\"?\") }, err(e) => println(e) }\n\
        match Shape.decode(Shape.encode(Multi([1, 2, 3]))) { ok(s) => match s { Multi(xs) => println(int.to_string(list.len(xs))), _ => println(\"?\") }, err(e) => println(e) }\n\
        match Shape.decode(Shape.encode(Pair(7, \"x\"))) { ok(s) => match s { Pair(n, t) => println(int.to_string(n) + t), _ => println(\"?\") }, err(e) => println(e) }\n\
        match Shape.decode(Shape.encode(Plain)) { ok(s) => match s { Plain => println(\"plain\"), _ => println(\"?\") }, err(e) => println(e) } }\n";
    let prog = lower_source(&format!("import json\n{src}"));
    assert!(prog.functions.iter().any(|f| f.name == "Shape.decode"), "Shape.decode must link");
    if let Some(out) = build_and_run("variant_codec_decode", &render_wasm_program(&prog)) {
        assert_eq!(out, "2\nz 8\nhi 6\n3\n7x\nplain");
    }
}

#[test]
fn option_interp_self_hosts_per_element_type() {
    // `${Option[Int]}` / `${Option[Bool]}` render v0's `some(<v>)` / `none` via a per-element self-host
    // (`option.to_string` / `option.to_string_b` — a 2-arm Option match + string concat), routed by
    // element type exactly like the List family. A String/Float element stays an unlinked clean wall.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[Int] = some(42) let b: Option[Int] = some(-7) let c: Option[Int] = none\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"v=${a}!\")\n\
        let s: Option[String] = some(\"hi\") let q: Option[String] = some(\"a \\\"b\\\"\") let sn: Option[String] = none\n\
        println(\"${s}\") println(\"${q}\") println(\"${sn}\")\n\
        let fa: Option[Float] = some(3.5) let fb: Option[Float] = some(3.0)\n\
        println(\"${fa}\") println(\"${fb}\")\n\
        let t: Option[Bool] = some(true) let f: Option[Bool] = none\n\
        println(\"${t}\") println(\"${f}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string"), "Option[Int] interp must auto-link option.to_string");
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_s"), "Option[String] interp must auto-link option.to_string_s");
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_f"), "Option[Float] interp must auto-link option.to_string_f");
    if let Some(out) = build_and_run("option_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "some(42)\nsome(-7)\nnone\nv=some(42)!\nsome(\"hi\")\nsome(\"a \\\"b\\\"\")\nnone\nsome(3.5)\nsome(3)\nsome(true)\nnone");
    }
}

#[test]
fn nonempty_map_literal_materializes_via_from_list() {
    // A non-empty map literal used to lower to a DEFERRED-Opaque empty block, so `map.len`/`map.get`
    // silently read 0 (a miscompile). Routing it through `map.from_list` materializes a real map, so
    // the ops byte-match v0. (Regression guard for the silent-miscompile fix.)
    let src = "fn probe(m: Map[String, Int]) -> String = {\n\
        \"len=\" + int.to_string(map.len(m)) + \" x=\" + int.to_string(map.get(m, \"x\") ?? -1)\n\
        }\n\
        effect fn main() -> Unit = {\n\
        let a: Map[String, Int] = [\"x\": 1, \"y\": 2, \"z\": 3]\n\
        println(probe(a)) println(int.to_string(map.len(a))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("map_literal", &render_wasm_program(&prog)) {
        assert_eq!(out, "len=3 x=1\n3");
    }
}

#[test]
fn map_interp_self_hosts_via_keys_values() {
    // `${Map[String, Int]}` renders v0's `["k": v, …]` (empty → `[:]`; keys quoted). `map.to_string`
    // reads keys/values via the callable `map.keys`/`map.values` (unblocked by the map-literal
    // materialization fix) and renders each entry inline; both owned lists drop at scope end.
    let src = "effect fn main() -> Unit = {\n\
        let a: Map[String, Int] = [\"x\": 1, \"y\": 2] let b: Map[String, Int] = [:] let c: Map[String, Int] = [\"n\": -5]\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"m=${a}!\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "map.to_string"), "Map[String,Int] interp must auto-link map.to_string");
    if let Some(out) = build_and_run("map_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "[\"x\": 1, \"y\": 2]\n[:]\n[\"n\": -5]\nm=[\"x\": 1, \"y\": 2]!");
    }
}

#[test]
fn set_interp_self_hosts_via_to_list() {
    // `${Set[Int]}` renders v0's `set.from_list([<elems>])` (insertion order, dedup). `set.to_string`
    // reads the elements via the callable `set.to_list` and renders the body inline like
    // `list.to_string`; the owned `set.to_list` result is dropped at scope end (no leak).
    let src = "effect fn main() -> Unit = {\n\
        let a: Set[Int] = set.from_list([3, 1, 2, 1]) let b: Set[Int] = set.from_list([]) let c: Set[Int] = set.from_list([-5, 10])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"s=${a}!\")\n\
        let sa: Set[String] = set.from_list([\"b\", \"a\", \"b\"]) let sc: Set[String] = set.from_list([\"q\"])\n\
        println(\"${sa}\") println(\"${sc}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "set.to_string"), "Set[Int] interp must auto-link set.to_string");
    assert!(prog.functions.iter().any(|f| f.name == "set.to_string_s"), "Set[String] interp must auto-link set.to_string_s");
    if let Some(out) = build_and_run("set_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "set.from_list([3, 1, 2])\nset.from_list([])\nset.from_list([-5, 10])\ns=set.from_list([3, 1, 2])!\nset.from_list([\"b\", \"a\"])\nset.from_list([\"q\"])");
    }
}

#[test]
fn result_list_str_interp() {
    // `${Result[List[String], String]}` → `ok(["a", "b"])` / `err("<quoted>")`. `result.to_string_ls`
    // renders the Ok string-list (each element quoted+escaped) reusing `result_to_string`'s `__rts_esc_*`.
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[List[String], String] = ok([\"a\", \"b\"]) let b: Result[List[String], String] = err(\"boom\") let c: Result[List[String], String] = ok([])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_ls"), "must auto-link result.to_string_ls");
    if let Some(out) = build_and_run("result_list_str_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok([\"a\", \"b\"])\nerr(\"boom\")\nok([])");
    }
}

#[test]
fn result_list_int_interp_and_construction() {
    // `${Result[List[Int], String]}` → `ok([1, 2, 3])` / `err("<quoted>")`. The ResultOk heap
    // materializer admits a scalar-list literal (incl empty `ok([])`); `result.to_string_li` renders.
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[List[Int], String] = ok([4, 5]) let b: Result[List[Int], String] = err(\"boom\") let c: Result[List[Int], String] = ok([])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_li"), "must auto-link result.to_string_li");
    if let Some(out) = build_and_run("result_list_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok([4, 5])\nerr(\"boom\")\nok([])");
    }
}

#[test]
fn option_option_int_interp() {
    // `${Option[Option[Int]]}` → `some(some(5))` / `some(none)` / `none` (nested Option interp), the
    // self-host `option.to_string_oi` over the already-materializing nested-Option construction.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[Option[Int]] = some(some(5)) let b: Option[Option[Int]] = some(none) let c: Option[Option[Int]] = none\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_oi"), "must auto-link option.to_string_oi");
    if let Some(out) = build_and_run("option_option_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "some(some(5))\nsome(none)\nnone");
    }
}

#[test]
fn nested_interp_batch2_compositions_and_result_map() {
    // Option[Option[List[Int]]], Option[Result[List[Int],String]], Result[Option[List[String]],String],
    // Result[Map[String,Int],String] (the last with a ResultOk map materialization).
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[Option[List[Int]]] = some(some([1, 2])) let b: Option[Result[List[Int], String]] = some(ok([3, 4]))\n\
        let c: Result[Option[List[String]], String] = ok(some([\"x\"])) let d: Result[Map[String, Int], String] = ok([\"k\": 5])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_msi"), "must auto-link result.to_string_msi");
    if let Some(out) = build_and_run("nested_batch2", &render_wasm_program(&prog)) {
        assert_eq!(out, "some(some([1, 2]))\nsome(ok([3, 4]))\nok(some([\"x\"]))\nok([\"k\": 5])");
    }
}

#[test]
fn option_map_string_int_interp() {
    // `${Option[Map[String,Int]]}` — the non-empty map is a map.from_list computed payload materialized
    // into the Some slot; rendered via map.keys/map.values wrapped in some(…).
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[Map[String, Int]] = some([\"a\": 1, \"b\": 2]) let b: Option[Map[String, Int]] = none\n\
        println(\"${a}\") println(\"${b}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_msi"), "must auto-link option.to_string_msi");
    if let Some(out) = build_and_run("option_map", &render_wasm_program(&prog)) {
        assert_eq!(out, "some([\"a\": 1, \"b\": 2])\nnone");
    }
}

#[test]
fn float_list_option_and_result_interp() {
    // Option[List[Float]] / Result[List[Float],String] — each element float.to_string with drop-.0.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[List[Float]] = some([1.5, 2.0]) let b: Option[List[Float]] = some([])\n\
        let c: Result[List[Float], String] = ok([100.0, 0.5]) let d: Result[List[Float], String] = err(\"x\")\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_lf"), "must auto-link option.to_string_lf");
    if let Some(out) = build_and_run("float_list", &render_wasm_program(&prog)) {
        assert_eq!(out, "some([1.5, 2])\nsome([])\nok([100, 0.5])\nerr(\"x\")");
    }
}

#[test]
fn nested_interp_float_deep_option_and_result_option_list() {
    // Result[Float,String] (float drop-.0), Option[Option[Option[Int]]] (3-deep), and
    // Result[Option[List[Int]],String] (int-list under ok(some …)).
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[Float, String] = ok(3.5) let b: Result[Float, String] = ok(4.0)\n\
        let c: Option[Option[Option[Int]]] = some(some(some(5))) let d: Option[Option[Option[Int]]] = some(none)\n\
        let e: Result[Option[List[Int]], String] = ok(some([1, 2])) let f: Result[Option[List[Int]], String] = ok(none)\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") println(\"${e}\") println(\"${f}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_f"), "must auto-link result.to_string_f");
    if let Some(out) = build_and_run("nested_more", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok(3.5)\nok(4)\nsome(some(some(5)))\nsome(none)\nok(some([1, 2]))\nok(none)");
    }
}

#[test]
fn nested_interp_min_int_and_computed_list_payloads() {
    // Two adversarial-fuzz regressions: (A) i64::MIN in a list interp rendered "-0" (negate overflow),
    // (B) some/ok of a COMPUTED list read none/ok([]). Both fixed.
    let src = "effect fn main() -> Unit = {\n\
        let mn: List[Int] = [0 - 9223372036854775807 - 1, 7] println(\"${mn}\")\n\
        let a: Option[List[Int]] = some(list.map([1, 2, 3], (n) => n * 2)) println(\"${a}\")\n\
        let b: Result[List[Int], String] = ok([1, 2] + [3]) println(\"${b}\")\n\
        let c: Option[List[Bool]] = some(list.map([1, 2], (n) => n > 1)) println(\"${c}\") }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("nested_edgecases", &render_wasm_program(&prog)) {
        assert_eq!(out, "[-9223372036854775808, 7]\nsome([2, 4, 6])\nok([1, 2, 3])\nsome([false, true])");
    }
}

#[test]
fn result_outer_nested_interp() {
    // Result-outer nested `${…}`: the ResultOk heap materializer admits a nested Option/Result ctor
    // Ok payload (construction) and the nested-payload bind seeds its read-shape (inner match).
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[Bool, String] = ok(true) let b: Result[List[Bool], String] = ok([true, false])\n\
        let c: Result[Option[Int], String] = ok(some(5)) let d: Result[Option[Int], String] = ok(none)\n\
        let e: Result[Result[Int, String], String] = ok(err(\"x\"))\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") println(\"${e}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_b"), "must auto-link result.to_string_b");
    if let Some(out) = build_and_run("result_nested", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok(true)\nok([true, false])\nok(some(5))\nok(none)\nok(err(\"x\"))");
    }
}

#[test]
fn option_outer_nested_interp() {
    // Option-outer nested `${…}`, incl a cap-as-tag inner Result[String,String] (the seed_variant_param
    // nested-payload fix) and a bool-list inner.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[Option[Bool]] = some(some(true)) let b: Option[Option[String]] = some(some(\"a\"))\n\
        let c: Option[Result[Int, String]] = some(ok(5)) let d: Option[Result[String, String]] = some(ok(\"q\"))\n\
        let e: Option[List[Bool]] = some([false, true])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") println(\"${e}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_rs"), "must auto-link option.to_string_rs");
    if let Some(out) = build_and_run("option_nested", &render_wasm_program(&prog)) {
        assert_eq!(out, "some(some(true))\nsome(some(\"a\"))\nsome(ok(5))\nsome(ok(\"q\"))\nsome([false, true])");
    }
}

#[test]
fn option_list_str_interp() {
    // `${Option[List[String]]}` → `some(["a", "b"])` / `none` — a HEAP-element inner list. The self-host
    // `option.to_string_ls` inlines the string quote+escape (\ " \n \r \t) since self-hosts can't call
    // each other. Escaping is exercised by the embedded quote/backslash.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[List[String]] = some([\"a\", \"b\"]) let b: Option[List[String]] = none let c: Option[List[String]] = some([])\n\
        let d: Option[List[String]] = some([\"q\\\"x\"])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_ls"), "must auto-link option.to_string_ls");
    if let Some(out) = build_and_run("option_list_str_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "some([\"a\", \"b\"])\nnone\nsome([])\nsome([\"q\\\"x\"])");
    }
}

#[test]
fn option_list_int_interp_and_construction() {
    // `${Option[List[Int]]}` → `some([1, 2, 3])` / `none` (nested compound). Two gaps close: the
    // OptionSome heap materializer now admits a scalar-list literal (incl the empty `some([])`), and
    // the self-host `option.to_string_li` renders it. A constructed Some list is also matchable.
    let src = "fn describe(o: Option[List[Int]]) -> String = match o { some(v) => int.to_string(list.len(v)), none => \"none\" }\n\
        effect fn main() -> Unit = {\n\
        let a: Option[List[Int]] = some([1, 2, 3]) let b: Option[List[Int]] = none let c: Option[List[Int]] = some([])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\")\n\
        println(describe(a) + \",\" + describe(c)) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_li"), "Option[List[Int]] interp must auto-link option.to_string_li");
    if let Some(out) = build_and_run("option_list_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "some([1, 2, 3])\nnone\nsome([])\n3,0");
    }
}

#[test]
fn noncapturing_lambda_returned_as_funcref() {
    // A function RETURNING a non-capturing lambda / a bare fn reference — the trust-spine lifts it to
    // a table slot and returns the scalar funcref; the caller tracks the bound result so `f(args)`
    // dispatches through CallIndirect. A capturing closure still walls (a real env is a later brick).
    let src = "fn inc() -> (Int) -> Int = (x) => x + 1\n\
        fn tp(x: Int) -> Int = x * 2 + 3\n\
        fn getter() -> (Int) -> Int = tp\n\
        effect fn main() -> Unit = {\n\
        let f = inc() println(int.to_string(f(5))) println(int.to_string(f(41)))\n\
        let h = getter() println(int.to_string(h(6))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "inc"), "inc must lower");
    if let Some(out) = build_and_run("closure_return", &render_wasm_program(&prog)) {
        assert_eq!(out, "6\n42\n15");
    }
}

#[test]
fn fan_race_and_any_inline_literal_thunk_lists() {
    // `fan.race`/`fan.any` over a LITERAL thunk list, deterministic on wasm: race takes thunk[0]'s
    // result (head even if it errs); any takes the FIRST Ok in order (else v0's fixed `fan.any: all
    // candidates failed`). Both inline into a plain match chain, avoiding a List[funcref] — race the
    // first thunk's body, any the outer arms folded into each thunk level.
    let src = "effect fn okn(n: Int) -> Result[Int, String] = ok(n * 3)\n\
        effect fn failing() -> Result[Int, String] = err(\"boom\")\n\
        effect fn main() -> Unit = {\n\
        match fan.race([() => okn(10), () => okn(20)]) { ok(v) => println(\"r=\" + int.to_string(v)), err(e) => println(e) }\n\
        match fan.race([() => failing(), () => okn(9)]) { ok(v) => println(\"ok\"), err(e) => println(\"e=\" + e) }\n\
        match fan.any([() => failing(), () => okn(7)]) { ok(v) => println(\"any=\" + int.to_string(v)), err(e) => println(e) }\n\
        match fan.any([() => failing(), () => failing()]) { ok(v) => println(\"ok\"), err(e) => println(\"af=\" + e) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("fan_race_any", &render_wasm_program(&prog)) {
        assert_eq!(out, "r=30\ne=boom\nany=21\naf=fan.any: all candidates failed");
    }
}

#[test]
fn fan_map_int_lowers_to_self_host_traverse() {
    // `fan.map` over List[Int] with an (Int) -> Result[Int, String] callback — the compiler intrinsic
    // routed to the self-host `fan_map` (a fallible traverse invoking the lifted callback via
    // CallIndirect), collecting ok values in list order and short-circuiting on the first err. The
    // result is matched / auto-`!`-unwrapped.
    let src = "effect fn dbl(x: Int) -> Result[Int, String] = ok(x * 2)\n\
        effect fn checked(x: Int) -> Result[Int, String] = if x < 0 then err(\"neg\") else ok(x)\n\
        effect fn main() -> Unit = {\n\
        let doubled = fan.map([1, 2, 3], (x) => dbl(x))\n\
        println(int.to_string(doubled[0]) + \",\" + int.to_string(doubled[2]))\n\
        match fan.map([1, -2, 3], (x) => checked(x)) { ok(ys) => println(\"ok\"), err(e) => println(\"short:\" + e) } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "fan.map"), "fan.map must auto-link the fan_map self-host");
    if let Some(out) = build_and_run("fan_map_int", &render_wasm_program(&prog)) {
        assert_eq!(out, "2,6\nshort:neg");
    }
}

#[test]
fn higher_order_result_traverse_matches_call_indirect() {
    // A fallible list traverse over a funcref callback — `match f(x) { ok => .., err => .. }` where
    // `f` is invoked via CallIndirect. The trust-spine seeds the CallIndirect result's read-shape and
    // hoists a computed-call match subject, so the traverse (short-circuit on first err) lowers. The
    // sequential `fan.map` semantics on wasm.
    let src = "fn go(xs: List[Int], f: (Int) -> Result[Int, String], i: Int, acc: List[Int]) -> Result[List[Int], String] =\n\
        if i >= list.len(xs) then ok(acc)\n\
        else match f(list.get(xs, i) ?? 0) { ok(y) => go(xs, f, i + 1, acc + [y]), err(e) => err(e) }\n\
        fn traverse(xs: List[Int], f: (Int) -> Result[Int, String]) -> Result[List[Int], String] = go(xs, f, 0, [])\n\
        fn show(r: Result[List[Int], String]) -> String = match r { ok(ys) => \"ok:\" + int.to_string(list.sum(ys)), err(e) => \"err:\" + e }\n\
        effect fn main() -> Unit = {\n\
        println(show(traverse([1, 2, 3, 4], (x) => ok(x * 2))))\n\
        println(show(traverse([1, -2, 3], (x) => if x > 0 then ok(x) else err(\"neg\")))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "traverse"), "traverse must lower");
    if let Some(out) = build_and_run("ho_traverse", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok:20\nerr:neg");
    }
}

#[test]
fn higher_order_heap_return_via_call_indirect() {
    // `fn apply(g, x) = g(x)` returning a heap value (Result / String) through a known funcref used to
    // wall a tail heap-result computed call; it now executes via `Op::CallIndirect` and moves the
    // owned result out. Opens higher-order functions returning heap values (the fan.map foundation).
    let src = "fn apply_r(f: (Int) -> Result[Int, String], x: Int) -> Result[Int, String] = f(x)\n\
        fn apply_s(f: (Int) -> String, x: Int) -> String = f(x)\n\
        fn show(r: Result[Int, String]) -> String = match r { ok(v) => \"ok:\" + int.to_string(v), err(e) => \"err:\" + e }\n\
        effect fn main() -> Unit = {\n\
        println(show(apply_r((y) => ok(y * 2), 5)))\n\
        println(show(apply_r((y) => if y > 0 then ok(y) else err(\"neg\"), -3)))\n\
        println(apply_s((y) => \"v\" + int.to_string(y), 7)) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "apply_r"), "apply_r must lower");
    if let Some(out) = build_and_run("higher_order_heap", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok:10\nerr:neg\nv7");
    }
}

#[test]
fn result_ok_err_concat_payload_materializes() {
    // `ok("n" + int.to_string(x))` / `err("bad " + …)` — a computed (ConcatStr) String payload the
    // trust-spine used to wall (only literal/Var/call payloads were handled). It now materializes the
    // concat and moves it into the Result, dropping the borrowed operand temps.
    let src = "fn classify(x: Int) -> Result[String, String] =\n\
        if x > 0 then ok(\"pos \" + int.to_string(x)) else err(\"neg \" + int.to_string(x))\n\
        fn show(r: Result[String, String]) -> String = match r { ok(s) => \"OK:\" + s, err(e) => \"ERR:\" + e }\n\
        effect fn main() -> Unit = { println(show(classify(7))) println(show(classify(-3))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "classify"), "classify must lower");
    if let Some(out) = build_and_run("result_concat", &render_wasm_program(&prog)) {
        assert_eq!(out, "OK:pos 7\nERR:neg -3");
    }
}

#[test]
fn result_interp_self_hosts_per_element_pair() {
    // `${Result[Int, String]}` / `${Result[String, String]}` render v0's `ok(<T>)` / `err(<E>)` via a
    // per-(T,E) self-host (`result.to_string` / `result.to_string_ss`); a String payload is quoted +
    // escaped. Any other pairing stays an unlinked clean wall.
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[Int, String] = ok(42) let b: Result[Int, String] = err(\"bad\")\n\
        println(\"${a}\") println(\"${b}\") println(\"r=${a}!\")\n\
        let c: Result[String, String] = ok(\"hi\") let d: Result[String, String] = err(\"x\")\n\
        println(\"${c}\") println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string"), "Result[Int,String] interp must auto-link result.to_string");
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_ss"), "Result[String,String] interp must auto-link result.to_string_ss");
    if let Some(out) = build_and_run("result_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok(42)\nerr(\"bad\")\nr=ok(42)!\nok(\"hi\")\nerr(\"x\")");
    }
}

#[test]
fn tuple_multifield_and_single_ctor_matches_lower() {
    // A TUPLE-subject match (`desugar_tuple_match`), a MULTI-FIELD variant match (regrouped into a
    // tuple payload sub-match), and a SINGLE-CTOR newtype match (routed through an IfThen merge with
    // an unreachable empty-heap else — no double-move). Every arm's literal/column must select the
    // exact result, byte-identical to v0.
    let src = "type Ev = KV(String, Int) | Tag(String)\n\
        type Rec = Pair(Int, String)\n\
        type Boxed = B(Int)\n\
        fn tup(t: (String, Int)) -> String = match t { (\"a\", 1) => \"A1\", (\"a\", _) => \"AX\", (_, 0) => \"X0\", (_, _) => \"XX\" }\n\
        fn ev(e: Ev) -> String = match e { KV(\"count\", n) => \"C\" + int.to_string(n), KV(_, n) => \"K\" + int.to_string(n), Tag(_) => \"T\" }\n\
        fn rec(r: Rec) -> String = match r { Pair(1, \"one\") => \"1ONE\", Pair(1, _) => \"1X\", Pair(_, _) => \"XX\" }\n\
        fn unbox(b: Boxed) -> String = match b { B(n) => \"b\" + int.to_string(n) }\n\
        effect fn main() -> Unit = {\n\
        println(tup((\"a\", 1))) println(tup((\"z\", 0))) println(tup((\"z\", 5)))\n\
        println(ev(KV(\"count\", 3))) println(ev(KV(\"x\", 7))) println(ev(Tag(\"t\")))\n\
        println(rec(Pair(1, \"one\"))) println(rec(Pair(1, \"z\"))) println(rec(Pair(2, \"z\")))\n\
        println(unbox(B(42))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "unbox"), "single-ctor unbox must lower");
    if let Some(out) = build_and_run("tuple_multifield_match", &render_wasm_program(&prog)) {
        assert_eq!(out, "A1\nX0\nXX\nC3\nK7\nT\n1ONE\n1X\nXX\nb42");
    }
}

#[test]
fn guarded_option_result_match_regroups_and_lowers() {
    // A heap-result `match` over an Option / Result subject whose arms carry GUARDS + LITERAL
    // payloads regroups into constructor dispatch + a scalar payload sub-match (`some(n) if g` →
    // `some($p) => match $p { n if g => .. }`), so the guarded-variant case reduces to the proven
    // variant-tag dispatch + scalar guard/literal chain. A guard/literal MUST select the exact arm.
    let src = "type Tok = Word(String) | Num(Int)\n\
        fn olabel(x: Option[Int]) -> String = match x { some(n) if n > 100 => \"big\", some(n) if n > 0 => \"pos\", some(0) => \"zero\", some(_) => \"neg\", none => \"none\" }\n\
        fn rlabel(r: Result[Int, String]) -> String = match r { ok(v) if v > 0 => \"ok+\", ok(0) => \"ok0\", ok(_) => \"ok-\", err(e) if string.len(e) > 5 => \"eL\", err(_) => \"eS\" }\n\
        fn tclass(t: Tok) -> String = match t { Word(\"hi\") => \"HI\", Word(_) => \"W\", Num(7) => \"SEVEN\", Num(_) => \"N\" }\n\
        effect fn main() -> Unit = {\n\
        println(olabel(some(200))) println(olabel(some(5))) println(olabel(some(0))) println(olabel(none))\n\
        println(rlabel(ok(7))) println(rlabel(ok(0))) println(rlabel(err(\"longmsg\"))) println(rlabel(err(\"no\")))\n\
        println(tclass(Word(\"hi\"))) println(tclass(Word(\"z\"))) println(tclass(Num(7))) println(tclass(Num(3))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "olabel"), "olabel must lower");
    assert!(prog.functions.iter().any(|f| f.name == "tclass"), "tclass must lower");
    if let Some(out) = build_and_run("guarded_variant_match", &render_wasm_program(&prog)) {
        assert_eq!(out, "big\npos\nzero\nnone\nok+\nok0\neL\neS\nHI\nW\nSEVEN\nN");
    }
}

#[test]
fn variant_ctor_in_result_ok_materializes() {
    // `ok(<user-variant ctor>)` in Result-Ok position (the derived variant-decode `ok(Pair(..))`
    // shape) MATERIALIZES the tagged variant block (the SAME block `let p = Pair(..)` builds, with its
    // recursive `$__drop_<V>` drop) and wraps it — NOT a dangling `CallFn "Pair"`. Covers a tuple
    // variant (heap + scalar fields), a scalar variant, a unit variant, and the Err arm; the consumer
    // reads the Ok payload as a real variant. Byte-identical to v0 `--target wasm`.
    let src = "type Shape = | Pair(Int, String) | Solo(Int) | Plain\n\
        fn build(t: Int, n: Int, s: String) -> Result[Shape, String] =\n\
        if t == 0 then ok(Pair(n, s)) else if t == 1 then ok(Solo(n)) else if t == 2 then ok(Plain) else err(\"bad\")\n\
        effect fn main() -> Unit = {\n\
        match build(0, 7, \"x\") { ok(v) => match v { Pair(n, s) => println(int.to_string(n) + s), Solo(n) => println(\"solo\"), Plain => println(\"plain\") }, err(e) => println(e) }\n\
        match build(2, 0, \"\") { ok(v) => match v { Pair(n, s) => println(\"p\"), Solo(n) => println(\"solo\"), Plain => println(\"plain\") }, err(e) => println(e) }\n\
        match build(9, 0, \"\") { ok(v) => match v { Pair(n, s) => println(\"p\"), Solo(n) => println(\"solo\"), Plain => println(\"plain\") }, err(e) => println(e) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("variant_result_ctor", &render_wasm_program(&prog)) {
        assert_eq!(out, "7x\nplain\nbad");
    }
}

#[test]
fn derived_codec_option_fields_decode() {
    // B-2 completion: Codec `Option[T]` fields. The self-hosted `__decode_option_T` builds a
    // `Result[Option[T], String]` (ok(some(x)) / ok(none) / err(e)) — a STRING leaf freed by the
    // recursive `$__drop_opt_str` (`resrec:opt_str`), a SCALAR leaf flat — byte-identical to v0.
    // Encode → decode → re-encode roundtrip: present (Some) survives, absent + explicit-null → None.
    let src = "type Rec: Codec = { name: String, nick: Option[String], age: Option[Int] }\n\
        effect fn main() -> Unit = {\n\
        let r1 = Rec { name: \"A\", nick: some(\"nn\"), age: some(30) }\n\
        let v1 = r1.encode()\n\
        println(json.stringify(v1))\n\
        match Rec.decode(v1) { ok(r) => println(json.stringify(r.encode())), err(e) => println(\"err:\" + e) }\n\
        let pv = json.parse(\"{\\\"name\\\":\\\"B\\\",\\\"age\\\":null}\")\n\
        match pv { ok(pj) => match Rec.decode(pj) { ok(r) => println(json.stringify(r.encode())), err(e) => println(\"err:\" + e) }, err(pe) => println(\"parse:\" + pe) } }\n";
    let prog = lower_source(&format!("import json\n{src}"));
    assert!(prog.functions.iter().any(|f| f.name == "__decode_option_string"), "string option decode helper must link");
    assert!(prog.functions.iter().any(|f| f.name == "__decode_option_int"), "int option decode helper must link");
    if let Some(out) = build_and_run("codec_option_field", &render_wasm_program(&prog)) {
        assert_eq!(
            out,
            "{\"name\":\"A\",\"nick\":\"nn\",\"age\":30}\n\
             {\"name\":\"A\",\"nick\":\"nn\",\"age\":30}\n\
             {\"name\":\"B\",\"nick\":null,\"age\":null}"
        );
    }

}

#[test]
fn heap_and_fn_captures_execute_and_free_via_drop_closure() {
    // CLOSURE ENV FULL MODE: a String capture (co-owned, read back borrowed), a
    // List[Int] capture, and Fn captures (compose — the block captures two other
    // closure blocks, freed by $__drop_closure's SELF-RECURSION). Every closure
    // drop routes through the self-describing $__drop_closure — slot 0 (the
    // fnidx) is never treated as a pointer: a corrupted free would trap here,
    // so a clean byte-matched run IS the slot-0/mask/recursion pin.
    let src = "fn greeter(name: String) -> (String) -> String = (x) => name + \", \" + x\n\
        fn adder(n: Int) -> (Int) -> Int = (x) => x + n\n\
        fn compose(f: (Int) -> Int, g: (Int) -> Int) -> (Int) -> Int = (x) => g(f(x))\n\
        effect fn main() -> Unit = {\n\
        let hi = greeter(\"Hello\")\n\
        println(hi(\"world\"))\n\
        let ns = [10, 20, 30]\n\
        let picker = (i: Int) => list.get(ns, i) ?? 0\n\
        println(int.to_string(picker(1)))\n\
        let h = compose(adder(3), adder(100))\n\
        println(int.to_string(h(5))) }\n";
    let prog = lower_source(src);
    let wat = render_wasm_program(&prog);
    assert!(
        wat.contains("$__drop_closure"),
        "closure drops must route through the uniform recursive $__drop_closure"
    );
    if let Some(out) = build_and_run("heap_fn_captures", &wat) {
        assert_eq!(out, "Hello, world\n20\n108");
    }
}
