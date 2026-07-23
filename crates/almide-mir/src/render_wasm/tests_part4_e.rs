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

include!("tests_part4_j.rs");
