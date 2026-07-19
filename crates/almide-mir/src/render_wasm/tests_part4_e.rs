    #[test]
    fn option_map_loop_is_bounded() {
        // ADVERSARIAL leak/double-free guard: a loop materializing an Option, mapping it through a
        // closure (CallIndirect via the __opt_map_some helper) and matching the fresh result each
        // iteration must run in BOUNDED memory — the input Option, the mapped Option and the "k"
        // arm string are all one-slot (20-byte) so the head-only free-list reuses them.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 4000 {\n    \
              let o = list.first([5, 6])\n    let m = option.map(o, (x) => x * 2)\n    \
              match m {\n      Some(v) => println(\"k\"),\n      None => println(\"n\"),\n    }\n    \
              i = i + 1\n  }\n  \
            println(\"done\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("option_map_loop_bounded", &render_wasm_program(&prog)) {
            assert!(out.ends_with("done"), "loop must terminate (bounded memory)");
        }
    }

    #[test]
    fn self_hosted_int_from_hex() {
        // SELF-HOSTED int.from_hex = i64::from_str_radix(s.trim().trim_start_matches("0x"), 16).
        // The "0x" strip is lowercase + REPEATED and BEFORE the sign: "0xff"=255, "0x0xff"=255 (strip
        // twice), "0x1a"=26, "-2a"=-42 (printed negated=42). The quirks fail as Rust does: "0XFF"
        // (uppercase 0X not stripped → 'X' invalid), "0x" (empty after strip). Overflow uses the
        // SIGNED checked accumulator so "8000000000000000"=2^63 is too large but "-8000000000000000"
        // = i64::MIN parses (verified as 0-(v+1) = i64::MAX = 9223372036854775807).
        let src = "fn main() -> Unit = {\n  \
            let a = int.from_hex(\"ff\")\n  match a {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  }\n  \
            let b = int.from_hex(\"0x1a\")\n  match b {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  }\n  \
            let c = int.from_hex(\"0x0xff\")\n  match c {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  }\n  \
            let d = int.from_hex(\"-2a\")\n  match d {\n    Ok(v) => { let n = 0 - v\n println(int.to_string(n)) },\n    Err(e) => println(e),\n  }\n  \
            let e = int.from_hex(\"0XFF\")\n  match e {\n    Ok(v) => println(int.to_string(v)),\n    Err(g) => println(g),\n  }\n  \
            let f = int.from_hex(\"0x\")\n  match f {\n    Ok(v) => println(int.to_string(v)),\n    Err(g) => println(g),\n  }\n  \
            let big = int.from_hex(\"8000000000000000\")\n  match big {\n    Ok(v) => println(int.to_string(v)),\n    Err(g) => println(g),\n  }\n  \
            let mn = int.from_hex(\"-8000000000000000\")\n  match mn {\n    Ok(v) => { let w = v + 1\n let x = 0 - w\n println(int.to_string(x)) },\n    Err(g) => println(g),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.from_hex"));
        if let Some(out) = build_and_run("self_hosted_int_from_hex", &render_wasm_program(&prog)) {
            assert_eq!(out, "255\n26\n255\n42\ninvalid digit found in string\ncannot parse integer from empty string\nnumber too large to fit in target type\n9223372036854775807");
        }
    }

    #[test]
    fn self_hosted_result_match_ok_err() {
        // THE Result match-extract: `match int.parse(s) { Ok(v) => …, Err(e) => … }` EXECUTES — the
        // Ok arm binds the scalar value (slot 0, len-tag 0), the Err arm binds the message String
        // (borrowed LoadHandle of slot 0, freed by the Result's scope-end DropListStr). This finally
        // prints the Err MESSAGE directly, byte-matching Rust's exact ParseIntError strings.
        let src = "fn main() -> Unit = {\n  \
            let r1 = int.parse(\"42\")\n  match r1 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  }\n  \
            let r2 = int.parse(\" -7 \")\n  match r2 {\n    Ok(v) => { let n = 0 - v\n println(int.to_string(n)) },\n    Err(e) => println(e),\n  }\n  \
            let r3 = int.parse(\"abc\")\n  match r3 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  }\n  \
            let r4 = int.parse(\"\")\n  match r4 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  }\n  \
            let r5 = int.parse(\"9999999999999999999999\")\n  match r5 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.parse"));
        if let Some(out) = build_and_run("self_hosted_result_match_ok_err", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\n7\ninvalid digit found in string\ncannot parse integer from empty string\nnumber too large to fit in target type");
        }
    }

    #[test]
    fn result_match_loop_is_bounded() {
        // ADVERSARIAL leak/double-free guard for the Result MATCH-extract: a loop that materializes
        // a Result, tag-matches it (Ok arm binds the scalar value), and drops it each iteration must
        // run in BOUNDED memory. The Ok block is one element-slot wide (same 20-byte size as the "k"
        // arm string), so the head-only free-list reuses them. The match reads are borrows (no
        // ownership change); the subject is freed by the scope-end DropListStr exactly once.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 4000 {\n    \
              let r = int.parse(\"42\")\n    \
              match r {\n      Ok(v) => println(\"k\"),\n      Err(e) => println(e),\n    }\n    \
              i = i + 1\n  }\n  \
            println(\"done\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("result_match_loop_bounded", &render_wasm_program(&prog)) {
            assert!(out.ends_with("done"), "loop must terminate (bounded memory)");
        }
    }

    #[test]
    fn self_hosted_string_to_int() {
        // SELF-HOSTED string.to_int = s.trim().parse::<i64>(). Ok values via result.unwrap_or (a
        // negative is printed as its negation since int.to_string is non-negative); the Err path is
        // checked by is_err + the message LENGTH and first byte, which pin Rust's exact ParseIntError
        // strings: empty=38/'c', invalid digit=29/'i', too large=38/'n'@'l', too small=38/'n'@'s'.
        let src = "fn ms(r: Result[Int, String]) -> Int = {\n  let mh = prim.load64(prim.handle(r) + 12)\n  prim.load32(mh + 4)\n}\n\
                   fn mb(r: Result[Int, String], at: Int) -> Int = {\n  let mh = prim.load64(prim.handle(r) + 12)\n  prim.load8(mh + 12 + at)\n}\n\
                   fn main() -> Unit = {\n  \
            let a = int.parse(\"42\")\n  println(int.to_string(result.unwrap_or(a, 0)))\n  \
            let b = int.parse(\" -7 \")\n  let bv = result.unwrap_or(b, 0)\n  let bn = 0 - bv\n  println(int.to_string(bn))\n  \
            let c = int.parse(\"+99\")\n  println(int.to_string(result.unwrap_or(c, 0)))\n  \
            let z = int.parse(\"0\")\n  println(int.to_string(result.unwrap_or(z, 0 - 1)))\n  \
            let mx = int.parse(\"9223372036854775807\")\n  println(int.to_string(result.unwrap_or(mx, 0)))\n  \
            let e = int.parse(\"\")\n  let e1 = result.is_err(e)\n  let ze = if e1 then 1 else 0\n  println(int.to_string(ze))\n  println(int.to_string(ms(e)))\n  println(int.to_string(mb(e, 0)))\n  \
            let iv = int.parse(\"12a\")\n  let iv1 = result.is_err(iv)\n  let zi = if iv1 then 1 else 0\n  println(int.to_string(zi))\n  println(int.to_string(ms(iv)))\n  println(int.to_string(mb(iv, 0)))\n  \
            let tl = int.parse(\"9223372036854775808\")\n  let tl1 = result.is_err(tl)\n  let zl = if tl1 then 1 else 0\n  println(int.to_string(zl))\n  println(int.to_string(ms(tl)))\n  println(int.to_string(mb(tl, 11)))\n  \
            let tsm = int.parse(\"-9223372036854775809\")\n  let ts1 = result.is_err(tsm)\n  let zs = if ts1 then 1 else 0\n  println(int.to_string(zs))\n  println(int.to_string(ms(tsm)))\n  println(int.to_string(mb(tsm, 11))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.parse"));
        if let Some(out) = build_and_run("self_hosted_string_to_int", &render_wasm_program(&prog)) {
            // 42 ; -7 (printed negated = 7) ; 99 ; 0 ; i64::MAX ; empty(is_err 1, len 38, 'c'=99) ;
            // invalid(1, 29, 'i'=105) ; too-large(1, 38, byte11 'l'=108) ; too-small(1, 38, 's'=115)
            assert_eq!(out, "42\n7\n99\n0\n9223372036854775807\n1\n38\n99\n1\n29\n105\n1\n38\n108\n1\n38\n115");
        }
    }

    #[test]
    fn self_hosted_result_to_option() {
        // SELF-HOSTED result.to_option: Ok → Some(value), Err → None. It READS the Result len-tag and
        // BUILDS a fresh Option[Int] over v1's own Option machinery; a `match` over the result then
        // EXECUTES (result.to_option is tracked in is_self_host_option_module_fn). to_option(Ok(5))=
        // Some(5) → prints 5 ; to_option(Err)=None → prints none. Byte-matches v0.
        let src = "fn mk(n: Int) -> Result[Int, String] = if n >= 0 then Ok(n) else Err(\"neg\")\n\
                   fn main() -> Unit = {\n  \
            let r1 = mk(5)\n  let o1 = result.to_option(r1)\n  \
            match o1 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let r2 = mk(0 - 1)\n  let o2 = result.to_option(r2)\n  \
            match o2 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.to_option"));
        if let Some(out) = build_and_run("self_hosted_result_to_option", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\nnone");
        }
    }

    #[test]
    fn result_err_string_allocating_loop_is_bounded() {
        // ADVERSARIAL leak/double-free guard for the Result machinery: a loop building thousands of
        // `Err(msg)` AND `Ok(int)` Results must run in BOUNDED memory — each Err owns a fresh message
        // String freed by the scope-end DropListStr (len1, frees String + block), each Ok frees only
        // its block (len0). If the Err String leaked at the RC level it would OOM; if it double-freed
        // it would trap. The message is one element-slot wide (so the String block, the Err Result and
        // the Ok Result are all the same 20-byte size → the head-only $alloc free-list reuses them; a
        // MIXED-size churn is bound by that pre-existing FreeList property, not the ownership cert,
        // which holds for any size — every block is freed exactly once). Runs to completion = sound.
        let src = "fn mk(n: Int) -> Result[Int, String] = if n >= 0 then Ok(n) else Err(\"e\")\n\
                   fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 4000 {\n    \
              let r = mk(0 - 1)\n    let _x = result.is_err(r)\n    \
              let s = mk(i)\n    let _y = result.is_ok(s)\n    \
              i = i + 1\n  }\n  \
            println(\"done\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("result_err_loop_bounded", &render_wasm_program(&prog)) {
            assert_eq!(out, "done");
        }
    }

    #[test]
    fn heap_arg_closure_executes() {
        // A closure taking a HEAP (String) arg executes: the closure ABI is uniform i64, so a
        // lifted lambda receives the String handle as an i64 raw param and NARROWS it to its Ptr
        // at entry (the dual of the CallIndirect's i64.extend widen). `(x) => string.len(x)` over
        // "hello" returns 5.
        let src = "fn ap(s: String, f: (String) -> Int) -> Int = f(s)\n                   fn main() -> Unit = {\n  let n = ap(\"hello\", (x) => string.len(x))\n  println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("probe_heap_arg_closure", &render_wasm_program(&prog)) {
            assert_eq!(out, "5");
        }
    }

    #[test]
    fn scalar_var_alias_does_not_zero() {
        // `let q = p` aliasing a scalar param must keep p's value (was a silent Const(0) zeroing).
        let src = "fn f(p: Int) -> Int = { let q = p\n  q + 1 }\n\
            fn g(a: Float) -> Float = { let b = a\n  b }\n\
            fn main() -> Unit = {\n  \
              println(int.to_string(f(41)))\n  \
              let r = g(2.5)\n  let eq = prim.feq(r, 2.5)\n  let n = if eq then 1 else 0\n  println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("scalar_var_alias_does_not_zero", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\n1");
        }
    }

    #[test]
    fn self_hosted_log_gamma_matches_v0() {
        // math.log_gamma via Lanczos, calling the self-hosted math.log transitively.
        let src = "fn main() -> Unit = {\n\
            println(int.to_string(float.to_bits(math.log_gamma(1.0))))\n\
            println(int.to_string(float.to_bits(math.log_gamma(2.0))))\n\
            println(int.to_string(float.to_bits(math.log_gamma(5.0))))\n\
            println(int.to_string(float.to_bits(math.log_gamma(0.5))))\n\
            println(int.to_string(float.to_bits(math.log_gamma(10.5))))\n\
            println(int.to_string(float.to_bits(math.log_gamma(100.0))))\n\
            println(int.to_string(float.to_bits(math.log_gamma(3.0)))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_log_gamma_matches_v0", &render_wasm_program(&prog)) {
            assert_eq!(
                out,
                "-4841369599423283200\n0\n4614338759823076597\n4603330624632627640\n4624037492372685718\n4645025571947385953\n4604418534313441784"
            );
        }
    }

    #[test]
    fn scalar_call_in_operand_position_lowers() {
        let src = "fn helper(v: Int) -> Int = {\n\
              let m = prim.band(v, 4294967295)\n\
              if m >= 2147483648 then m - 4294967296 else m\n\
            }\n\
            fn floortest(x: Float) -> Int = {\n\
              let ui = prim.fbits(x)\n\
              let e = helper(prim.band(prim.bshr_u(ui, 52), 2047)) - 1023\n\
              e\n\
            }\n\
            fn main() -> Unit = { println(int.to_string(floortest(8.0))) }\n";
        let prog = lower_source(src);
        let wat = render_wasm_program(&prog);
        // 8.0 bits = 0x4020000000000000; high>>52 & 2047 = exponent field = 1026; -1023 = 3
        if let Some(out) = build_and_run("scalar_call_in_operand_position_lowers", &wat) {
            assert_eq!(out, "3");
        }
    }

    #[test]
    fn single_funcref_invoke_probe() {
        // SINGLE f(x) invocation (what list.map/filter/fold actually do per element).
        let src = "fn applyf(f: (Int) -> Int, x: Int) -> Int = f(x)\n\
            fn dbl(n: Int) -> Int = n + n\n\
            fn main() -> Unit = {\n\
              println(int.to_string(applyf(dbl, 5))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("single_funcref_invoke_probe", &render_wasm_program(&prog)) {
            eprintln!("SINGLE-PROBE out={:?}", out);
            assert_eq!(out, "10");
        }
    }

    #[test]
    fn self_hosted_read_f16_le_matches_v0() {
        // f16 decode (f32 semantics, widened) through v1, incl. the OOB→0.0 path (pos 7).
        let src = "fn main() -> Unit = {\n\
            let b = bytes.from_string(\"ABCDEFGH\")\n\
            println(int.to_string(float.to_bits(bytes.read_f16_le(b, 0))))\n\
            println(int.to_string(float.to_bits(bytes.read_f16_le(b, 1))))\n\
            println(int.to_string(float.to_bits(bytes.read_f16_le(b, 2))))\n\
            println(int.to_string(float.to_bits(bytes.read_f16_le(b, 3))))\n\
            println(int.to_string(float.to_bits(bytes.read_f16_le(b, 6))))\n\
            println(int.to_string(float.to_bits(bytes.read_f16_le(b, 7)))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_read_f16_le_matches_v0", &render_wasm_program(&prog)) {
            assert_eq!(out, "4614223691264294912\n4615353989217648640\n4616484287171002368\n4617614585124356096\n4621005478984417280\n0");
        }
    }

    #[test]
    fn self_hosted_read_f16_le_array_matches_v0() {
        // read_f16_le_array(2,4): read the 4 f64 slots directly (= float.to_bits per element).
        let src = "fn main() -> Unit = {\n\
            let b = bytes.from_string(\"ABCDEFGH\")\n\
            let xs = bytes.read_f16_le_array(b, 2, 4)\n\
            let h = prim.handle(xs)\n\
            println(int.to_string(prim.load64(h + 12)))\n\
            println(int.to_string(prim.load64(h + 20)))\n\
            println(int.to_string(prim.load64(h + 28)))\n\
            println(int.to_string(prim.load64(h + 36))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_read_f16_le_array_matches_v0", &render_wasm_program(&prog)) {
            assert_eq!(out, "4616484287171002368\n4618744883077709824\n4621005478984417280\n0");
        }
    }

    #[test]
    fn self_hosted_copy_within_matches_v0() {
        // In-place overlap-safe byte move: copy_within("ABCDEFGH", 0,3,4) → ABCDABCH.
        let src = "fn main() -> Unit = {\n\
            var b = bytes.from_string(\"ABCDEFGH\")\n\
            bytes.copy_within(b, 0, 3, 4)\n\
            var i = 0\n\
            while i < 8 { println(int.to_string(bytes.get_or(b, i, 0)))\n  i = i + 1 } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_copy_within_matches_v0", &render_wasm_program(&prog)) {
            assert_eq!(out, "65\n66\n67\n68\n65\n66\n67\n72");
        }
    }

    #[test]
    fn self_hosted_copy_from_matches_v0() {
        // Cross-buffer copy: copy_from(d="________", s="ABCDEFGH", 2,1,3) → d = "__BCD___".
        let src = "fn main() -> Unit = {\n\
            var d = bytes.from_string(\"________\")\n\
            let s = bytes.from_string(\"ABCDEFGH\")\n\
            bytes.copy_from(d, s, 2, 1, 3)\n\
            var i = 0\n\
            while i < 8 { println(int.to_string(bytes.get_or(d, i, 0)))\n  i = i + 1 } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_copy_from_matches_v0", &render_wasm_program(&prog)) {
            assert_eq!(out, "95\n95\n66\n67\n68\n95\n95\n95");
        }
    }

    #[test]
    fn self_hosted_skip_length_prefixed_le_matches_v0() {
        let src = "fn main() -> Unit = {\n\
            let b = bytes.from_string(\"ABCDEFGHIJKL\")\n\
            println(int.to_string(bytes.skip_length_prefixed_le(b, 0, 0)))\n\
            println(int.to_string(bytes.skip_length_prefixed_le(b, 0, 1)))\n\
            println(int.to_string(bytes.skip_length_prefixed_le(b, 8, 1)))\n\
            println(int.to_string(bytes.skip_length_prefixed_le(b, 10, 1))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_skip_length_prefixed_le_matches_v0", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\n1145258565\n1280002645\n10");
        }
    }

    #[test]
    fn self_hosted_uint_conversions_match_v0() {
        // uint8/16/32/64 → int/uint/float/string, the two-hop int composition (byte-identical to v0).
        let src = "fn main() -> Unit = {\n\
            println(uint8.to_string(int.to_uint8(200)))\n\
            println(uint16.to_string(int.to_uint16(60000)))\n\
            println(uint32.to_string(int.to_uint32(4000000000)))\n\
            println(uint64.to_string(int.to_uint64(0 - 1)))\n\
            println(uint64.to_string(uint8.to_uint64(int.to_uint8(200))))\n\
            println(uint8.to_string(uint16.to_uint8(int.to_uint16(300))))\n\
            println(int8.to_string(uint16.to_int8(int.to_uint16(200))))\n\
            println(int32.to_string(uint64.to_int32(int.to_uint64(123456789))))\n\
            println(int.to_string(float.to_bits(uint8.to_float64(int.to_uint8(200))))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_uint_conversions_match_v0", &render_wasm_program(&prog)) {
            assert_eq!(out, "200\n60000\n4000000000\n-1\n200\n44\n-56\n123456789\n4641240890982006784");
        }
    }

    #[test]
    fn heap_result_match_with_stdlib_call_arm() {
        // The real-program shape the smoke test surfaced: a String-returning if/match where
        // one arm is a literal and the other is a pure stdlib call (int.to_string).
        let if_src = "fn f(n: Int) -> String = if n == 0 then \"a\" else int.to_string(n)\n\
            fn main() -> Unit = { println(f(0))\n  println(f(7)) }\n";
        let prog = lower_source(if_src);
        if let Some(out) = build_and_run("heap_result_if_call_arm", &render_wasm_program(&prog)) {
            assert_eq!(out, "a\n7");
        }
        let match_src = "fn classify(n: Int) -> String = match n % 3 {\n  0 => \"fizz\",\n  _ => int.to_string(n),\n}\n\
            fn main() -> Unit = {\n  println(classify(9))\n  println(classify(7)) }\n";
        let prog = lower_source(match_src);
        if let Some(out) = build_and_run("heap_result_match_call_arm", &render_wasm_program(&prog)) {
            assert_eq!(out, "fizz\n7");
        }
        // The full smoke: pipe + closures + combinators + match-with-call-arm.
        let smoke = "fn classify(n: Int) -> String = match n % 3 {\n  0 => \"fizz\",\n  _ => int.to_string(n),\n}\n\
            fn main() -> Unit = {\n\
              let nums = [3, 1, 4, 1, 5, 9, 2, 6]\n\
              let evens = nums |> list.filter((n) => n % 2 == 0)\n\
              let doubled = evens |> list.map((n) => n * 2)\n\
              let total = doubled |> list.fold(0, (acc, n) => acc + n)\n\
              println(int.to_string(total))\n  println(int.to_string(list.len(nums)))\n  println(classify(9))\n  println(classify(7)) }\n";
        let prog = lower_source(smoke);
        if let Some(out) = build_and_run("smoke_full", &render_wasm_program(&prog)) {
            assert_eq!(out, "24\n8\nfizz\n7");
        }
    }

    #[test]
    fn heap_result_call_arm_bounded_loop_no_leak() {
        // Adversarial: 5000 iterations each producing a fresh String via the heap-result match
        // call-arm. If the per-arm "im" balance leaked or double-freed, this would OOM/trap.
        let src = "fn label(n: Int) -> String = match n % 2 { 0 => \"even\", _ => int.to_string(n) }\n\
            fn main() -> Unit = {\n  var i = 0\n  while i < 5000 { println(label(i))\n    i = i + 1 } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("heap_result_call_arm_bounded_loop_no_leak", &render_wasm_program(&prog)) {
            // 5000 lines; just check it ran to completion (last line = label(4999) = "4999").
            assert!(out.ends_with("4999"), "loop did not complete: tail={:?}", &out[out.len().saturating_sub(20)..]);
            assert_eq!(out.lines().count(), 5000);
        }
    }

    #[test]
    fn opt_unwrap_or_in_call_arg() {
        // `int.to_string(opt ?? default)` — `??` over a materialized Option in call-arg position.
        let src = "fn main() -> Unit = {\n  let xs = [7]\n\
            println(int.to_string(list.get(xs, 0) ?? 0))\n\
            println(int.to_string(list.get(xs, 5) ?? 99))\n\
            println(int.to_string(list.first(xs) ?? 0)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("opt_unwrap_or_in_call_arg", &render_wasm_program(&prog)) {
            assert_eq!(out, "7\n99\n7");
        }
    }

    #[test]
    fn opt_unwrap_or_call_arg_bounded_loop() {
        // Adversarial: 5000 iters, each materializes an Option for ?? in a call-arg. No leak/trap.
        let src = "fn main() -> Unit = {\n  let xs = [42]\n  var i = 0\n\
            while i < 5000 { println(int.to_string(list.get(xs, 0) ?? 0))\n    i = i + 1 } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("opt_unwrap_or_call_arg_bounded_loop", &render_wasm_program(&prog)) {
            assert_eq!(out.lines().count(), 5000);
            assert!(out.ends_with("42"));
        }
    }

    #[test]
    fn opt_unwrap_or_var_form() {
        // `let o = list.get(...); o ?? default` — the bind-then-use form (most common).
        let src = "fn main() -> Unit = {\n  let xs = [7]\n\
            let a = list.get(xs, 0)\n  println(int.to_string(a ?? 0))\n\
            let b = list.get(xs, 9)\n  println(int.to_string(b ?? 99))\n\
            let c = list.first(xs)\n  println(int.to_string(c ?? 0)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("opt_unwrap_or_var_form", &render_wasm_program(&prog)) {
            assert_eq!(out, "7\n99\n7");
        }
    }

    #[test]
    fn for_in_list_executes() {
        // `for x in xs { println(int.to_string(x)) }` over a List[Int] — REAL iteration.
        let src = "fn main() -> Unit = {\n  let xs = [10, 20, 30]\n  for x in xs { println(int.to_string(x)) } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("for_in_list_executes", &render_wasm_program(&prog)) {
            assert_eq!(out, "10\n20\n30");
        }
        // sum accumulation via a mutable scalar
        let src2 = "fn main() -> Unit = {\n  let xs = [1, 2, 3, 4, 5]\n  var total = 0\n  for x in xs { total = total + x }\n  println(int.to_string(total)) }\n";
        let prog2 = lower_source(src2);
        if let Some(out) = build_and_run("for_in_list_sum", &render_wasm_program(&prog2)) {
            assert_eq!(out, "15");
        }
    }

    #[test]
    fn smoke_combined_real_program() {
        // Real program combining the session's language fixes: for-in over list + ?? default
        // (var + call-arg) + a heap-result match with a stdlib module-call arm. Must byte-match v0.
        let src = "fn label(n: Int) -> String = match n % 2 {\n  0 => \"even\",\n  _ => int.to_string(n),\n}\n\
            fn main() -> Unit = {\n\
              let xs = [10, 7, 4, 3]\n\
              for x in xs { println(label(x)) }\n\
              let first = list.first(xs) ?? 0\n\
              let missing = list.get(xs, 99) ?? 0\n\
              println(int.to_string(first + missing))\n\
              var total = 0\n\
              for x in xs { total = total + x }\n\
              println(int.to_string(total)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("smoke_combined_real_program", &render_wasm_program(&prog)) {
            assert_eq!(out, "even\n7\neven\n3\n10\n24");
        }
    }

    #[test]
    fn string_concat_bind_and_arg() {
        let src = "fn main() -> Unit = {\n  let s = \"Hi, \" + \"World\"\n  println(s)\n  println(\"a\" + \"b\")\n  let name = \"Al\"\n  println(\"Hello, \" + name + \"!\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_concat_bind_and_arg", &render_wasm_program(&prog)) {
            assert_eq!(out, "Hi, World\nab\nHello, Al!");
        }
    }

    #[test]
    fn string_concat_tail_and_loop() {
        // tail-position concat (fn greet = "Hi, " + n) + bounded loop (per-iter fresh String).
        let src = "fn greet(n: String) -> String = \"Hi, \" + n\n\
            fn main() -> Unit = {\n  println(greet(\"Al\"))\n\
              var i = 0\n  while i < 3000 { println(\"x\" + int.to_string(i))\n    i = i + 1 } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_concat_tail_and_loop", &render_wasm_program(&prog)) {
            assert!(out.starts_with("Hi, Al\nx0\nx1\n"));
            assert!(out.ends_with("x2999"));
            assert_eq!(out.lines().count(), 3001);
        }
    }


    #[test]
    fn str_list_literal_with_concat() {
        let src = "fn main() -> Unit = {\n  let xs = [\"a\" + \"b\", \"c\", \"d\" + \"e\"]\n  println(list.join(xs, \",\")) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("str_list_literal_with_concat", &render_wasm_program(&prog)) {
            assert_eq!(out, "ab,c,de");
        }
    }

    // ── String interpolation `"…${e}…"` — the executable subset (fix-0276) ──────────
    //
    // A StringInterp whose parts are all Lit / String Var-or-LitStr / Int Var-or-LitInt
    // lowers to a fresh owned String via the `__str_concat` chain (seeded with an empty
    // "" leaf), byte-matching v0's `emit_string_interp`. These detectors pin the four
    // value positions (call-arg / bind / tail / match-arm) + the structural guard that a
    // NON-subset interp (a `${list.len(x)}` call operand) stays the sound Opaque fallback.
    // (Goldens captured from `almide run` on the native v0 path.)

    #[test]
    fn string_interp_call_arg_executes() {
        // The HIGHEST-traffic position: `println("…${x}…")`. Mixed String + Int operands.
        // v0: "x=42 y=world", "count:42", "world" (single-part passthrough).
        let src = "fn main() -> Unit = {\n  \
            let n = 42\n  let s = \"world\"\n  \
            println(\"x=${n} y=${s}\")\n  \
            println(\"count:${n}\")\n  \
            println(\"${s}\") }\n";
        let prog = lower_source(src);
        // STRUCTURAL GUARD: a lowerable interp routes through __str_concat, never an empty
        // Opaque — auto-linking the self-host concat runtime is the observable signature.
        assert!(
            prog.functions.iter().any(|f| f.name == "__str_concat"),
            "a lowerable interp must auto-link __str_concat (not defer to empty Opaque)"
        );
        if let Some(out) = build_and_run("string_interp_call_arg", &render_wasm_program(&prog)) {
            assert_eq!(out, "x=42 y=world\ncount:42\nworld");
        }
    }

    #[test]
    fn string_interp_bind_position_executes() {
        // A `let lbl = "[${s}]"` BIND — the interp result is owned by the binding and
        // dropped at scope end. v0: "[world]".
        let src = "fn main() -> Unit = {\n  \
            let s = \"world\"\n  let lbl = \"[${s}]\"\n  println(lbl) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_bind", &render_wasm_program(&prog)) {
            assert_eq!(out, "[world]");
        }
    }

    #[test]
    fn string_interp_tail_position_executes() {
        // A RETURN/tail-position interp (`fn greet(name) = "Hi, ${name}!"`) — moved out as
        // the result. v0: "Hi, Ada!".
        let src = "fn greet(name: String) -> String = \"Hi, ${name}!\"\n\
            fn main() -> Unit = {\n  println(greet(\"Ada\")) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_tail", &render_wasm_program(&prog)) {
            assert_eq!(out, "Hi, Ada!");
        }
    }

    #[test]
    fn string_interp_match_arm_executes() {
        // A heap-result MATCH-arm interp (`match k { _ => "v=${n}" }`). The arm folds the
        // interp per-arm (cert `im`), only the taken arm runs. v0: "v=7" / "other".
        let src = "fn label(k: Int, n: Int) -> String = match k {\n  \
            0 => \"other\",\n  _ => \"v=${n}\",\n}\n\
            fn main() -> Unit = {\n  \
            println(label(1, 7))\n  println(label(0, 9)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_match_arm", &render_wasm_program(&prog)) {
            assert_eq!(out, "v=7\nother");
        }
    }

    #[test]
    fn string_interp_multipart_int_and_string() {
        // A 4-part interp mixing two Int Vars and a String Var with literals — exercises the
        // K-concat + I-int.to_string glue count exactly. v0: "p(3,4)=ok".
        let src = "fn main() -> Unit = {\n  \
            let a = 3\n  let b = 4\n  let r = \"ok\"\n  \
            println(\"p(${a},${b})=${r}\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_multipart", &render_wasm_program(&prog)) {
            assert_eq!(out, "p(3,4)=ok");
        }
    }

    #[test]
    fn string_interp_loop_reclaims() {
        // SOUNDNESS: a bounded loop building a fresh interp String each iteration must
        // reclaim every round (no leak / no double-free → no OOM). The chain allocs K+1
        // intermediate Strings per round, all freed at the iteration frame's end.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  while i < 4000 { println(\"row ${i} done\")\n    i = i + 1 } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_loop", &render_wasm_program(&prog)) {
            assert!(out.starts_with("row 0 done\nrow 1 done\n"));
            assert!(out.ends_with("row 3999 done"));
            assert_eq!(out.lines().count(), 4000);
        }
    }

    #[test]
    fn string_interp_single_part_var_is_owned_copy() {
        // OWNERSHIP soundness for the single-part `"${s}"` passthrough: the interp builds a
        // FRESH owned String (`"" ++ s`), so the original `s` stays live and independently
        // owned — using BOTH afterward (and concatenating them) must not double-free. v0:
        // "hello\nhello\nhellohello".
        let src = "fn main() -> Unit = {\n  \
            let s = \"hello\"\n  let t = \"${s}\"\n  \
            println(t)\n  println(s)\n  println(s + t) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_single_part_alias", &render_wasm_program(&prog)) {
            assert_eq!(out, "hello\nhello\nhellohello");
        }
    }

    #[test]
    fn string_interp_int_call_operand_executes_and_byte_matches_v0() {
        // The uniform desugar wraps an Int part by TYPE, not by operand shape — so an Int
        // `${list.len(x)}` with a CALL operand now folds to `int.to_string(list.len(x))`.
        // Both `int.to_string` and `list.len` are self-hosted, so the function fully LINKS
        // and EXECUTES, byte-matching v0 ("len=3"). (Before the uniform desugar this stayed
        // a deferred Opaque — the per-operand predicate rejected a non-Var operand. This is
        // a coverage GAIN, not a regression: the call is MATERIALIZED, so caps stay honest.)
        let src = "fn main() -> Unit = {\n  \
            let xs = [1, 2, 3]\n  println(\"len=${list.len(xs)}\") }\n";
        let prog = lower_source(src);
        let main = prog.functions.iter().find(|f| f.name == "main").expect("main lowered");
        // The list.len call is MATERIALIZED as a real CallFn (its result feeds int.to_string),
        // not silently dropped — its capabilities are visible to the transitive fold.
        assert!(
            main.ops.iter().any(|op| matches!(op,
                Op::CallFn { name, .. } if name == "list.len")),
            "the Int part's call operand must be materialized as a real CallFn"
        );
        // Fully linkable (int.to_string + list.len both registered) → renders cleanly.
        assert!(
            crate::render_wasm::unlinked_call_names(&prog).is_empty(),
            "an Int-call-operand interp must be fully linkable, got {:?}",
            crate::render_wasm::unlinked_call_names(&prog)
        );
        if let Some(out) = build_and_run("string_interp_int_call_operand", &render_wasm_program(&prog)) {
            assert_eq!(out, "len=3"); // v0 golden
        }
    }

    #[test]
    fn scalar_call_in_arithmetic_operand_executes_and_byte_matches_v0() {
        // THE fix-0276 GAP: a scalar Int/Bool CALL (or if/match) used as a BinOp/comparison
        // OPERAND used to DEFER to `Const 0` — `5 + string.len(s)` silently computed `5 + 0`.
        // Now `lower_scalar_value` MATERIALIZES the operand call (a real CallFn over its
        // borrowed/materialized heap args, the self-rollback wrapper making it safe), so the
        // arithmetic is correct. The optimizer inlines `let s = "abc"`, so the call here is
        // `string.len("abc")` — a FRESH heap-LITERAL arg materialized + dropped at scope end.
        // v0 golden ("8") via `almide run`.
        let src = "fn main() -> Unit = {\n  \
            let s = \"abc\"\n  let n = 5 + string.len(s)\n  println(\"${n}\") }\n";
        let prog = lower_source(src);
        // The operand call is a REAL CallFn (its result feeds the IntBinOp), not dropped to 0.
        let any_fn = prog.functions.iter().flat_map(|f| f.ops.iter()).any(|op| matches!(op,
            Op::CallFn { name, .. } if name == "string.len"));
        assert!(any_fn, "the arithmetic operand's string.len call must be materialized as a real CallFn");
        if let Some(out) = build_and_run("scalar_call_operand_arith", &render_wasm_program(&prog)) {
            assert_eq!(out, "8"); // v0 golden: 5 + len("abc")=5+3
        }
    }

    #[test]
    fn scalar_if_in_arithmetic_operand_executes_and_byte_matches_v0() {
        // The `if`/`match` half of the fix-0276 gap: `a + (if c then 1 else 2)` used to defer
        // the parenthesized `if` operand to `Const 0`. Now it EXECUTES via `try_lower_scalar_if`
        // (only the taken arm runs), so the sum is right. v0 golden: "11 12".
        let src = "fn main() -> Unit = {\n  \
            let a = 10\n  \
            let r1 = a + (if true then 1 else 2)\n  \
            let r2 = a + (if false then 1 else 2)\n  \
            println(\"${r1} ${r2}\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("scalar_if_operand_arith", &render_wasm_program(&prog)) {
            assert_eq!(out, "11 12"); // v0 golden
        }
    }

    #[test]
    fn noisy_call_in_operand_keeps_caller_caps_tainted() {
        // FALSE-GREEN GUARD for fix-0276: materializing an operand call makes MORE calls real,
        // so the caps fold must still TAINT a caller whose operand call reaches Stdout. A
        // `noisy()` that `println`s, used as `5 + noisy(3)`, becomes a real CallFn edge — the
        // transitive cap fold (`reachable_caps`) must report Stdout reachable for `compute`, so
        // it can never be falsely caps-VERIFIED. (A PURE operand call like `string.len` stays
        // empty-reachable — the contrast that proves the taint is precise, not blanket.)
        use crate::certificate::reachable_caps;
        let noisy_src = "fn noisy(x: Int) -> Int = {\n  println(\"side\")\n  x + 1\n}\n\
            fn compute() -> Int = { 5 + noisy(3) }\n\
            fn main() -> Unit = { println(\"${compute()}\") }\n";
        let prog = lower_source(noisy_src);
        let program: std::collections::BTreeMap<String, crate::MirFunction> =
            prog.functions.iter().map(|f| (f.name.clone(), f.clone())).collect();
        let mut visited = std::collections::BTreeSet::new();
        let reach = reachable_caps("compute", &program, &mut visited);
        assert!(
            reach.contains(&crate::Capability::Stdout),
            "a printing operand call must keep the caller's transitive caps tainted (Stdout reachable), got {reach:?}"
        );
        // Contrast: a PURE operand call leaves the caller empty-reachable (no false taint).
        let pure_src = "fn compute() -> Int = { let s = \"abc\"\n  5 + string.len(s) }\n\
            fn main() -> Unit = { println(\"${compute()}\") }\n";
        let pure = lower_source(pure_src);
        let pure_program: std::collections::BTreeMap<String, crate::MirFunction> =
            pure.functions.iter().map(|f| (f.name.clone(), f.clone())).collect();
        let mut v2 = std::collections::BTreeSet::new();
        assert!(
            reachable_caps("compute", &pure_program, &mut v2).is_empty(),
            "a pure operand call must NOT taint the caller's transitive caps"
        );
    }

    #[test]
    fn c1_defunc_capturing_map_executes_and_is_pure() {
        // C1: a CAPTURING inline lambda in `list.map` is DEFUNCTIONALIZED inline — the
        // capture `k` resolves through the in-scope binding, NOT a closure env. It EXECUTES
        // (byte-matches v0 `[10, 20, 30]`) AND, with the result-producing work isolated in a
        // pure fn, the transitive cap fold reports EMPTY (the inlined `x * k` reaches no host
        // capability — no CallIndirect conservatism, no lifted-lambda Stdout). The inline path
        // is NOT a caps regression: a pure body stays pure.
        use crate::certificate::reachable_caps;
        let src = "fn build() -> List[Int] = {\n  let k = 10\n  \
            list.map([1, 2, 3], (x) => x * k) }\n\
            fn main() -> Unit = { println(\"${build()}\") }\n";
        let prog = lower_source(src);
        // No lifted lambda, no `list.map` combinator — the closure is defunctionalized away.
        assert!(
            !prog.functions.iter().any(|f| f.name.starts_with("__lambda_") || f.name == "list.map"),
            "the capturing map lambda is inlined (no __lambda_*, no list.map combinator)"
        );
        let program: std::collections::BTreeMap<String, crate::MirFunction> =
            prog.functions.iter().map(|f| (f.name.clone(), f.clone())).collect();
        let mut visited = std::collections::BTreeSet::new();
        assert!(
            reachable_caps("build", &program, &mut visited).is_empty(),
            "a pure inlined map body must NOT taint the producer's transitive caps"
        );
        if let Some(out) = build_and_run("c1_capturing_map", &render_wasm_program(&prog)) {
            assert_eq!(out, "[10, 20, 30]");
        }
    }

    #[test]
    fn c1_defunc_filter_and_fold_execute() {
        // C1: an inline `filter` predicate and an inline `fold` reducer are defunctionalized
        // as loops at the call site (over-allocate+pack+patch-len for filter, a stable
        // accumulator local for fold). Byte-matches v0: filter([1..4], x>2)=[3,4],
        // fold([1..4], 0, +)=10. No combinator CallFn, no closure.
        let src = "fn main() -> Unit = {\n  \
            let a = list.filter([1, 2, 3, 4], (x) => x > 2)\n  println(\"${a}\")\n  \
            let s = list.fold([1, 2, 3, 4], 0, (acc, x) => acc + x)\n  println(int.to_string(s)) }\n";
        let prog = lower_source(src);
        assert!(
            !prog.functions.iter().any(|f| f.name == "list.filter" || f.name == "list.fold"),
            "filter/fold inline lambdas are defunctionalized, not auto-linked"
        );
        if let Some(out) = build_and_run("c1_filter_fold", &render_wasm_program(&prog)) {
            assert_eq!(out, "[3, 4]\n10");
        }
    }

    #[test]
    fn c1_defunc_map_false_green_keeps_caller_caps_tainted() {
        // FALSE-GREEN GUARD for C1: a `list.map(xs, (x) => { println("hit"); x })` body has a
        // REAL Stdout edge. My defunctionalization declines a side-effecting body (it is not
        // scalar-pure-lowerable) → the lambda falls to the self-host path (LIFTED + CallIndirect).
        // The lifted lambda's Stdout MUST reach the caller's transitive witness via the FuncRef
        // edge — so a printing map can NEVER be falsely caps-VERIFIED. (This is the exact
        // accept-but-unsafe the discipline forbids: the inline must not swallow the println.)
        use crate::certificate::reachable_caps;
        let src = "fn run() -> Unit = {\n  \
            let _r = list.map([1, 2, 3], (x) => { println(\"hit\"); x }) }\n\
            fn main() -> Unit = { run() }\n";
        let prog = lower_source(src);
        let program: std::collections::BTreeMap<String, crate::MirFunction> =
            prog.functions.iter().map(|f| (f.name.clone(), f.clone())).collect();
        let mut visited = std::collections::BTreeSet::new();
        let reach = reachable_caps("run", &program, &mut visited);
        assert!(
            reach.contains(&crate::Capability::Stdout),
            "a printing map lambda must keep the caller's transitive caps tainted (Stdout reachable), got {reach:?}"
        );
        // And it still EXECUTES the side effect (prints "hit" thrice), byte-matching v0.
        if let Some(out) = build_and_run("c1_false_green", &render_wasm_program(&prog)) {
            assert_eq!(out, "hit\nhit\nhit");
        }
    }

    #[test]
    fn c1_direct_call_inline_executes_captured_lambda() {
        // C1 DIRECT-CALL INLINE: `let s = "ab"; let f = (x) => string.len(s) + x; f(1)` — the
        // CAPTURING let-bound lambda is inlined at its DIRECT call site (the capture `s` resolves
        // through the in-scope binding). EXECUTES to 3 (was a silent `Const 0` before C1 — the
        // capturing lambda deferred to an Opaque + zero). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  let s = \"ab\"\n  \
            let f = (x) => string.len(s) + x\n  \
            println(int.to_string(f(1))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("c1_direct_inline", &render_wasm_program(&prog)) {
            assert_eq!(out, "3");
        }
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // THE UNLINKED-CALL WALL (the StringInterp→to_string prerequisite brick).
    //
    // Before this wall, a `CallFn` to a stdlib fn NOT in the self-host registry (and not
    // a user fn / preamble fn) rendered as a dangling `(call $name)` → an INVALID wasm
    // module that wasmtime/wat2wasm reject — yet `render_wasm_program` returned it as a
    // plain String (invalid-wasm-passing-as-Ok). `try_render_wasm_program` now detects
    // the unresolved name at the resolution point and returns `LowerError::Unsupported`.
    // ──────────────────────────────────────────────────────────────────────────────

    #[test]
    fn unlinked_stdlib_call_is_walled_not_dangling_wasm() {
        use crate::lower::LowerError;
        use crate::render_wasm::{try_render_wasm_program, unlinked_call_names};
        // `list.bundled_probe` is NOT in the self-host registry — it is the bundled-body
        // MACHINERY PROBE (stdlib/list.almd), deliberately never self-hosted, so it stays a
        // bare `CallFn` — a STABLE canonical unlinked call (the previous exemplar,
        // `float.to_fixed`, got self-hosted 2026-07-17 and broke this test's premise).
        // (`float.from_int` AND `float.to_string` ARE registered — the contrast that proves
        // the wall is precise, not a blanket reject.)
        let src = "fn main() -> Unit = {\n  \
            let x = float.from_int(3)\n  println(float.to_string(x))\n  \
            println(int.to_string(list.bundled_probe(3))) }\n";
        let prog = lower_source(src);
        // The resolution check flags exactly the unlinked name, nothing else.
        let missing = unlinked_call_names(&prog);
        assert!(
            missing.contains("list.bundled_probe"),
            "the unlinked list.bundled_probe must be detected, got {missing:?}"
        );
        assert!(
            !missing.contains("float.from_int"),
            "a REGISTERED (auto-linked) call must NOT be walled — {missing:?}"
        );
        // The walled render returns a clean Unsupported (a loud reject), NOT a String.
        match try_render_wasm_program(&prog) {
            Err(LowerError::Unsupported(msg)) => {
                assert!(
                    msg.contains("list.bundled_probe"),
                    "the wall message must name the unlinked callee, got {msg:?}"
                );
            }
            Err(other) => panic!("expected Unsupported, got {other:?}"),
            Ok(_) => panic!("an unlinked call must be walled, not rendered to (possibly invalid) wasm"),
        }
    }

    #[test]
    fn leaf_interp_still_lowers_and_byte_matches_v0_after_the_wall() {
        // NO REGRESSION on the 83a72efa leaf slice: a LEAF interp (`${n}` Int, `${s}`
        // String) is fully linkable (every synthetic call — __str_concat / int.to_string —
        // is in the registry), so try_render_wasm_program returns Ok and the module runs,
        // byte-matching v0. Contrast with the unlinked-call wall above.
        let src = "fn main() -> Unit = {\n  \
            let n = 42\n  let s = \"world\"\n  \
            println(\"x=${n} y=${s}\") }\n";
        let prog = lower_source(src);
        assert!(
            crate::render_wasm::unlinked_call_names(&prog).is_empty(),
            "a leaf interp must be fully linkable (no wall), got {:?}",
            crate::render_wasm::unlinked_call_names(&prog)
        );
        let wat = crate::render_wasm::try_render_wasm_program(&prog)
            .expect("a leaf interp must render cleanly (no wall)");
        if let Some(out) = build_and_run("leaf_interp_after_wall", &wat) {
            assert_eq!(out, "x=42 y=world"); // v0 golden
        }
    }

    // ── Uniform interp desugar (fix-0276): per-type `to_string`, the wall walls Float/compound ──
    //
    // The StringInterp lowering is now a single uniform desugar: each part is wrapped in its
    // type's `to_string` (Lit/String passthrough, Int → int.to_string, Bool → bool.to_string,
    // Float → float.to_string [UNLINKED → walls], compound → <module>.to_string [UNLINKED →
    // walls]). The COVERED types byte-match v0; the UNCOVERED ones clean-WALL (Unsupported),
    // never invalid wasm. Goldens captured from `almide run` on the native v0 path.

    #[test]
    fn string_interp_bool_part_byte_matches_v0() {
        // The NEW covered type: a Bool `${b}` folds via the self-hosted `bool.to_string`
        // (`if b then "true" else "false"`), byte-matching v0's interned "true"/"false"
        // select. v0: "flag=true", "flag=false", "true and false".
        let src = "fn main() -> Unit = {\n  \
            let b = true\n  let c = false\n  \
            println(\"flag=${b}\")\n  \
            println(\"flag=${c}\")\n  \
            println(\"${b} and ${c}\") }\n";
        let prog = lower_source(src);
        // STRUCTURAL: a Bool interp must auto-link bool.to_string (not defer to Opaque).
        assert!(
            prog.functions.iter().any(|f| f.name == "bool.to_string"),
            "a Bool interp part must auto-link bool.to_string"
        );
        // A covered-only interp is fully linkable — render cleanly (no wall).
        assert!(
            crate::render_wasm::unlinked_call_names(&prog).is_empty(),
            "a Bool interp must be fully linkable (no wall), got {:?}",
            crate::render_wasm::unlinked_call_names(&prog)
        );
        if let Some(out) = build_and_run("string_interp_bool", &render_wasm_program(&prog)) {
            assert_eq!(out, "flag=true\nflag=false\ntrue and false");
        }
    }

    #[test]
    fn string_interp_pure_literal_and_edges_byte_match_v0() {
        // A pure-literal interp (no `${}`) plus leading / trailing interp positions. The
        // `""` seed makes a leading `${x}` byte-identical to v0's single-part passthrough.
        // v0: "no placeholders", "world!", "[world".
        let src = "fn main() -> Unit = {\n  \
            let s = \"world\"\n  \
            println(\"no placeholders\")\n  \
            println(\"${s}!\")\n  \
            println(\"[${s}\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_edges", &render_wasm_program(&prog)) {
            assert_eq!(out, "no placeholders\nworld!\n[world");
        }
    }

    #[test]
    fn string_interp_float_part_links_and_byte_matches_v0() {
        // The Float interp type, now COVERED: `${f}` desugars to `float.to_string(f)`, which is
        // self-hosted (the faithful Dragon4 in stdlib/float_to_string.almd) and AUTO-LINKED — so
        // the interp lowers and byte-matches v0's float_display, instead of clean-walling. The
        // goldens are v0's `almide run` output for the same interps:
        //   f=2.5  → "f=2.5" ; g=0.001 → "g=0.001" (negative-k leading zeros) ;
        //   1.0/3.0 → "third=0.3333333333333333" (shortest round-trip).
        let src = "fn main() -> Unit = {\n  \
            let f = 2.5\n  println(\"f=${f}\")\n  \
            let g = 0.001\n  println(\"g=${g}\")\n  \
            let h = 1.0 / 3.0\n  println(\"third=${h}\") }\n";
        let prog = lower_source(src);
        // STRUCTURAL: a Float interp must auto-link float.to_string.
        assert!(
            prog.functions.iter().any(|f| f.name == "float.to_string"),
            "a Float interp part must auto-link float.to_string"
        );
        // Fully linkable now — no wall.
        assert!(
            crate::render_wasm::unlinked_call_names(&prog).is_empty(),
            "a Float interp must be fully linkable (no wall), got {:?}",
            crate::render_wasm::unlinked_call_names(&prog)
        );
        if let Some(out) = build_and_run("string_interp_float", &render_wasm_program(&prog)) {
            assert_eq!(out, "f=2.5\ng=0.001\nthird=0.3333333333333333");
        }
    }

    #[test]
    fn string_interp_compound_part_walls_not_invalid_wasm() {
        // RESOLVED frontier: `${xs}` over a nested `List[List[Int]]` now renders through
        // the composed `list.to_string_ll` self-host (byte-matching v0's Debug form).
        // DEEPER nesting (List[List[List[_]]]) still routes to the unlinked
        // `list.to_string_x` and walls as a unit — the guard this test keeps.
        let src = "fn main() -> Unit = {\n  let xs: List[List[Int]] = [[1, 2], [3]]\n  println(\"xs=${xs}\") }\n";
        let prog = lower_source(src);
        assert!(
            prog.functions.iter().any(|f| f.name == "main"),
            "the nested list interp must lower now"
        );
        if let Some(out) = build_and_run("string_interp_nested_list", &render_wasm_program(&prog)) {
            assert_eq!(out, "xs=[[1, 2], [3]]");
        }
        let deeper = "fn main() -> Unit = {\n  let xs: List[List[List[Int]]] = [[[1]]]\n  println(\"${xs}\") }\n";
        let prog2 = lower_source(deeper);
        assert!(
            !prog2.functions.iter().any(|f| f.name == "main"),
            "a triply-nested list literal must still wall as a unit"
        );
    }
