    #[test]
    fn recursive_variant_to_string_executes_on_wasmtime() {
        // THE #1 LEVER (ADT brick 5b): a RECURSIVE custom variant `Expr = Lit(Int) | Add(Expr,
        // Expr) | Neg(Expr)` with a recursive `to_string` — nested-variant ctor construct
        // (`Add(Lit(1), Neg(Lit(2)))`), heap-field match binds passed to the recursive call, and
        // the GENERATED recursive drop `$__drop_Expr` (the only thing freeing the tree; a flat
        // free would leak grandchildren). The 2000x build+tos+drop loop is the LEAK GATE — a leak
        // or double-free traps via the freelist. Byte-matches v0.
        let src = "type Expr = Lit(Int) | Add(Expr, Expr) | Neg(Expr)\n\
            fn tos(e: Expr) -> String = match e {\n  \
              Lit(n)    => int.to_string(n),\n  \
              Add(l, r) => \"(\" + tos(l) + \" + \" + tos(r) + \")\",\n  \
              Neg(x)    => \"-\" + tos(x),\n}\n\
            fn main() -> Unit = {\n  \
              println(tos(Add(Lit(1), Neg(Lit(2)))))\n  \
              println(tos(Add(Neg(Add(Lit(3), Lit(4))), Neg(Neg(Lit(5))))))\n  \
              var acc = 0\n  for i in 0..2000 { acc = acc + string.len(tos(Add(Lit(i), Neg(Lit(i))))) }\n  \
              println(int.to_string(acc)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "__drop_Expr"), "the recursive drop fn must be generated + linked");
        if let Some(out) = build_and_run("recursive_variant", &render_wasm_program(&prog)) {
            assert_eq!(out, "(1 + -2)\n(-(3 + 4) + --5)\n25780");
        }
    }

    #[test]
    fn custom_variant_unit_statement_match_runs_one_arm() {
        // A UNIT-result custom-variant `match` in STATEMENT position (ADT brick 3, unit path):
        // only the TAKEN arm's effect runs — the regression guard for the both-arms
        // linearization that ran EVERY arm (`num sym eof` per call = a silent miscompile). v0 =
        // one line per call.
        let src = "type Tok = Num(Int) | Sym(Int) | Eof\n\
            fn show(t: Tok) -> Unit = match t {\n  \
              Num(n) => println(int.to_string(n * 2)),\n  \
              Sym(s) => println(int.to_string(s)),\n  \
              Eof    => println(\"end\"),\n}\n\
            fn main() -> Unit = { show(Num(5)); show(Sym(3)); show(Eof) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("custom_variant_unit", &render_wasm_program(&prog)) {
            assert_eq!(out, "10\n3\nend");
        }
    }

    #[test]
    fn freelist_reuses_a_freed_block() {
        // A1.2-render: alloc p1, free p1 (-> the free-list), then alloc p2 of the
        // SAME size. p2 must REUSE p1's freed block (FreeList.alloc reusing a
        // free-list block), so memory is bounded under churn — AND the reused block
        // must be correctly USABLE (re-initialized by list_new, writable, readable).
        // Prints `1` (p1 == p2, reuse happened) then `2` (p2[1] read back) — if the
        // reused block were corrupted the read-back would be wrong.
        let wat = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $p1 i32) (local $p2 i32)\n\
             \u{20}   (local.set $p1 (call $list_new (i32.const 3) (i32.const 3)))\n\
             \u{20}   (call $rc_dec (local.get $p1))\n\
             \u{20}   (local.set $p2 (call $list_new (i32.const 3) (i32.const 3)))\n\
             \u{20}   (call $list_set (local.get $p2) (i32.const 0) (i64.const 1))\n\
             \u{20}   (call $list_set (local.get $p2) (i32.const 1) (i64.const 2))\n\
             \u{20}   (call $list_set (local.get $p2) (i32.const 2) (i64.const 3))\n\
             \u{20}   (call $print_int (i64.extend_i32_s (i32.eq (local.get $p1) (local.get $p2))))\n\
             \u{20}   (call $print_int (call $list_get (local.get $p2) (i32.const 1))))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(out) = build_and_run("reuse", &wat) {
            assert_eq!(out, "1\n2", "second alloc must REUSE the freed block AND be usable");
        }
    }

    #[test]
    fn rc_cell_values_match_the_interpreter_on_wasmtime() {
        // `WasmExec.run_g` PROVES (in Coq, on the grounded bytes): `$rc_inc` takes
        // the rc cell +1 (rt_inc), and a valid `$rc_dec` takes it 1→0 (leak-freedom).
        // Confirm the PRODUCTION engine (wasmtime) computes the same cell values on
        // the renderer's actual `$rc_inc`/`$rc_dec` — grounding the interpreter model
        // against the real engine, so the WasmExec residual shrinks from "trust run_g
        // matches the wasm spec" to "wasmtime matches the spec" (a trusted engine, the
        // same trust level as the wat2wasm byte grounding). `$list_new` inits rc to 1.
        let inc = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $b i32)\n\
             \u{20}   (local.set $b (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $rc_inc (local.get $b))\n\
             \u{20}   (call $print_int (i64.extend_i32_s (i32.load (local.get $b)))))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(out) = build_and_run("rcinc_cell", &inc) {
            assert_eq!(out, "2", "rc_inc: cell 1→2 (rt_inc) — wasmtime must match run_g");
        }
        let dec = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $b i32)\n\
             \u{20}   (local.set $b (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $rc_dec (local.get $b))\n\
             \u{20}   (call $print_int (i64.extend_i32_s (i32.load (local.get $b)))))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(out) = build_and_run("rcdec_cell", &dec) {
            assert_eq!(out, "0", "rc_dec: cell 1→0 (leak-freedom) — wasmtime must match run_g");
        }
    }

    #[test]
    fn out_of_bounds_index_traps() {
        // The index-bounds memory-safety WALL: a `$list_set` with idx >= cap would
        // write OUTSIDE the block and corrupt memory (and the ownership checker —
        // which tracks RC, not bounds — would ACCEPT it). `$elem_addr` now traps
        // instead, so OOB is a controlled halt, never silent corruption.
        let oob = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $b i32)\n\
             \u{20}   (local.set $b (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $list_set (local.get $b) (i32.const 5) (i64.const 9)))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(success) = run_status("oob_idx", &oob) {
            assert!(!success, "an out-of-bounds index must TRAP (the bounds wall), not corrupt memory");
        }
        // An in-bounds index (0 <= idx < cap) must NOT trap.
        let ok = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $b i32)\n\
             \u{20}   (local.set $b (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $list_set (local.get $b) (i32.const 0) (i64.const 9)))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(success) = run_status("inbounds_idx", &ok) {
            assert!(success, "an in-bounds index must not trap");
        }
    }

    fn value_semantics_mir() -> MirFunction {
        // var a = [1,2,3]; var b = a; a[0] = 9; print a; print b
        let (a, b) = (ValueId(0), ValueId(1));
        MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::IntList(vec![1, 2, 3]) },
                Op::Dup { dst: b, src: a },
                Op::MakeUnique { v: a },
                Op::Call {
                    dst: None,
                    func: RtFn::ListSet,
                    args: vec![CallArg::Handle(a), CallArg::Imm(0), CallArg::Imm(9)],
                result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(a), CallArg::Label("a".into())] , result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(b), CallArg::Label("b".into())] , result: None },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn alloc_initializes_the_rc_cell_at_offset_zero() {
        // A1.1a: the heap block now carries a refcount cell at offset 0 — the
        // physical home of RuntimeModel.v's `read_rc m base` (RC_OFFSET = 0),
        // initialized to 1 (the `Alloc` +1 the proof's `exec` folds from). The
        // release path that decrements it is the next brick; today the renderer
        // is still Dec-free, so this is purely the foundation relayout.
        let wat = preamble();
        // `$list_new` writes rc = 1 at the rc offset, then len/cap at the shifted
        // offsets — proving the cell exists and is initialized (non-vacuous).
        assert!(
            wat.contains(&format!(
                "(i32.store (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))"
            )),
            "list_new must initialize the rc cell to 1 at RC_OFFSET"
        );
        // The relayout shifted len off offset 0 (where rc now lives): the header
        // is rc + len + cap = 12 bytes, and offsets are derived, not bare.
        assert_eq!(LIST_RC_OFFSET, 0);
        assert_eq!(LIST_LEN_OFFSET, 4);
        assert_eq!(LIST_CAP_OFFSET, 8);
        assert_eq!(LIST_HEADER, 12);
        // The release primitive now EXISTS (A1.1b): the preamble defines `$rc_dec`
        // — the realization of RuntimeModel.v's rt_dec that a `Drop` calls — and it
        // guards against a double-free (it traps on an already-0 cell).
        assert!(wat.contains("(func $rc_dec "), "the rc_dec release primitive must be defined");
        assert!(wat.contains("(unreachable)"), "rc_dec must trap on an already-freed cell");
    }

    #[test]
    fn wasm_runs_value_semantics_matching_rust() {
        let mir = value_semantics_mir();
        assert_eq!(verify_ownership(&mir), Ok(()));
        if let Some(out) = build_and_run("valuesem", &render_wasm(&mir)) {
            assert_eq!(out, "a=9,2,3\nb=1,2,3");
            // The dual-renderer thesis: the SAME MIR on the OTHER target agrees.
            let rust_out = crate::render_rust::render_rust(&mir);
            // (sanity that the two renderers were given the same program)
            assert!(rust_out.contains("v0[0] = 9"));
        }
    }

    #[test]
    fn wasm_push_through_alias_keeps_sibling_independent() {
        // var a = [1]; var b = a; a.push(2); print a; print b → a=[1,2], b=[1]
        let (a, b) = (ValueId(0), ValueId(1));
        let mir = MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::IntList(vec![1]) },
                Op::Dup { dst: b, src: a },
                Op::MakeUnique { v: a },
                Op::Call {
                    dst: Some(a),
                    func: RtFn::ListPush,
                    args: vec![CallArg::Handle(a), CallArg::Imm(2)],
                result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(a), CallArg::Label("a".into())] , result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(b), CallArg::Label("b".into())] , result: None },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
            ..Default::default()
        };
        assert_eq!(verify_ownership(&mir), Ok(()));
        if let Some(out) = build_and_run("push", &render_wasm(&mir)) {
            assert_eq!(out, "a=1,2\nb=1");
        }
    }

    #[test]
    fn self_hosted_string_from_codepoint_encodes_utf8() {
        // string.from_codepoint self-hosted: UTF-8 encode a scalar value, "" for an
        // invalid one (negative / surrogate / > 10FFFF). 72->"H", 12354->"あ" (3-byte),
        // -1->"" (empty, placed mid-stream so the last printed line is non-empty), 97->"a".
        let src = "fn main() -> Unit = {\n  \
            let a = string.from_codepoint(72)\n  println(a)\n  \
            let b = string.from_codepoint(12354)\n  println(b)\n  \
            let c = string.from_codepoint(0 - 1)\n  println(c)\n  \
            let d = string.from_codepoint(97)\n  println(d) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.from_codepoint"));
        if let Some(out) = build_and_run("string_from_codepoint", &render_wasm_program(&prog)) {
            assert_eq!(out, "H\nあ\n\na");
        }
    }

    #[test]
    fn self_hosted_list_binary_search_returns_some_index_or_none() {
        // list.binary_search over a sorted List[Int], replicating Rust std's loop so the
        // index byte-matches v0. [1,3,5,7,9]: find 5 -> Some(2), 7 -> Some(3), 1 -> Some(0),
        // 9 -> Some(4); 4 -> None, 0 -> None, 10 -> None. Printed via unwrap_or(-1).
        let src = "fn main() -> Unit = {\n  \
            let a = list.binary_search([1, 3, 5, 7, 9], 5) ?? (0 - 1)\n  let sa = int.to_string(a)\n  println(sa)\n  \
            let b = list.binary_search([1, 3, 5, 7, 9], 9) ?? (0 - 1)\n  let sb = int.to_string(b)\n  println(sb)\n  \
            let c = list.binary_search([1, 3, 5, 7, 9], 1) ?? (0 - 1)\n  let sc = int.to_string(c)\n  println(sc)\n  \
            let d = list.binary_search([1, 3, 5, 7, 9], 4) ?? (0 - 1)\n  let sd = int.to_string(d)\n  println(sd)\n  \
            let e = list.binary_search([1, 3, 5, 7, 9], 10) ?? (0 - 1)\n  let se = int.to_string(e)\n  println(se) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.binary_search"));
        if let Some(out) = build_and_run("list_binary_search", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n4\n0\n-1\n-1");
        }
    }

    #[test]
    fn self_hosted_list_tail_drops_the_head() {
        // list.tail = list.drop(xs,1): elements [1,n) as a fresh List[Int], empty for a
        // 0/1-element list. tail([10,20,30])=[20,30] ([0]=20, len 2); tail([42])=[] (len 0).
        let src = "fn main() -> Unit = {\n  \
            let a = list.tail([10, 20, 30])\n  let a0 = list.get_or(a, 0, 0)\n  let la = list.len(a)\n  let sa = int.to_string(a0)\n  println(sa)\n  let sla = int.to_string(la)\n  println(sla)\n  \
            let b = list.tail([42])\n  let lb = list.len(b)\n  let slb = int.to_string(lb)\n  println(slb) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.tail"));
        if let Some(out) = build_and_run("list_tail", &render_wasm_program(&prog)) {
            assert_eq!(out, "20\n2\n0");
        }
    }

    #[test]
    fn self_hosted_string_codepoint_decodes_first_char() {
        // string.codepoint self-hosted: first codepoint's scalar value, None for "".
        // "A"->65, "あ"->12354 (3-byte), "日"->26085, ""->None (printed as -1 via ??).
        let src = "fn main() -> Unit = {\n  \
            let a = string.codepoint(\"A\") ?? (0 - 1)\n  let sa = int.to_string(a)\n  println(sa)\n  \
            let b = string.codepoint(\"あ\") ?? (0 - 1)\n  let sb = int.to_string(b)\n  println(sb)\n  \
            let c = string.codepoint(\"日\") ?? (0 - 1)\n  let sc = int.to_string(c)\n  println(sc)\n  \
            let d = string.codepoint(\"\") ?? (0 - 1)\n  let sd = int.to_string(d)\n  println(sd) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.codepoint"));
        if let Some(out) = build_and_run("string_codepoint", &render_wasm_program(&prog)) {
            assert_eq!(out, "65\n12354\n26085\n-1");
        }
    }

    #[test]
    fn self_hosted_int_to_sized_saturating() {
        // int.to_int8/16/32_saturating self-hosted: clamp to the signed N-bit range.
        // to_int8_sat(200)=127, (-200)=-128, (50)=50; to_int16_sat(40000)=32767;
        // to_int32_sat(3000000000)=2147483647.
        let src = "fn main() -> Unit = {\n  \
            let a = int.to_int8_saturating(200)\n  let sa = int.to_string(a)\n  println(sa)\n  \
            let b = int.to_int8_saturating(0 - 200)\n  let sb = int.to_string(b)\n  println(sb)\n  \
            let c = int.to_int8_saturating(50)\n  let sc = int.to_string(c)\n  println(sc)\n  \
            let d = int.to_int16_saturating(40000)\n  let sd = int.to_string(d)\n  println(sd)\n  \
            let e = int.to_int32_saturating(3000000000)\n  let se = int.to_string(e)\n  println(se) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.to_int8_saturating"));
        if let Some(out) = build_and_run("int_sized_sat", &render_wasm_program(&prog)) {
            assert_eq!(out, "127\n-128\n50\n32767\n2147483647");
        }
    }

    #[test]
    fn self_hosted_int_64bit_conversions_are_bit_identity() {
        // int.to_uint64/from_int64/from_uint64 self-hosted: bit-identity over the shared
        // i64 repr. from_int64(to_int64(42))=42, from_uint64(to_uint64(99))=99,
        // to_uint64(7)=7.
        let src = "fn main() -> Unit = {\n  \
            let t = int.to_int64(42)\n  let b = int.from_int64(t)\n  let sb = int.to_string(b)\n  println(sb)\n  \
            let u = int.to_uint64(99)\n  let c = int.from_uint64(u)\n  let sc = int.to_string(c)\n  println(sc)\n  \
            let d = int.to_uint64(7)\n  let sd = int.to_string(d)\n  println(sd) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.from_int64"));
        assert!(prog.functions.iter().any(|f| f.name == "int.to_uint64"));
        if let Some(out) = build_and_run("int_widen", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\n99\n7");
        }
    }

    #[test]
    fn self_hosted_int_to_unsigned_narrowing() {
        // int.to_uint8/16/32 self-hosted: low N bits, zero-extended (band mask).
        // to_uint8(-1)=255, to_uint8(300)=44, to_uint16(-1)=65535, to_uint32(-1)=4294967295.
        let src = "fn main() -> Unit = {\n  \
            let a = int.to_uint8(0 - 1)\n  let sa = int.to_string(a)\n  println(sa)\n  \
            let b = int.to_uint8(300)\n  let sb = int.to_string(b)\n  println(sb)\n  \
            let c = int.to_uint16(0 - 1)\n  let sc = int.to_string(c)\n  println(sc)\n  \
            let d = int.to_uint32(0 - 1)\n  let sd = int.to_string(d)\n  println(sd) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.to_uint8"));
        if let Some(out) = build_and_run("int_uint", &render_wasm_program(&prog)) {
            assert_eq!(out, "255\n44\n65535\n4294967295");
        }
    }

    #[test]
    fn self_hosted_int_from_sized_widening() {
        // int.from_int8/16/32 + from_uint8/16/32 self-hosted: identity over v1's i64-uniform
        // scalars. Round-trip through to_*: from_int8(to_int8(200))=-56,
        // from_uint8(to_uint8(200))=200, from_int16(to_int16(40000))=-25536,
        // from_uint16(to_uint16(70000))=4464.
        let src = "fn main() -> Unit = {\n  \
            let a = int.to_int8(200)\n  let fa = int.from_int8(a)\n  let sa = int.to_string(fa)\n  println(sa)\n  \
            let b = int.to_uint8(200)\n  let fb = int.from_uint8(b)\n  let sb = int.to_string(fb)\n  println(sb)\n  \
            let c = int.to_int16(40000)\n  let fc = int.from_int16(c)\n  let sc = int.to_string(fc)\n  println(sc)\n  \
            let d = int.to_uint16(70000)\n  let fd = int.from_uint16(d)\n  let sd = int.to_string(fd)\n  println(sd) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.from_int8"));
        assert!(prog.functions.iter().any(|f| f.name == "int.from_uint16"));
        if let Some(out) = build_and_run("int_from_sized", &render_wasm_program(&prog)) {
            assert_eq!(out, "-56\n200\n-25536\n4464");
        }
    }

    #[test]
    fn self_hosted_int_to_unsigned_saturating() {
        // int.to_uint8/16/32/64_saturating self-hosted: clamp to [0, 2^N-1] (scalar value,
        // no Option). to_uint8_sat(300)=255, (-5)=0, (100)=100; to_uint16_sat(70000)=65535;
        // to_uint64_sat(-1)=0.
        let src = "fn main() -> Unit = {\n  \
            let a = int.to_uint8_saturating(300)\n  let sa = int.to_string(a)\n  println(sa)\n  \
            let b = int.to_uint8_saturating(0 - 5)\n  let sb = int.to_string(b)\n  println(sb)\n  \
            let c = int.to_uint8_saturating(100)\n  let sc = int.to_string(c)\n  println(sc)\n  \
            let d = int.to_uint16_saturating(70000)\n  let sd = int.to_string(d)\n  println(sd)\n  \
            let e = int.to_uint64_saturating(0 - 1)\n  let se = int.to_string(e)\n  println(se) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.to_uint8_saturating"));
        if let Some(out) = build_and_run("int_usat", &render_wasm_program(&prog)) {
            assert_eq!(out, "255\n0\n100\n65535\n0");
        }
    }

    #[test]
    fn self_hosted_option_is_some_is_none() {
        // option.is_some/is_none self-hosted: read the materialized Option's header length
        // (Some=1, None=0). is_some(Some 5)=T, is_some(None)=F, is_none(None)=T.
        let src = "fn main() -> Unit = {\n  \
            let a: Option[Int] = Some(5)\n  let s1 = option.is_some(a)\n  if s1 then println(\"T\") else println(\"F\")\n  \
            let b: Option[Int] = None\n  let s2 = option.is_some(b)\n  if s2 then println(\"T2\") else println(\"F2\")\n  \
            let s3 = option.is_none(b)\n  if s3 then println(\"T3\") else println(\"F3\") }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.is_some"));
        assert!(prog.functions.iter().any(|f| f.name == "option.is_none"));
        if let Some(out) = build_and_run("option_pred", &render_wasm_program(&prog)) {
            assert_eq!(out, "T\nF2\nT3");
        }
    }

    #[test]
    fn self_hosted_option_unwrap_or_function() {
        // option.unwrap_or (the function form of ??): the Some payload, else the default.
        // unwrap_or(Some(42), 0)=42, unwrap_or(None, 7)=7.
        let src = "fn main() -> Unit = {\n  \
            let a: Option[Int] = Some(42)\n  let x = option.unwrap_or(a, 0)\n  let sx = int.to_string(x)\n  println(sx)\n  \
            let b: Option[Int] = None\n  let y = option.unwrap_or(b, 7)\n  let sy = int.to_string(y)\n  println(sy) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.unwrap_or"));
        if let Some(out) = build_and_run("option_unwrap_or_fn", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\n7");
        }
    }

    #[test]
    fn self_hosted_option_to_list() {
        // option.to_list: Some(x) -> [x], None -> []. to_list(Some 9) has len 1 + [0]=9;
        // to_list(None) has len 0. (List[Int]; read back via list.len + list.get_or.)
        let src = "fn main() -> Unit = {\n  \
            let a: Option[Int] = Some(9)\n  let la = option.to_list(a)\n  let na = list.len(la)\n  let sna = int.to_string(na)\n  println(sna)\n  \
            let ea = list.get_or(la, 0, 0)\n  let sea = int.to_string(ea)\n  println(sea)\n  \
            let b: Option[Int] = None\n  let lb = option.to_list(b)\n  let nb = list.len(lb)\n  let snb = int.to_string(nb)\n  println(snb) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.to_list"));
        if let Some(out) = build_and_run("option_to_list", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n9\n0");
        }
    }

    #[test]
    fn self_hosted_bytes_string_reads() {
        // bytes.to_string_lossy / read_string_at / read_string_be self-hosted: a Bytes is the
        // [rc][len][cap][data] byte block; each builds a FRESH String by a prim byte-copy of the
        // selected window. to_string_lossy(from_string "hello")="hello"; read_string_at(b,1,3)
        // ="ell" (bytes 1..4); read_string_be over [0,0,0,3,'h','i','j'] reads the BE-4 length
        // prefix (3) then copies the 3 body bytes -> "hij". Byte-matches v0 for valid UTF-8.
        let src = "fn main() -> Unit = {\n  \
            let b = bytes.from_string(\"hello\")\n  \
            println(bytes.to_string_lossy(b))\n  \
            println(bytes.read_string_at(b, 1, 3))\n  \
            let p = bytes.from_list([0, 0, 0, 3, 104, 105, 106])\n  \
            println(bytes.read_string_be(p, 0)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.to_string_lossy"));
        assert!(prog.functions.iter().any(|f| f.name == "bytes.read_string_be"));
        if let Some(out) = build_and_run("bytes_string_reads", &render_wasm_program(&prog)) {
            assert_eq!(out, "hello\nell\nhij");
        }
    }

    #[test]
    fn self_hosted_datetime_format() {
        // datetime.format(ts, pattern): strftime specifier substitution (%Y %m %d %H %M %S) in the
        // SAME sequential string.replace order as the native + v0-wasm backends, composing the
        // self-hosted datetime.year/.../second + string.replace + __dt_pad zero-padding. ts=0 = the
        // unix epoch 1970-01-01T00:00:00Z; ts=86400 = 1970-01-02.
        let src = "fn main() -> Unit = {\n  \
            println(datetime.format(0, \"%Y-%m-%d %H:%M:%S\"))\n  \
            println(datetime.format(86400, \"%d/%m/%Y\")) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "datetime.format"));
        if let Some(out) = build_and_run("datetime_format", &render_wasm_program(&prog)) {
            assert_eq!(out, "1970-01-01 00:00:00\n02/01/1970");
        }
    }

    #[test]
    fn self_hosted_json_scalar() {
        // json scalar constructors + accessors over the SHARED Value repr (value_core's tag@4 block).
        // from_int(7) |> as_int = Some 7; from_bool(true) |> as_bool = Some true; a TAG MISMATCH ->
        // None (as_bool on an Int Value, as_int on null) -> the `??` fallback. Exercises the
        // materialized-Option return + DropValue (flat scalar drop) end-to-end through v1.
        let src = "import json\nfn main() -> Unit = {\n  \
            let vi = json.from_int(7)\n  \
            let oi = json.as_int(vi)\n  let i = oi ?? 0\n  println(int.to_string(i))\n  \
            let vf = json.from_float(3.0)\n  \
            let ofi = json.as_int(vf)\n  let fi = ofi ?? 0\n  println(int.to_string(fi))\n  \
            let vb = json.from_bool(true)\n  \
            let ob = json.as_bool(vb)\n  let b = ob ?? false\n  let bi = if b then 1 else 0\n  println(int.to_string(bi))\n  \
            let on = json.as_bool(vi)\n  let nb = on ?? false\n  let nbi = if nb then 1 else 0\n  println(int.to_string(nbi))\n  \
            let vn = json.null()\n  \
            let onv = json.as_int(vn)\n  let nv = onv ?? 0\n  println(int.to_string(nv)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "json.from_int"));
        assert!(prog.functions.iter().any(|f| f.name == "json.as_int"));
        if let Some(out) = build_and_run("json_scalar", &render_wasm_program(&prog)) {
            // as_int(Int 7)=7; as_int(Float 3.0)=3 (the f64->i64 WIDENING); as_bool(Bool true)=1;
            // as_bool(Int)=None->0; as_int(null)=None->0. Materialized-Option return + DropValue e2e.
            assert_eq!(out, "7\n3\n1\n0\n0");
        }
    }

    #[test]
    fn self_hosted_json_string() {
        // json STR-payload over the SHARED Value repr (tag 4 = Str, the payload String @12). from_string
        // builds a Str Value owning a deep copy; as_string returns Option[String] (the repr-poly 0-or-1-
        // element DynListStr materialization, same path as list.get_str). as_string(Str "hi")=Some("hi")
        // -> match "hi"; as_string(Int)=None -> "none". The `??` lines exercise json.as_string in the
        // heap-`??` path (the case originally dodged with `match`, now CLOSED via option.unwrap_or_str):
        // as_string(Str "Z") ?? "X" = "Z"; as_string(Int) ?? "X" = "X". The trailing 4000-iter loop
        // builds + drops a Str Value AND its Option each round (string.len reads the borrowed Some
        // payload = 5): bounded, no leak/double-free — DropValue (tag-dispatched Str free) + Option e2e.
        let src = "import json\nfn main() -> Unit = {\n  \
            let vs = json.from_string(\"hi\")\n  \
            let os = json.as_string(vs)\n  match os {\n    Some(v) => println(v),\n    None => println(\"none\"),\n  }\n  \
            let vi = json.from_int(5)\n  \
            let oi = json.as_string(vi)\n  match oi {\n    Some(v) => println(v),\n    None => println(\"none\"),\n  }\n  \
            let vz = json.from_string(\"Z\")\n  let oz = json.as_string(vz)\n  let sz = oz ?? \"X\"\n  println(sz)\n  \
            let vj = json.from_int(9)\n  let sj = json.as_string(vj) ?? \"X\"\n  println(sj)\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let vx = json.from_string(\"abcde\")\n    let ox = json.as_string(vx)\n    \
              match ox { Some(s) => { let n = string.len(s)\n last = n }, None => { last = 0 }, }\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "json.from_string"));
        assert!(prog.functions.iter().any(|f| f.name == "json.as_string"));
        if let Some(out) = build_and_run("json_string", &render_wasm_program(&prog)) {
            // as_string(Str "hi")=Some->match->"hi"; as_string(Int 5)=None->"none"; as_string(Str "Z")
            // ?? "X" = "Z"; as_string(Int 9) ?? "X" = "X"; loop last = string.len("abcde") = 5.
            assert_eq!(out, "hi\nnone\nZ\nX\n5");
        }
    }

    #[test]
    fn self_hosted_float_to_string_matches_v0_dragon4() {
        // The hard dtoa self-host: `float.to_string` is a FAITHFUL Dragon4 (Steele & White)
        // free-format shortest correctly-rounded decimal over the prim bignum floor — byte-
        // matching v0's `format!("{}", x)` (shortest round-trip, ALWAYS full decimal, never
        // scientific; integer-valued floats get a ".0"). This e2e exercises:
        //   - integer-valued (".0" suffix): 1.0, 100.0, 2.0
        //   - leading-zero negative-k (the signed-k slot fix; load32 would have dropped the sign):
        //     0.001, 0.0001, 0.000001
        //   - shortest round-trip: 1.0/3.0 = 0.3333333333333333, 0.1+0.2 = 0.30000000000000004
        //   - full-decimal large (no sci notation): 1e20 = 100000000000000000000.0
        //   - specials: +inf / -inf / NaN, signed zero -0.0.
        // The exhaustive correctness gate (thousands of random + boundary f64) is the
        // out-of-tree dual-oracle (corpus-wall does not check output bytes).
        let src = "fn show(f: Float) -> Unit = println(float.to_string(f))\n\
            fn main() -> Unit = {\n  \
              show(1.0)\n  show(100.0)\n  show(2.0)\n  \
              show(0.001)\n  show(0.0001)\n  show(0.000001)\n  \
              show(1.0 / 3.0)\n  show(0.1 + 0.2)\n  \
              show(1e20)\n  \
              show(1.0 / 0.0)\n  show(-1.0 / 0.0)\n  show(0.0 / 0.0)\n  \
              show(-0.0)\n  show(0.5)\n }\n";
        let prog = lower_source(src);
        assert!(
            prog.functions.iter().any(|f| f.name == "float.to_string"),
            "float.to_string must be auto-linked"
        );
        if let Some(out) = build_and_run("float_to_string", &render_wasm_program(&prog)) {
            assert_eq!(
                out,
                "1.0\n100.0\n2.0\n\
                 0.001\n0.0001\n0.000001\n\
                 0.3333333333333333\n0.30000000000000004\n\
                 100000000000000000000.0\n\
                 inf\n-inf\nNaN\n\
                 -0.0\n0.5"
            );
        }
    }

    #[test]
    fn record_result_match_subject_recursive_drop_executes_on_wasmtime() {
        // HOLE-1 closed: a `match make(n) { ok(r) => .., err(e) => .. }` over a record-Ok
        // `Result[Rec, String]` SUBJECT (Rec = { tags: List[String], name: String }). The subject's
        // scope-end drop is the recursive `Op::DropWrapperRec { is_result: true }` into the generated
        // `$__drop_Rec` — at the Ok tag it recurses into the @12 record (frees the tags List[String]
        // + name String + record block), at the Err tag it `rc_dec`s the @12 String, then frees the
        // wrapper. A flat `DropListStr` would free ONLY the @12 handle and LEAK tags+name (the
        // gate-invisible HOLE-1 leak). The 4000x build+match+drop loop is the LEAK GATE — a leak or
        // double-free traps via the freelist / grows memory; both ok and err arms byte-match v0.
        let src = "type Rec = { tags: List[String], name: String }\n\
            fn make(n: Int) -> Result[Rec, String] =\n  \
              if n > 0 then ok({ tags: [\"alpha\", \"beta\", \"gamma\"], name: \"record\" }) else err(\"empty\")\n\
            fn describe(n: Int) -> String = match make(n) {\n  \
              ok(r)  => \"ok:\" + int.to_string(string.len(r.name) + list.len(r.tags)),\n  \
              err(e) => \"err:\" + e,\n}\n\
            fn main() -> Unit = {\n  \
              println(describe(1))\n  \
              println(describe(0))\n  \
              var acc = 0\n  \
              for i in 0..4000 { acc = acc + string.len(describe(if i % 3 == 0 then 0 else 1)) }\n  \
              println(int.to_string(acc)) }\n";
        let prog = lower_source(src);
        assert!(
            prog.functions.iter().any(|f| f.name == "__drop_Rec"),
            "the record recursive drop $__drop_Rec must be generated + linked"
        );
        assert!(
            prog.functions.iter().any(|f| f.name == "describe"),
            "the record-Ok match-subject fn must lower (not wall)"
        );
        if let Some(out) = build_and_run("record_result_subject", &render_wasm_program(&prog)) {
            // describe(1) = ok, len("record")=6 + len(tags)=3 = 9; describe(0) = err "empty".
            // loop i in 0..4000: i%3==0 (1334) -> err "empty" len 9; else (2666) -> ok "ok:9" len 4.
            assert_eq!(out, "ok:9\nerr:empty\n22670");
        }
    }
