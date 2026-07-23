// render_wasm test suite — part 3 of 3 (self-hosted stdlib e2e + rc/runtime tests).
    #[test]
    fn bundled_int_to_string_works_without_user_impl() {
        // The v0-parity target: `int.to_string(n)` is a BUILT-IN — the program writes NO
        // to_str. The v1 linker auto-includes the self-hosted int_to_string (+ its helpers)
        // from stdlib/int_to_string.almd. The Stop-hook's example program prints 0..9.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 10 {\n    println(int.to_string(i))\n    i = i + 1\n  } }\n";
        let prog = lower_source(src);
        assert!(
            prog.functions.iter().any(|f| f.name == "int.to_string"),
            "int.to_string must be auto-linked"
        );
        if let Some(out) = build_and_run("bundled_itos", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\n1\n2\n3\n4\n5\n6\n7\n8\n9");
        }
    }

    #[test]
    fn self_hosted_string_len_counts_codepoints() {
        // The first FUNCTION-call stdlib self-host: string.len (a Module call → 1:1 IR/MIR,
        // no mir>ir gate issue). v0's string.len is the CODEPOINT count, so the self-hosted
        // impl decodes UTF-8: len("hello")=5, len("日本語")=3 (3 chars, 9 bytes).
        let src = "fn main() -> Unit = {\n  \
            let n = string.len(\"hello\")\n  \
            println(int.to_string(n))\n  \
            let m = string.len(\"日本語\")\n  \
            println(int.to_string(m)) }\n";
        let prog = lower_source(src);
        assert!(
            prog.functions.iter().any(|f| f.name == "string.len"),
            "string.len must be auto-linked"
        );
        if let Some(out) = build_and_run("string_len", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\n3");
        }
    }

    #[test]
    fn self_hosted_string_repeat_builds_the_repetition() {
        // string.repeat(s, n) self-hosted: alloc len(s)*n bytes, byte-copy s n times.
        // repeat("ab",3)="ababab", repeat("x",5)="xxxxx", byte-matching v0's s.repeat(n).
        let src = "fn main() -> Unit = {\n  \
            let a = string.repeat(\"ab\", 3)\n  \
            println(a)\n  \
            let b = string.repeat(\"x\", 5)\n  \
            println(b) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.repeat"));
        if let Some(out) = build_and_run("string_repeat", &render_wasm_program(&prog)) {
            assert_eq!(out, "ababab\nxxxxx");
        }
    }

    #[test]
    fn string_accumulator_parser_tco_executes_on_wasmtime() {
        // The parser-combinator shape (csv/toml): a tail-self-recursive STRING accumulator
        // `scan(text, pos+1, acc + c)` returning a TUPLE `(String, Int)`. Two fixes meet here:
        // (1) the append-accumulator TCO now covers a `ConcatStr` slot (not just `ConcatList`) +
        // a tuple-result base; (2) the loop body's heap `let c = string.get(...) ?? ""` makes the
        // base-check a BLOCK-TAIL `if`, which now routes through try_lower_unit_if so the loop
        // BRANCHES — before, a block-tail `if` fell straight to lower_branch (ran BOTH arms with
        // the cond elided), so the loop ran exactly ONCE: a silent miscompile (v0 "hello", v1 "h").
        // The 2000x re-scan is the LEAK gate (the String slot's drop-old/alloc-new per iter).
        let src = "fn scan(text: String, pos: Int, acc: String) -> (String, Int) =\n  \
              if pos >= string.len(text) then (acc, pos)\n  \
              else { let c = string.get(text, pos) ?? \"\"; if c == \",\" then (acc, pos) else scan(text, pos + 1, acc + c) }\n\
            fn main() -> Unit = {\n  \
              let (s, p) = scan(\"hello,world\", 0, \"\")\n  \
              println(s + \" @ \" + int.to_string(p))\n  \
              var n = 0\n  for i in 0..2000 { let (t, q) = scan(\"hello,world\", 0, \"\"); n = n + string.len(t) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_acc_parser", &render_wasm_program(&prog)) {
            assert_eq!(out, "hello @ 5\n10000");
        }
    }

    #[test]
    fn self_hosted_string_is_empty_tests_the_length() {
        // string.is_empty(s) self-hosted: the header byte-length is 0 iff empty.
        let src = "fn main() -> Unit = {\n  \
            let a = string.is_empty(\"\")\n  \
            if a then println(\"empty\") else println(\"nonempty\")\n  \
            let b = string.is_empty(\"x\")\n  \
            if b then println(\"empty\") else println(\"nonempty\") }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.is_empty"));
        if let Some(out) = build_and_run("string_is_empty", &render_wasm_program(&prog)) {
            assert_eq!(out, "empty\nnonempty");
        }
    }

    #[test]
    fn self_hosted_math_int_abs_max_min() {
        // Scalar Int math self-hosted from one shared file (math_int.almd, grouped registry
        // entry): pure scalar if/arithmetic, no heap/prim. abs(-5)=5, max(3,7)=7, min(3,7)=3.
        let src = "fn main() -> Unit = {\n  \
            let a = math.abs(0 - 5)\n  println(int.to_string(a))\n  \
            let b = math.max(3, 7)\n  println(int.to_string(b))\n  \
            let c = math.min(3, 7)\n  println(int.to_string(c)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "math.abs"));
        assert!(prog.functions.iter().any(|f| f.name == "math.min"));
        if let Some(out) = build_and_run("math_int", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\n7\n3");
        }
    }

    #[test]
    fn self_hosted_list_len_reads_the_element_count() {
        // The first LIST self-host: list.len reads the header element-count field. Needs
        // the generic prim.handle[A] (now accepts List too). len([1,2,3])=3, len(5 elems)=5.
        let src = "fn main() -> Unit = {\n  \
            let n = list.len([1, 2, 3])\n  println(int.to_string(n))\n  \
            let m = list.len([10, 20, 30, 40, 50])\n  println(int.to_string(m)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.len"));
        if let Some(out) = build_and_run("list_len", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n5");
        }
    }

    #[test]
    fn self_hosted_list_sum_iterates_elements() {
        // list.sum self-hosted: iterate the i64 element slots (prim.load64) and add.
        // sum([1,2,3,4])=10, sum([100,200])=300, matching v0's fold(0,+).
        let src = "fn main() -> Unit = {\n  \
            let s = list.sum([1, 2, 3, 4])\n  println(int.to_string(s))\n  \
            let t = list.sum([100, 200])\n  println(int.to_string(t)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.sum"));
        if let Some(out) = build_and_run("list_sum", &render_wasm_program(&prog)) {
            assert_eq!(out, "10\n300");
        }
    }

    #[test]
    fn self_hosted_list_is_empty_tests_the_count() {
        // list.is_empty self-hosted: the element-count field is 0 iff empty.
        let src = "fn main() -> Unit = {\n  \
            let a = list.is_empty([1, 2])\n  \
            if a then println(\"empty\") else println(\"nonempty\") }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.is_empty"));
        if let Some(out) = build_and_run("list_is_empty", &render_wasm_program(&prog)) {
            assert_eq!(out, "nonempty");
        }
    }

    #[test]
    fn self_hosted_string_slice_uses_codepoint_indices() {
        // string.slice self-hosted: codepoint indices, clamped, UTF-8 byte-range copy.
        // slice("hello",1,4)="ell"; slice("日本語",0,2)="日本" (codepoint, not byte, indices).
        let src = "fn main() -> Unit = {\n  \
            let a = string.slice(\"hello\", 1, 4)\n  println(a)\n  \
            let b = string.slice(\"日本語\", 0, 2)\n  println(b) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.slice"));
        if let Some(out) = build_and_run("string_slice", &render_wasm_program(&prog)) {
            assert_eq!(out, "ell\n日本");
        }
    }

    #[test]
    fn self_hosted_list_insert_and_remove_at() {
        // list.insert/remove_at self-hosted: length-changing List[Int] copies. insert at a
        // clamped index, remove a valid index (else unchanged). insert([1,2,3],1,9) =
        // [1,9,2,3] (len 4, [1]=9); insert([1,2],9,7) appends ([2]=7, len 3);
        // remove_at([4,5,6,7],1) = [4,6,7] ([1]=6, len 3); remove_at([8,9],5) unchanged.
        let src = "fn main() -> Unit = {\n  \
            let a = list.insert([1, 2, 3], 1, 9)\n  let a1 = list.get_or(a, 1, 0)\n  let la = list.len(a)\n  let sa = int.to_string(a1)\n  println(sa)\n  let sla = int.to_string(la)\n  println(sla)\n  \
            let b = list.insert([1, 2], 9, 7)\n  let b2 = list.get_or(b, 2, 0)\n  let sb = int.to_string(b2)\n  println(sb)\n  \
            let c = list.remove_at([4, 5, 6, 7], 1)\n  let c1 = list.get_or(c, 1, 0)\n  let lc = list.len(c)\n  let sc = int.to_string(c1)\n  println(sc)\n  let slc = int.to_string(lc)\n  println(slc)\n  \
            let d = list.remove_at([8, 9], 5)\n  let ld = list.len(d)\n  let sld = int.to_string(ld)\n  println(sld) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.insert"));
        assert!(prog.functions.iter().any(|f| f.name == "list.remove_at"));
        if let Some(out) = build_and_run("list_modify2", &render_wasm_program(&prog)) {
            assert_eq!(out, "9\n4\n7\n6\n3\n2");
        }
    }

    #[test]
    fn self_hosted_list_set_and_swap() {
        // list.set/swap self-hosted: a fresh same-length List[Int] copy with one/two slots
        // changed; OOB index no-ops (v0's `i as usize`). set([10,20,30],1,99)[1]=99;
        // swap([1,2,3,4],0,3) → [0]=4,[3]=1; set([5,6,7],9,0) unchanged → [0]=5.
        let src = "fn main() -> Unit = {\n  \
            let r = list.set([10, 20, 30], 1, 99)\n  let a = list.get_or(r, 1, 0)\n  let sa = int.to_string(a)\n  println(sa)\n  \
            let s = list.swap([1, 2, 3, 4], 0, 3)\n  let b = list.get_or(s, 0, 0)\n  let sb = int.to_string(b)\n  println(sb)\n  \
            let c = list.get_or(s, 3, 0)\n  let sc = int.to_string(c)\n  println(sc)\n  \
            let t = list.set([5, 6, 7], 9, 0)\n  let d = list.get_or(t, 0, 0)\n  let sd = int.to_string(d)\n  println(sd) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.set"));
        assert!(prog.functions.iter().any(|f| f.name == "list.swap"));
        if let Some(out) = build_and_run("list_modify", &render_wasm_program(&prog)) {
            assert_eq!(out, "99\n4\n1\n5");
        }
    }

    #[test]
    fn self_hosted_math_factorial_and_choose() {
        // math.factorial = (1..=n).product() (1 for n<=1); math.choose = C(n,k) via the
        // running multiply-before-divide product. factorial(0)=1, (5)=120, (-3)=1;
        // choose(5,2)=10, choose(10,3)=120, choose(5,7)=0.
        let src = "fn main() -> Unit = {\n  \
            let a = math.factorial(0)\n  let sa = int.to_string(a)\n  println(sa)\n  \
            let b = math.factorial(5)\n  let sb = int.to_string(b)\n  println(sb)\n  \
            let c = math.factorial(0 - 3)\n  let sc = int.to_string(c)\n  println(sc)\n  \
            let d = math.choose(5, 2)\n  let sd = int.to_string(d)\n  println(sd)\n  \
            let e = math.choose(10, 3)\n  let se = int.to_string(e)\n  println(se)\n  \
            let f = math.choose(5, 7)\n  let sf = int.to_string(f)\n  println(sf) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "math.factorial"));
        assert!(prog.functions.iter().any(|f| f.name == "math.choose"));
        if let Some(out) = build_and_run("math_choose", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n120\n1\n10\n120\n0");
        }
    }

    #[test]
    fn self_hosted_int_to_sized_signed() {
        // int.to_int8/16/32/64 self-hosted: low N bits, sign-extended. to_int8(200)=-56,
        // to_int8(100)=100, to_int16(40000)=-25536, to_int32(3000000000)=-1294967296,
        // to_int64(123)=123.
        let src = "fn main() -> Unit = {\n  \
            let a = int.to_int8(200)\n  let sa = int.to_string(a)\n  println(sa)\n  \
            let b = int.to_int8(100)\n  let sb = int.to_string(b)\n  println(sb)\n  \
            let c = int.to_int16(40000)\n  let sc = int.to_string(c)\n  println(sc)\n  \
            let d = int.to_int32(3000000000)\n  let sd = int.to_string(d)\n  println(sd)\n  \
            let e = int.to_int64(123)\n  let se = int.to_string(e)\n  println(se) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.to_int8"));
        assert!(prog.functions.iter().any(|f| f.name == "int.to_int32"));
        if let Some(out) = build_and_run("int_sized", &render_wasm_program(&prog)) {
            assert_eq!(out, "-56\n100\n-25536\n-1294967296\n123");
        }
    }

    #[test]
    fn self_hosted_int_wrap_and_narrow() {
        // int.wrap_add/wrap_mul/to_u32/to_u8 self-hosted (band/mask over the prim floor).
        // wrap_add(250,10,8)=260&0xFF=4; wrap_mul(20,20,8)=400&0xFF=144; to_u8(-1)=255;
        // to_u32(-1)=4294967295; wrap_add(1,2,64)=3 (no mask).
        let src = "fn main() -> Unit = {\n  \
            let a = int.wrap_add(250, 10, 8)\n  let sa = int.to_string(a)\n  println(sa)\n  \
            let b = int.wrap_mul(20, 20, 8)\n  let sb = int.to_string(b)\n  println(sb)\n  \
            let c = int.to_u8(0 - 1)\n  let sc = int.to_string(c)\n  println(sc)\n  \
            let d = int.to_u32(0 - 1)\n  let sd = int.to_string(d)\n  println(sd)\n  \
            let e = int.wrap_add(1, 2, 64)\n  let se = int.to_string(e)\n  println(se) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.wrap_add"));
        assert!(prog.functions.iter().any(|f| f.name == "int.to_u8"));
        if let Some(out) = build_and_run("int_wrap", &render_wasm_program(&prog)) {
            assert_eq!(out, "4\n144\n255\n4294967295\n3");
        }
    }

    #[test]
    fn self_hosted_int_log2_ceil() {
        // int.log2_ceil self-hosted (reuse __clz): n<=1 → 0, else 64 - clz(n-1) =
        // bit_width(n-1). ceil: 1→0, 2→1, 3→2, 4→2, 5→3, 8→3, 9→4.
        let src = "fn main() -> Unit = {\n  \
            let a = int.log2_ceil(2)\n  let sa = int.to_string(a)\n  println(sa)\n  \
            let b = int.log2_ceil(3)\n  let sb = int.to_string(b)\n  println(sb)\n  \
            let c = int.log2_ceil(4)\n  let sc = int.to_string(c)\n  println(sc)\n  \
            let d = int.log2_ceil(5)\n  let sd = int.to_string(d)\n  println(sd)\n  \
            let e = int.log2_ceil(9)\n  let se = int.to_string(e)\n  println(se)\n  \
            let f = int.log2_ceil(1)\n  let sf = int.to_string(f)\n  println(sf) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.log2_ceil"));
        if let Some(out) = build_and_run("int_log2_ceil", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n2\n2\n3\n4\n0");
        }
    }

    #[test]
    fn self_hosted_string_is_digit_ascii_byte_scan() {
        // string.is_digit self-hosted: !empty && every codepoint an ASCII digit, via a
        // byte scan over [48,57]. is_digit("12345")=true, is_digit("12a45")=false,
        // is_digit("")=false, is_digit("日")=false (multibyte lead byte >= 0x80).
        let src = "fn main() -> Unit = {\n  \
            let a = string.is_digit(\"12345\")\n  if a then println(\"T1\") else println(\"F1\")\n  \
            let b = string.is_digit(\"12a45\")\n  if b then println(\"T2\") else println(\"F2\")\n  \
            let c = string.is_digit(\"\")\n  if c then println(\"T3\") else println(\"F3\")\n  \
            let d = string.is_digit(\"日\")\n  if d then println(\"T4\") else println(\"F4\") }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.is_digit"));
        if let Some(out) = build_and_run("string_is_digit", &render_wasm_program(&prog)) {
            assert_eq!(out, "T1\nF2\nF3\nF4");
        }
    }

    #[test]
    fn self_hosted_string_take_drop_codepoint_indexed() {
        // string.take/drop/take_end/drop_end self-hosted: codepoint-indexed prefixes &
        // suffixes, reducing to the slice walk. take("hello",3)="hel", drop("hello",2)=
        // "llo", take_end("hello",2)="lo", drop_end("hello",2)="hel"; multibyte:
        // take("日本語",2)="日本", drop("日本語",1)="本語" (codepoints, not bytes).
        let src = "fn main() -> Unit = {\n  \
            let a = string.take(\"hello\", 3)\n  println(a)\n  \
            let b = string.drop(\"hello\", 2)\n  println(b)\n  \
            let c = string.take_end(\"hello\", 2)\n  println(c)\n  \
            let d = string.drop_end(\"hello\", 2)\n  println(d)\n  \
            let e = string.take(\"日本語\", 2)\n  println(e)\n  \
            let f = string.drop(\"日本語\", 1)\n  println(f) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.take"));
        assert!(prog.functions.iter().any(|f| f.name == "string.drop"));
        if let Some(out) = build_and_run("string_take_drop", &render_wasm_program(&prog)) {
            assert_eq!(out, "hel\nllo\nlo\nhel\n日本\n本語");
        }
    }

    #[test]
    fn self_hosted_string_trim_strips_ascii_whitespace() {
        // string.trim self-hosted: scan past leading/trailing ASCII WS, copy the middle.
        // trim("  hello  ")="hello", trim("world   ")="world".
        let src = "fn main() -> Unit = {\n  \
            let a = string.trim(\"  hello  \")\n  println(a)\n  \
            let b = string.trim(\"world   \")\n  println(b) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.trim"));
        if let Some(out) = build_and_run("string_trim", &render_wasm_program(&prog)) {
            assert_eq!(out, "hello\nworld");
        }
    }

    #[test]
    fn self_hosted_list_get_or_reads_element_or_default() {
        // list.get_or self-hosted: read element i (prim.load64) or the default out of bounds.
        // get_or([10,20,30],1,99)=20, get_or([10,20,30],5,99)=99.
        let src = "fn main() -> Unit = {\n  \
            let a = list.get_or([10, 20, 30], 1, 99)\n  println(int.to_string(a))\n  \
            let b = list.get_or([10, 20, 30], 5, 99)\n  println(int.to_string(b)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.get_or"));
        if let Some(out) = build_and_run("list_get_or", &render_wasm_program(&prog)) {
            assert_eq!(out, "20\n99");
        }
    }

    #[test]
    fn match_unit_executes_only_matched_arm() {
        // A Unit `match` over Int literal patterns (+ a `_` catch-all) desugars to a
        // nested `if n == lit then … else …` and EXECUTES: only the matched arm's
        // println runs — byte-identical to v0's match.
        let src = "fn classify(n: Int) -> Unit = match n {\n  \
            0 => println(\"zero\"),\n  \
            1 => println(\"one\"),\n  \
            _ => println(\"other\"),\n  \
            }\n\
            fn main() -> Unit = {\n  \
            classify(0)\n  classify(1)\n  classify(7) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("match_unit", &render_wasm_program(&prog)) {
            assert_eq!(out, "zero\none\nother");
        }
    }

    #[test]
    fn scalar_while_loop_runs_n_times() {
        // A real `while i < n { … i = i + 1 }` EXECUTES N iterations (LoopStart/
        // LoopBreakUnless/LoopEnd markers + SetLocal carry the counter) — count_to(4)
        // prints 0..3, byte-matching v0. The string-free body keeps it scalar-state.
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn count_to(n: Int) -> Unit = {\n  \
            var i = 0\n  \
            while i < n {\n    write_int(i)\n    i = i + 1\n  } }\n\
            fn main() -> Unit = count_to(4)\n";
        let prog = lower_source(src);
        // The loop must lower to REAL markers (executes), not the deferred one-iteration form.
        let count_fn = prog.functions.iter().find(|f| f.name == "count_to").expect("lowered fn \"count_to\" not found");
        assert!(
            count_fn.ops.iter().any(|op| matches!(op, Op::LoopStart)),
            "count_to's while must lower to LoopStart (executable), got {:?}",
            count_fn.ops
        );
        if let Some(out) = build_and_run("scalar_while", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\n1\n2\n3");
        }
    }

    #[test]
    fn while_loop_accumulates_via_counter() {
        // The loop-carried scalar state truly accumulates: sum 1+2+3+4+5 = 15, computed
        // in the loop and printed once after it. Verifies SetLocal threads `total`/`i`
        // across iterations (not a single modelled iteration).
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn sum_to(n: Int) -> Unit = {\n  \
            var i = 1\n  var total = 0\n  \
            while i <= n {\n    total = total + i\n    i = i + 1\n  }\n  \
            write_int(total) }\n\
            fn main() -> Unit = sum_to(5)\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("while_sum", &render_wasm_program(&prog)) {
            assert_eq!(out, "15");
        }
    }

    #[test]
    fn nested_while_loops_use_distinct_labels() {
        // Two nested loops exercise the per-loop label ids ($brk0/$cont0 vs $brk1/$cont1)
        // and the inner counter reset each outer iteration. grid(2,3) walks r*3+c = 0..5.
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn grid(rows: Int, cols: Int) -> Unit = {\n  \
            var r = 0\n  \
            while r < rows {\n    \
            var c = 0\n    \
            while c < cols {\n      write_int(r * cols + c)\n      c = c + 1\n    }\n    \
            r = r + 1\n  } }\n\
            fn main() -> Unit = grid(2, 3)\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("nested_while", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\n1\n2\n3\n4\n5");
        }
    }

    #[test]
    fn for_in_exclusive_range_executes_each_step() {
        // `for i in 0..n` desugars to the while machinery and RUNS each step: the index
        // is a fresh, mutable local stepped by 1, the end snapshot once. print_range(4)
        // prints 0,1,2,3 (exclusive), byte-matching v0.
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn print_range(n: Int) -> Unit = {\n  \
            for i in 0..n {\n    write_int(i)\n  } }\n\
            fn main() -> Unit = print_range(4)\n";
        let prog = lower_source(src);
        let f = prog.functions.iter().find(|f| f.name == "print_range").expect("lowered fn \"print_range\" not found");
        assert!(
            f.ops.iter().any(|op| matches!(op, Op::LoopStart)),
            "for-in must lower to LoopStart (executable), got {:?}",
            f.ops
        );
        if let Some(out) = build_and_run("for_range", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\n1\n2\n3");
        }
    }

    #[test]
    fn for_in_inclusive_range_includes_end() {
        // `for i in 1..=n` is INCLUSIVE (i <= n): sum_range(5) accumulates 1+2+3+4+5 = 15,
        // proving the index threads through and the inclusive bound includes `n`.
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn sum_range(n: Int) -> Unit = {\n  \
            var total = 0\n  \
            for i in 1..=n {\n    total = total + i\n  }\n  \
            write_int(total) }\n\
            fn main() -> Unit = sum_range(5)\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("for_incl", &render_wasm_program(&prog)) {
            assert_eq!(out, "15");
        }
    }

    #[test]
    fn match_scalar_value_selects_matched_arm() {
        // A scalar-result `match` over Int literals computes the matched arm's value
        // (here printed via the self-hosted itoa). pick(1) selects the `1 => 200` arm.
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn pick(n: Int) -> Int = match n {\n  \
            0 => 100,\n  1 => 200,\n  _ => 999,\n  }\n\
            fn main() -> Unit = write_int(pick(1))\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("match_scalar", &render_wasm_program(&prog)) {
            assert_eq!(out, "200");
        }
    }

    /// The hand-written WAT runtime is the BOOTSTRAP debt (§4.1). This guard
    /// makes the "never grow" rule MECHANICAL (not a comment): the count may only
    /// ratchet DOWN as the runtime self-hosts into Almide. If you added a
    /// hand-written WAT routine and this fails — STOP: write it in Almide and
    /// call it via `CallFn` instead. v0's wasm emitter rotted because nothing
    /// kept its hand-written surface small; this is that forcing function.
    /// The proven MEMORY-MODEL primitives in the preamble — the wasm realization
    /// of `proofs/RuntimeModel.v`'s `rt_inc`/`rt_dec`. A CLOSED set bounded by the
    /// PROOF (it grows only when the model gains an RC op), NOT by hand-mapping
    /// discipline, so the convergence guard accounts it SEPARATELY from the
    /// open-stdlib ratchet (§4.1): the trust spine's own core is not "another
    /// stdlib routine." The ratchet on the open surface stays exactly as strict.
    const RC_PRIMITIVE_FNS: &[&str] = &["$rc_dec", "$rc_inc"];

    /// The WASI HOST-EFFECT FLOOR primitives — the irreducible host-call sequences a
    /// `PrimKind` renders to, bounded by the CAPABILITY set, NOT by hand-mapping
    /// discipline. `$args_get_list` (`PrimKind::ArgsGetList`, Capability::CliArgs) is
    /// the `args_sizes_get` + `args_get` + canonical-`List[String]` assembly the
    /// self-hosted `env.args` reaches; `$read_text_file` (`PrimKind::ReadTextFile`,
    /// Capability::FsRead) is the `path_open` + `fd_read` + canonical-`Result[String,
    /// String]` assembly the self-hosted `fs.read_text` reaches (with `$rtf_str` /
    /// `$rtf_result`, the two leaves of that one sequence — copy host bytes into a
    /// canonical String, wrap a handle in the cap-as-tag Result block; plus `$alloc8`,
    /// the 8-aligned TRANSIENT WASI-scratch bump the host's i64 out-params require, the
    /// spine's immortal-scratch counterpart of the emit backend's `__alloc_pinned`).
    /// They CANNOT be written in Almide (each is the host-call boundary itself, like the
    /// `$fd_write`/`$random_get` IMPORTS — a multi-value-out-param WASI sequence that
    /// turns raw host bytes/handles into a heap value has no pure-Almide form).
    /// `$read_dir` (`PrimKind::ReadDir`, Capability::FsRead) is the `path_open(O_DIRECTORY)`
    /// + `fd_readdir` + dirent-parse + canonical-`Result[List[String], String]` assembly the
    /// self-hosted `fs.list_dir` reaches — the directory-listing twin of `$read_text_file`;
    /// its two leaves `$str_lt` (lexicographic compare of two raw dirent names, the sort that
    /// matches native `names.sort()`) and `$is_dot_entry` (the `.`/`..` skip over a raw dirent
    /// buffer pointer) operate on the raw fd_readdir buffer, not canonical values, so — like
    /// `$rtf_str`/`$rtf_result` for read_text — they are inseparable leaves of that one host
    /// sequence with no pure-Almide form. Accounted SEPARATELY from the open stdlib surface for
    /// the same reason RC primitives are: a host-floor exit is the trust spine's own core, not
    /// "another stdlib routine." This set grows ONLY when the capability vocabulary gains a new
    /// heap-result host floor (here: directory listing); the open ratchet stays exactly as strict.
    // `$write_text_file` (`PrimKind::WriteTextFile`, Capability::FsWrite), `$make_dir`
    // (`PrimKind::MakeDir`, Capability::FsWrite) and `$remove_all` (`PrimKind::RemoveAll`,
    // Capability::FsWrite, with its recursive byte-path leaf `$remove_path`) are the
    // filesystem-WRITE host-floor sequences (path_open(O_CREAT|O_TRUNC)+fd_write /
    // path_create_directory / path_remove_directory+path_unlink_file + the cap-as-tag
    // `Result[Unit, String]` build); `$read_line` (`PrimKind::ReadLine`, Capability::Stdin) is
    // the byte-by-byte fd_read-from-stdin + canonical-String build the self-hosted
    // `io.read_line` reaches. `$path_filestat_q` (`PrimKind::PathFilestat`, Capability::FsRead)
    // is the FULL path_filestat_get bridge the self-hosted `fs.stat` reaches (the host writes
    // the raw 64-byte filestat into the self-host's own scratch — the field reads stay Almide).
    // Each is a host-call boundary with no pure-Almide form, accounted in
    // the closed host-floor set exactly like the read sequences above.
    // `$env_get` (`PrimKind::EnvGet`, Capability::CliArgs — the Env profile's cap) is the
    // environ_sizes_get/environ_get lookup + Option[String] build the self-hosted
    // `env.get` reaches (C-133) — the same host-call-boundary class as `$args_get_list`.
    // `$path_norm` is the shared PATH-RESOLUTION bridge every fs floor fn calls first
    // (C-137): an absolute path drops its leading '/' (fd-3-relative), a RELATIVE path
    // is resolved against the host CWD by scanning the environ for PWD
    // (environ_sizes_get/environ_get — the SAME host-call sequence as `$env_get`) and
    // prepending it. It cannot be self-hosted: the fs floor fns that need it are
    // themselves WAT (the link direction is self-host → floor, never floor → almd),
    // and inlining it would duplicate one host-call sequence across all seven callers.
    const WASI_FLOOR_FNS: &[&str] = &[
        "$args_get_list", "$env_get", "$read_text_file", "$rtf_str", "$rtf_result", "$alloc8",
        "$read_dir", "$str_lt", "$is_dot_entry",
        "$write_text_file", "$make_dir", "$remove_all", "$remove_path", "$read_line",
        "$read_n_bytes", "$path_exists", "$path_filestat_q", "$path_norm",
    ];

    // The §13 TERMINATION-CONVENTION floor: contract-mandated aborts (C-001/C-035
    // — identical stderr + exit 1 cross-target). These CANNOT be self-hosted: each
    // is a diverging stderr writer the capability model deliberately excludes
    // (an abort is a halt, not an effect). A new entry must correspond to a
    // contract-pinned abort fixture in spec/wasm_cross.
    // `$__main_err` is C-035's v1 realization (the explicit-Result main Err protocol:
    // `Error: <msg>` on STDERR + proc_exit(1)) — the same diverging-stderr-writer class as
    // `$__div_trap`, pinned by C-035's spec/wasm_cross fixtures.
    const TERMINATION_FLOOR_FNS: &[&str] =
        &["$__div_trap", "$__chk_div", "$__chk_rem", "$__die", "$elem_addr_chk", "$__main_err"];

    #[test]
    fn handwritten_wasm_runtime_does_not_grow() {
        // The guard is SPLIT by principle: the proven memory-model primitives
        // (RC_PRIMITIVE_FNS — RuntimeModel.v's rt_inc/rt_dec) and the WASI host-floor
        // primitives (WASI_FLOOR_FNS — capability-bounded host-call sequences) are
        // closed sets accounted separately; the OPEN stdlib surface is what the
        // convergence rule (§4.1) ratchets DOWN only.
        let pre = preamble();
        let total = pre.matches("\n  (func $").count();
        let rc_count =
            RC_PRIMITIVE_FNS.iter().filter(|n| pre.contains(&format!("\n  (func {n} "))).count();
        let wasi_count =
            WASI_FLOOR_FNS.iter().filter(|n| pre.contains(&format!("\n  (func {n} "))).count();
        let term_count = TERMINATION_FLOOR_FNS
            .iter()
            .filter(|n| pre.contains(&format!("\n  (func {n} ")))
            .count();
        let stdlib_count = total - rc_count - wasi_count - term_count;
        // (a) The OPEN stdlib runtime surface — ratchet DOWN only, never raise.
        const BOOTSTRAP_RUNTIME_FN_BASELINE: usize = 11;
        assert!(
            stdlib_count <= BOOTSTRAP_RUNTIME_FN_BASELINE,
            "hand-written stdlib WAT runtime grew to {stdlib_count} funcs (baseline \
             {BOOTSTRAP_RUNTIME_FN_BASELINE}); §4.1 forbids growing it — self-host \
             the new routine in Almide and call it via CallFn"
        );
        // (b) The CLOSED proven-RC-primitive set — present as declared, no more.
        assert!(
            rc_count <= RC_PRIMITIVE_FNS.len(),
            "more RC primitive funcs ({rc_count}) than the proven closed set \
             ({}); an RC primitive must correspond to a RuntimeModel.v op",
            RC_PRIMITIVE_FNS.len()
        );
        // (c) The CLOSED WASI host-floor set — present as declared, no more. A new
        // entry here must correspond to a Capability + a heap-result host PrimKind.
        assert!(
            wasi_count <= WASI_FLOOR_FNS.len(),
            "more WASI host-floor funcs ({wasi_count}) than the closed set ({}); a \
             host-floor helper must correspond to a Capability + heap-result PrimKind",
            WASI_FLOOR_FNS.len()
        );
        // (d) The CLOSED termination-convention floor — present as declared, no more.
        // A new abort must correspond to a contract-pinned fixture (C-001/C-035 kin).
        assert!(
            term_count <= TERMINATION_FLOOR_FNS.len(),
            "more termination-floor funcs ({term_count}) than the closed set ({}); an \
             abort must correspond to a contract-pinned trap fixture",
            TERMINATION_FLOOR_FNS.len()
        );
    }

    fn build_and_run(label: &str, wat: &str) -> Option<String> {
        let dir = std::env::temp_dir().join(format!("almide_mir_wasm_{label}"));
        std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
        let wat_path = dir.join("m.wat");
        std::fs::write(&wat_path, wat).expect("failed to write the test scratch wat file");
        match Command::new("wasmtime").arg("run").arg(&wat_path).output() {
            Ok(o) if o.status.code() != Some(127) => {
                assert!(
                    o.status.success(),
                    "wasmtime failed:\n{}\n--- wat ---\n{wat}",
                    String::from_utf8_lossy(&o.stderr)
                );
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            }
            _ => None, // wasmtime unavailable → skip
        }
    }

    /// Run a WAT on wasmtime and report whether it exited cleanly. `None` =
    /// wasmtime unavailable (skip), `Some(true/false)` = ran and exited
    /// success/trap. Unlike `build_and_run` this does NOT assert success — it is
    /// for tests that EXPECT a trap (the double-free sentinel).
    fn run_status(label: &str, wat: &str) -> Option<bool> {
        let dir = std::env::temp_dir().join(format!("almide_mir_wasm_{label}"));
        std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
        let wat_path = dir.join("m.wat");
        std::fs::write(&wat_path, wat).expect("failed to write the test scratch wat file");
        match Command::new("wasmtime").arg("run").arg(&wat_path).output() {
            Ok(o) if o.status.code() != Some(127) => Some(o.status.success()),
            _ => None, // wasmtime unavailable → skip
        }
    }

    #[test]
    fn rc_dec_traps_on_double_free() {
        // The double-free CLASS — the one v0 bled on — is now TRAPPED on the real
        // bytes: a second release of an already-0 cell hits the `$rc_dec` sentinel
        // (`unreachable`). This is the runtime backstop for the safety the
        // ownership checker already proves statically.
        let double = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $p i32)\n\
             \u{20}   (local.set $p (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $rc_dec (local.get $p))\n\
             \u{20}   (call $rc_dec (local.get $p)))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(success) = run_status("doublefree", &double) {
            assert!(!success, "a double `rc_dec` must TRAP (the sentinel), got a clean exit");
        }
        // A SINGLE legitimate release (rc 1 → 0) must NOT trap — the sentinel
        // fires only on the already-freed cell, never on a valid free.
        let single = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $p i32)\n\
             \u{20}   (local.set $p (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $rc_dec (local.get $p)))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(success) = run_status("singlefree", &single) {
            assert!(success, "a single legitimate free must NOT trap");
        }
    }

    include!("tests_part3_b.rs");
    include!("tests_part3_c.rs");
