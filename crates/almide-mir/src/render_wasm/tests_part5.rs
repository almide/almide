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
    // A NESTED List[List[Int]] element is OUT of subset (v1 does not materialize the inner-list
    // handles): it routes to the never-registered `list.to_string_x`, so the fn WALLS (Unsupported)
    // rather than emit wrong bytes or invalid wasm. This is the all-or-nothing soundness boundary.
    use crate::lower::LowerError;
    use crate::render_wasm::{try_render_wasm_program, unlinked_call_names};
    let src = "fn main() -> Unit = {\n  \
        let xs: List[List[Int]] = [[1, 2], [3]]\n  \
        println(\"${xs}\") }\n";
    let prog = lower_source(src);
    assert!(
        unlinked_call_names(&prog).contains("list.to_string_x"),
        "a nested List interp must route to the unlinked list.to_string_x, got {:?}",
        unlinked_call_names(&prog)
    );
    match try_render_wasm_program(&prog) {
        Err(LowerError::Unsupported(_)) => {}
        Err(other) => panic!("expected Unsupported, got {other:?}"),
        Ok(_) => panic!("a nested-List interp must wall, not render to (possibly invalid) wasm"),
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
fn spread_heap_field_record_walls_safely() {
    // A SPREAD heap-field record (`R { ...r, n: 9 }`) is OUT of the construction subset (the
    // spread base's fields are not re-materialized here) — it must DEFER to a safe Opaque
    // block, NOT a masked record (a masked drop over garbage slots would `rc_dec` junk and
    // trap). The guard: a deferred spread record is NOT in `record_masks`, so its drop is a
    // plain `Drop` (no recursive slot free), and its heap-field reads fall to the safe
    // container-grain alias (no slot dereference). This is the WALL discipline for the
    // not-yet-materialized spread shape — memory-safe, just deferred-functional.
    let src = "type R = { name: String, n: Int }\n\
        fn main() -> Unit = {\n  \
        let r = R { name: \"a\", n: 1 }\n  \
        let r2 = R { ...r, n: 9 }\n  \
        println(int.to_string(r.n)) }\n";
    let prog = lower_source(src);
    let main = prog.functions.iter().find(|f| f.name == "main").expect("main");
    // `r2` (the spread) must NOT be in any heap_slot_mask — only the genuinely-materialized
    // `r` is masked. (A spread record incorrectly masked would free garbage slots.)
    assert!(
        main.heap_slot_masks.len() <= 1,
        "only the materialized record `r` may carry a heap-slot mask, not the deferred spread"
    );
    // The cert stays balanced (no double-free from the deferred spread).
    for f in &prog.functions {
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
        // r.n reads correctly (1); r2 is deferred but never dereferenced unsafely.
        assert_eq!(out, "1");
    }
}




