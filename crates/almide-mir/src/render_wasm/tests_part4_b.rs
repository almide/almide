    #[test]
    fn self_hosted_string_strip_prefix_suffix() {
        // SELF-HOSTED string.strip_prefix / strip_suffix -> Option[String] (the remainder after
        // stripping a matching prefix/suffix, else None) over the Option[String] construction
        // machinery. CONTENT verified via prim reads of the materialized Option: strip_prefix
        // ("hello","he")=Some("llo"= 3 bytes lead 'l'=108), ("hello","xy")=None; strip_suffix
        // ("hello","lo")=Some("hel"= 3 bytes lead 'h'=104), ("hello","xy")=None. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let o1 = string.strip_prefix(\"hello\", \"he\")\n  let h1 = prim.handle(o1)\n  \
            println(int.to_string(prim.load32(h1 + 4)))\n  \
            let e1 = prim.load64(h1 + 12)\n  println(int.to_string(prim.load32(e1 + 4)))\n  println(int.to_string(prim.load8(e1 + 12)))\n  \
            let o2 = string.strip_prefix(\"hello\", \"xy\")\n  println(int.to_string(prim.load32(prim.handle(o2) + 4)))\n  \
            let o3 = string.strip_suffix(\"hello\", \"lo\")\n  let e3 = prim.load64(prim.handle(o3) + 12)\n  \
            println(int.to_string(prim.load8(e3 + 12)))\n  println(int.to_string(prim.load32(e3 + 4)))\n  \
            let o4 = string.strip_suffix(\"hello\", \"xy\")\n  println(int.to_string(prim.load32(prim.handle(o4) + 4))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.strip_prefix"));
        assert!(prog.functions.iter().any(|f| f.name == "string.strip_suffix"));
        if let Some(out) = build_and_run("self_hosted_string_strip_prefix_suffix", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n3\n108\n0\n104\n3\n0");
        }
    }

    #[test]
    fn string_char_at_option_loop_is_bounded() {
        // ADVERSARIAL leak guard: a loop materializing an Option[String] (char_at owning a slice)
        // and tag-matching it each iteration runs in BOUNDED memory (DropListStr frees each).
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 4000 {\n    \
            let o = string.get(\"abc\", 1)\n    match o { Some(_) => println(\"y\"), None => println(\"n\"), }\n    \
            i = i + 1\n  }\n  \
            println(\"done\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_char_at_option_loop", &render_wasm_program(&prog)) {
            assert!(out.ends_with("done"), "loop must terminate (bounded memory)");
        }
    }

    #[test]
    fn string_first_option_loop_is_bounded() {
        // ADVERSARIAL leak guard: a loop that MATERIALIZES an Option[String] (string.first owning a
        // slice String) and tag-matches it every iteration must run in BOUNDED memory — each
        // iteration's Option + its owned element are freed by DropListStr before the back-edge,
        // no OOM / double-free.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 4000 {\n    \
            let o = string.first(\"xyz\")\n    match o { Some(c) => println(c), None => println(\"n\"), }\n    \
            i = i + 1\n  }\n  \
            println(\"done\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_first_option_loop", &render_wasm_program(&prog)) {
            assert!(out.ends_with("done"), "loop must terminate (bounded memory), got tail {:?}", &out[out.len().saturating_sub(20)..]);
        }
    }

    #[test]
    fn self_hosted_string_is_whitespace() {
        // SELF-HOSTED string.is_whitespace — every codepoint is in the Unicode White_Space set
        // (decoded per-codepoint via UTF-8, checked against the exact set incl. U+3000 ideographic
        // space). "   "=true, "a b"=false, ""=true (all() over empty), "　"(U+3000)=true,
        // "　x"=false. Byte-matches v0's s.chars().all(|c| c.is_whitespace()). Bool printed 1/0.
        let src = "fn main() -> Unit = {\n  \
            let a = string.is_whitespace(\"   \")\n  let na = if a then 1 else 0\n  println(int.to_string(na))\n  \
            let b = string.is_whitespace(\"a b\")\n  let nb = if b then 1 else 0\n  println(int.to_string(nb))\n  \
            let c = string.is_whitespace(\"\")\n  let nc = if c then 1 else 0\n  println(int.to_string(nc))\n  \
            let d = string.is_whitespace(\"　\")\n  let nd = if d then 1 else 0\n  println(int.to_string(nd))\n  \
            let e = string.is_whitespace(\"　x\")\n  let ne = if e then 1 else 0\n  println(int.to_string(ne)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.is_whitespace"));
        if let Some(out) = build_and_run("self_hosted_string_is_whitespace", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n0\n1\n1\n0");
        }
    }

    #[test]
    fn self_hosted_string_length() {
        // SELF-HOSTED string.length — the explicit-name alias of string.len (both = the UTF-8
        // codepoint count). length("hello")=5, length("日本語")=3, length("")=0. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(string.length(\"hello\")))\n  \
            println(int.to_string(string.length(\"日本語\")))\n  \
            println(int.to_string(string.length(\"\"))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.length"));
        if let Some(out) = build_and_run("self_hosted_string_length", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\n3\n0");
        }
    }

    #[test]
    fn self_hosted_string_from_bytes() {
        // SELF-HOSTED string.from_bytes — copy a List[Int]'s low bytes into a fresh String. For
        // VALID UTF-8 it byte-matches v0's from_utf8_lossy: [72,105]="Hi", "hello" bytes, and the
        // 4-byte sequence [240,159,152,128] = the U+1F600 emoji. (Invalid UTF-8 lossy = a refinement.)
        let src = "fn main() -> Unit = {\n  \
            let a = [72, 105]\n  println(string.from_bytes(a))\n  \
            let b = [104, 101, 108, 108, 111]\n  println(string.from_bytes(b))\n  \
            let c = [240, 159, 152, 128]\n  println(string.from_bytes(c)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.from_bytes"));
        if let Some(out) = build_and_run("self_hosted_string_from_bytes", &render_wasm_program(&prog)) {
            assert_eq!(out, "Hi\nhello\n😀");
        }
    }

    #[test]
    fn self_hosted_math_pi_e() {
        // SELF-HOSTED math.pi / math.e — the f64 constants (declared `= _` in stdlib/math.almd).
        // Verified via float.to_int(const * 1000): pi -> 3141, e -> 2718. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let cpi = math.pi()\n  let cpi1000 = prim.fmul(cpi, 1000.0)\n  println(int.to_string(float.to_int(cpi1000)))\n  \
            let ce = math.e()\n  let ce1000 = prim.fmul(ce, 1000.0)\n  println(int.to_string(float.to_int(ce1000))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "math.pi"));
        assert!(prog.functions.iter().any(|f| f.name == "math.e"));
        if let Some(out) = build_and_run("self_hosted_math_pi_e", &render_wasm_program(&prog)) {
            assert_eq!(out, "3141\n2718");
        }
    }

    #[test]
    fn self_hosted_hex_encode() {
        // SELF-HOSTED hex.encode / hex.encode_upper (Bytes -> hex String) over the bytes machinery
        // + the bitwise prim floor. Each byte -> two hex digits (high nibble byte>>4, low byte&0xF).
        // encode([0,15,255,16])="000fff10"; encode_upper([171,205])="ABCD". Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = [0, 15, 255, 16]\n  let b = bytes.from_list(xs)\n  \
            println(hex.encode(b))\n  \
            let ys = [171, 205]\n  let b2 = bytes.from_list(ys)\n  \
            println(hex.encode_upper(b2))\n  \
            println(hex.encode(b2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "hex.encode"));
        assert!(prog.functions.iter().any(|f| f.name == "hex.encode_upper"));
        if let Some(out) = build_and_run("self_hosted_hex_encode", &render_wasm_program(&prog)) {
            assert_eq!(out, "000fff10\nABCD\nabcd");
        }
    }

    #[test]
    fn self_hosted_base64_encode() {
        // SELF-HOSTED base64.encode / base64.encode_url (RFC 4648, padded) over the bytes machinery
        // + bitwise prim floor. encode("Man")="TWFu"; "Ma"="TWE="; "M"="TQ=="; the 2-byte tail
        // [251,255] exercises the alphabet-specific 62/63: std "+/8=", url "-_8=". Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m = bytes.from_string(\"Man\")\n  println(base64.encode(m))\n  \
            let m2 = bytes.from_string(\"Ma\")\n  println(base64.encode(m2))\n  \
            let m1 = bytes.from_string(\"M\")\n  println(base64.encode(m1))\n  \
            let ff = [251, 255]\n  let bf = bytes.from_list(ff)\n  \
            println(base64.encode_url(bf))\n  \
            println(base64.encode(bf)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "base64.encode"));
        assert!(prog.functions.iter().any(|f| f.name == "base64.encode_url"));
        if let Some(out) = build_and_run("self_hosted_base64_encode", &render_wasm_program(&prog)) {
            assert_eq!(out, "TWFu\nTWE=\nTQ==\n-_8=\n+/8=");
        }
    }

    #[test]
    fn self_hosted_math_sqrt() {
        // SELF-HOSTED math.sqrt = prim.fsqrt (f64.sqrt, byte-exact with v0). sqrt(16)=4,
        // sqrt(2)=1.41…→to_int 1, sqrt(81)=9.
        let src = "fn main() -> Unit = {\n  \
            let a = float.to_int(math.sqrt(16.0))\n  println(int.to_string(a))\n  \
            let b = float.to_int(math.sqrt(2.0))\n  println(int.to_string(b))\n  \
            let c = float.to_int(math.sqrt(81.0))\n  println(int.to_string(c)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "math.sqrt"));
        if let Some(out) = build_and_run("self_hosted_math_sqrt", &render_wasm_program(&prog)) {
            assert_eq!(out, "4\n1\n9");
        }
    }

    #[test]
    fn self_hosted_float_round() {
        // SELF-HOSTED float.round — round half AWAY from zero (v0's f64::round, NOT half-even).
        // round(2.5)=3, round(2.4)=2, round(3.5)=4 (half-even would give 2 and 4 — the 2.5 case
        // distinguishes). to_int printed.
        let src = "fn main() -> Unit = {\n  \
            let a = float.to_int(float.round(2.5))\n  println(int.to_string(a))\n  \
            let b = float.to_int(float.round(2.4))\n  println(int.to_string(b))\n  \
            let c = float.to_int(float.round(3.5))\n  println(int.to_string(c))\n  \
            let d = float.to_int(float.round(2.6))\n  println(int.to_string(d)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.round"));
        if let Some(out) = build_and_run("self_hosted_float_round", &render_wasm_program(&prog)) {
            // round: 2.5->3, 2.4->2, 3.5->4, 2.6->3
            assert_eq!(out, "3\n2\n4\n3");
        }
    }

    #[test]
    fn self_hosted_list_flat_map() {
        // SELF-HOSTED `list.flat_map` — closure returns a LIST (heap-returning closure). f =
        // (x) => list.range(x, x+2) = [x, x+1]; flat_map([1,2,3], f) = [1,2]++[2,3]++[3,4] =
        // [1,2,2,3,3,4]. len 6, sum 15, ys[0]=1, ys[5]=4. Two-pass; each owned sublist dropped.
        let src = "fn main() -> Unit = {\n  \
            let ys = list.flat_map([1, 2, 3], (x) => list.range(x, x + 2))\n  \
            println(int.to_string(list.len(ys)))\n  \
            println(int.to_string(list.sum(ys)))\n  \
            println(int.to_string(list.get_or(ys, 0, 0)))\n  \
            println(int.to_string(list.get_or(ys, 5, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.flat_map"));
        if let Some(out) = build_and_run("self_hosted_list_flat_map", &render_wasm_program(&prog)) {
            // [1,2,2,3,3,4]: len6 sum15 ys[0]=1 ys[5]=4
            assert_eq!(out, "6\n15\n1\n4");
        }
    }

    #[test]
    fn flat_map_sublists_do_not_leak() {
        // BOUNDED-LOOP LEAK GUARD for the heap-returning (List) closure. flat_map over 1000
        // elems allocates 1000 owned sublists; each is dropped on its __step's return + reused
        // (fixed 1-elem sublists are same-size, so the head-only free-list reclaims them). If a
        // sublist leaked the 1-page memory would trap. f = (x) => [x] → result len n.
        let src = "fn main() -> Unit = {\n  \
            let xs = list.range(0, 1000)\n  \
            let ys = list.flat_map(xs, (x) => list.range(x, x + 1))\n  \
            println(int.to_string(list.len(ys))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("flat_map_no_leak", &render_wasm_program(&prog)) {
            // range(x, x+1) = [x] (1 elem) per element → result len = 1000
            assert_eq!(out, "1000");
        }
    }

    #[test]
    fn self_hosted_list_take_end_drop_end() {
        // list.take_end/drop_end self-hosted: last n / all-but-last n, List[Int] slot-copy.
        // take_end([1,2,3,4,5],2)=[4,5] ([0]=4,len 2); drop_end([1,2,3,4,5],2)=[1,2,3]
        // ([2]=3,len 3); take_end(xs,9) (n>=len) = whole (len 5); drop_end(xs,9) = [] (len 0).
        let src = "fn main() -> Unit = {\n  \
            let a = list.take_end([1, 2, 3, 4, 5], 2)\n  let a0 = list.get_or(a, 0, 0)\n  let la = list.len(a)\n  let sa0 = int.to_string(a0)\n  println(sa0)\n  let sla = int.to_string(la)\n  println(sla)\n  \
            let b = list.drop_end([1, 2, 3, 4, 5], 2)\n  let b2 = list.get_or(b, 2, 0)\n  let lb = list.len(b)\n  let sb2 = int.to_string(b2)\n  println(sb2)\n  let slb = int.to_string(lb)\n  println(slb)\n  \
            let c = list.take_end([1, 2, 3, 4, 5], 9)\n  let lc = list.len(c)\n  let slc = int.to_string(lc)\n  println(slc)\n  \
            let d = list.drop_end([1, 2, 3, 4, 5], 9)\n  let ld = list.len(d)\n  let sld = int.to_string(ld)\n  println(sld) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.take_end"));
        assert!(prog.functions.iter().any(|f| f.name == "list.drop_end"));
        if let Some(out) = build_and_run("list_take_drop_end", &render_wasm_program(&prog)) {
            assert_eq!(out, "4\n2\n3\n3\n5\n0");
        }
    }

    #[test]
    fn self_hosted_list_update() {
        // SELF-HOSTED list.update(xs, i, f): a same-length copy with slot i replaced by f(xs[i])
        // (the closure invoked via CallIndirect, here CONDITIONALLY). In-bounds: update([10,20,30],
        // 1, *100)=[10,2000,30]. Out-of-range (i>=len OR i<0): the copy is returned unchanged, f
        // never called (matching v0's get_mut no-op). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let a = list.update([10, 20, 30], 1, (x) => x * 100)\n  \
            println(int.to_string(list.get_or(a, 0, 0)))\n  \
            println(int.to_string(list.get_or(a, 1, 0)))\n  \
            println(int.to_string(list.get_or(a, 2, 0)))\n  \
            println(int.to_string(list.len(a)))\n  \
            let b = list.update([1, 2, 3], 5, (x) => x + 1)\n  \
            println(int.to_string(list.sum(b)))\n  \
            let c = list.update([1, 2, 3], 0 - 1, (x) => x + 1)\n  \
            println(int.to_string(list.sum(c))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.update"));
        if let Some(out) = build_and_run("self_hosted_list_update", &render_wasm_program(&prog)) {
            // [10,2000,30]: 10, 2000, 30, len 3 ; out-of-range sum 6 ; negative sum 6
            assert_eq!(out, "10\n2000\n30\n3\n6\n6");
        }
    }

    #[test]
    fn self_hosted_result_is_ok_is_err() {
        // THE Result machinery floor: `Ok(int)` / `Err(string)` MATERIALIZE (the parse-family shape
        // `if ok then Ok(v) else Err("msg")`) as a DynListStr with len-AS-TAG (Ok=len0 with the int
        // in slot 0, Err=len1 owning the message) — reusing the Option[String] cert (no new Init,
        // no checker change). SELF-HOSTED result.is_ok / result.is_err read the tag. mk(5)=Ok →
        // is_ok 1 / is_err 0 ; mk(-1)=Err → is_ok 0 / is_err 1. Byte-matches v0.
        let src = "fn mk(n: Int) -> Result[Int, String] = if n >= 0 then Ok(n) else Err(\"neg\")\n\
                   fn main() -> Unit = {\n  \
            let r1 = mk(5)\n  let a = result.is_ok(r1)\n  let za = if a then 1 else 0\n  println(int.to_string(za))\n  \
            let r2 = mk(0 - 1)\n  let b = result.is_ok(r2)\n  let zb = if b then 1 else 0\n  println(int.to_string(zb))\n  \
            let r3 = mk(0 - 2)\n  let c = result.is_err(r3)\n  let zc = if c then 1 else 0\n  println(int.to_string(zc))\n  \
            let r4 = mk(7)\n  let d = result.is_err(r4)\n  let zd = if d then 1 else 0\n  println(int.to_string(zd)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.is_ok"));
        assert!(prog.functions.iter().any(|f| f.name == "result.is_err"));
        if let Some(out) = build_and_run("self_hosted_result_is_ok_is_err", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n0\n1\n0");
        }
    }

    #[test]
    fn self_hosted_result_unwrap_or() {
        // SELF-HOSTED result.unwrap_or: extract the Ok value (slot 0, len-tag 0) or the default for
        // Err. unwrap_or(Ok(5), -1)=5 ; unwrap_or(Ok(0), 99)=0 (the Ok value, NOT the default) ;
        // unwrap_or(Err, -1)=-1. Byte-matches v0.
        let src = "fn mk(n: Int) -> Result[Int, String] = if n >= 0 then Ok(n) else Err(\"neg\")\n\
                   fn main() -> Unit = {\n  \
            let r1 = mk(5)\n  let v1 = result.unwrap_or(r1, 0 - 1)\n  println(int.to_string(v1))\n  \
            let r2 = mk(0)\n  let v2 = result.unwrap_or(r2, 99)\n  println(int.to_string(v2))\n  \
            let r3 = mk(0 - 1)\n  let v3 = result.unwrap_or(r3, 0 - 1)\n  println(int.to_string(v3)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.unwrap_or"));
        if let Some(out) = build_and_run("self_hosted_result_unwrap_or", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\n0\n-1");
        }
    }

    #[test]
    fn self_hosted_option_map() {
        // SELF-HOSTED option.map(o, f) = o.map(f): Some(x) → Some(f(x)) (the closure invoked via
        // CallIndirect, ONLY on the Some arm), None → None. The result is a fresh materialized
        // Option so a `match` over it EXECUTES. map(first[5,6,7], *10)=Some(50); map(get[5] oob,
        // *10)=None. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let o1 = list.first([5, 6, 7])\n  let m1 = option.map(o1, (x) => x * 10)\n  \
            match m1 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let o2 = list.get([5], 9)\n  let m2 = option.map(o2, (x) => x * 10)\n  \
            match m2 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let o3 = list.first([7, 8])\n  let m3 = option.map(o3, (x) => x + 100)\n  \
            match m3 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.map"));
        if let Some(out) = build_and_run("self_hosted_option_map", &render_wasm_program(&prog)) {
            // map(Some(5),*10)=50 ; map(None,*10)=none ; map(Some(7),+100)=107
            assert_eq!(out, "50\nnone\n107");
        }
    }

    #[test]
    fn self_hosted_bytes_read_array() {
        // SELF-HOSTED bytes.read_u16_le_array / read_u32_le_array / read_i32_le_array — read `count`
        // fixed-width LE values into a List[Int], reusing the scalar reads (0-padded out of range).
        // u16 [1,0,2,0,255,255]@0×3=[1,2,65535]; u32 [1,0,0,0,2,0,0,0]@0×2 sum 3; i32 [255×4]×1=-1
        // (printed negated = 1); OOB u16 [1,0]@0×3=[1,0,0] (len 3, sum 1). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let b = bytes.from_list([1, 0, 2, 0, 255, 255])\n  let a = bytes.read_u16_le_array(b, 0, 3)\n  \
            println(int.to_string(list.len(a)))\n  println(int.to_string(list.get_or(a, 0, 0)))\n  \
            println(int.to_string(list.get_or(a, 2, 0)))\n  \
            let b2 = bytes.from_list([1, 0, 0, 0, 2, 0, 0, 0])\n  let a2 = bytes.read_u32_le_array(b2, 0, 2)\n  \
            println(int.to_string(list.sum(a2)))\n  \
            let b3 = bytes.from_list([255, 255, 255, 255])\n  let a3 = bytes.read_i32_le_array(b3, 0, 1)\n  \
            let v = list.get_or(a3, 0, 0)\n  let nv = 0 - v\n  println(int.to_string(nv))\n  \
            let b4 = bytes.from_list([1, 0])\n  let a4 = bytes.read_u16_le_array(b4, 0, 3)\n  \
            println(int.to_string(list.len(a4)))\n  println(int.to_string(list.sum(a4))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.read_u16_le_array"));
        if let Some(out) = build_and_run("self_hosted_bytes_read_array", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n1\n65535\n3\n1\n3\n1");
        }
    }

    #[test]
    fn self_hosted_bytes_read_f64_array() {
        // SELF-HOSTED bytes.read_f64_le_array / read_f64_be_array → List[Float] (the List[Float]
        // machinery: prim.alloc_list generalized to List[A], f64 BITS stored in each i64 slot via
        // prim.fbits + store64; plain non-nested-ownership drop). Read the raw slot bits back via
        // prim.load64 and byte-match v0's f64 bit pattern: 1.0=0x3FF0000000000000=4607182418800017408,
        // 2.0=0x4000000000000000=4611686018427387904. LE [..,240,63]=1.0; BE [63,240,..]=1.0.
        let src = "fn main() -> Unit = {\n  \
            let b = bytes.from_list([0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 64])\n  \
            let a = bytes.read_f64_le_array(b, 0, 2)\n  \
            println(int.to_string(prim.load64(prim.handle(a) + 12)))\n  \
            println(int.to_string(prim.load64(prim.handle(a) + 12 + 8)))\n  \
            let b2 = bytes.from_list([63, 240, 0, 0, 0, 0, 0, 0])\n  \
            let a2 = bytes.read_f64_be_array(b2, 0, 1)\n  \
            println(int.to_string(prim.load64(prim.handle(a2) + 12))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.read_f64_le_array"));
        if let Some(out) = build_and_run("self_hosted_bytes_read_f64_array", &render_wasm_program(&prog)) {
            assert_eq!(out, "4607182418800017408\n4611686018427387904\n4607182418800017408");
        }
    }

    #[test]
    fn self_hosted_set_core() {
        // SELF-HOSTED Set[Int] core (the Set machinery: a v1 Set[Int] IS a v1 List block with
        // unique insertion-ordered Int elements; prim.alloc_set + dedup-fill + len-patch). v0:
        // set.from_list([3,1,2,2,3,1])={3,1,2} len 3; contains(2)=true contains(9)=false;
        // to_list=[3,1,2] (insertion order) sum 6, first elem 3. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let s = set.from_list([3, 1, 2, 2, 3, 1])\n  \
            println(int.to_string(set.len(s)))\n  \
            let c2 = set.contains(s, 2)\n  let v2 = if c2 then 1 else 0\n  println(int.to_string(v2))\n  \
            let c9 = set.contains(s, 9)\n  let v9 = if c9 then 1 else 0\n  println(int.to_string(v9))\n  \
            let e = set.is_empty(s)\n  let ve = if e then 1 else 0\n  println(int.to_string(ve))\n  \
            let xs = set.to_list(s)\n  \
            println(int.to_string(list.sum(xs)))\n  \
            println(int.to_string(list.get_or(xs, 0, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.from_list"));
        if let Some(out) = build_and_run("self_hosted_set_core", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n1\n0\n0\n6\n3");
        }
    }

    #[test]
    fn self_hosted_set_algebra() {
        // SELF-HOSTED set.union/intersection/difference/is_subset/is_disjoint over Set[Int],
        // reusing __set_has + the alloc_set/dedup/len-patch core. a={1,2,3,4} b={3,4,5,6}:
        // union={1,2,3,4,5,6} len 6 sum 21; intersection={3,4} sum 7; difference(a,b)={1,2} sum 3;
        // is_subset({3,4},a)=true; is_disjoint(a,b)=false. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let a = set.from_list([1, 2, 3, 4])\n  let b = set.from_list([3, 4, 5, 6])\n  \
            let u = set.union(a, b)\n  println(int.to_string(set.len(u)))\n  \
            let ul = set.to_list(u)\n  println(int.to_string(list.sum(ul)))\n  \
            let inter = set.intersection(a, b)\n  let il = set.to_list(inter)\n  \
            println(int.to_string(set.len(inter)))\n  println(int.to_string(list.sum(il)))\n  \
            let diff = set.difference(a, b)\n  let dl = set.to_list(diff)\n  \
            println(int.to_string(list.sum(dl)))\n  \
            let sub = set.is_subset(inter, a)\n  let vsub = if sub then 1 else 0\n  \
            println(int.to_string(vsub))\n  \
            let dj = set.is_disjoint(a, b)\n  let vdj = if dj then 1 else 0\n  \
            println(int.to_string(vdj)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.union"));
        if let Some(out) = build_and_run("self_hosted_set_algebra", &render_wasm_program(&prog)) {
            assert_eq!(out, "6\n21\n2\n7\n3\n1\n0");
        }
    }

    #[test]
    fn self_hosted_set_construction() {
        // SELF-HOSTED set.new/insert/remove/symmetric_difference over Set[Int]. new()→insert 5,7,5
        // (dup ignored)={5,7} len 2; remove(5)={7} len 1 sum 7; sym_diff({1,2,3},{3,4})={1,2,4}
        // len 3 sum 7. Byte-matches v0 (insert/remove clone-then-mutate, insertion order).
        let src = "fn main() -> Unit = {\n  \
            let e = set.new()\n  let s1 = set.insert(e, 5)\n  let s2 = set.insert(s1, 7)\n  \
            let s3 = set.insert(s2, 5)\n  println(int.to_string(set.len(s3)))\n  \
            let r = set.remove(s3, 5)\n  println(int.to_string(set.len(r)))\n  \
            let rl = set.to_list(r)\n  println(int.to_string(list.sum(rl)))\n  \
            let a = set.from_list([1, 2, 3])\n  let b = set.from_list([3, 4])\n  \
            let sd = set.symmetric_difference(a, b)\n  let sdl = set.to_list(sd)\n  \
            println(int.to_string(set.len(sd)))\n  println(int.to_string(list.sum(sdl))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.insert"));
        if let Some(out) = build_and_run("self_hosted_set_construction", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n1\n7\n3\n7");
        }
    }

    #[test]
    fn self_hosted_set_higher_order() {
        // SELF-HOSTED set.filter/all/any over Set[Int] (closures machinery — the predicate is a
        // lambda invoked via CallIndirect). s={1,2,3,4,5}: filter(x>2)={3,4,5} len 3 sum 12;
        // all(x>0)=true; any(x>4)=true; all(x>3)=false. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let s = set.from_list([1, 2, 3, 4, 5])\n  \
            let big = set.filter(s, (x) => x > 2)\n  println(int.to_string(set.len(big)))\n  \
            let bl = set.to_list(big)\n  println(int.to_string(list.sum(bl)))\n  \
            let allpos = set.all(s, (x) => x > 0)\n  let va = if allpos then 1 else 0\n  \
            println(int.to_string(va))\n  \
            let anybig = set.any(s, (x) => x > 4)\n  let vb = if anybig then 1 else 0\n  \
            println(int.to_string(vb))\n  \
            let allbig = set.all(s, (x) => x > 3)\n  let vc = if allbig then 1 else 0\n  \
            println(int.to_string(vc)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.filter"));
        if let Some(out) = build_and_run("self_hosted_set_higher_order", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n12\n1\n1\n0");
        }
    }

    #[test]
    fn self_hosted_set_map_fold() {
        // SELF-HOSTED set.map (result re-dedup — f may collapse distinct inputs) + set.fold
        // (2-arity closure, acc-first). s={1,2,3,4}: map(x => x % 2)={1,0} len 2 sum 1 (1,0,1,0
        // dedups to {1,0}); fold(0, (acc,x) => acc + x)=10. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let s = set.from_list([1, 2, 3, 4])\n  \
            let m = set.map(s, (x) => x % 2)\n  println(int.to_string(set.len(m)))\n  \
            let ml = set.to_list(m)\n  println(int.to_string(list.sum(ml)))\n  \
            let total = set.fold(s, 0, (acc, x) => acc + x)\n  println(int.to_string(total)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.map"));
        assert!(prog.functions.iter().any(|f| f.name == "set.fold"));
        if let Some(out) = build_and_run("self_hosted_set_map_fold", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n1\n10");
        }
    }

    #[test]
    fn self_hosted_map_core() {
        // SELF-HOSTED Map[Int,Int] core (the Map machinery: a v1 Map IS a v1 List of PAIRED
        // 16-byte slots [k,v,...]; prim.alloc_map + insertion-order set/remove + len-patch). v0:
        // new→set(1,10)→set(2,20)→set(1,99) [update in place] = {1:99, 2:20} len 2; get_or(2,0)=20,
        // get_or(9,-1)=-1; contains(1)=true; remove(1)={2:20} len 1; keys sum 1+2=3 before remove;
        // values sum 99+20=119. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m0 = map.new()\n  let m1 = map.set(m0, 1, 10)\n  let m2 = map.set(m1, 2, 20)\n  \
            let m = map.set(m2, 1, 99)\n  \
            println(int.to_string(map.len(m)))\n  \
            println(int.to_string(map.get_or(m, 2, 0)))\n  \
            println(int.to_string(map.get_or(m, 9, 0 - 1)))\n  \
            let c1 = map.contains(m, 1)\n  let vc1 = if c1 then 1 else 0\n  println(int.to_string(vc1))\n  \
            let ks = map.keys(m)\n  println(int.to_string(list.sum(ks)))\n  \
            let vs = map.values(m)\n  println(int.to_string(list.sum(vs)))\n  \
            let r = map.remove(m, 1)\n  println(int.to_string(map.len(r)))\n  \
            println(int.to_string(map.get_or(r, 2, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.set"));
        if let Some(out) = build_and_run("self_hosted_map_core", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n20\n-1\n1\n3\n119\n1\n20");
        }
    }

    #[test]
    fn self_hosted_map_get_merge_update() {
        // SELF-HOSTED map.get (materialized Option[Int], match-extractable) + map.merge (b
        // overrides a's values, appends new keys) + map.update (closure on present key). m={1:10,
        // 2:20}: get(2)=Some(20) match→20, get(9)=None match→-1; merge with {2:99,3:30}={1:10,2:99,
        // 3:30} get(2)=99 get(3)=30; update(1, x=>x*5)={1:50,...} get(1)=50. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m1 = map.set(map.new(), 1, 10)\n  let m = map.set(m1, 2, 20)\n  \
            let g2 = map.get(m, 2)\n  \
            match g2 { Some(v) => println(int.to_string(v)), None => println(\"none\"), }\n  \
            let g9 = map.get(m, 9)\n  \
            match g9 { Some(v) => println(int.to_string(v)), None => println(\"none\"), }\n  \
            let b1 = map.set(map.new(), 2, 99)\n  let b = map.set(b1, 3, 30)\n  \
            let mg = map.merge(m, b)\n  \
            println(int.to_string(map.len(mg)))\n  \
            println(int.to_string(map.get_or(mg, 2, 0)))\n  \
            println(int.to_string(map.get_or(mg, 3, 0)))\n  \
            let mu = map.update(m, 1, (x) => x * 5)\n  \
            println(int.to_string(map.get_or(mu, 1, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.get"));
        assert!(prog.functions.iter().any(|f| f.name == "map.merge"));
        if let Some(out) = build_and_run("self_hosted_map_get_merge_update", &render_wasm_program(&prog)) {
            assert_eq!(out, "20\nnone\n3\n99\n30\n50");
        }
    }

    #[test]
    fn self_hosted_map_higher_order() {
        // SELF-HOSTED map.filter/all/any/count over Map[Int,Int] (closures machinery — a 2-arity
        // predicate f(k,v) via CallIndirect). m={1:10,2:20,3:30}: filter(v>15)={2:20,3:30} len 2
        // values sum 50; all(v>0)=true; any(k==2)=true; count(v>=20)=2. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m1 = map.set(map.new(), 1, 10)\n  let m2 = map.set(m1, 2, 20)\n  \
            let m = map.set(m2, 3, 30)\n  \
            let fm = map.filter(m, (k, v) => v > 15)\n  println(int.to_string(map.len(fm)))\n  \
            let fv = map.values(fm)\n  println(int.to_string(list.sum(fv)))\n  \
            let ap = map.all(m, (k, v) => v > 0)\n  let va = if ap then 1 else 0\n  println(int.to_string(va))\n  \
            let an = map.any(m, (k, v) => k == 2)\n  let vn = if an then 1 else 0\n  println(int.to_string(vn))\n  \
            let cn = map.count(m, (k, v) => v >= 20)\n  println(int.to_string(cn)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.filter"));
        if let Some(out) = build_and_run("self_hosted_map_higher_order", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n50\n1\n1\n2");
        }
    }

    #[test]
    fn self_hosted_map_fold_three_arity_closure() {
        // SELF-HOSTED map.fold — the FIRST 3-arity closure f(acc,k,v) via $closure_fn3 (the render
        // auto-generates a closure type per arity, so 3-arity needs no new machinery). m={1:10,
        // 2:20,3:30}: fold(0, (a,k,v) => a + k + v) = (1+10)+(2+20)+(3+30) = 66. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m1 = map.set(map.new(), 1, 10)\n  let m2 = map.set(m1, 2, 20)\n  \
            let m = map.set(m2, 3, 30)\n  \
            let total = map.fold(m, 0, (a, k, v) => a + k + v)\n  \
            println(int.to_string(total)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.fold"));
        if let Some(out) = build_and_run("self_hosted_map_fold_three_arity_closure", &render_wasm_program(&prog)) {
            assert_eq!(out, "66");
        }
    }

    #[test]
    fn self_hosted_value_scalar_constructors() {
        // SELF-HOSTED scalar Value constructors (the dynamic data model: a v1 block whose `len`
        // header field carries the variant TAG, payload at +12). value.int(42)→tag 2 payload 42;
        // value.null()→tag 0; value.bool(true)→tag 1 payload 1; value.float(1.0)→tag 3 payload
        // 0x3FF...=4607182418800017408 (f64 bits). Verified by reading the block via prim.
        let src = "fn main() -> Unit = {\n  \
            let vi = value.int(42)\n  let hi = prim.handle(vi)\n  \
            println(int.to_string(prim.load32(hi + 4)))\n  println(int.to_string(prim.load64(hi + 12)))\n  \
            let vn = value.null()\n  println(int.to_string(prim.load32(prim.handle(vn) + 4)))\n  \
            let vb = value.bool(true)\n  let hb = prim.handle(vb)\n  \
            println(int.to_string(prim.load32(hb + 4)))\n  println(int.to_string(prim.load64(hb + 12)))\n  \
            let vf = value.float(1.0)\n  let hf = prim.handle(vf)\n  \
            println(int.to_string(prim.load32(hf + 4)))\n  println(int.to_string(prim.load64(hf + 12))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "value.int"));
        if let Some(out) = build_and_run("self_hosted_value_scalar_constructors", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n42\n0\n1\n1\n3\n4607182418800017408");
        }
    }

    #[test]
    fn self_hosted_value_scalar_extractors() {
        // SELF-HOSTED value.as_int/as_bool/as_float → Result[T, String] (read the tag, Ok(payload)
        // on a match else Err("expected T"); materialized Result, match-executable). as_int(int 42)=
        // Ok(42); as_int(null)=Err("expected Int"); as_bool(bool true)=Ok(true)→"yes"; as_float(int 7)
        // widens to Ok(7.0)="float-ok"; as_float(null)=Err("expected Float"). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let oi = value.as_int(value.int(42))\n  \
            match oi { Ok(n) => println(int.to_string(n)), Err(e) => println(e), }\n  \
            let oi2 = value.as_int(value.null())\n  \
            match oi2 { Ok(n) => println(int.to_string(n)), Err(e) => println(e), }\n  \
            let ob = value.as_bool(value.bool(true))\n  \
            match ob { Ok(b) => if b then println(\"yes\") else println(\"no\"), Err(e) => println(e), }\n  \
            let of = value.as_float(value.int(7))\n  \
            match of { Ok(f) => println(\"float-ok\"), Err(e) => println(e), }\n  \
            let of2 = value.as_float(value.null())\n  \
            match of2 { Ok(f) => println(\"float-ok\"), Err(e) => println(e), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "value.as_int"));
        if let Some(out) = build_and_run("self_hosted_value_scalar_extractors", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\nexpected Int\nyes\nfloat-ok\nexpected Float");
        }
    }

    #[test]
    fn self_hosted_set_string() {
        // SELF-HOSTED Set[String] (heap-element / nested-ownership set, DynListStr; dedup via byte
        // string equality __str_eq; deep-copied elements). The repr-poly dispatch routes set.from_list
        // /contains/to_list over a Set[String] to the _str variant. from_list(["apple","banana",
        // "apple","cherry","banana"])={apple,banana,cherry} len 3; contains("banana")=true,
        // ("grape")=false; to_list first="apple". Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"apple,banana,apple,cherry,banana\", \",\")\n  \
            let s = set.from_list(xs)\n  println(int.to_string(set.len(s)))\n  \
            let ca = set.contains(s, \"banana\")\n  if ca then println(\"has-banana\") else println(\"no\")\n  \
            let cb = set.contains(s, \"grape\")\n  if cb then println(\"has-grape\") else println(\"no\")\n  \
            let ys = set.to_list(s)\n  println(int.to_string(list.len(ys)))\n  \
            let first = list.get(ys, 0)\n  match first { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.from_list_str"));
        assert!(prog.functions.iter().any(|f| f.name == "set.contains_str"));
        if let Some(out) = build_and_run("self_hosted_set_string", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\nhas-banana\nno\n3\napple");
        }
    }

    #[test]
    fn self_hosted_set_string_algebra() {
        // SELF-HOSTED Set[String] union/intersection/difference/is_subset/is_disjoint (heap-element
        // algebra, deep-copied results, __str_eq membership; repr-poly dispatch to the _str variant).
        // a={a,b,c,d} b={c,d,e,f}: union len 6; intersection={c,d} len 2 first "c"; difference(a,b)=
        // {a,b} len 2; is_subset({c,d},a)=true; is_disjoint(a,b)=false. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let a = set.from_list(string.split(\"a,b,c,d\", \",\"))\n  \
            let b = set.from_list(string.split(\"c,d,e,f\", \",\"))\n  \
            let u = set.union(a, b)\n  println(int.to_string(set.len(u)))\n  \
            let inter = set.intersection(a, b)\n  println(int.to_string(set.len(inter)))\n  \
            let il = set.to_list(inter)\n  let f = list.get(il, 0)\n  \
            match f { Some(v) => println(v), None => println(\"none\"), }\n  \
            let diff = set.difference(a, b)\n  println(int.to_string(set.len(diff)))\n  \
            let sub = set.is_subset(inter, a)\n  if sub then println(\"subset\") else println(\"no\")\n  \
            let dj = set.is_disjoint(a, b)\n  if dj then println(\"disjoint\") else println(\"no\") }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.union_str"));
        assert!(prog.functions.iter().any(|f| f.name == "set.is_subset_str"));
        if let Some(out) = build_and_run("self_hosted_set_string_algebra", &render_wasm_program(&prog)) {
            assert_eq!(out, "6\n2\nc\n2\nsubset\nno");
        }
    }

    #[test]
    fn self_hosted_set_string_construction() {
        // SELF-HOSTED Set[String] new/insert/remove/symmetric_difference (heap-element, deep copies).
        // new()→insert "a","b","a"(dup)={a,b} len 2; remove "a"={b} len 1 first "b"; sym_diff(
        // {a,b,c},{b,c,d})={a,d} len 2. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let e = set.new()\n  let s1 = set.insert(e, \"a\")\n  let s2 = set.insert(s1, \"b\")\n  \
            let s = set.insert(s2, \"a\")\n  println(int.to_string(set.len(s)))\n  \
            let r = set.remove(s, \"a\")\n  println(int.to_string(set.len(r)))\n  \
            let rl = set.to_list(r)\n  let f = list.get(rl, 0)\n  \
            match f { Some(v) => println(v), None => println(\"none\"), }\n  \
            let a = set.from_list(string.split(\"a,b,c\", \",\"))\n  \
            let b = set.from_list(string.split(\"b,c,d\", \",\"))\n  \
            let sd = set.symmetric_difference(a, b)\n  println(int.to_string(set.len(sd))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.insert_str"));
        if let Some(out) = build_and_run("self_hosted_set_string_construction", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n1\nb\n2");
        }
    }

    #[test]
    fn self_hosted_set_string_higher_order() {
        // SELF-HOSTED Set[String] filter/all/any/fold (closures over String elements, heap-arg ABI).
        // s={apple,banana,kiwi,fig}: filter(len>3)={apple,banana,kiwi} len 3; all(len>2)=true;
        // any(len<4)=true (fig=3); fold(0, +len)=5+6+4+3=18. Byte-matches v0. Closure bodies use a
        // let-bind (scalar-call-as-operand gap workaround).
        let src = "fn main() -> Unit = {\n  \
            let s = set.from_list(string.split(\"apple,banana,kiwi,fig\", \",\"))\n  \
            let big = set.filter(s, (x) => { let l = string.len(x)\n l > 3 })\n  \
            println(int.to_string(set.len(big)))\n  \
            let al = set.all(s, (x) => { let l = string.len(x)\n l > 2 })\n  \
            if al then println(\"all-long\") else println(\"no\")\n  \
            let an = set.any(s, (x) => { let l = string.len(x)\n l < 4 })\n  \
            if an then println(\"has-short\") else println(\"no\")\n  \
            let total = set.fold(s, 0, (acc, x) => { let l = string.len(x)\n acc + l })\n  \
            println(int.to_string(total)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "set.filter_str"));
        assert!(prog.functions.iter().any(|f| f.name == "set.fold_str"));
        if let Some(out) = build_and_run("self_hosted_set_string_higher_order", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\nall-long\nhas-short\n18");
        }
    }

    #[test]
    fn self_hosted_list_string_search() {
        // SELF-HOSTED list.contains / list.index_of over a List[String] (byte string equality
        // __str_eq; arg-keyed dispatch). Also exercises set.contains_str in the SAME program so the
        // duplicate __str_eq (in list_str.almd + set_str.almd) must DEDUP by name (no "duplicate
        // func"). xs=[a,b,c,d]: contains("c")=true, contains("z")=false; index_of("c")=Some(2),
        // index_of("z")=None; set.contains over the same elements = true. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"a,b,c,d\", \",\")\n  \
            let c1 = list.contains(xs, \"c\")\n  if c1 then println(\"has-c\") else println(\"no\")\n  \
            let c2 = list.contains(xs, \"z\")\n  if c2 then println(\"has-z\") else println(\"no\")\n  \
            let i1 = list.index_of(xs, \"c\")\n  \
            match i1 { Some(v) => println(int.to_string(v)), None => println(\"none\"), }\n  \
            let i2 = list.index_of(xs, \"z\")\n  \
            match i2 { Some(v) => println(int.to_string(v)), None => println(\"none\"), }\n  \
            let s = set.from_list(xs)\n  let sc = set.contains(s, \"b\")\n  \
            if sc then println(\"set-has-b\") else println(\"no\") }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.contains_str"));
        assert!(prog.functions.iter().any(|f| f.name == "list.index_of_str"));
        if let Some(out) = build_and_run("self_hosted_list_string_search", &render_wasm_program(&prog)) {
            assert_eq!(out, "has-c\nno\n2\nnone\nset-has-b");
        }
    }

    #[test]
    fn self_hosted_list_string_higher_order() {
        // SELF-HOSTED list.all/any/count over a List[String] (predicate over String elements,
        // heap-arg closure; arg-keyed dispatch). xs=[apple,banana,kiwi,fig]: all(len>2)=true;
        // any(len<4)=true (fig); count(len>4)=2 (banana=6,apple=5). Byte-matches v0. Closure body
        // uses a let-bind for the scalar call.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"apple,banana,kiwi,fig\", \",\")\n  \
            let al = list.all(xs, (x) => { let l = string.len(x)\n l > 2 })\n  \
            if al then println(\"all-long\") else println(\"no\")\n  \
            let an = list.any(xs, (x) => { let l = string.len(x)\n l < 4 })\n  \
            if an then println(\"has-short\") else println(\"no\")\n  \
            let cn = list.count(xs, (x) => { let l = string.len(x)\n l > 4 })\n  \
            println(int.to_string(cn)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.count_str"));
        if let Some(out) = build_and_run("self_hosted_list_string_higher_order", &render_wasm_program(&prog)) {
            assert_eq!(out, "all-long\nhas-short\n2");
        }
    }

    #[test]
    fn self_hosted_list_string_fold() {
        // SELF-HOSTED list.fold over a List[String] — f(acc, x) 2-arity closure, String element 2nd
        // arg (heap-widened). xs=[ab,cde,f]: fold(0, acc + len(x)) = 2+3+1 = 6. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"ab,cde,f\", \",\")\n  \
            let total = list.fold(xs, 0, (acc, x) => { let l = string.len(x)\n acc + l })\n  \
            println(int.to_string(total)) }\n";
        let prog = lower_source(src);
        // The BLOCK-bodied lambda now DEFUNCTIONALIZES (see self_hosted_list_filter_str).
        if let Some(out) = build_and_run("self_hosted_list_string_fold", &render_wasm_program(&prog)) {
            assert_eq!(out, "6");
        }
    }

    #[test]
    fn self_hosted_list_string_unique() {
        // SELF-HOSTED list.unique over a List[String] — keep first occurrence, drop all later dups
        // (membership vs the result via __str_eq; kept elements deep-copied). xs=[a,b,a,c,b,a]=
        // [a,b,c] len 3, first "a", sum-of-lens 3. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"a,b,a,c,b,a\", \",\")\n  \
            let u = list.unique(xs)\n  println(int.to_string(list.len(u)))\n  \
            let f = list.get(u, 0)\n  match f { Some(v) => println(v), None => println(\"none\"), }\n  \
            let last = list.get(u, 2)\n  match last { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.unique_str"));
        if let Some(out) = build_and_run("self_hosted_list_string_unique", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\na\nc");
        }
    }

    #[test]
    fn self_hosted_list_string_unique_loop_reclaims() {
        // SOUNDNESS for list.unique_str: a bounded loop building + dropping a fresh deduped
        // List[String] each iteration must reclaim every kept element + the block — no leak/double-
        // free. 3000 iters, prints the last result's len (2).
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 3000 {\n    \
              let u = list.unique(string.split(\"xx,yy,xx,yy\", \",\"))\n    \
              last = list.len(u)\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_list_string_unique_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "2");
        }
    }

    #[test]
    fn self_hosted_list_string_dedup() {
        // SELF-HOSTED list.dedup over a List[String] — drop CONSECUTIVE duplicates (cur vs source
        // i-1 via __str_eq; kept elements deep-copied). xs=[a,a,b,b,b,a,c]=[a,b,a,c] len 4, first
        // "a", elem 2 "a", last "c". Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"a,a,b,b,b,a,c\", \",\")\n  \
            let d = list.dedup(xs)\n  println(int.to_string(list.len(d)))\n  \
            let e0 = list.get(d, 0)\n  match e0 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let e2 = list.get(d, 2)\n  match e2 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let e3 = list.get(d, 3)\n  match e3 { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.dedup_str"));
        if let Some(out) = build_and_run("self_hosted_list_string_dedup", &render_wasm_program(&prog)) {
            assert_eq!(out, "4\na\na\nc");
        }
    }

    #[test]
    fn scalar_tuple_var_field_construct() {
        // TUPLE machinery: a `(a, b)` of NON-LITERAL scalar fields (vars / computed exprs)
        // constructs dynamically (alloc + Prim::Store each field), then destructures precisely.
        // a=3,b=7: (a,b)→(3,7); (a+1, b*2)→(4,14). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let a = 3\n  let b = 7\n  \
            let t = (a, b)\n  let (x, y) = t\n  \
            println(int.to_string(x))\n  println(int.to_string(y))\n  \
            let u = (a + 1, b * 2)\n  let (p, q) = u\n  \
            println(int.to_string(p))\n  println(int.to_string(q)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("scalar_tuple_var_field_construct", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n7\n4\n14");
        }
    }

    #[test]
    fn scalar_tuple_construct_and_destructure() {
        // TUPLE machinery (scalar-field slice): a `(3, 7)` literal materializes a 2-slot block
        // (Init::IntList, like a List[Int] literal); a `let (a, b) = t` destructure LOADS each
        // field at its slot (precise extraction, not the container-grain alias). Computes a=3, b=7,
        // a+b via the bound scalars. Byte-matches v0. The foundation for list.zip/enumerate/etc.
        let src = "fn main() -> Unit = {\n  \
            let t = (3, 7)\n  let (a, b) = t\n  \
            println(int.to_string(a))\n  println(int.to_string(b))\n  \
            let s = a + b\n  println(int.to_string(s)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("scalar_tuple_construct_and_destructure", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n7\n10");
        }
    }

    #[test]
    fn self_hosted_list_string_intersperse() {
        // SELF-HOSTED list.intersperse over a List[String] — insert sep between elements (deep
        // copies). xs=[a,b,c] sep="-" → [a,-,b,-,c] len 5, idx0 "a", idx1 "-", idx4 "c". v0-match.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"a,b,c\", \",\")\n  \
            let r = list.intersperse(xs, \"-\")\n  println(int.to_string(list.len(r)))\n  \
            let e0 = list.get(r, 0)\n  match e0 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let e1 = list.get(r, 1)\n  match e1 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let e4 = list.get(r, 4)\n  match e4 { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.intersperse_str"));
        if let Some(out) = build_and_run("self_hosted_list_string_intersperse", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\na\n-\nc");
        }
    }

    #[test]
    fn self_hosted_list_string_find() {
        // SELF-HOSTED list.find over a List[String] → Option[String] (predicate higher-order; the
        // matched element returned as Some(deep copy), else None; materialized Option, match-exec).
        // xs=[apple,banana,kiwi,fig]: find(len==6)=Some("banana"); find(len==99)=None. v0-match.
        let src = "fn main() -> Unit = {\n  \
            let xs = string.split(\"apple,banana,kiwi,fig\", \",\")\n  \
            let f1 = list.find(xs, (x) => { let l = string.len(x)\n l == 6 })\n  \
            match f1 { Some(v) => println(v), None => println(\"none\"), }\n  \
            let f2 = list.find(xs, (x) => { let l = string.len(x)\n l == 99 })\n  \
            match f2 { Some(v) => println(v), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        // C1: the inline closure is defunctionalized — `list.find` is inlined as an
        // early-exit loop (try_lower_defunc_find), NOT auto-linked as a combinator.
        assert!(
            !prog.functions.iter().any(|f| f.name == "list.find_str"),
            "list.find is inlined, NOT auto-linked as a combinator"
        );
        if let Some(out) = build_and_run("self_hosted_list_string_find", &render_wasm_program(&prog)) {
            assert_eq!(out, "banana\nnone");
        }
    }

    #[test]
    fn self_hosted_map_string_string() {
        // SELF-HOSTED Map[String,String] (uniform-heap DynListStr, len=2*entries, DropListStr frees
        // all keys+values). Keys built via string.split (List[String] literal gap). new→set("a","x")
        // →set("b","y")→set("a","z")[update in place]={a:z, b:y} len 2; get("a")="z", get("b")="y",
        // get("zz")=None; contains("b")=true. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let m0 = map.new()\n  \
            let m1 = map.set(m0, \"a\", \"x\")\n  let m2 = map.set(m1, \"b\", \"y\")\n  \
            let m = map.set(m2, \"a\", \"z\")\n  \
            println(int.to_string(map.len(m)))\n  \
            let ga = map.get(m, \"a\")\n  match ga { Some(v) => println(v), None => println(\"none\"), }\n  \
            let gb = map.get(m, \"b\")\n  match gb { Some(v) => println(v), None => println(\"none\"), }\n  \
            let gz = map.get(m, \"zz\")\n  match gz { Some(v) => println(v), None => println(\"none\"), }\n  \
            let c = map.contains(m, \"b\")\n  let vc = if c then 1 else 0\n  println(int.to_string(vc)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "map.set_str"));
        assert!(prog.functions.iter().any(|f| f.name == "map.get_str"));
        if let Some(out) = build_and_run("self_hosted_map_string_string", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\nz\ny\nnone\n1");
        }
    }

