
    #[test]
    fn self_hosted_string_lines() {
        // SELF-HOSTED string.lines (Machinery 2). Verified by line COUNT (list.len): "a\nb\nc"
        // =3, "a\nb\n"=2 (trailing newline drops the empty final line), ""=0, "\n"=1, "a\n\nb"
        // =3 (empty middle line kept), "x"=1. Byte-matches v0's s.lines() boundary rules.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(list.len(string.lines(\"a\\nb\\nc\"))))\n  \
            println(int.to_string(list.len(string.lines(\"a\\nb\\n\"))))\n  \
            println(int.to_string(list.len(string.lines(\"\"))))\n  \
            println(int.to_string(list.len(string.lines(\"\\n\"))))\n  \
            println(int.to_string(list.len(string.lines(\"a\\n\\nb\"))))\n  \
            println(int.to_string(list.len(string.lines(\"x\")))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.lines"));
        if let Some(out) = build_and_run("self_hosted_string_lines", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n2\n0\n1\n3\n1");
        }
    }

    #[test]
    fn self_hosted_string_join() {
        // SELF-HOSTED string.join — reads a List[String] (borrowed) + concatenates with sep.
        // join(split("a,bb,ccc",","), "-")="a-bb-ccc" (also CONTENT-verifies split's pieces!);
        // join(split("x",","),"-")="x"; multi-byte sep join(split("p::q::r","::"),"+")="p+q+r".
        let src = "fn main() -> Unit = {\n  \
            println(string.join(string.split(\"a,bb,ccc\", \",\"), \"-\"))\n  \
            println(string.join(string.split(\"x\", \",\"), \"-\"))\n  \
            println(string.join(string.split(\"p::q::r\", \"::\"), \"+\")) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.join"));
        if let Some(out) = build_and_run("self_hosted_string_join", &render_wasm_program(&prog)) {
            assert_eq!(out, "a-bb-ccc\nx\np+q+r");
        }
    }

    #[test]
    fn self_hosted_list_length_and_join() {
        // list.length is an exact alias of list.len (element count). list.join is identical to
        // string.join (`xs.join(sep)`) — both reuse the existing self-host impls. length([10,20,
        // 30])=3; join(split("a,bb,ccc",","),"-")="a-bb-ccc"; empty-sep join(split("xy",""),"")
        // round-trips the codepoints "xy".
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(list.length([10, 20, 30])))\n  \
            println(list.join(string.split(\"a,bb,ccc\", \",\"), \"-\"))\n  \
            println(list.join(string.split(\"p::q::r\", \"::\"), \"+\")) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.length"));
        assert!(prog.functions.iter().any(|f| f.name == "list.join"));
        if let Some(out) = build_and_run("self_hosted_list_length_join", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\na-bb-ccc\np+q+r");
        }
    }

    #[test]
    fn self_hosted_math_fmin_fmax_and_float_id() {
        // math.fmin/fmax are NaN-AWARE (return the non-NaN operand, NOT wasm's NaN-propagating
        // f64.min/max): fmin(3,7)=3, fmax(3,7)=7, fmin(NaN,5)=5 (the NaN case distinguishes the
        // replicated logic from a raw f64.min). float.to_float64/from_float64 = identity.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(float.to_int(math.fmin(float.from_int(3), float.from_int(7)))))\n  \
            println(int.to_string(float.to_int(math.fmax(float.from_int(3), float.from_int(7)))))\n  \
            let nan = float.sqrt(float.from_int(0 - 1))\n  \
            println(int.to_string(float.to_int(math.fmin(nan, float.from_int(5)))))\n  \
            println(int.to_string(float.to_int(float.to_float64(float.from_int(9))))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "math.fmin"));
        assert!(prog.functions.iter().any(|f| f.name == "float.to_float64"));
        if let Some(out) = build_and_run("self_hosted_math_fminmax", &render_wasm_program(&prog)) {
            // fmin(3,7)=3, fmax(3,7)=7, fmin(NaN,5)=5, to_float64(9)=9
            assert_eq!(out, "3\n7\n5\n9");
        }
    }

    #[test]
    fn self_hosted_bytes_core() {
        // SELF-HOSTED bytes.* core (Bytes = a byte block, same layout as String). from_string
        // ("hi")=[104,105]: len 2, byte[0]=104 ('h'), byte[1]=105 ('i'), out-of-bounds→default
        // 99. is_empty(from_string(""))=true, is_empty(from_string("x"))=false.
        let src = "fn main() -> Unit = {\n  \
            let b = bytes.from_string(\"hi\")\n  \
            println(int.to_string(bytes.len(b)))\n  \
            println(int.to_string(bytes.get_or(b, 0, 0)))\n  \
            println(int.to_string(bytes.get_or(b, 1, 0)))\n  \
            println(int.to_string(bytes.get_or(b, 5, 99)))\n  \
            let e = bytes.from_string(\"\")\n  let ee = bytes.is_empty(e)\n  let n1 = if ee then 1 else 0\n  println(int.to_string(n1))\n  \
            let x = bytes.from_string(\"x\")\n  let xe = bytes.is_empty(x)\n  let n2 = if xe then 1 else 0\n  println(int.to_string(n2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.from_string"));
        assert!(prog.functions.iter().any(|f| f.name == "bytes.len"));
        if let Some(out) = build_and_run("self_hosted_bytes_core", &render_wasm_program(&prog)) {
            // len2, byte[0]=104, byte[1]=105, oob=99, is_empty("")=1, is_empty("x")=0
            assert_eq!(out, "2\n104\n105\n99\n1\n0");
        }
    }

    #[test]
    fn self_hosted_bytes_transform() {
        // SELF-HOSTED bytes.new/concat/reverse/repeat/starts_with/ends_with over the Bytes
        // block machinery. new(3)=[0,0,0]; concat("ab","cd")="abcd"; reverse("abc")="cba";
        // repeat("xy",3)="xyxyxy"; starts_with/ends_with byte-compare. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let n3 = bytes.new(3)\n  \
            println(int.to_string(bytes.len(n3)))\n  \
            println(int.to_string(bytes.get_or(n3, 0, 9)))\n  \
            let ab = bytes.from_string(\"ab\")\n  let cd = bytes.from_string(\"cd\")\n  \
            let cc = bytes.concat(ab, cd)\n  \
            println(int.to_string(bytes.len(cc)))\n  \
            println(int.to_string(bytes.get_or(cc, 0, 0)))\n  \
            println(int.to_string(bytes.get_or(cc, 3, 0)))\n  \
            let abc = bytes.from_string(\"abc\")\n  let rev = bytes.reverse(abc)\n  \
            println(int.to_string(bytes.get_or(rev, 0, 0)))\n  \
            println(int.to_string(bytes.get_or(rev, 2, 0)))\n  \
            let xy = bytes.from_string(\"xy\")\n  let rep = bytes.repeat(xy, 3)\n  \
            println(int.to_string(bytes.len(rep)))\n  \
            println(int.to_string(bytes.get_or(rep, 5, 0)))\n  \
            let hello = bytes.from_string(\"hello\")\n  let he = bytes.from_string(\"he\")\n  let lo = bytes.from_string(\"lo\")\n  \
            let sw1 = bytes.starts_with(hello, he)\n  let s1 = if sw1 then 1 else 0\n  println(int.to_string(s1))\n  \
            let sw2 = bytes.starts_with(hello, lo)\n  let s2 = if sw2 then 1 else 0\n  println(int.to_string(s2))\n  \
            let ew1 = bytes.ends_with(hello, lo)\n  let e1 = if ew1 then 1 else 0\n  println(int.to_string(e1))\n  \
            let ew2 = bytes.ends_with(hello, he)\n  let e2 = if ew2 then 1 else 0\n  println(int.to_string(e2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.concat"));
        assert!(prog.functions.iter().any(|f| f.name == "bytes.starts_with"));
        if let Some(out) = build_and_run("self_hosted_bytes_transform", &render_wasm_program(&prog)) {
            // new3 len3 b0=0; concat len4 b0=97 b3=100; reverse b0=99 b2=97; repeat len6 b5=121;
            // starts_with("hello","he")=1 ("hello","lo")=0; ends_with("hello","lo")=1 ("hello","he")=0
            assert_eq!(out, "3\n0\n4\n97\n100\n99\n97\n6\n121\n1\n0\n1\n0");
        }
    }

    #[test]
    fn self_hosted_bytes_slice_cmp_set_pad() {
        // SELF-HOSTED bytes.slice/cmp/set/pad_left/pad_right. slice("hello",1,4)="ell";
        // slice("hello",3,1)=empty; cmp lexicographic (printed as -1/0/1 -> 0/1/2 since
        // int.to_string is non-negative); set copies+overwrites one byte (oob = no change);
        // pad_left/pad_right extend with a fill byte. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let hello = bytes.from_string(\"hello\")\n  \
            let sl = bytes.slice(hello, 1, 4)\n  \
            println(int.to_string(bytes.len(sl)))\n  \
            println(int.to_string(bytes.get_or(sl, 0, 0)))\n  \
            println(int.to_string(bytes.get_or(sl, 2, 0)))\n  \
            let sl2 = bytes.slice(hello, 3, 1)\n  \
            println(int.to_string(bytes.len(sl2)))\n  \
            let abc = bytes.from_string(\"abc\")\n  let abd = bytes.from_string(\"abd\")\n  let ab = bytes.from_string(\"ab\")\n  \
            let c1 = bytes.cmp(abc, abd)\n  let r1 = if c1 < 0 then 0 else (if c1 > 0 then 2 else 1)\n  println(int.to_string(r1))\n  \
            let c2 = bytes.cmp(abc, abc)\n  let r2 = if c2 < 0 then 0 else (if c2 > 0 then 2 else 1)\n  println(int.to_string(r2))\n  \
            let c3 = bytes.cmp(abd, abc)\n  let r3 = if c3 < 0 then 0 else (if c3 > 0 then 2 else 1)\n  println(int.to_string(r3))\n  \
            let c4 = bytes.cmp(ab, abc)\n  let r4 = if c4 < 0 then 0 else (if c4 > 0 then 2 else 1)\n  println(int.to_string(r4))\n  \
            let st = bytes.set(abc, 1, 90)\n  println(int.to_string(bytes.get_or(st, 1, 0)))\n  \
            let st2 = bytes.set(abc, 5, 90)\n  println(int.to_string(bytes.get_or(st2, 1, 0)))\n  \
            let hi = bytes.from_string(\"hi\")\n  \
            let pl = bytes.pad_left(hi, 5, 48)\n  \
            println(int.to_string(bytes.len(pl)))\n  \
            println(int.to_string(bytes.get_or(pl, 0, 0)))\n  \
            println(int.to_string(bytes.get_or(pl, 4, 0)))\n  \
            let pr = bytes.pad_right(hi, 5, 48)\n  \
            println(int.to_string(bytes.get_or(pr, 0, 0)))\n  \
            println(int.to_string(bytes.get_or(pr, 4, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.slice"));
        assert!(prog.functions.iter().any(|f| f.name == "bytes.cmp"));
        if let Some(out) = build_and_run("self_hosted_bytes_slice_cmp_set_pad", &render_wasm_program(&prog)) {
            // slice len3 b0=101 b2=108; empty len0; cmp 0,1,2,0; set b1=90, oob b1=98;
            // pad_left len5 b0=48 b4=105; pad_right b0=104 b4=48
            assert_eq!(out, "3\n101\n108\n0\n0\n1\n2\n0\n90\n98\n5\n48\n105\n104\n48");
        }
    }

    #[test]
    fn self_hosted_bytes_list_contains_xor() {
        // SELF-HOSTED bytes.from_list/to_list/contains/xor (the List[Int] <-> Bytes bridge +
        // substring search + xor). from_list([72,73,74])="HIJ"; to_list("AB")=[65,66];
        // contains is sub-sequence search; xor([12,10],[10,6])=[6,12]. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = [72, 73, 74]\n  let fl = bytes.from_list(xs)\n  \
            println(int.to_string(bytes.len(fl)))\n  \
            println(int.to_string(bytes.get_or(fl, 0, 0)))\n  \
            println(int.to_string(bytes.get_or(fl, 2, 0)))\n  \
            let ab = bytes.from_string(\"AB\")\n  let tl = bytes.to_list(ab)\n  \
            println(int.to_string(list.len(tl)))\n  \
            println(int.to_string(list.get_or(tl, 0, 0)))\n  \
            println(int.to_string(list.get_or(tl, 1, 0)))\n  \
            let hello = bytes.from_string(\"hello\")\n  let ell = bytes.from_string(\"ell\")\n  let xyz = bytes.from_string(\"xyz\")\n  let emp = bytes.from_string(\"\")\n  \
            let c1 = bytes.contains(hello, ell)\n  let n1 = if c1 then 1 else 0\n  println(int.to_string(n1))\n  \
            let c2 = bytes.contains(hello, xyz)\n  let n2 = if c2 then 1 else 0\n  println(int.to_string(n2))\n  \
            let c3 = bytes.contains(hello, emp)\n  let n3 = if c3 then 1 else 0\n  println(int.to_string(n3))\n  \
            let ya = [12, 10]\n  let yb = [10, 6]\n  let ba = bytes.from_list(ya)\n  let bb = bytes.from_list(yb)\n  \
            let xr = bytes.xor(ba, bb)\n  \
            println(int.to_string(bytes.get_or(xr, 0, 0)))\n  \
            println(int.to_string(bytes.get_or(xr, 1, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.from_list"));
        assert!(prog.functions.iter().any(|f| f.name == "bytes.to_list"));
        if let Some(out) = build_and_run("self_hosted_bytes_list_contains_xor", &render_wasm_program(&prog)) {
            // from_list len3 b0=72 b2=74; to_list len2 e0=65 e1=66; contains 1,0,1; xor [6,12]
            assert_eq!(out, "3\n72\n74\n2\n65\n66\n1\n0\n1\n6\n12");
        }
    }

    #[test]
    fn self_hosted_bytes_insert_remove_read() {
        // SELF-HOSTED bytes.insert/remove_at + the big-endian/little-endian integer reads
        // (read_u16_be/i16_be/i16_le/i32_be over the bitwise prim floor with sign extension).
        // insert clamps pos; remove_at clones on out-of-range; reads return 0 out of range.
        // A negative i16 is mapped to 999 (int.to_string is non-negative). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let abc = bytes.from_string(\"abc\")\n  \
            let i1 = bytes.insert(abc, 1, 90)\n  \
            println(int.to_string(bytes.len(i1)))\n  \
            println(int.to_string(bytes.get_or(i1, 1, 0)))\n  \
            println(int.to_string(bytes.get_or(i1, 2, 0)))\n  \
            let i2 = bytes.insert(abc, 0, 88)\n  println(int.to_string(bytes.get_or(i2, 0, 0)))\n  \
            let i3 = bytes.insert(abc, 10, 88)\n  println(int.to_string(bytes.get_or(i3, 3, 0)))\n  \
            let r1 = bytes.remove_at(abc, 1)\n  \
            println(int.to_string(bytes.len(r1)))\n  \
            println(int.to_string(bytes.get_or(r1, 1, 0)))\n  \
            let r2 = bytes.remove_at(abc, 5)\n  \
            println(int.to_string(bytes.len(r2)))\n  \
            println(int.to_string(bytes.get_or(r2, 0, 0)))\n  \
            let p12 = [1, 2]\n  let bp = bytes.from_list(p12)\n  \
            println(int.to_string(bytes.read_u16_be(bp, 0)))\n  \
            println(int.to_string(bytes.read_i16_be(bp, 0)))\n  \
            let ff = [255, 255]\n  let bf = bytes.from_list(ff)\n  \
            let v = bytes.read_i16_be(bf, 0)\n  let m = if v < 0 then 999 else v\n  println(int.to_string(m))\n  \
            let p21 = [2, 1]\n  let bl = bytes.from_list(p21)\n  \
            println(int.to_string(bytes.read_i16_le(bl, 0)))\n  \
            let q = [0, 0, 1, 0]\n  let bq = bytes.from_list(q)\n  \
            println(int.to_string(bytes.read_i32_be(bq, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.insert"));
        assert!(prog.functions.iter().any(|f| f.name == "bytes.read_i32_be"));
        if let Some(out) = build_and_run("self_hosted_bytes_insert_remove_read", &render_wasm_program(&prog)) {
            // insert(1) len4 b1=90 b2=98; insert(0) b0=88; insert(10) b3=88; remove(1) len2 b1=99;
            // remove(5) clone len3 b0=97; read_u16_be 258; read_i16_be 258; neg->999; read_i16_le 258; read_i32_be 256
            assert_eq!(out, "4\n90\n98\n88\n88\n2\n99\n3\n97\n258\n258\n999\n258\n256");
        }
    }

    #[test]
    fn self_hosted_bytes_more_reads() {
        // SELF-HOSTED bytes.read_u8/read_bool/read_u16_le/read_u32_be/read_u32_le/read_i32_le/
        // read_i64_be/read_i64_le. Unsigned reads stay non-negative (u32 max = 4294967295), i32_le
        // sign-extends (-1 -> 999), i64 reads are full-width. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let ab = [65, 200]\n  let bab = bytes.from_list(ab)\n  \
            println(int.to_string(bytes.read_u8(bab, 0)))\n  \
            println(int.to_string(bytes.read_u8(bab, 1)))\n  \
            println(int.to_string(bytes.read_u8(bab, 5)))\n  \
            let bl = [0, 7]\n  let bbl = bytes.from_list(bl)\n  \
            let rb0 = bytes.read_bool(bbl, 0)\n  let z0 = if rb0 then 1 else 0\n  println(int.to_string(z0))\n  \
            let rb1 = bytes.read_bool(bbl, 1)\n  let z1 = if rb1 then 1 else 0\n  println(int.to_string(z1))\n  \
            let p12 = [1, 2]\n  let b12 = bytes.from_list(p12)\n  \
            println(int.to_string(bytes.read_u16_le(b12, 0)))\n  \
            let q = [0, 0, 1, 0]\n  let bq = bytes.from_list(q)\n  \
            println(int.to_string(bytes.read_u32_be(bq, 0)))\n  \
            let ql = [0, 1, 0, 0]\n  let bql = bytes.from_list(ql)\n  \
            println(int.to_string(bytes.read_u32_le(bql, 0)))\n  \
            let big = [255, 255, 255, 255]\n  let bbig = bytes.from_list(big)\n  \
            println(int.to_string(bytes.read_u32_be(bbig, 0)))\n  \
            let vi = bytes.read_i32_le(bbig, 0)\n  let mi = if vi < 0 then 999 else vi\n  println(int.to_string(mi))\n  \
            let e8 = [0, 0, 0, 0, 0, 0, 1, 0]\n  let be8 = bytes.from_list(e8)\n  \
            println(int.to_string(bytes.read_i64_be(be8, 0)))\n  \
            let e8l = [0, 1, 0, 0, 0, 0, 0, 0]\n  let be8l = bytes.from_list(e8l)\n  \
            println(int.to_string(bytes.read_i64_le(be8l, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.read_u8"));
        assert!(prog.functions.iter().any(|f| f.name == "bytes.read_i64_be"));
        if let Some(out) = build_and_run("self_hosted_bytes_more_reads", &render_wasm_program(&prog)) {
            assert_eq!(out, "65\n200\n0\n0\n1\n513\n256\n256\n4294967295\n999\n256\n256");
        }
    }

    #[test]
    fn self_hosted_bytes_read_f64() {
        // SELF-HOSTED bytes.read_f64_be / read_f64_le — combine 8 bytes into the i64 bit pattern
        // then prim.ffrombits reinterprets it as the Float. Verified via float.to_int (1.0->1,
        // 2.5->2, 256.0->256); read_f64_le reads the same value from reversed bytes. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let one = [63, 240, 0, 0, 0, 0, 0, 0]\n  let bone = bytes.from_list(one)\n  \
            println(int.to_string(float.to_int(bytes.read_f64_be(bone, 0))))\n  \
            let two5 = [64, 4, 0, 0, 0, 0, 0, 0]\n  let bt = bytes.from_list(two5)\n  \
            println(int.to_string(float.to_int(bytes.read_f64_be(bt, 0))))\n  \
            let big = [64, 112, 0, 0, 0, 0, 0, 0]\n  let bb = bytes.from_list(big)\n  \
            println(int.to_string(float.to_int(bytes.read_f64_be(bb, 0))))\n  \
            let onele = [0, 0, 0, 0, 0, 0, 240, 63]\n  let bol = bytes.from_list(onele)\n  \
            println(int.to_string(float.to_int(bytes.read_f64_le(bol, 0)))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.read_f64_be"));
        assert!(prog.functions.iter().any(|f| f.name == "bytes.read_f64_le"));
        if let Some(out) = build_and_run("self_hosted_bytes_read_f64", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n2\n256\n1");
        }
    }

    #[test]
    fn self_hosted_bytes_get_index_of() {
        // SELF-HOSTED bytes.get / bytes.index_of returning a MATERIALIZED Option[Int] (the
        // 0-or-1-element-list layout, reusing the list.get machinery). A `match` over the result
        // EXECUTES (the call is tracked in is_self_host_option_module_fn): get(1)=Some(20),
        // get(5)=None; index_of("ll")=Some(2), index_of("xyz")=None, index_of("")=Some(0).
        let src = "fn main() -> Unit = {\n  \
            let xs = [10, 20, 30]\n  let b = bytes.from_list(xs)\n  \
            let g1 = bytes.get(b, 1)\n  \
            match g1 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let g2 = bytes.get(b, 5)\n  \
            match g2 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let hello = bytes.from_string(\"hello\")\n  let ll = bytes.from_string(\"ll\")\n  \
            let i1 = bytes.index_of(hello, ll)\n  \
            match i1 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let xyz = bytes.from_string(\"xyz\")\n  \
            let i2 = bytes.index_of(hello, xyz)\n  \
            match i2 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let emp = bytes.from_string(\"\")\n  \
            let i3 = bytes.index_of(hello, emp)\n  \
            match i3 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.get"));
        assert!(prog.functions.iter().any(|f| f.name == "bytes.index_of"));
        if let Some(out) = build_and_run("self_hosted_bytes_get_index_of", &render_wasm_program(&prog)) {
            assert_eq!(out, "20\nnone\n2\nnone\n0");
        }
    }

    #[test]
    fn self_hosted_bytes_eof_and_valid_utf8() {
        // SELF-HOSTED bytes.eof (negative pos wraps to usize -> always past-end) and
        // bytes.is_valid_utf8 (= std::str::from_utf8().is_ok(), Rust's exact validation table:
        // valid ASCII / multibyte / 4-byte emoji = true; lone-continuation, overlong, surrogate,
        // truncated and > U+10FFFF = false). Bools print as 1/0. Byte-matches v0.
        // NB: the `if X then 1 else 0` is BOUND to a let before int.to_string (an if directly in a
        // call-arg defers to 0 — the known call-arg-if gap), and each Bytes is pre-bound (no nested
        // call as an arg).
        let src = "fn main() -> Unit = {\n  \
            let abc = bytes.from_string(\"abc\")\n  \
            let e0 = bytes.eof(abc, 0)\n  let z0 = if e0 then 1 else 0\n  println(int.to_string(z0))\n  \
            let e3 = bytes.eof(abc, 3)\n  let z3 = if e3 then 1 else 0\n  println(int.to_string(z3))\n  \
            let e2 = bytes.eof(abc, 2)\n  let z2 = if e2 then 1 else 0\n  println(int.to_string(z2))\n  \
            let en = bytes.eof(abc, 0 - 1)\n  let zn = if en then 1 else 0\n  println(int.to_string(zn))\n  \
            let hi = bytes.from_string(\"hi\")\n  let vhi = bytes.is_valid_utf8(hi)\n  let yhi = if vhi then 1 else 0\n  println(int.to_string(yhi))\n  \
            let jp = bytes.from_string(\"日\")\n  let vjp = bytes.is_valid_utf8(jp)\n  let yjp = if vjp then 1 else 0\n  println(int.to_string(yjp))\n  \
            let ff = bytes.from_list([255])\n  let vff = bytes.is_valid_utf8(ff)\n  let yff = if vff then 1 else 0\n  println(int.to_string(yff))\n  \
            let ov = bytes.from_list([192, 128])\n  let vov = bytes.is_valid_utf8(ov)\n  let yov = if vov then 1 else 0\n  println(int.to_string(yov))\n  \
            let sg = bytes.from_list([237, 160, 128])\n  let vsg = bytes.is_valid_utf8(sg)\n  let ysg = if vsg then 1 else 0\n  println(int.to_string(ysg))\n  \
            let tr = bytes.from_list([194])\n  let vtr = bytes.is_valid_utf8(tr)\n  let ytr = if vtr then 1 else 0\n  println(int.to_string(ytr))\n  \
            let lc = bytes.from_list([128])\n  let vlc = bytes.is_valid_utf8(lc)\n  let ylc = if vlc then 1 else 0\n  println(int.to_string(ylc))\n  \
            let h4 = bytes.from_list([244, 144, 128, 128])\n  let vh4 = bytes.is_valid_utf8(h4)\n  let yh4 = if vh4 then 1 else 0\n  println(int.to_string(yh4))\n  \
            let em = bytes.from_list([240, 159, 152, 128])\n  let vem = bytes.is_valid_utf8(em)\n  let yem = if vem then 1 else 0\n  println(int.to_string(yem)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.eof"));
        assert!(prog.functions.iter().any(|f| f.name == "bytes.is_valid_utf8"));
        if let Some(out) = build_and_run("self_hosted_bytes_eof_valid_utf8", &render_wasm_program(&prog)) {
            // eof: false,true,false,true ; valid: hi=1, 日=1, [FF]=0, overlong=0, surrogate=0,
            // truncated=0, lone-cont=0, >10FFFF=0, emoji=1
            assert_eq!(out, "0\n1\n0\n1\n1\n1\n0\n0\n0\n0\n0\n0\n1");
        }
    }

    #[test]
    fn self_hosted_int_to_float() {
        // SELF-HOSTED int.to_float / int.to_float64 = prim.i2f (i64 -> f64). Verified via a
        // round-trip through the self-hosted float.to_int: to_int(to_float(42))=42. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(float.to_int(int.to_float(42))))\n  \
            println(int.to_string(float.to_int(int.to_float64(7)))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.to_float"));
        if let Some(out) = build_and_run("self_hosted_int_to_float", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\n7");
        }
    }

    #[test]
    fn self_hosted_float_to_uint64() {
        // SELF-HOSTED float.to_uint64 — saturating `f as u64`. For the in-i64-range inputs:
        // to_uint64(5.0)=5, (1000.9)=1000 (truncate), (-3.0)=0, (0.0)=0 — verified via int.to_string.
        // (For f >= 2^63 the body splits off 2^63 and ORs the high bit back so the u64 is exact; the
        // probe `prim.bor(prim.f2i(f-2^63), 1<<63)` produces i64::MIN for f=2^63, but a UInt64 result
        // can't be int.to_string'd or `==`-compared from the test — verified inline separately.)
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(float.to_uint64(5.0)))\n  \
            println(int.to_string(float.to_uint64(1000.9)))\n  \
            let neg = float.from_int(0 - 3)\n  println(int.to_string(float.to_uint64(neg)))\n  \
            println(int.to_string(float.to_uint64(0.0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.to_uint64"));
        if let Some(out) = build_and_run("self_hosted_float_to_uint64", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\n1000\n0\n0");
        }
    }

    #[test]
    fn self_hosted_float_convert() {
        // SELF-HOSTED float.to_int8/16/32 + to_uint8/16/32 — Rust's saturating `f as iN`/`as uN`
        // (out-of-range clamps to the type's min/max). prim.f2i truncates toward zero; the body
        // clamps to the narrower range. A negative result is printed as 0 - v. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(float.to_int8(100.0)))\n  \
            println(int.to_string(float.to_int8(200.0)))\n  \
            let nf = float.from_int(0 - 200)\n  let neg = float.to_int8(nf)\n  let nm = 0 - neg\n  println(int.to_string(nm))\n  \
            let f16 = float.from_int(40000)\n  println(int.to_string(float.to_int16(f16)))\n  \
            let f32 = float.from_int(3000000000)\n  println(int.to_string(float.to_int32(f32)))\n  \
            println(int.to_string(float.to_uint8(200.0)))\n  \
            println(int.to_string(float.to_uint8(300.0)))\n  \
            let un = float.from_int(0 - 5)\n  println(int.to_string(float.to_uint8(un)))\n  \
            let u16 = float.from_int(70000)\n  println(int.to_string(float.to_uint16(u16)))\n  \
            let u32 = float.from_int(5000000000)\n  println(int.to_string(float.to_uint32(u32))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.to_int8"));
        assert!(prog.functions.iter().any(|f| f.name == "float.to_uint32"));
        if let Some(out) = build_and_run("self_hosted_float_convert", &render_wasm_program(&prog)) {
            // 100; sat-high 127; sat-low -128 -> 128; 32767; 2147483647; 200; 255; neg->0; 65535; 4294967295
            assert_eq!(out, "100\n127\n128\n32767\n2147483647\n200\n255\n0\n65535\n4294967295");
        }
    }

    #[test]
    fn self_hosted_float_to_int64() {
        // SELF-HOSTED float.to_int64 = prim.f2i (full-width saturating `f as i64`, truncating
        // toward zero). to_int64(2.9)=2, to_int64(100.0)=100. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(float.to_int64(2.9)))\n  \
            println(int.to_string(float.to_int64(100.0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.to_int64"));
        if let Some(out) = build_and_run("self_hosted_float_to_int64", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n100");
        }
    }

    #[test]
    fn list_float_literal_materializes_its_element_bits() {
        // A `List[Float]` LITERAL now materializes its slots (alloc_init maps a LitFloat element to
        // its f64 BITS, the i64-uniform Float repr) — previously such a list silently lowered to an
        // empty/Opaque block (len 0), a latent miscompile. Read back via prim.load64 + ffrombits:
        // len 3, xs[1] = 20.0 -> float.to_int 20, xs[2] = 30.0 -> 30. The lowering floor that a
        // future List[Float] stdlib fn will read.
        let src = "fn main() -> Unit = {\n  \
            let xs = [10.0, 20.0, 30.0]\n  let h = prim.handle(xs)\n  \
            println(int.to_string(prim.load32(h + 4)))\n  \
            let e1 = prim.ffrombits(prim.load64(h + 12 + 8))\n  println(int.to_string(float.to_int(e1)))\n  \
            let e2 = prim.ffrombits(prim.load64(h + 12 + 16))\n  println(int.to_string(float.to_int(e2))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("list_float_literal_materializes", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n20\n30");
        }
    }

    #[test]
    fn self_hosted_string_first_last() {
        // SELF-HOSTED string.first / string.last returning Option[String]. The ERGONOMIC content
        // EXTRACT now works: `match Some(c) => println(c)` reads the element handle via LoadHandle
        // (i32 Ptr repr) and BORROWS it (the Option keeps ownership, frees it at scope-end
        // DropListStr — sound). first("hi")="h", first("日本")="日", first("")=None; last("hi")="i",
        // last("")=None. Byte-matches v0's s.chars().next()/.last().
        let src = "fn main() -> Unit = {\n  \
            let f1 = string.first(\"hi\")\n  match f1 { Some(c) => println(c), None => println(\"empty\"), }\n  \
            let f2 = string.first(\"日本\")\n  match f2 { Some(c) => println(c), None => println(\"empty\"), }\n  \
            let f3 = string.first(\"\")\n  match f3 { Some(c) => println(c), None => println(\"empty\"), }\n  \
            let l1 = string.last(\"hi\")\n  match l1 { Some(c) => println(c), None => println(\"empty\"), }\n  \
            let l3 = string.last(\"\")\n  match l3 { Some(c) => println(c), None => println(\"empty\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.first"));
        assert!(prog.functions.iter().any(|f| f.name == "string.last"));
        if let Some(out) = build_and_run("self_hosted_string_first_last", &render_wasm_program(&prog)) {
            assert_eq!(out, "h\n日\nempty\ni\nempty");
        }
    }

    #[test]
    fn self_hosted_string_char_at() {
        // SELF-HOSTED string.get(s, idx) -> Option[String] (the idx-th codepoint) over the
        // Option[String] construction machinery. CONTENT is verified by reading the materialized
        // Option directly via the prim floor (len-as-tag at +4, element handle at +12, its bytes):
        // char_at("hello",1)=Some("e"=101), [0]=Some("h"=104), [10]=None(len0); char_at("日本語",1)
        // =Some("本"= 3 bytes, lead 0xE6=230); [-1]=None. Byte-matches v0's s.chars().nth(idx).
        let src = "fn main() -> Unit = {\n  \
            let o1 = string.get(\"hello\", 1)\n  let h1 = prim.handle(o1)\n  \
            println(int.to_string(prim.load32(h1 + 4)))\n  \
            let e1 = prim.load64(h1 + 12)\n  println(int.to_string(prim.load8(e1 + 12)))\n  \
            let o0 = string.get(\"hello\", 0)\n  let e0 = prim.load64(prim.handle(o0) + 12)\n  \
            println(int.to_string(prim.load8(e0 + 12)))\n  \
            let oN = string.get(\"hello\", 10)\n  println(int.to_string(prim.load32(prim.handle(oN) + 4)))\n  \
            let oJ = string.get(\"日本語\", 1)\n  let eJ = prim.load64(prim.handle(oJ) + 12)\n  \
            println(int.to_string(prim.load32(eJ + 4)))\n  println(int.to_string(prim.load8(eJ + 12)))\n  \
            let oNeg = string.get(\"hello\", 0 - 1)\n  println(int.to_string(prim.load32(prim.handle(oNeg) + 4))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.get"));
        if let Some(out) = build_and_run("self_hosted_string_char_at", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n101\n104\n0\n3\n230\n0");
        }
    }

include!("tests_part4_b.rs");
include!("tests_part4_c.rs");
include!("tests_part4_d.rs");
include!("tests_part4_e.rs");
