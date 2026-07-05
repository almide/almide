    #[test]
    fn self_hosted_map_string_string_keys_values_remove() {
        // SELF-HOSTED Map[String,String] keys/values/remove. m={a:x, b:y, c:z}: keys=[a,b,c] (first
        // "a"), values=[x,y,z] (first "x"); remove("b")={a:x, c:z} len 2, get("b")=None, get("c")="z".
        let src = "fn main() -> Unit = {\n  \
            let m1 = map.set(map.new(), \"a\", \"x\")\n  let m2 = map.set(m1, \"b\", \"y\")\n  \
            let m = map.set(m2, \"c\", \"z\")\n  \
            let ks = map.keys(m)\n  println(int.to_string(list.len(ks)))\n  \
            let k0 = list.get(ks, 0)\n  match k0 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let vs = map.values(m)\n  let v0 = list.get(vs, 0)\n  \
            match v0 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let r = map.remove(m, \"b\")\n  println(int.to_string(map.len(r)))\n  \
            let gb = map.get(r, \"b\")\n  match gb { Some(v) => println(v), None => println(\"none\"), }\n  \
            let gc = map.get(r, \"c\")\n  match gc { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.keys_str"));
        assert!(prog.functions.iter().any(|f| f.name == "map.remove_str"));
        if let Some(out) = build_and_run("self_hosted_map_string_string_keys_values_remove", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\na\nx\n2\nnone\nz");
        }
    }

    #[test]
    fn self_hosted_map_string_string_merge_update() {
        // SELF-HOSTED Map[String,String] merge/update. a={x:1, y:2} b={y:9, z:3}: merge={x:1, y:9,
        // z:3} (b overrides y, appends z) len 3, get("y")="9", get("z")="3". update("x", v=>repeat
        // v×2): {x:11, y:2} get("x")="11". Byte-matches v0. Closure body is a let-bound self-host call.
        let src = "fn main() -> Unit = {\n  \
            let a = map.set(map.set(map.new(), \"x\", \"1\"), \"y\", \"2\")\n  \
            let b = map.set(map.set(map.new(), \"y\", \"9\"), \"z\", \"3\")\n  \
            let mg = map.merge(a, b)\n  println(int.to_string(map.len(mg)))\n  \
            let gy = map.get(mg, \"y\")\n  match gy { Some(v) => println(v), None => println(\"none\"), }\n  \
            let gz = map.get(mg, \"z\")\n  match gz { Some(v) => println(v), None => println(\"none\"), }\n  \
            let mu = map.update(a, \"x\", (s) => { let r = string.repeat(s, 2)\n r })\n  \
            let gx = map.get(mu, \"x\")\n  match gx { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.merge_str"));
        assert!(prog.functions.iter().any(|f| f.name == "map.update_str"));
        if let Some(out) = build_and_run("self_hosted_map_string_string_merge_update", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n9\n3\n11");
        }
    }

    #[test]
    fn self_hosted_map_string_string_higher_order() {
        // SELF-HOSTED higher-order Map[String,String]: filter/all/any/count ((K,V)->Bool, 2-arg) +
        // fold ((Acc,K,V)->Acc, 3-arg, mixing scalar Acc + heap K/V). m={a:xx, bb:y, c:zzz}: filter
        // (len(v)>1)={a:xx, c:zzz} len 2; all(len(k)>=1)=true; any(len(v)==3)=true; count(len(k)==1)
        // =2 (a,c); fold(0, acc+len(k)+len(v)) = (1+2)+(2+1)+(1+3) = 10. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m1 = map.set(map.new(), \"a\", \"xx\")\n  let m2 = map.set(m1, \"bb\", \"y\")\n  \
            let m = map.set(m2, \"c\", \"zzz\")\n  \
            let fm = map.filter(m, (k, v) => { let l = string.len(v)\n l > 1 })\n  \
            println(int.to_string(map.len(fm)))\n  \
            let al = map.all(m, (k, v) => { let l = string.len(k)\n l >= 1 })\n  \
            let va = if al then 1 else 0\n  println(int.to_string(va))\n  \
            let an = map.any(m, (k, v) => { let l = string.len(v)\n l == 3 })\n  \
            let vn = if an then 1 else 0\n  println(int.to_string(vn))\n  \
            let cn = map.count(m, (k, v) => { let l = string.len(k)\n l == 1 })\n  \
            println(int.to_string(cn))\n  \
            let tot = map.fold(m, 0, (acc, k, v) => { let lk = string.len(k)\n let lv = string.len(v)\n acc + lk + lv })\n  \
            println(int.to_string(tot)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.filter_str"));
        assert!(prog.functions.iter().any(|f| f.name == "map.fold_str"));
        if let Some(out) = build_and_run("self_hosted_map_string_string_higher_order", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n1\n1\n2\n10");
        }
    }

    #[test]
    fn self_hosted_map_string_string_loop_reclaims() {
        // SOUNDNESS for the Map[String,String] nested-ownership path: a bounded loop building +
        // dropping a fresh Map[String,String] each iteration must reclaim every key + value String +
        // the block (DropListStr over all slots) — no leak/double-free. 3000 iters, last len 2.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 3000 {\n    \
              let m1 = map.set(map.new(), \"key-one\", \"val-one\")\n    \
              let m = map.set(m1, \"key-two\", \"val-two\")\n    \
              last = map.len(m)\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_map_string_string_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "2");
        }
    }

    #[test]
    fn self_hosted_float_saturating_conversions() {
        // SELF-HOSTED float.to_{int,uint}N_saturating → the f64→iN cast (already saturating: out-of-
        // range clamps to min/max, NaN → 0), forwarded to the registered float.to_intN. Result is a
        // sized Int (scalar) → widen via int.from_intN to print. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(int.from_int8(float.to_int8_saturating(300.0))))\n  \
            println(int.to_string(int.from_int8(float.to_int8_saturating(-300.0))))\n  \
            println(int.to_string(int.from_uint8(float.to_uint8_saturating(300.0))))\n  \
            println(int.to_string(int.from_uint8(float.to_uint8_saturating(-5.0))))\n  \
            println(int.to_string(int.from_int16(float.to_int16_saturating(70000.0)))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.to_int8_saturating"));
        if let Some(out) = build_and_run("self_hosted_float_saturating_conversions", &render_wasm_program(&prog)) {
            // 300→127, -300→-128, u8 300→255, u8 -5→0, i16 70000→32767.
            assert_eq!(out, "127\n-128\n255\n0\n32767");
        }
    }

    #[test]
    fn self_hosted_int_checked_conversions() {
        // SELF-HOSTED int.to_{int,uint}N_checked → Some(n) iff n fits the N-bit range, else None. The
        // discriminant (the only thing that can differ from v0 — the in-range value is Some(n) by
        // construction) byte-matches v0's range logic.
        let src = "fn main() -> Unit = {\n  \
            match int.to_int8_checked(100) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match int.to_int8_checked(200) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match int.to_int8_checked(-200) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match int.to_uint8_checked(-1) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match int.to_uint8_checked(255) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match int.to_uint16_checked(70000) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match int.to_uint64_checked(-5) { Some(v) => println(\"some\"), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.to_int8_checked"));
        if let Some(out) = build_and_run("self_hosted_int_checked_conversions", &render_wasm_program(&prog)) {
            assert_eq!(out, "some\nnone\nnone\nnone\nsome\nnone\nnone");
        }
    }

    #[test]
    fn self_hosted_int_checked_loop_reclaims() {
        // SOUNDNESS: a bounded loop building + dropping the scalar Option[Int] (Some/None) from a
        // _checked conversion — no leak/double-free. 4000 iters; `last` counts the in-range hits.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              match int.to_uint8_checked(i) { Some(v) => { last = last + 1 }, None => { last = last }, }\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_int_checked_loop_reclaims", &render_wasm_program(&prog)) {
            // i in 0..3999; uint8 in-range is 0..255 → 256 hits.
            assert_eq!(out, "256");
        }
    }

    #[test]
    fn self_hosted_float_checked_conversions() {
        // SELF-HOSTED float.to_{int,uint}{8,16,32}_checked → Some(T) iff `n` is an EXACT integer in
        // T's range, else None (out-of-range / fractional / negative-for-unsigned). The discriminant
        // byte-matches v0's round-trip-via-int.to_float guard; the in-range value is `to_T(n)` by
        // construction (the 7th line prints the actual value 200 to confirm the payload flows).
        let src = "fn main() -> Unit = {\n  \
            match float.to_int8_checked(100.0) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_int8_checked(200.0) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_int8_checked(100.5) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_int16_checked(30000.0) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_int32_checked(2000000000.0) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_uint8_checked(-1.0) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_uint8_checked(200.0) { Some(v) => { let iv = int.from_uint8(v); println(int.to_string(iv)) }, None => println(\"none\"), }\n  \
            match float.to_uint16_checked(65535.0) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_uint32_checked(5000000000.0) { Some(v) => println(\"some\"), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.to_int8_checked"));
        if let Some(out) = build_and_run("self_hosted_float_checked_conversions", &render_wasm_program(&prog)) {
            assert_eq!(out, "some\nnone\nnone\nsome\nsome\nnone\n200\nsome\nnone");
        }
    }

    #[test]
    fn self_hosted_float32_demote_promote() {
        // SELF-HOSTED float.to_float32 (f32.demote_f64) / float.from_float32 (f64.promote_f32) /
        // int.bits_to_f32 over the new f32 prim floor. Verified by the IEEE round-trip property
        // (which holds bit-identically in v0, since both use the same demote/promote): an
        // f32-representable value round-trips exactly; a non-representable one loses precision; the
        // demote is idempotent on an already-f32 value; bits_to_f32(0x3F800000) widens to 1.0.
        let src = "fn main() -> Unit = {\n  \
            let t1 = float.to_float32(3.5)\n  let a = float.from_float32(t1)\n  let ra = prim.feq(a, 3.5)\n  let na = if ra then 1 else 0\n  println(int.to_string(na))\n  \
            let t2 = float.to_float32(0.1)\n  let b = float.from_float32(t2)\n  let rb = prim.feq(b, 0.1)\n  let nb = if rb then 1 else 0\n  println(int.to_string(nb))\n  \
            let t3 = float.to_float32(b)\n  let c = float.from_float32(t3)\n  let rc = prim.feq(c, b)\n  let nc = if rc then 1 else 0\n  println(int.to_string(nc))\n  \
            let d = int.bits_to_f32(1065353216)\n  let rd = prim.feq(d, 1.0)\n  let nd = if rd then 1 else 0\n  println(int.to_string(nd)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.to_float32"));
        if let Some(out) = build_and_run("self_hosted_float32_demote_promote", &render_wasm_program(&prog)) {
            // exact f32 / inexact / idempotent / bits_to_f32(1.0-pattern) = 1.0.
            assert_eq!(out, "1\n0\n1\n1");
        }
    }

    #[test]
    fn self_hosted_float32_convert_followons() {
        // int.to_float32 (f32.convert_i64_s, single rounding) + float.to_float32_checked (Option,
        // round-trip exactness). 2^24+1 is not f32-representable → rounds to 2^24 (16777216).
        let src = "fn main() -> Unit = {\n  \
            let g = int.to_float32(16777217)\n  let gw = float.from_float32(g)\n  let r1 = prim.feq(gw, 16777216.0)\n  let n1 = if r1 then 1 else 0\n  println(int.to_string(n1))\n  \
            let h = int.to_float32(100)\n  let hw = float.from_float32(h)\n  let r2 = prim.feq(hw, 100.0)\n  let n2 = if r2 then 1 else 0\n  println(int.to_string(n2))\n  \
            match float.to_float32_checked(3.5) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match float.to_float32_checked(0.1) { Some(v) => println(\"some\"), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.to_float32"));
        assert!(prog.functions.iter().any(|f| f.name == "float.to_float32_checked"));
        if let Some(out) = build_and_run("self_hosted_float32_convert_followons", &render_wasm_program(&prog)) {
            // 2^24+1 → 2^24 (rounded) / 100 exact / 3.5 f32-exact → Some / 0.1 lossy → None.
            assert_eq!(out, "1\n1\nsome\nnone");
        }
    }

    #[test]
    fn prim_f2i_saturates_like_v0_as_cast() {
        // prim.f2i (float.to_int / to_int64) is SATURATING (i64.trunc_sat_f64_s), matching Rust's
        // `n as i64` (v0): an out-of-range float clamps to i64::MAX instead of TRAPPING. Regression
        // for the trunc→trunc_sat render fix that the 64-bit _checked conversions require.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(float.to_int64(1e19)))\n  \
            println(int.to_string(float.to_int64(123.9))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("prim_f2i_saturates_like_v0_as_cast", &render_wasm_program(&prog)) {
            // 1e19 > i64::MAX → 9223372036854775807 (saturated, NOT a trap); 123.9 → 123 (truncate).
            assert_eq!(out, "9223372036854775807\n123");
        }
    }

    #[test]
    fn self_hosted_int8_conversions() {
        // SELF-HOSTED int8.to_* sized conversions (two-hop int.from_int8 → int.to_<dst>), byte-
        // matching v0's int8.almd. Covers widening (100 → 100), unsigned wrap of a negative
        // (Int8 -1 → u8 255 / u16 65535), and int→float (100 → 100.0).
        let src = "fn main() -> Unit = {\n  \
            let p: Int8 = int.to_int8(100)\n  let r1 = int8.to_int16(p)\n  println(int.to_string(int.from_int16(r1)))\n  \
            let n: Int8 = int.to_int8(255)\n  let r2 = int8.to_uint8(n)\n  println(int.to_string(int.from_uint8(r2)))\n  \
            let r3 = int8.to_uint16(n)\n  println(int.to_string(int.from_uint16(r3)))\n  \
            let r4 = int8.to_int64(p)\n  println(int.to_string(r4))\n  \
            let r5 = int8.to_float64(p)\n  let eq = prim.feq(r5, 100.0)\n  let n5 = if eq then 1 else 0\n  println(int.to_string(n5)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int8.to_int16"));
        if let Some(out) = build_and_run("self_hosted_int8_conversions", &render_wasm_program(&prog)) {
            assert_eq!(out, "100\n255\n65535\n100\n1");
        }
    }

    #[test]
    fn self_hosted_sized_int_to_string() {
        // SELF-HOSTED int8/16/32.to_string via int.to_string (which formats SIGNED values). The
        // negative cases (-128 / -1000 / -100000) exercise the leading-'-' path, byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let a: Int8 = int.to_int8(255)\n  println(int8.to_string(a))\n  \
            let b: Int8 = int.to_int8(42)\n  println(int8.to_string(b))\n  \
            let c: Int16 = int.to_int16(64536)\n  println(int16.to_string(c))\n  \
            let d: Int32 = int.to_int32(4294867296)\n  println(int32.to_string(d)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int8.to_string"));
        if let Some(out) = build_and_run("self_hosted_sized_int_to_string", &render_wasm_program(&prog)) {
            // 255→i8 -1; 42→42; 64536→i16 -1000; 4294867296→i32 -100000.
            assert_eq!(out, "-1\n42\n-1000\n-100000");
        }
    }

    #[test]
    fn self_hosted_bytes_fixed_size_mutation() {
        // SELF-HOSTED in-place fixed-size byte mutations (borrow + prim.store). The mutation is
        // visible to the caller (shared block); set_u16_be/le verify endianness; fill/clear too.
        let src = "fn main() -> Unit = {\n  \
            let b = bytes.new(10)\n  \
            bytes.set_u8(b, 0, 200)\n  \
            bytes.set_u16_be(b, 1, 4660)\n  \
            bytes.set_u16_le(b, 3, 4660)\n  \
            bytes.set_u32_be(b, 5, 16909060)\n  \
            let g0 = bytes.get_or(b, 0, 0)\n  println(int.to_string(g0))\n  \
            let g1 = bytes.get_or(b, 1, 0)\n  println(int.to_string(g1))\n  \
            let g2 = bytes.get_or(b, 2, 0)\n  println(int.to_string(g2))\n  \
            let g3 = bytes.get_or(b, 3, 0)\n  println(int.to_string(g3))\n  \
            let g4 = bytes.get_or(b, 4, 0)\n  println(int.to_string(g4))\n  \
            let g5 = bytes.get_or(b, 5, 0)\n  println(int.to_string(g5))\n  \
            let g8 = bytes.get_or(b, 8, 0)\n  println(int.to_string(g8))\n  \
            let c = bytes.new(8)\n  bytes.set_f64_be(c, 0, 1.5)\n  let cf = bytes.get_or(c, 0, 0)\n  println(int.to_string(cf))\n  \
            bytes.fill(b, 255)\n  let gf = bytes.get_or(b, 7, 0)\n  println(int.to_string(gf))\n  \
            bytes.clear(b)\n  let gl = bytes.len(b)\n  println(int.to_string(gl)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.set_u8"));
        if let Some(out) = build_and_run("self_hosted_bytes_fixed_size_mutation", &render_wasm_program(&prog)) {
            // u8=200; u16_be 4660=0x1234 -> 18,52; u16_le -> 52,18; u32_be 16909060=0x01020304 -> b5=1,b8=4;
            // f64 1.5 = 0x3FF8.. -> BE byte0=0x3F=63; fill 255 -> b7=255; clear -> len 0.
            assert_eq!(out, "200\n18\n52\n52\n18\n1\n4\n63\n255\n0");
        }
    }

    #[test]
    fn self_hosted_bytes_set_f32() {
        // SELF-HOSTED bytes.set_f32_be/le (demote f64->f32, store 4 bits via the new f32bits prim).
        // Round-trip through read_f32_be/le: 1.5 and 3.0 are f32-exact.
        let src = "fn main() -> Unit = {\n  \
            let b = bytes.new(8)\n  \
            bytes.set_f32_be(b, 0, 1.5)\n  let g0 = bytes.read_f32_be(b, 0)\n  let e0 = prim.feq(g0, 1.5)\n  let n0 = if e0 then 1 else 0\n  println(int.to_string(n0))\n  \
            bytes.set_f32_le(b, 4, 3.0)\n  let g1 = bytes.read_f32_le(b, 4)\n  let e1 = prim.feq(g1, 3.0)\n  let n1 = if e1 then 1 else 0\n  println(int.to_string(n1)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.set_f32_be"));
        if let Some(out) = build_and_run("self_hosted_bytes_set_f32", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n1");
        }
    }

    #[test]
    fn self_hosted_bytes_read_f32_array() {
        // SELF-HOSTED bytes.read_f32_be/le_array -> List[Float]. 0x3FC00000=1.5, 0x40400000=3.0.
        let src = "fn main() -> Unit = {\n  \
            let b = bytes.new(8)\n  bytes.set_u32_be(b, 0, 1069547520)\n  bytes.set_u32_be(b, 4, 1077936128)\n  \
            let arr = bytes.read_f32_be_array(b, 0, 2)\n  let ah = prim.handle(arr)\n  \
            let bits0 = prim.load64(ah + 12)\n  let f0 = prim.ffrombits(bits0)\n  let eq0 = prim.feq(f0, 1.5)\n  let n0 = if eq0 then 1 else 0\n  println(int.to_string(n0))\n  \
            let bits1 = prim.load64(ah + 20)\n  let f1 = prim.ffrombits(bits1)\n  let eq1 = prim.feq(f1, 3.0)\n  let n1 = if eq1 then 1 else 0\n  println(int.to_string(n1))\n  \
            let c = bytes.new(4)\n  bytes.set_u32_le(c, 0, 1069547520)\n  let arrle = bytes.read_f32_le_array(c, 0, 1)\n  \
            let lbits = prim.load64(prim.handle(arrle) + 12)\n  let lf = prim.ffrombits(lbits)\n  let leq = prim.feq(lf, 1.5)\n  let ln = if leq then 1 else 0\n  println(int.to_string(ln)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.read_f32_be_array"));
        if let Some(out) = build_and_run("self_hosted_bytes_read_f32_array", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n1\n1");
        }
    }

    #[test]
    fn self_hosted_audit_clean_batch() {
        // int.to_float32_checked (Option round-trip), string.clear (borrow+store len=0), and
        // bytes.read_f32_be/le (4-byte f32 read → Float). All audit-confirmed clean.
        let src = "fn main() -> Unit = {\n  \
            match int.to_float32_checked(100) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            match int.to_float32_checked(16777217) { Some(v) => println(\"some\"), None => println(\"none\"), }\n  \
            let s = string.repeat(\"ab\", 3)\n  string.clear(s)\n  let sl = string.len(s)\n  println(int.to_string(sl))\n  \
            let b = bytes.new(8)\n  bytes.set_u32_be(b, 0, 1069547520)\n  \
            let f = bytes.read_f32_be(b, 0)\n  let eq = prim.feq(f, 1.5)\n  let ne = if eq then 1 else 0\n  println(int.to_string(ne))\n  \
            bytes.set_u32_le(b, 0, 1069547520)\n  let g = bytes.read_f32_le(b, 0)\n  let eq2 = prim.feq(g, 1.5)\n  let ne2 = if eq2 then 1 else 0\n  println(int.to_string(ne2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.to_float32_checked"));
        assert!(prog.functions.iter().any(|f| f.name == "string.clear"));
        assert!(prog.functions.iter().any(|f| f.name == "bytes.read_f32_be"));
        if let Some(out) = build_and_run("self_hosted_audit_clean_batch", &render_wasm_program(&prog)) {
            // 100 round-trips f32 (some); 2^24+1 loses precision (none); clear -> len 0; 1069547520=0x3FC00000=f32 1.5.
            assert_eq!(out, "some\nnone\n0\n1\n1");
        }
    }

    #[test]
    fn self_hosted_math_log_bit_exact() {
        // SELF-HOSTED math.log (faithful libm transcription) — BIT-EXACT vs v0 (reference bits
        // captured by running v0: log(2)=4604418534313441775, log(10)=4612367379483415830,
        // log(1)=0, log(100)=4616870979110786326).
        let src = "fn main() -> Unit = {\n  \
            let a = math.log(2.0)\n  println(int.to_string(float.to_bits(a)))\n  \
            let b = math.log(10.0)\n  println(int.to_string(float.to_bits(b)))\n  \
            let c = math.log(1.0)\n  println(int.to_string(float.to_bits(c)))\n  \
            let d = math.log(100.0)\n  println(int.to_string(float.to_bits(d))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "math.log"));
        if let Some(out) = build_and_run("self_hosted_math_log_bit_exact", &render_wasm_program(&prog)) {
            assert_eq!(out, "4604418534313441775\n4612367379483415830\n0\n4616870979110786326");
        }
    }

    #[test]
    fn self_hosted_math_fpow_bit_exact() {
        // SELF-HOSTED math.fpow (faithful libm pow, agent-transcribed, verified vs vendored libm
        // over 32k inputs) — BIT-EXACT vs v0: fpow(2,10)=1024, fpow(2,.5)=sqrt2, fpow(-2,3)=-8,
        // fpow(10,-2)=.01, fpow(3,3)=27.
        let src = "fn main() -> Unit = {\n  \
            let a = math.fpow(2.0, 10.0)\n  println(int.to_string(float.to_bits(a)))\n  \
            let b = math.fpow(2.0, 0.5)\n  println(int.to_string(float.to_bits(b)))\n  \
            let c = math.fpow(0.0 - 2.0, 3.0)\n  println(int.to_string(float.to_bits(c)))\n  \
            let d = math.fpow(10.0, 0.0 - 2.0)\n  println(int.to_string(float.to_bits(d)))\n  \
            let e = math.fpow(3.0, 3.0)\n  println(int.to_string(float.to_bits(e))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "math.fpow"));
        if let Some(out) = build_and_run("self_hosted_math_fpow_bit_exact", &render_wasm_program(&prog)) {
            assert_eq!(out, "4652218415073722368\n4609047870845172685\n-4602678819172646912\n4576918229304087675\n4628293042053316608");
        }
    }

    #[test]
    fn self_hosted_math_exp_bit_exact() {
        // SELF-HOSTED math.exp (faithful libm) — BIT-EXACT vs v0: exp(1)=4613303445314885482,
        // exp(0)=4607182418800017408, exp(2.5)=4623047752462491835, exp(-1)=4600298746774613816.
        let src = "fn main() -> Unit = {\n  \
            let a = math.exp(1.0)\n  println(int.to_string(float.to_bits(a)))\n  \
            let b = math.exp(0.0)\n  println(int.to_string(float.to_bits(b)))\n  \
            let c = math.exp(2.5)\n  println(int.to_string(float.to_bits(c)))\n  \
            let d = math.exp(0.0 - 1.0)\n  println(int.to_string(float.to_bits(d))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_math_exp_bit_exact", &render_wasm_program(&prog)) {
            assert_eq!(out, "4613303445314885482\n4607182418800017408\n4623047752462491835\n4600298746774613816");
        }
    }

    #[test]
    fn self_hosted_math_log2_bit_exact() {
        // SELF-HOSTED math.log2 — BIT-EXACT vs v0: log2(8)=4613937818241073152,
        // log2(10)=4614662735865160561, log2(1)=0.
        let src = "fn main() -> Unit = {\n  \
            let a = math.log2(8.0)\n  println(int.to_string(float.to_bits(a)))\n  \
            let b = math.log2(10.0)\n  println(int.to_string(float.to_bits(b)))\n  \
            let c = math.log2(1.0)\n  println(int.to_string(float.to_bits(c))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_math_log2_bit_exact", &render_wasm_program(&prog)) {
            assert_eq!(out, "4613937818241073152\n4614662735865160561\n0");
        }
    }

    #[test]
    fn self_hosted_math_log10_bit_exact() {
        // SELF-HOSTED math.log10 — BIT-EXACT vs v0: log10(1000)=4613937818241073152,
        // log10(2)=4599094494223104511, log10(1)=0.
        let src = "fn main() -> Unit = {\n  \
            let a = math.log10(1000.0)\n  println(int.to_string(float.to_bits(a)))\n  \
            let b = math.log10(2.0)\n  println(int.to_string(float.to_bits(b)))\n  \
            let c = math.log10(1.0)\n  println(int.to_string(float.to_bits(c))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_math_log10_bit_exact", &render_wasm_program(&prog)) {
            assert_eq!(out, "4613937818241073152\n4599094494223104511\n0");
        }
    }





















    #[test]
    fn math_trig_independent_golden_cross_check() {
        // INDEPENDENT cross-check of the self-hosted sin/cos/tan: golden bits generated SEPARATELY
        // via the production `~/.local/bin/almide` binary (a different oracle path than the agent's
        // own dev sweep), covering every reduction path — π/4 (no reduction), 100 (Cody-Waite),
        // 1e6 (Cody-Waite tier), π/2 (special: sin=1, cos≈0, tan huge), 1e20 (Payne-Hanek). If the
        // v1-self-host trig byte-matches THESE independently-derived goldens, the port is confirmed.
        // Direct literal args (for-in over a List[Float] literal is a separate v1 gap — elements
        // read as 0.0; the agent's tests use direct args for the same reason).
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(float.to_bits(math.sin(0.7853981633974483))))\n  \
            println(int.to_string(float.to_bits(math.cos(0.7853981633974483))))\n  \
            println(int.to_string(float.to_bits(math.tan(0.7853981633974483))))\n  \
            println(int.to_string(float.to_bits(math.sin(100.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(100.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(100.0))))\n  \
            println(int.to_string(float.to_bits(math.sin(1000000.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(1000000.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(1000000.0))))\n  \
            println(int.to_string(float.to_bits(math.sin(1.5707963267948966))))\n  \
            println(int.to_string(float.to_bits(math.cos(1.5707963267948966))))\n  \
            println(int.to_string(float.to_bits(math.tan(1.5707963267948966))))\n  \
            println(int.to_string(float.to_bits(math.sin(100000000000000000000.0))))\n  \
            println(int.to_string(float.to_bits(math.cos(100000000000000000000.0))))\n  \
            println(int.to_string(float.to_bits(math.tan(100000000000000000000.0)))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("math_trig_independent_golden", &render_wasm_program(&prog)) {
            assert_eq!(
                out,
                // π/4                100.0                1e6                  π/2                  1e20
                "4604544271217802188\n4604544271217802189\n4607182418800017407\n\
                 -4620635881084269128\n4605942297449095135\n-4619907664570524360\n\
                 -4623395494513026969\n4606612732610269996\n-4622969797129849664\n\
                 4607182418800017408\n4364452196894661639\n4849535219099880885\n\
                 -4619384910413732784\n4605056453202808125\n-4617589314634034284"
            );
        }
    }

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

