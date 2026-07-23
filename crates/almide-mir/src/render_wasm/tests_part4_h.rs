
    #[test]
    fn self_hosted_math_sin_bit_exact() {
        // SELF-HOSTED math.sin (faithful libm transcription incl. Payne-Hanek large path) —
        // BIT-EXACT vs v0 across small (no reduction) / medium (Cody-Waite) / large (Payne-Hanek)
        // / special inputs. Golden bits captured by running the production binary.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(float.to_bits(math.sin(0.0))))\n  \
            println(int.to_string(float.to_bits(math.sin(0.5))))\n  \
            println(int.to_string(float.to_bits(math.sin(0.7))))\n  \
            println(int.to_string(float.to_bits(math.sin(0.0 - 0.5))))\n  \
            println(int.to_string(float.to_bits(math.sin(1.0))))\n  \
            println(int.to_string(float.to_bits(math.sin(1.5))))\n  \
            println(int.to_string(float.to_bits(math.sin(2.0))))\n  \
            println(int.to_string(float.to_bits(math.sin(3.0))))\n  \
            println(int.to_string(float.to_bits(math.sin(3.14159))))\n  \
            println(int.to_string(float.to_bits(math.sin(5.0))))\n  \
            println(int.to_string(float.to_bits(math.sin(10.0))))\n  \
            println(int.to_string(float.to_bits(math.sin(100.0))))\n  \
            println(int.to_string(float.to_bits(math.sin(1000.0))))\n  \
            println(int.to_string(float.to_bits(math.sin(1000000.0))))\n  \
            println(int.to_string(float.to_bits(math.sin(1.0e20))))\n  \
            println(int.to_string(float.to_bits(math.sin(1.0e100))))\n  \
            println(int.to_string(float.to_bits(math.sin(0.0 - 1.0e20)))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "math.sin"));
        if let Some(out) = build_and_run("self_hosted_math_sin_bit_exact", &render_wasm_program(&prog)) {
            assert_eq!(
                out,
                "0\n4602308182625945072\n4603977816617654712\n-4621063854228830736\n\
                 4605754516372524270\n4607159855645224331\n4606365442650518598\n4594252404416238939\n\
                 4523376038002314448\n-4616559595297400525\n-4620296710764933294\n-4620635881084269128\n\
                 4605623088326516843\n-4623395494513026969\n-4619384910413732784\n-4622843457162800295\n\
                 4603987126441043024"
            );
        }
    }

    #[test]
    fn self_hosted_math_cos_bit_exact() {
        // SELF-HOSTED math.cos — BIT-EXACT vs v0 across small / medium / Payne-Hanek / special.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(float.to_bits(math.cos(0.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(0.5))))\n  \
            println(int.to_string(float.to_bits(math.cos(0.7))))\n  \
            println(int.to_string(float.to_bits(math.cos(0.0 - 0.5))))\n  \
            println(int.to_string(float.to_bits(math.cos(1.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(1.5))))\n  \
            println(int.to_string(float.to_bits(math.cos(2.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(3.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(3.14159))))\n  \
            println(int.to_string(float.to_bits(math.cos(5.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(10.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(100.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(1000.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(1000000.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(1.0e20))))\n  \
            println(int.to_string(float.to_bits(math.cos(1.0e100))))\n  \
            println(int.to_string(float.to_bits(math.cos(0.0 - 1.0e20)))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "math.cos"));
        if let Some(out) = build_and_run("self_hosted_math_cos_bit_exact", &render_wasm_program(&prog)) {
            assert_eq!(
                out,
                "4607182418800017408\n4606079780542709072\n4605064305524579731\n4606079780542709072\n\
                 4603041830072026764\n4589761573224315303\n-4622203781984849403\n-4616279757631920686\n\
                 -4616189618054790112\n4598781623568911065\n-4617639132858127585\n4605942297449095135\n\
                 4603240679942123964\n4606612732610269996\n4605056453202808125\n4606504395019403765\n\
                 4605056453202808125"
            );
        }
    }

    #[test]
    fn self_hosted_math_tan_bit_exact() {
        // SELF-HOSTED math.tan — BIT-EXACT vs v0 across small / medium / Payne-Hanek / special.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(float.to_bits(math.tan(0.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(0.5))))\n  \
            println(int.to_string(float.to_bits(math.tan(0.7))))\n  \
            println(int.to_string(float.to_bits(math.tan(0.0 - 0.5))))\n  \
            println(int.to_string(float.to_bits(math.tan(1.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(1.5))))\n  \
            println(int.to_string(float.to_bits(math.tan(2.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(3.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(3.14159))))\n  \
            println(int.to_string(float.to_bits(math.tan(5.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(10.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(100.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(1000.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(1000000.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(1.0e20))))\n  \
            println(int.to_string(float.to_bits(math.tan(1.0e100))))\n  \
            println(int.to_string(float.to_bits(math.tan(0.0 - 1.0e20)))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "math.tan"));
        if let Some(out) = build_and_run("self_hosted_math_tan_bit_exact", &render_wasm_program(&prog)) {
            assert_eq!(
                out,
                "0\n4603095874924660554\n4605761878818060462\n-4620276161930115254\n\
                 4609692760021066662\n4624128011757193079\n-4611269345697771272\n-4629068236098062225\n\
                 -4699995998852439300\n-4608577374993532153\n4604015134707169154\n-4619907664570524360\n\
                 4609300570492383514\n-4622969797129849664\n-4617589314634034284\n-4622285276847837315\n\
                 4605782722220741524"
            );
        }
    }

    #[test]
    fn self_hosted_trig_payne_hanek_q0_positive() {
        // The Payne-Hanek large-arg path exercises BOTH the q0<0 and the q0>0 sub-cases of
        // rem_pio2_large (q0 = e0 - 24*(jv+1)). 1e22 lands on q0=2 (the rare path that adjusts
        // iq[jz-1] and triggers a recompute); 1e21/1e50/1e200/1.5e308 hit q0<0. All byte-match v0.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(float.to_bits(math.sin(1.0e22))))\n  \
            println(int.to_string(float.to_bits(math.cos(1.0e22))))\n  \
            println(int.to_string(float.to_bits(math.tan(1.0e22))))\n  \
            println(int.to_string(float.to_bits(math.sin(1.0e21))))\n  \
            println(int.to_string(float.to_bits(math.sin(1.0e50))))\n  \
            println(int.to_string(float.to_bits(math.sin(1.0e200))))\n  \
            println(int.to_string(float.to_bits(math.sin(1.5e308))))\n  \
            println(int.to_string(float.to_bits(math.sin(1.0e30)))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_trig_payne_hanek_q0_positive", &render_wasm_program(&prog)) {
            assert_eq!(
                out,
                "-4617520874450586729\n4602887919370356980\n-4613357852672216488\n\
                 -4619187932947755553\n-4621044495868110040\n-4619396462747793844\n\
                 4605030192930844164\n4576532847381215079"
            );
        }
    }

    #[test]
    fn float_arithmetic_operators_lower() {
        // Float +/-/*// and comparison operators lower to the prim float floor (no explicit
        // prim.fmul needed). 3*2 + 3/2 = 7.5; 3<2 false; 3>2 true.
        let src = "fn fcomp(a: Float, b: Float) -> Float = a * b + a / b\n\
            fn main() -> Unit = {\n  \
              let x = 3.0\n  let y = 2.0\n  \
              let r = fcomp(x, y)\n  let eq = prim.feq(r, 7.5)\n  let n = if eq then 1 else 0\n  println(int.to_string(n))\n  \
              let lt = if x < y then 1 else 0\n  println(int.to_string(lt))\n  \
              let gt = if x > y then 1 else 0\n  println(int.to_string(gt)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("float_arithmetic_operators_lower", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n0\n1");
        }
    }

    #[test]
    fn self_hosted_bytes_split_lines_chunks() {
        // SELF-HOSTED bytes.split / lines / chunks -> List[Bytes] (nested-ownership via
        // alloc_list_str + store_str). Counts via prim.load32(handle+4); content via load_str.
        let src = "fn main() -> Unit = {\n  \
            let b = bytes.from_string(\"a,b,c\")\n  let sep = bytes.from_string(\",\")\n  let parts = bytes.split(b, sep)\n  println(int.to_string(prim.load32(prim.handle(parts) + 4)))\n  \
            let t = bytes.from_string(\"x\\ny\")\n  let ls = bytes.lines(t)\n  println(int.to_string(prim.load32(prim.handle(ls) + 4)))\n  \
            let c = bytes.from_string(\"abcd\")\n  let cks = bytes.chunks(c, 2)\n  println(int.to_string(prim.load32(prim.handle(cks) + 4)))\n  \
            let cks0 = bytes.chunks(c, 0)\n  println(int.to_string(prim.load32(prim.handle(cks0) + 4)))\n  \
            let elem = prim.load_str(prim.handle(cks) + 12)\n  println(int.to_string(bytes.get_or(elem, 0, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.split"));
        if let Some(out) = build_and_run("self_hosted_bytes_split_lines_chunks", &render_wasm_program(&prog)) {
            // split "a,b,c"->3 ; lines "x\ny"->2 ; chunks "abcd"/2 ->2 ; chunks /0 ->0 ; chunk0[0]='a'=97.
            assert_eq!(out, "3\n2\n2\n0\n97");
        }
    }

    #[test]
    fn self_hosted_bytes_split_loop_reclaims() {
        // SOUNDNESS: a bounded loop building + dropping a List[Bytes] (split) — the nested-ownership
        // pieces must be freed (DropListStr), no leak / double-free. 4000 iters.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let b = bytes.from_string(\"a,b,c,d\")\n    let sep = bytes.from_string(\",\")\n    let parts = bytes.split(b, sep)\n    \
              last = prim.load32(prim.handle(parts) + 4)\n    i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_bytes_split_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "4");
        }
    }

    #[test]
    fn self_hosted_datetime_to_iso() {
        // SELF-HOSTED datetime.to_iso (fixed-width buffer fill, no concat), byte-matching v0:
        // ts=0 -> 1970-01-01T00:00:00Z; ts=1e9 -> 2001-09-09T01:46:40Z.
        let src = "fn main() -> Unit = {\n  \
            println(datetime.to_iso(0))\n  \
            let t = 1000000000\n  println(datetime.to_iso(t)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "datetime.to_iso"));
        if let Some(out) = build_and_run("self_hosted_datetime_to_iso", &render_wasm_program(&prog)) {
            assert_eq!(out, "1970-01-01T00:00:00Z\n2001-09-09T01:46:40Z");
        }
    }

    #[test]
    fn self_hosted_datetime_from_parts() {
        // SELF-HOSTED datetime.from_parts (inverse civil), byte-matching v0: round-trips epoch 0 and
        // 1e9 from their (y,m,d,h,min,s) parts.
        let src = "fn main() -> Unit = {\n  \
            let a = datetime.from_parts(1970, 1, 1, 0, 0, 0)\n  println(int.to_string(a))\n  \
            let b = datetime.from_parts(2001, 9, 9, 1, 46, 40)\n  println(int.to_string(b)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "datetime.from_parts"));
        if let Some(out) = build_and_run("self_hosted_datetime_from_parts", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\n1000000000");
        }
    }

    #[test]
    fn self_hosted_datetime_calendar() {
        // SELF-HOSTED datetime.year/month/day/weekday (Hinnant civil algorithm), byte-matching v0:
        // ts=0 → 1970-01-01 Thursday; ts=1e9 → 2001-09-09 Sunday.
        let src = "fn main() -> Unit = {\n  \
            let y0 = datetime.year(0)\n  println(int.to_string(y0))\n  \
            let m0 = datetime.month(0)\n  println(int.to_string(m0))\n  \
            let d0 = datetime.day(0)\n  println(int.to_string(d0))\n  \
            println(datetime.weekday(0))\n  \
            let t = 1000000000\n  \
            let y1 = datetime.year(t)\n  println(int.to_string(y1))\n  \
            let m1 = datetime.month(t)\n  println(int.to_string(m1))\n  \
            let d1 = datetime.day(t)\n  println(int.to_string(d1))\n  \
            println(datetime.weekday(t)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "datetime.year"));
        if let Some(out) = build_and_run("self_hosted_datetime_calendar", &render_wasm_program(&prog)) {
            assert_eq!(out, "1970\n1\n1\nThursday\n2001\n9\n9\nSunday");
        }
    }

    #[test]
    fn self_hosted_datetime_arithmetic() {
        // SELF-HOSTED pure-arithmetic datetime.* (Unix-seconds i64), byte-matching v0: add_*,
        // diff_seconds, from/to_unix identity, the floor-mod component extractors (3661s = 1h1m1s),
        // is_before/is_after.
        let src = "fn main() -> Unit = {\n  \
            let r1 = datetime.add_days(100, 1)\n  println(int.to_string(r1))\n  \
            let r2 = datetime.add_hours(100, 1)\n  println(int.to_string(r2))\n  \
            let r3 = datetime.add_minutes(100, 1)\n  println(int.to_string(r3))\n  \
            let r4 = datetime.add_seconds(100, 1)\n  println(int.to_string(r4))\n  \
            let r5 = datetime.diff_seconds(200, 100)\n  println(int.to_string(r5))\n  \
            let r6 = datetime.from_unix(555)\n  println(int.to_string(r6))\n  \
            let r7 = datetime.to_unix(555)\n  println(int.to_string(r7))\n  \
            let r8 = datetime.hour(3661)\n  println(int.to_string(r8))\n  \
            let r9 = datetime.minute(3661)\n  println(int.to_string(r9))\n  \
            let r10 = datetime.second(3661)\n  println(int.to_string(r10))\n  \
            let bf = datetime.is_before(5, 10)\n  let nb = if bf then 1 else 0\n  println(int.to_string(nb))\n  \
            let af = datetime.is_after(5, 10)\n  let na = if af then 1 else 0\n  println(int.to_string(na)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "datetime.add_days"));
        if let Some(out) = build_and_run("self_hosted_datetime_arithmetic", &render_wasm_program(&prog)) {
            assert_eq!(out, "86500\n3700\n160\n101\n100\n555\n555\n1\n1\n1\n1\n0");
        }
    }

    #[test]
    fn self_hosted_float32_conversions() {
        // SELF-HOSTED float32.to_* (widen f32→f64 then float.to_<dst>), byte-matching v0: 3.5 → i32
        // 3 (truncate), 200.0 → i8 127 (clamp), 200.0 → u8 200, 3.5 → f64 3.5 (exact widen).
        let src = "fn main() -> Unit = {\n  \
            let a: Float32 = float.to_float32(3.5)\n  let r1 = float32.to_int32(a)\n  println(int.to_string(int.from_int32(r1)))\n  \
            let b: Float32 = float.to_float32(200.0)\n  let r2 = float32.to_int8(b)\n  println(int.to_string(int.from_int8(r2)))\n  \
            let r3 = float32.to_uint8(b)\n  println(int.to_string(int.from_uint8(r3)))\n  \
            let r4 = float32.to_float64(a)\n  let eq = prim.feq(r4, 3.5)\n  let n4 = if eq then 1 else 0\n  println(int.to_string(n4)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float32.to_int32"));
        if let Some(out) = build_and_run("self_hosted_float32_conversions", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n127\n200\n1");
        }
    }

    #[test]
    fn self_hosted_int16_int32_conversions() {
        // SELF-HOSTED int16.to_* / int32.to_* (same two-hop shape as int8), byte-matching v0:
        // widening (1000 → 1000 / 100000 → 100000) and unsigned narrowing wrap (1000 & 0xFF = 232,
        // 100000 & 0xFFFF = 34464).
        let src = "fn main() -> Unit = {\n  \
            let a: Int16 = int.to_int16(1000)\n  let r1 = int16.to_int32(a)\n  println(int.to_string(int.from_int32(r1)))\n  \
            let r2 = int16.to_uint8(a)\n  println(int.to_string(int.from_uint8(r2)))\n  \
            let b: Int32 = int.to_int32(100000)\n  let r3 = int32.to_int64(b)\n  println(int.to_string(r3))\n  \
            let r4 = int32.to_uint16(b)\n  println(int.to_string(int.from_uint16(r4))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int16.to_int32"));
        assert!(prog.functions.iter().any(|f| f.name == "int32.to_int64"));
        if let Some(out) = build_and_run("self_hosted_int16_int32_conversions", &render_wasm_program(&prog)) {
            assert_eq!(out, "1000\n232\n100000\n34464");
        }
    }

    #[test]
    fn self_hosted_float_checked_64bit() {
        // The 64-bit variants: 2^63 / 2^64 bound built at runtime. to_uint64 of a value in
        // [2^63, 2^64) is rejected by the ROUND-TRIP (i64-repr wraps negative ≠ n), not the range —
        // matching v0. 1e19 (> 2^63, < 2^64) exercises both the int64 range reject and that uint64
        // round-trip reject. The last line prints the actual Some value (1000000).
        let src = "fn main() -> Unit = {\n  \
            match float.to_int64_checked(2000000000.0) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_int64_checked(2000000000.5) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_int64_checked(1e19) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_uint64_checked(-1.0) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_uint64_checked(1e19) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_uint64_checked(1000000.0) { Some(v) => { let iv = int.from_uint64(v); println(int.to_string(iv)) }, None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.to_int64_checked"));
        if let Some(out) = build_and_run("self_hosted_float_checked_64bit", &render_wasm_program(&prog)) {
            assert_eq!(out, "some\nnone\nnone\nnone\nnone\n1000000");
        }
    }

    #[test]
    fn self_hosted_float_checked_loop_reclaims() {
        // SOUNDNESS: a bounded loop building + dropping the scalar Option from a float _checked
        // conversion — no leak / double-free. 4000 iters; `last` counts the in-range integer hits.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let fi = int.to_float(i)\n    \
              match float.to_uint8_checked(fi) { Some(v) => { last = last + 1 }, None => { last = last }, }\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_float_checked_loop_reclaims", &render_wasm_program(&prog)) {
            // i in 0..3999; an in-range integer uint8 is 0..255 → 256 hits.
            assert_eq!(out, "256");
        }
    }

    #[test]
    fn self_hosted_option_zip() {
        // SELF-HOSTED option.zip → Some((a, b)) when both Some (a SCALAR (Int,Int) tuple owned by a
        // heap Some), None otherwise. The Some payload is a HEAP tuple — the variant-match binds it by
        // LoadHandle (the borrowed tuple handle) and the scope-end DropListStr frees it. Args are
        // let-bound Options (a literal `Some(x)` call-arg does not materialize). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let a = Some(3)\n  let b = Some(4)\n  \
            match option.zip(a, b) { Some(p) => { let (x, y) = p\n println(int.to_string(x + y)) }, None => println(\"none\"), }\n  \
            let c: Option[Int] = None\n  \
            match option.zip(a, c) { Some(p) => { let (x, y) = p\n println(int.to_string(x + y)) }, None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.zip"));
        if let Some(out) = build_and_run("self_hosted_option_zip", &render_wasm_program(&prog)) {
            assert_eq!(out, "7\nnone");
        }
    }

    #[test]
    fn self_hosted_option_zip_loop_reclaims() {
        // SOUNDNESS for the Option[heap-tuple] match path: a bounded loop building option.zip, matching
        // + destructuring (the heap Some owns a flat scalar tuple, freed by DropListStr each iter) — no
        // leak/double-free. 4000 iters; `last` accumulates the destructured sum.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let a = Some(i)\n    let b = Some(1)\n    \
              match option.zip(a, b) { Some(p) => { let (x, y) = p\n last = x + y }, None => { last = 0 }, }\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_option_zip_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "4000");
        }
    }

    #[test]
    fn self_hosted_result_zip() {
        // SELF-HOSTED result.zip → both Ok → Ok((va, vb)) (a scalar tuple in a HEAP-Ok Result), else
        // the first Err (message deep-copied). Ok payload destructured; Err message printed. v0-match.
        let src = "fn main() -> Unit = {\n  \
            let a: Result[Int, String] = Ok(3)\n  let b: Result[Int, String] = Ok(4)\n  \
            match result.zip(a, b) { Ok(p) => { let (x, y) = p\n println(int.to_string(x + y)) }, Err(e) => println(e), }\n  \
            let c: Result[Int, String] = Err(\"bad\")\n  \
            match result.zip(a, c) { Ok(p) => { let (x, y) = p\n println(int.to_string(x + y)) }, Err(e) => println(e), }\n  \
            match result.zip(c, b) { Ok(p) => { let (x, y) = p\n println(int.to_string(x + y)) }, Err(e) => println(e), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.zip"));
        if let Some(out) = build_and_run("self_hosted_result_zip", &render_wasm_program(&prog)) {
            assert_eq!(out, "7\nbad\nbad");
        }
    }

    #[test]
    fn self_hosted_result_zip_loop_reclaims() {
        // SOUNDNESS: a bounded loop building result.zip, matching + destructuring the Ok tuple (the
        // heap-Ok Result owns the tuple, freed by DropListStr each iter) — no leak/double-free.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let a: Result[Int, String] = Ok(i)\n    let b: Result[Int, String] = Ok(1)\n    \
              match result.zip(a, b) { Ok(p) => { let (x, y) = p\n last = x + y }, Err(e) => { last = 0 }, }\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_result_zip_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "4000");
        }
    }

    #[test]
    fn self_hosted_result_filter() {
        // SELF-HOSTED result.filter → Ok(v) kept iff pred(v) (else Err(err_val)); Err propagated. The
        // closure runs only on the Ok arm; the err_val / propagated message are deep-copied. v0-match.
        let src = "fn main() -> Unit = {\n  \
            let a: Result[Int, String] = Ok(5)\n  let ra = result.filter(a, (x) => x > 3, \"small\")\n  \
            match ra { Ok(v) => println(int.to_string(v)), Err(e) => println(e), }\n  \
            let b: Result[Int, String] = Ok(2)\n  let rb = result.filter(b, (x) => x > 3, \"small\")\n  \
            match rb { Ok(v) => println(int.to_string(v)), Err(e) => println(e), }\n  \
            let c: Result[Int, String] = Err(\"bad\")\n  let rc = result.filter(c, (x) => x > 3, \"small\")\n  \
            match rc { Ok(v) => println(int.to_string(v)), Err(e) => println(e), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.filter"));
        if let Some(out) = build_and_run("self_hosted_result_filter", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\nsmall\nbad");
        }
    }

    #[test]
    fn self_hosted_result_filter_loop_reclaims() {
        // SOUNDNESS: a bounded loop result.filter-ing, matching both arms (Err copies a String each
        // fail iter) — no leak/double-free. 4000 iters; even i pass (>2 are kept... use i%... simpler:
        // keep iff i is large), `last` counts kept.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let a: Result[Int, String] = Ok(i)\n    let ra = result.filter(a, (x) => x > 1000, \"small\")\n    \
              match ra { Ok(v) => { last = last + 1 }, Err(e) => { last = last }, }\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_result_filter_loop_reclaims", &render_wasm_program(&prog)) {
            // i in 0..3999; kept iff i > 1000 → 1001..3999 = 2999.
            assert_eq!(out, "2999");
        }
    }

    #[test]
    fn self_hosted_result_or_else() {
        // SELF-HOSTED result.or_else → Ok(v) kept, Err(e) → f(e) (recovery closure given a copy of the
        // error). Closure runs only on the Err arm. v0-match (closure within the v1 subset).
        let src = "fn main() -> Unit = {\n  \
            let a: Result[Int, String] = Ok(5)\n  let ra = result.or_else(a, (e) => Ok(0))\n  \
            match ra { Ok(v) => println(int.to_string(v)), Err(e) => println(e), }\n  \
            let b: Result[Int, String] = Err(\"bad\")\n  let rb = result.or_else(b, (e) => Ok(99))\n  \
            match rb { Ok(v) => println(int.to_string(v)), Err(e) => println(e), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.or_else"));
        if let Some(out) = build_and_run("self_hosted_result_or_else", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\n99");
        }
    }

    #[test]
    fn self_hosted_result_or_else_loop_reclaims() {
        // SOUNDNESS: a bounded loop or_else-ing an Err (the error is deep-copied each iter then the
        // recovery closure returns Ok) — no leak/double-free. 4000 iters.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let b: Result[Int, String] = Err(\"bad\")\n    let rb = result.or_else(b, (e) => Ok(7))\n    \
              match rb { Ok(v) => { last = v }, Err(e) => { last = 0 }, }\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_result_or_else_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "7");
        }
    }

