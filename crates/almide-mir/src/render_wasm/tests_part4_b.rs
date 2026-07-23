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

include!("tests_part4_g.rs");
