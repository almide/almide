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
        let count_fn = prog.functions.iter().find(|f| f.name == "count_to").unwrap();
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
        let f = prog.functions.iter().find(|f| f.name == "print_range").unwrap();
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

    #[test]
    fn handwritten_wasm_runtime_does_not_grow() {
        // The guard is SPLIT by principle: the proven memory-model primitives
        // (RC_PRIMITIVE_FNS — RuntimeModel.v's rt_inc/rt_dec) are a closed set
        // bounded by the PROOF, accounted separately; the OPEN stdlib surface is
        // what the convergence rule (§4.1) ratchets DOWN only.
        let pre = preamble();
        let total = pre.matches("\n  (func $").count();
        let rc_count =
            RC_PRIMITIVE_FNS.iter().filter(|n| pre.contains(&format!("\n  (func {n} "))).count();
        let stdlib_count = total - rc_count;
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
    }

    fn build_and_run(label: &str, wat: &str) -> Option<String> {
        let dir = std::env::temp_dir().join(format!("almide_mir_wasm_{label}"));
        std::fs::create_dir_all(&dir).unwrap();
        let wat_path = dir.join("m.wat");
        std::fs::write(&wat_path, wat).unwrap();
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
        std::fs::create_dir_all(&dir).unwrap();
        let wat_path = dir.join("m.wat");
        std::fs::write(&wat_path, wat).unwrap();
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

    #[test]
    fn custom_variant_scalar_match_executes_on_wasmtime() {
        // A custom ADT `Tok = Num(Int) | Sym(Int) | Eof` end-to-end through v1 (ADT bricks 2+3):
        // ctor construct (the tagged value-model block — tag@slot0 + scalar field slot) in BOTH
        // arg (`val(Num(7))`) and let (`let t = Num(9)`) positions, and an N-arm tag-dispatch
        // `match` → scalar result. Byte-matches v0 (7 / 40 / -1 / 9). Scalar fields only ⇒ the
        // block frees flat, no `$__drop_value`.
        let src = "type Tok = Num(Int) | Sym(Int) | Eof\n\
            fn val(t: Tok) -> Int = match t { Num(n) => n, Sym(s) => s * 10, Eof => -1 }\n\
            fn main() -> Unit = {\n  \
              println(int.to_string(val(Num(7))))\n  \
              println(int.to_string(val(Sym(4))))\n  \
              println(int.to_string(val(Eof)))\n  \
              let t = Num(9)\n  println(int.to_string(val(t))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "val"));
        if let Some(out) = build_and_run("custom_variant", &render_wasm_program(&prog)) {
            assert_eq!(out, "7\n40\n-1\n9");
        }
    }

    #[test]
    fn custom_variant_heap_result_match_executes_on_wasmtime() {
        // A HEAP (String) result custom-variant `match` over a BORROWED param subject (ADT
        // brick 4) — each arm moves out a fresh String; the bound scalar field `n` is read from
        // the borrowed subject's slot. The shape of recursive `to_string` minus the recursion.
        // Byte-matches v0.
        let src = "type Tok = Num(Int) | Sym(Int) | Eof\n\
            fn name(t: Tok) -> String = match t {\n  \
              Num(n) => \"num:\" + int.to_string(n),\n  \
              Sym(s) => \"sym\",\n  \
              Eof    => \"eof\",\n}\n\
            fn main() -> Unit = { println(name(Num(7))); println(name(Sym(2))); println(name(Eof)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("custom_variant_heap", &render_wasm_program(&prog)) {
            assert_eq!(out, "num:7\nsym\neof");
        }
    }

    #[test]
    fn custom_variant_string_field_construct_drops_clean() {
        // A custom ADT with a LEAF heap (`String`) ctor field (`Text(String)`): construct moves
        // the String into the masked slot (ADT brick 5a), the block's scope-end drop frees that
        // slot (the String-field record's DropListStr machinery) — verified by a 1000x
        // construct+drop loop that must not leak/trap. The field is matched with a WILDCARD (the
        // heap-field BIND is a later brick); byte-matches v0.
        let src = "type Msg = Text(String) | Code(Int) | Quit\n\
            fn tag(m: Msg) -> Int = match m { Text(_) => 1, Code(c) => c, Quit => 0 }\n\
            fn main() -> Unit = {\n  \
              var t = 0\n  for i in 0..1000 { t = t + tag(Text(\"xyz\")) }\n  \
              println(int.to_string(t))\n  \
              println(int.to_string(tag(Code(7))))\n  \
              println(int.to_string(tag(Quit))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("custom_variant_strfield", &render_wasm_program(&prog)) {
            assert_eq!(out, "1000\n7\n0");
        }
    }

    #[test]
    fn custom_variant_heap_field_bind_executes_on_wasmtime() {
        // A multi-arm custom-variant `match` that BINDS a leaf-heap (`String`) ctor field (ADT
        // brick 5c): `Text(s) => s` moves it out (auto-`Dup` in lower_heap_result_arm),
        // `string.len(s)` reads it (borrow). The subject keeps ownership (its masked drop frees
        // the slot); a 1000x construct+match+drop loop must not leak. Byte-matches v0. (A
        // SINGLE-arm heap match — a 1-ctor newtype — is walled: its direct-to-ret double-move.)
        let src = "type Msg = Text(String) | Code(Int) | Quit\n\
            fn name(m: Msg) -> String = match m { Text(s) => s, Code(c) => \"code\", Quit => \"quit\" }\n\
            fn weight(m: Msg) -> Int = match m { Text(s) => string.len(s), Code(c) => c, Quit => 0 }\n\
            fn main() -> Unit = {\n  \
              println(name(Text(\"hi\")))\n  println(name(Code(7)))\n  println(name(Quit))\n  \
              var n = 0\n  for i in 0..1000 { n = n + weight(Text(\"abc\")) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("custom_variant_heapfield", &render_wasm_program(&prog)) {
            assert_eq!(out, "hi\ncode\nquit\n3000");
        }
    }

    #[test]
    fn value_get_and_as_array_unwrap_execute_on_wasmtime() {
        // THE LAYOUT BRICK read side: a heap-Result-of-Value (`value.get` → Result[Value,String]) and
        // a heap-Result-of-List (`value.as_array` → Result[List[Value],String]) round-trip through
        // BOTH a `match` (tag@16 read, @12 payload bound as a borrow) AND a `??` (routed to the
        // self-hosted result.value_unwrap_or / result.list_value_unwrap_or, the Ok arm Dup'ing @12).
        // The Err message is the byte-exact "missing field '<k>'". 2000x is the leak gate.
        let src = "import json\n\
            effect fn main() -> Unit = {\n  \
              let o = value.object([(\"a\", value.int(7)), (\"b\", value.str(\"hi\"))])\n  \
              match value.get(o, \"a\") { ok(v) => println(int.to_string(value.as_int(v) ?? 0)), err(e) => println(\"e:\" + e) }\n  \
              match value.get(o, \"zzz\") { ok(v) => println(\"got\"), err(e) => println(e) }\n  \
              let g = value.get(o, \"b\") ?? value.null()\n  println(value.stringify(g))\n  \
              let arr = value.array([value.int(10), value.int(20), value.int(30)])\n  \
              let items = value.as_array(arr) ?? []\n  \
              var s = 0\n  for it in items { s = s + (value.as_int(it) ?? 0) }\n  println(int.to_string(s))\n  \
              var n = 0\n  for i in 0..2000 { let oo = value.object([(\"k\", value.int(i))]); let gg = value.get(oo, \"k\") ?? value.null(); n = n + (value.as_int(gg) ?? 0) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "value.get"));
        if let Some(out) = build_and_run("value_get_unwrap", &render_wasm_program(&prog)) {
            assert_eq!(out, "7\nmissing field 'zzz'\n\"hi\"\n60\n1999000");
        }
    }

    #[test]
    fn list_get_value_option_unwrap_executes_on_wasmtime() {
        // Option-of-Value read (`list.get(rows, i) ?? d` — the stringify_records row accessor). list.get
        // on a List[Value] dispatches to list.get_value (NOT the `_str` variant, which deep-copies the
        // element as a String — corrupting an Object to {}); it SHARES the element via Some(Dup), and the
        // `??` routes to option.value_unwrap_or (prim-based, since the value-match Some-arm rejects a heap
        // payload). The 2000x loop is the leak gate: value.as_array's OWNED list drops recursively
        // (value_result_lists), the shared element Values flat (co-owned). Was returning {} + OOM-leaking.
        let src = "import json\n\
            effect fn main() -> Unit = {\n  \
              let rows = value.as_array(value.array([value.object([(\"a\", value.int(1))]), value.str(\"x\")])) ?? []\n  \
              println(value.stringify(list.get(rows, 0) ?? value.null()))\n  \
              println(value.stringify(list.get(rows, 1) ?? value.null()))\n  \
              println(value.stringify(list.get(rows, 9) ?? value.object([])))\n  \
              match list.get(rows, 0) { some(v) => println(value.stringify(v)), none => println(\"none\") }\n  \
              var n = 0\n  for i in 0..2000 { let rs = value.as_array(value.array([value.object([(\"k\", value.int(i))])])) ?? []; let g = list.get(rs, 0) ?? value.null(); n = n + string.len(value.stringify(g)) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.get_value"));
        if let Some(out) = build_and_run("list_get_value_opt", &render_wasm_program(&prog)) {
            assert_eq!(out, "{\"a\":1}\n\"x\"\n{}\n{\"a\":1}\n18890");
        }
    }

    #[test]
    fn value_as_string_unwrap_executes_on_wasmtime() {
        // The String-payload Result `??` (`value.as_string(x) ?? "fb"` — Result[String,String]):
        // routed to the self-hosted result.str_unwrap_or, completing the Result-`??` family
        // (Value / List[Value] / String). Ok → the inner String, a tag-mismatch Err → the fallback.
        let src = "import json\n\
            effect fn main() -> Unit = {\n  \
              println(value.as_string(value.str(\"hello\")) ?? \"fb\")\n  \
              println(value.as_string(value.int(5)) ?? \"fb\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("value_as_string_unwrap", &render_wasm_program(&prog)) {
            assert_eq!(out, "hello\nfb");
        }
    }

    #[test]
    fn value_stringify_executes_on_wasmtime() {
        // The recursive JSON serializer, self-hosted in value_core, byte-identical to v0's
        // `almide_rt_value_stringify`: scalars direct, Str quoted+escaped (\ first), Array/Object
        // joined with "," via a String accumulator (the separator is `string.repeat(",", k)` with a
        // SCALAR-if k, sidestepping a heap-result-if in the loop body). 2000x is the leak gate — the
        // `prim.load_str` Str payload is a BORROW (not dropped as a call arg → no double-free).
        let src = "import json\n\
            effect fn main() -> Unit = {\n  \
              println(value.stringify(value.int(42)))\n  \
              println(value.stringify(value.bool(true)))\n  \
              println(value.stringify(value.null()))\n  \
              println(value.stringify(value.str(\"hi\\\"x\")))\n  \
              println(value.stringify(value.array([value.int(1), value.int(2), value.str(\"a\")])))\n  \
              println(value.stringify(value.object([(\"k\", value.int(1)), (\"s\", value.str(\"v\"))])))\n  \
              var n = 0\n  for i in 0..2000 { let s = value.stringify(value.object([(\"x\", value.str(\"v\")), (\"n\", value.int(i))])); n = n + string.len(s) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("value_stringify", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\ntrue\nnull\n\"hi\\\"x\"\n[1,2,\"a\"]\n{\"k\":1,\"s\":\"v\"}\n34890");
        }
    }

    #[test]
    fn value_object_and_json_keys_execute_on_wasmtime() {
        // The dynamic Value OBJECT (tag 6) self-host: `value.object(pairs)` builds a 2-slot-per-pair
        // block (key String + value Value, each rc_inc'd in — the Object co-owns them, freed by the
        // recursive __vdrop_obj at the last ref via __drop_value). `json.keys` reads them back. The
        // SLOT count (@8 = 2*pairs) is what the freelist reclaims — storing the pair count there
        // leaked 2 slots/iter (the 2-pair OOM this caught). 2000x is the leak gate (multi-pair).
        let src = "import json\n\
            effect fn main() -> Unit = {\n  \
              let o = value.object([(\"a\", value.int(1)), (\"bb\", value.str(\"x\"))])\n  \
              println(int.to_string(list.len(json.keys(o))))\n  \
              var k = 0\n  for i in 0..2000 { let p = value.object([(\"a\", value.int(i)), (\"b\", value.int(i))]); k = k + list.len(json.keys(p)) }\n  \
              println(int.to_string(k)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("value_object", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n4000");
        }
    }

    #[test]
    fn result_value_ok_wrapper_executes_on_wasmtime() {
        // The csv `parse` shape: a `Result[Value, String]` constructed by `ok(<Value>)` / `err(msg)`.
        // The Ok payload is a dynamic Value (materialized via lower_owned_heap_field), stored in the
        // len-1 + tag@16 block; marked `value_result_results` so the scope-end drop is the recursive
        // `Op::DropResultValue` ($__drop_value the Ok Value, rc_dec the Err String) — a flat
        // DropListStr would leak the Ok Value's nested payload. Round-trips: construct (ok/err),
        // match-read (ok(v)/err(e)), and the recursive drop at scope end.
        let src = "import json\n\
            effect fn wrap(n: Int) -> Result[Value, String] = if n < 0 then err(\"neg\") else ok(value.int(n))\n\
            effect fn main() -> Unit = {\n  \
              match wrap(42) { ok(v) => println(int.to_string(value.as_int(v) ?? 0)), err(e) => println(\"E:\" + e) }\n  \
              match wrap(0 - 1) { ok(v) => println(int.to_string(value.as_int(v) ?? 0)), err(e) => println(\"E:\" + e) } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("result_value_ok", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\nE:neg");
        }
    }

    #[test]
    fn empty_list_heap_result_if_arm_executes_on_wasmtime() {
        // A heap-result `if` with an EMPTY-list `[]` arm (`if cond then [] else <list>` — the parser
        // entry's empty-or-recurse split: `parse_rows = if is_empty(t) then [] else parse_rows_rec(...)`).
        // lower_heap_result_arm now materializes an empty `[]` arm (a fresh empty list block) +
        // Consumes it, alongside the populated-list-literal and call arms. Closes csv's parse_rows.
        let src = "fn gen(flag: Bool) -> List[String] = if flag then [] else [\"a\", \"b\"]\n\
            fn seq(n: Int) -> List[String] = if n <= 0 then [] else seq(n - 1) + [int.to_string(n)]\n\
            fn pick(flag: Bool, n: Int) -> List[String] = if flag then [] else seq(n)\n\
            fn main() -> Unit = {\n  \
              println(int.to_string(list.len(gen(true))) + \",\" + int.to_string(list.len(gen(false))))\n  \
              println(int.to_string(list.len(pick(true, 5))) + \",\" + int.to_string(list.len(pick(false, 5)))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("empty_list_arm", &render_wasm_program(&prog)) {
            assert_eq!(out, "0,2\n0,5");
        }
    }

    #[test]
    fn nested_list_of_lists_recursive_drop_executes_on_wasmtime() {
        // THE BOSS (csv `rows: List[List[String]]`): a list whose elements are owned `List[String]`
        // rows. Three pieces meet: (1) the list-of-lists CONCAT `rows + [cur]` (admit a List[String]
        // element via `__list_concat_rc`); (2) the singleton `[cur]` materialization; (3) the
        // RECURSIVE `Op::DropListListStr` — a NESTED wasm loop freeing each row's cell Strings, then
        // each row, then the outer block. A flat `DropListStr` would only `rc_dec` each row HANDLE,
        // leaking the cells. EVERY value of this type (concat result, call result, accumulator slot)
        // routes to `list_list_str_lists` so its drop is the nested one. The 2000x build+drop is the
        // LEAK GATE (an under-free OOMs the freelist as an OOB trap — exactly what this caught first).
        let src = "fn scan(text: String, pos: Int, rows: List[List[String]], cur: List[String]) -> List[List[String]] = {\n  \
              if pos >= string.len(text) then rows + [cur]\n  \
              else { let c = string.get(text, pos) ?? \"\"\n    \
                if c == \",\" then scan(text, pos + 1, rows, cur + [c])\n    \
                else if c == \"\\n\" then scan(text, pos + 1, rows + [cur], [])\n    \
                else scan(text, pos + 1, rows, cur + [c]) } }\n\
            fn main() -> Unit = {\n  \
              println(int.to_string(list.len(scan(\"ab,cd\\nef,gh\\n\", 0, [], []))))\n  \
              var n = 0\n  for i in 0..2000 { n = n + list.len(scan(\"ab,cd\\nef,gh\\n\", 0, [], [])) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("nested_list_drop", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n6000");
        }
    }

    #[test]
    fn scalar_var_list_literal_materializes_on_wasmtime() {
        // A `List[Int/Float/Bool]` literal with a VARIABLE element (`[n]`, `[a, b]`) in a value /
        // call-arg position. An all-LITERAL list folds to an `Init::IntList`, but a computed element
        // forced `alloc_init` to `Init::Opaque` (an empty list) → walled as unfaithful. Now the
        // call-arg path also tries `try_lower_scalar_list_construct` (flat `DynList` + `store64` each
        // element). This unblocks the append-accumulator element `acc + [n]` (the parser-row shape that
        // accumulates a scalar per step). Scalar elements own no heap, so the scope-end drop is flat.
        // 2000x is the leak gate.
        let src = "fn build(n: Int, acc: List[Int]) -> List[Int] =\n  \
              if n >= 8 then acc else build(n + 1, acc + [n * n])\n\
            fn main() -> Unit = {\n  \
              println(int.to_string(list.sum(build(0, []))))\n  \
              var s = 0\n  for i in 0..2000 { s = s + list.sum(build(0, [])) }\n  \
              println(int.to_string(s)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("scalar_var_list", &render_wasm_program(&prog)) {
            assert_eq!(out, "140\n280000");
        }
    }

    #[test]
    fn multi_accumulator_reset_and_cross_read_tco_executes_on_wasmtime() {
        // The csv-row shape: TWO heap accumulators where one's new value READS the other
        // (`out = out + cur`) while that other is RESET (`cur = ""`) in the same self-call. The TCO
        // append-accumulator now (1) admits a RESET to a fresh empty (`""`/`[]`) as a loop-carried
        // slot update, and (2) emits the per-iteration heap assigns in READ-DEPENDENCY order (the
        // reader `out` before the reset of `cur`), so `out` sees the OLD `cur`. A cyclic read
        // (`a=a+b; b=b+a`) still walls. 2000x is the leak gate (each slot's drop-old/alloc-new).
        let src = "fn scan(text: String, pos: Int, out: String, cur: String) -> String = {\n  \
              if pos >= string.len(text) then out + cur\n  \
              else { let c = string.get(text, pos) ?? \"\"; if c == \",\" then scan(text, pos + 1, out + cur, \"\") else scan(text, pos + 1, out, cur + c) } }\n\
            fn main() -> Unit = {\n  \
              println(scan(\"ab,cd,ef\", 0, \"\", \"\"))\n  \
              var n = 0\n  for i in 0..2000 { n = n + string.len(scan(\"ab,cd,ef\", 0, \"\", \"\")) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("multi_acc_tco", &render_wasm_program(&prog)) {
            assert_eq!(out, "abcdef\n12000");
        }
    }

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
        // datetime.format(ts, pattern): token substitution (YYYY/MM/DD/HH/mm/ss) in v0's SEQUENTIAL
        // .replace() order, composing the self-hosted datetime.year/.../second + string.replace +
        // __dt_pad zero-padding. ts=0 = the unix epoch 1970-01-01T00:00:00Z; ts=86400 = 1970-01-02.
        let src = "fn main() -> Unit = {\n  \
            println(datetime.format(0, \"YYYY-MM-DD HH:mm:ss\"))\n  \
            println(datetime.format(86400, \"DD/MM/YYYY\")) }\n";
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
