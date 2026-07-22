// render_wasm test suite — part 2 of 3 (self-hosted stdlib e2e: lists/ints/strings).
    #[test]
    fn self_hosted_int_bitwise_ops() {
        // int.band/bor/bxor/bnot/bshl/bshr self-hosted (i64 bitwise via the new IntOp
        // And/Or/Xor/Shl/Shr; bnot is -n-1). band(12,10)=8, bor=14, bxor=6, bnot(5)=-6,
        // bshl(1,4)=16, bshr(256,2)=64, bshr(-8,1)=-4 (arithmetic). byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let a = int.band(12, 10)\n  \
            let b = int.bor(12, 10)\n  \
            let c = int.bxor(12, 10)\n  \
            let d = int.bnot(5)\n  \
            let e = int.bshl(1, 4)\n  \
            let f = int.bshr(256, 2)\n  \
            let g = int.bshr(0 - 8, 1)\n  \
            println(int.to_string(a))\n  \
            println(int.to_string(b))\n  \
            println(int.to_string(c))\n  \
            println(int.to_string(d))\n  \
            println(int.to_string(e))\n  \
            println(int.to_string(f))\n  \
            println(int.to_string(g)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.bxor"), "linked");
        if let Some(out) = build_and_run("int_bits", &render_wasm_program(&prog)) {
            assert_eq!(out, "8\n14\n6\n-6\n16\n64\n-4");
        }
    }

    #[test]
    fn self_hosted_int_bit_counts() {
        // int.pop_count / count_trailing_zeros / count_leading_zeros self-hosted (composed
        // from bshr/band over 64 bits): pop_count(8)=1, pop_count(7)=3, ctz(8)=3, ctz(0)=64,
        // clz(8)=60, clz(1)=63. byte-matching v0's u64 count_ones/trailing/leading_zeros.
        let src = "fn main() -> Unit = {\n  \
            let a = int.pop_count(8)\n  \
            let b = int.pop_count(7)\n  \
            let c = int.count_trailing_zeros(8)\n  \
            let d = int.count_trailing_zeros(0)\n  \
            let e = int.count_leading_zeros(8)\n  \
            let f = int.count_leading_zeros(1)\n  \
            println(int.to_string(a))\n  \
            println(int.to_string(b))\n  \
            println(int.to_string(c))\n  \
            println(int.to_string(d))\n  \
            println(int.to_string(e))\n  \
            println(int.to_string(f)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.pop_count"), "linked");
        if let Some(out) = build_and_run("int_bitcount", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n3\n3\n64\n60\n63");
        }
    }

    #[test]
    fn self_hosted_int_bit_width_and_log2() {
        // int.bit_width / log2_floor self-hosted (reuse __clz): bit_width(0)=0, (1)=1,
        // (255)=8; log2_floor(1)=0, (8)=3, (0)=-1. byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let a = int.bit_width(0)\n  \
            let b = int.bit_width(1)\n  \
            let c = int.bit_width(255)\n  \
            let d = int.log2_floor(1)\n  \
            let e = int.log2_floor(8)\n  \
            let f = int.log2_floor(0)\n  \
            println(int.to_string(a))\n  \
            println(int.to_string(b))\n  \
            println(int.to_string(c))\n  \
            println(int.to_string(d))\n  \
            println(int.to_string(e))\n  \
            println(int.to_string(f)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.bit_width"), "linked");
        if let Some(out) = build_and_run("int_bitwidth", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\n1\n8\n0\n3\n-1");
        }
    }

    #[test]
    fn self_hosted_int_power_of_two() {
        // int.next_power_of_two / prev_power_of_two self-hosted (1 << bit_width(n-1) /
        // 1 << log2_floor(n)): next(5)=8, next(8)=8, next(0)=1; prev(5)=4, prev(8)=8,
        // prev(0)=0. byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let a = int.next_power_of_two(5)\n  \
            let b = int.next_power_of_two(8)\n  \
            let c = int.next_power_of_two(0)\n  \
            let d = int.prev_power_of_two(5)\n  \
            let e = int.prev_power_of_two(8)\n  \
            let f = int.prev_power_of_two(0)\n  \
            println(int.to_string(a))\n  \
            println(int.to_string(b))\n  \
            println(int.to_string(c))\n  \
            println(int.to_string(d))\n  \
            println(int.to_string(e))\n  \
            println(int.to_string(f)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.next_power_of_two"), "linked");
        if let Some(out) = build_and_run("int_pow2", &render_wasm_program(&prog)) {
            assert_eq!(out, "8\n8\n1\n4\n8\n0");
        }
    }

    #[test]
    fn self_hosted_int_to_hex() {
        // int.to_hex self-hosted (lowercase {:x}, no leading zeros, negatives as the full
        // unsigned 64-bit): to_hex(255)=ff, to_hex(0)=0, to_hex(16)=10, to_hex(10)=a,
        // to_hex(-1)=ffffffffffffffff. byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let a = int.to_hex(255)\n  \
            let b = int.to_hex(0)\n  \
            let c = int.to_hex(16)\n  \
            let d = int.to_hex(10)\n  \
            let e = int.to_hex(0 - 1)\n  \
            println(a)\n  \
            println(b)\n  \
            println(c)\n  \
            println(d)\n  \
            println(e) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.to_hex"), "linked");
        if let Some(out) = build_and_run("int_hex", &render_wasm_program(&prog)) {
            assert_eq!(out, "ff\n0\n10\na\nffffffffffffffff");
        }
    }

    #[test]
    fn self_hosted_int_byte_swap_and_bit_reverse() {
        // int.byte_swap / bit_reverse self-hosted (bit ops): byte_swap(2^56)=1 (top byte
        // -> bottom); both are involutions, so swap/reverse twice = the original (byte_swap
        // round-trips 305419896, bit_reverse round-trips 5). byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let a = int.byte_swap(72057594037927936)\n  \
            let bs1 = int.byte_swap(305419896)\n  \
            let bs2 = int.byte_swap(bs1)\n  \
            let br1 = int.bit_reverse(5)\n  \
            let br2 = int.bit_reverse(br1)\n  \
            println(int.to_string(a))\n  \
            println(int.to_string(bs2))\n  \
            println(int.to_string(br2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.byte_swap"), "linked");
        if let Some(out) = build_and_run("int_bswap", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n305419896\n5");
        }
    }

    #[test]
    fn self_hosted_option_unwrap_or_scalar() {
        // `<self-host Option[Int]> ?? <Int>` EXECUTES (tag read + payload/fallback): the
        // Some branch yields the payload, the None branch the fallback. index_of("abcdef",
        // "cd") ?? 99 = 2, index_of("abcdef","zz") ?? 99 = 99; list.get(xs,1) ?? 0 = 20,
        // list.get(xs,9) ?? 0 = 0. byte-matching v0's `??`.
        let src = "fn main() -> Unit = {\n  \
            let xs = [10, 20, 30]\n  \
            let a = string.index_of(\"abcdef\", \"cd\") ?? 99\n  \
            let b = string.index_of(\"abcdef\", \"zz\") ?? 99\n  \
            let c = list.get(xs, 1) ?? 0\n  \
            let d = list.get(xs, 9) ?? 0\n  \
            println(int.to_string(a))\n  \
            println(int.to_string(b))\n  \
            println(int.to_string(c))\n  \
            println(int.to_string(d)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("unwrap_or", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n99\n20\n0");
        }
    }

    #[test]
    fn self_hosted_option_unwrap_or_loop_bounded() {
        // ADVERSARIAL: `list.get(xs,1) ?? 0` every iteration materializes + drops the
        // Option (read for its scalar payload), so the loop runs bounded (no leak).
        let src = "fn main() -> Unit = {\n  \
            let xs = [10, 20, 30]\n  \
            var i = 0\n  \
            while i < 2000 {\n    \
            let v = list.get(xs, 1) ?? 0\n    \
            println(int.to_string(v))\n    \
            i = i + 1\n  } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("unwrap_or_loop", &render_wasm_program(&prog)) {
            assert_eq!(out.lines().count(), 2000, "every iteration prints (no OOM/leak)");
            assert!(out.lines().all(|l| l == "20"));
        }
    }

    #[test]
    fn self_hosted_math_sign_and_pow() {
        // math.sign/pow self-hosted (scalar i64): sign(7)=1, sign(-3)=-1, sign(0)=0;
        // pow(2,10)=1024, pow(3,0)=1, pow(5,3)=125. byte-matching v0 (pow via %2 / /2
        // exponentiation-by-squaring = v0's e&1 / e>>1, the same wrapped result).
        let src = "fn main() -> Unit = {\n  \
            let a = math.sign(7)\n  \
            let b = math.sign(0 - 3)\n  \
            let c = math.sign(0)\n  \
            let d = math.pow(2, 10)\n  \
            let e = math.pow(3, 0)\n  \
            let f = math.pow(5, 3)\n  \
            println(int.to_string(a))\n  \
            println(int.to_string(b))\n  \
            println(int.to_string(c))\n  \
            println(int.to_string(d))\n  \
            println(int.to_string(e))\n  \
            println(int.to_string(f)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "math.pow"), "linked");
        if let Some(out) = build_and_run("math_sign_pow", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n-1\n0\n1024\n1\n125");
        }
    }

    #[test]
    fn self_hosted_string_reverse() {
        // string.reverse self-hosted (CODEPOINT reversal, not byte): reverse("hello")=
        // "olleh"; reverse("ab日")="日ba" — the multibyte char's bytes stay in order, only
        // the char sequence reverses. byte-matching v0's chars().rev().
        let src = "fn main() -> Unit = {\n  \
            let a = string.reverse(\"hello\")\n  \
            let b = string.reverse(\"ab日\")\n  \
            println(a)\n  \
            println(b) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.reverse"), "linked");
        if let Some(out) = build_and_run("string_reverse", &render_wasm_program(&prog)) {
            assert_eq!(out, "olleh\n日ba");
        }
    }

    #[test]
    fn self_hosted_string_to_bytes() {
        // string.to_bytes self-hosted (the UTF-8 bytes as a List[Int]): to_bytes("ABC")=
        // [65,66,67]; to_bytes("日")=[230,151,165] (3 UTF-8 bytes). Read back via list.len +
        // list.get_or. byte-matching v0; a 2000-iter loop is bounded (List[Int], no leak).
        let src = "fn main() -> Unit = {\n  \
            let b = string.to_bytes(\"ABC\")\n  \
            let bl = list.len(b)\n  \
            let b0 = list.get_or(b, 0, 0)\n  \
            let b2 = list.get_or(b, 2, 0)\n  \
            let m = string.to_bytes(\"日\")\n  \
            let ml = list.len(m)\n  \
            let m0 = list.get_or(m, 0, 0)\n  \
            println(int.to_string(bl))\n  \
            println(int.to_string(b0))\n  \
            println(int.to_string(b2))\n  \
            println(int.to_string(ml))\n  \
            println(int.to_string(m0)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.to_bytes"), "linked");
        if let Some(out) = build_and_run("string_to_bytes", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n65\n67\n3\n230");
        }
    }

    #[test]
    fn self_hosted_string_replace() {
        // string.replace self-hosted (build via prim.alloc_str, result length computed up
        // front): replace("a,b,c",",","-")="a-b-c" (same len), replace("xax","a","YY")=
        // "xYYx" (growing), replace("aXbXc","X","")="abc" (shrinking), replace("hi","z","Q")
        // ="hi" (no match). byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let a = string.replace(\"a,b,c\", \",\", \"-\")\n  \
            let b = string.replace(\"xax\", \"a\", \"YY\")\n  \
            let c = string.replace(\"aXbXc\", \"X\", \"\")\n  \
            let d = string.replace(\"hi\", \"z\", \"Q\")\n  \
            println(a)\n  \
            println(b)\n  \
            println(c)\n  \
            println(d) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.replace"), "linked");
        if let Some(out) = build_and_run("string_replace", &render_wasm_program(&prog)) {
            assert_eq!(out, "a-b-c\nxYYx\nabc\nhi");
        }
    }

    #[test]
    fn self_hosted_string_replace_first() {
        // string.replace_first self-hosted (find the first match, splice in `to`): only the
        // first occurrence is replaced: replace_first("a,b,c",",","-")="a-b,c", growing
        // ("xax","a","YY")="xYYx", no match ("abc","z","Q")="abc". byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let a = string.replace_first(\"a,b,c\", \",\", \"-\")\n  \
            let b = string.replace_first(\"xax\", \"a\", \"YY\")\n  \
            let c = string.replace_first(\"abc\", \"z\", \"Q\")\n  \
            println(a)\n  \
            println(b)\n  \
            println(c) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.replace_first"), "linked");
        if let Some(out) = build_and_run("string_replace_first", &render_wasm_program(&prog)) {
            assert_eq!(out, "a-b,c\nxYYx\nabc");
        }
    }

    #[test]
    fn self_hosted_string_trim_start_and_end() {
        // trim_start strips LEADING ASCII whitespace (keeps trailing), trim_end strips
        // TRAILING (keeps leading): trim_start("  abc")="abc", trim_end("xyz  ")="xyz";
        // trim_start("  ab ")="ab " keeps the one trailing space (len 3). byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let a = string.trim_start(\"  abc\")\n  \
            let b = string.trim_end(\"xyz  \")\n  \
            let t = string.trim_start(\"  ab \")\n  \
            let lt = string.len(t)\n  \
            println(a)\n  \
            println(b)\n  \
            println(int.to_string(lt)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.trim_start"), "linked");
        if let Some(out) = build_and_run("string_trim_se", &render_wasm_program(&prog)) {
            assert_eq!(out, "abc\nxyz\n3");
        }
    }

    #[test]
    fn self_hosted_string_pad_start_and_end() {
        // string.pad_start/pad_end self-hosted (pad to a CODEPOINT width with the first
        // char of pad, build via prim.alloc_str): pad_start("ab",5,"x")="xxxab", already-
        // wide pad_start("ab",2,"x")="ab", pad_end("ab",5,"-")="ab---", pad_start("5",3,"0")
        // ="005". byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let a = string.pad_start(\"ab\", 5, \"x\")\n  \
            let b = string.pad_start(\"ab\", 2, \"x\")\n  \
            let c = string.pad_end(\"ab\", 5, \"-\")\n  \
            let d = string.pad_start(\"5\", 3, \"0\")\n  \
            println(a)\n  \
            println(b)\n  \
            println(c)\n  \
            println(d) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.pad_start"), "linked");
        if let Some(out) = build_and_run("string_pad", &render_wasm_program(&prog)) {
            assert_eq!(out, "xxxab\nab\nab---\n005");
        }
    }

    #[test]
    fn self_hosted_list_reverse() {
        // list.reverse self-hosted — the FIRST List-CONSTRUCTING fn (prim.alloc_list +
        // store64): reverse([10,20,30]) = [30,20,10], read back via list.get_or. List[Int]
        // (i64 value copies, fully sound). byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = [10, 20, 30]\n  \
            let r = list.reverse(xs)\n  \
            let a = list.get_or(r, 0, 0)\n  \
            let b = list.get_or(r, 1, 0)\n  \
            let c = list.get_or(r, 2, 0)\n  \
            println(int.to_string(a))\n  \
            println(int.to_string(b))\n  \
            println(int.to_string(c)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.reverse"), "linked");
        if let Some(out) = build_and_run("list_reverse", &render_wasm_program(&prog)) {
            assert_eq!(out, "30\n20\n10");
        }
    }

    #[test]
    fn self_hosted_list_reverse_loop_bounded() {
        // ADVERSARIAL: list.reverse every iteration allocates a new List[Int] and drops it
        // (flat rc_dec, no nested elements) — bounded memory, the alloc_list+drop balance.
        let src = "fn main() -> Unit = {\n  \
            let xs = [1, 2, 3]\n  \
            var i = 0\n  \
            while i < 2000 {\n    \
            let r = list.reverse(xs)\n    \
            let a = list.get_or(r, 0, 0)\n    \
            println(int.to_string(a))\n    \
            i = i + 1\n  } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("list_reverse_loop", &render_wasm_program(&prog)) {
            assert_eq!(out.lines().count(), 2000, "every iteration prints (no OOM/leak)");
            assert!(out.lines().all(|l| l == "3"));
        }
    }

    #[test]
    fn self_hosted_list_range_and_repeat() {
        // list.range/repeat self-hosted (List[Int] construction, i64 values = no leak):
        // range(2,6)=[2,3,4,5], range(5,5)=[], repeat(7,3)=[7,7,7]. Read back via list.len +
        // list.get_or. byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let r = list.range(2, 6)\n  \
            let e = list.range(5, 5)\n  \
            let p = list.repeat(7, 3)\n  \
            let rl = list.len(r)\n  \
            let r0 = list.get_or(r, 0, 0)\n  \
            let r3 = list.get_or(r, 3, 0)\n  \
            let el = list.len(e)\n  \
            let pl = list.len(p)\n  \
            let p2 = list.get_or(p, 2, 0)\n  \
            println(int.to_string(rl))\n  \
            println(int.to_string(r0))\n  \
            println(int.to_string(r3))\n  \
            println(int.to_string(el))\n  \
            println(int.to_string(pl))\n  \
            println(int.to_string(p2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.range"), "linked");
        assert!(prog.functions.iter().any(|f| f.name == "list.repeat"), "linked");
        if let Some(out) = build_and_run("list_make", &render_wasm_program(&prog)) {
            assert_eq!(out, "4\n2\n5\n0\n3\n7");
        }
    }

    #[test]
    fn self_hosted_list_range_loop_bounded() {
        // ADVERSARIAL: list.range every iteration allocs + drops a List[Int] (no heap
        // elements to leak) — bounded memory.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 2000 {\n    \
            let r = list.range(0, 5)\n    \
            let s = list.get_or(r, 4, 0)\n    \
            println(int.to_string(s))\n    \
            i = i + 1\n  } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("list_range_loop", &render_wasm_program(&prog)) {
            assert_eq!(out.lines().count(), 2000, "every iteration prints (no OOM/leak)");
            assert!(out.lines().all(|l| l == "4"));
        }
    }

    #[test]
    fn self_hosted_list_sort() {
        // list.sort self-hosted (selection sort over a fresh List[Int] buffer): sort(
        // [3,1,4,1,5,9,2,6]) = [1,1,2,3,4,5,6,9]. Read back via list.len + list.get_or.
        // byte-matching v0's ascending Ord sort.
        let src = "fn main() -> Unit = {\n  \
            let xs = [3, 1, 4, 1, 5, 9, 2, 6]\n  \
            let s = list.sort(xs)\n  \
            let sl = list.len(s)\n  \
            let s0 = list.get_or(s, 0, 0)\n  \
            let s1 = list.get_or(s, 1, 0)\n  \
            let s7 = list.get_or(s, 7, 0)\n  \
            println(int.to_string(sl))\n  \
            println(int.to_string(s0))\n  \
            println(int.to_string(s1))\n  \
            println(int.to_string(s7)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.sort"), "linked");
        if let Some(out) = build_and_run("list_sort", &render_wasm_program(&prog)) {
            assert_eq!(out, "8\n1\n1\n9");
        }
    }

    #[test]
    fn self_hosted_list_sort_loop_bounded() {
        // ADVERSARIAL: list.sort every iteration allocs + sorts + drops a List[Int] —
        // bounded memory (i64 values, no element leak).
        let src = "fn main() -> Unit = {\n  \
            let xs = [3, 1, 2]\n  \
            var i = 0\n  \
            while i < 2000 {\n    \
            let s = list.sort(xs)\n    \
            let lo = list.get_or(s, 0, 0)\n    \
            println(int.to_string(lo))\n    \
            i = i + 1\n  } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("list_sort_loop", &render_wasm_program(&prog)) {
            assert_eq!(out.lines().count(), 2000, "every iteration prints (no OOM/leak)");
            assert!(out.lines().all(|l| l == "1"));
        }
    }

    #[test]
    fn self_hosted_list_unique() {
        // list.unique self-hosted (keep the first occurrence of each value): unique(
        // [1,1,2,2,1,3]) = [1,2,3] (insertion order). Read back via list.len + list.get_or.
        // byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = [1, 1, 2, 2, 1, 3]\n  \
            let u = list.unique(xs)\n  \
            let ul = list.len(u)\n  \
            let u0 = list.get_or(u, 0, 0)\n  \
            let u1 = list.get_or(u, 1, 0)\n  \
            let u2 = list.get_or(u, 2, 0)\n  \
            println(int.to_string(ul))\n  \
            println(int.to_string(u0))\n  \
            println(int.to_string(u1))\n  \
            println(int.to_string(u2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.unique"), "linked");
        if let Some(out) = build_and_run("list_unique", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n1\n2\n3");
        }
    }

    #[test]
    fn self_hosted_list_dedup() {
        // list.dedup self-hosted (drop CONSECUTIVE duplicates only): dedup([1,1,2,2,1])=
        // [1,2,1] (the trailing 1 stays — not adjacent to the earlier 1s). Read back via
        // list.len + list.get_or. byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = [1, 1, 2, 2, 1]\n  \
            let d = list.dedup(xs)\n  \
            let dl = list.len(d)\n  \
            let d0 = list.get_or(d, 0, 0)\n  \
            let d1 = list.get_or(d, 1, 0)\n  \
            let d2 = list.get_or(d, 2, 0)\n  \
            println(int.to_string(dl))\n  \
            println(int.to_string(d0))\n  \
            println(int.to_string(d1))\n  \
            println(int.to_string(d2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.dedup"), "linked");
        if let Some(out) = build_and_run("list_dedup", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n1\n2\n1");
        }
    }

    #[test]
    fn self_hosted_list_intersperse() {
        // list.intersperse self-hosted (insert sep between elements): intersperse([1,2,3],0)
        // =[1,0,2,0,3], intersperse([5],9)=[5] (no sep for one element). Read back via
        // list.len + list.get_or. byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = [1, 2, 3]\n  \
            let r = list.intersperse(xs, 0)\n  \
            let rl = list.len(r)\n  \
            let r0 = list.get_or(r, 0, 9)\n  \
            let r1 = list.get_or(r, 1, 9)\n  \
            let r4 = list.get_or(r, 4, 9)\n  \
            let one = list.intersperse([5], 9)\n  \
            let ol = list.len(one)\n  \
            println(int.to_string(rl))\n  \
            println(int.to_string(r0))\n  \
            println(int.to_string(r1))\n  \
            println(int.to_string(r4))\n  \
            println(int.to_string(ol)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.intersperse"), "linked");
        if let Some(out) = build_and_run("list_intersperse", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\n1\n0\n3\n1");
        }
    }

    #[test]
    fn self_hosted_list_take_and_drop() {
        // list.take/drop self-hosted (List[Int] construction via alloc_list + __copy_slice):
        // take([10,20,30,40],2)=[10,20], drop(...,2)=[30,40]. Read back via list.len +
        // list.get_or. byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = [10, 20, 30, 40]\n  \
            let t = list.take(xs, 2)\n  \
            let d = list.drop(xs, 2)\n  \
            let tl = list.len(t)\n  \
            let t0 = list.get_or(t, 0, 0)\n  \
            let t1 = list.get_or(t, 1, 0)\n  \
            let d0 = list.get_or(d, 0, 0)\n  \
            let d1 = list.get_or(d, 1, 0)\n  \
            println(int.to_string(tl))\n  \
            println(int.to_string(t0))\n  \
            println(int.to_string(t1))\n  \
            println(int.to_string(d0))\n  \
            println(int.to_string(d1)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.take"), "linked");
        assert!(prog.functions.iter().any(|f| f.name == "list.drop"), "linked");
        if let Some(out) = build_and_run("list_take_drop", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n10\n20\n30\n40");
        }
    }

    #[test]
    fn self_hosted_list_slice() {
        // list.slice self-hosted (List[Int] construction): slice([10,20,30,40,50],1,4)=
        // [20,30,40]; end clamps to len (slice(...,3,99)=[40,50]); start>=end is empty
        // (slice(...,4,2)=[]). byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = [10, 20, 30, 40, 50]\n  \
            let a = list.slice(xs, 1, 4)\n  \
            let b = list.slice(xs, 3, 99)\n  \
            let c = list.slice(xs, 4, 2)\n  \
            let al = list.len(a)\n  \
            let a0 = list.get_or(a, 0, 0)\n  \
            let a2 = list.get_or(a, 2, 0)\n  \
            let bl = list.len(b)\n  \
            let cl = list.len(c)\n  \
            println(int.to_string(al))\n  \
            println(int.to_string(a0))\n  \
            println(int.to_string(a2))\n  \
            println(int.to_string(bl))\n  \
            println(int.to_string(cl)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.slice"), "linked");
        if let Some(out) = build_and_run("list_slice", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n20\n40\n2\n0");
        }
    }

    #[test]
    fn self_hosted_string_starts_and_ends_with() {
        // string.starts_with / string.ends_with self-hosted (pure byte comparison over the
        // prim floor): starts_with("hello","he")=true / ("hello","lo")=false; ends_with(
        // "hello","lo")=true / ("hello","he")=false — byte-matching v0. (The Bool result is
        // bound first since a scalar call in an if-cond does not lower; printed T/F.)
        let src = "fn main() -> Unit = {\n  \
            let a = string.starts_with(\"hello\", \"he\")\n  \
            let b = string.starts_with(\"hello\", \"lo\")\n  \
            let c = string.ends_with(\"hello\", \"lo\")\n  \
            let d = string.ends_with(\"hello\", \"he\")\n  \
            if a then println(\"T\") else println(\"F\")\n  \
            if b then println(\"T\") else println(\"F\")\n  \
            if c then println(\"T\") else println(\"F\")\n  \
            if d then println(\"T\") else println(\"F\") }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.starts_with"), "linked");
        assert!(prog.functions.iter().any(|f| f.name == "string.ends_with"), "linked");
        if let Some(out) = build_and_run("string_starts_ends", &render_wasm_program(&prog)) {
            assert_eq!(out, "T\nF\nT\nF");
        }
    }

    #[test]
    fn self_hosted_string_contains() {
        // string.contains self-hosted (a byte scan over each start position, reusing
        // __byte_eq): contains("hello world","lo w")=true, ("hello","xyz")=false,
        // ("hello","")=true (empty needle). byte-matching v0; results bound then printed T/F.
        let src = "fn main() -> Unit = {\n  \
            let a = string.contains(\"hello world\", \"lo w\")\n  \
            let b = string.contains(\"hello\", \"xyz\")\n  \
            let c = string.contains(\"hello\", \"\")\n  \
            if a then println(\"T\") else println(\"F\")\n  \
            if b then println(\"T\") else println(\"F\")\n  \
            if c then println(\"T\") else println(\"F\") }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.contains"), "linked");
        if let Some(out) = build_and_run("string_contains", &render_wasm_program(&prog)) {
            assert_eq!(out, "T\nF\nT");
        }
    }

    #[test]
    fn self_hosted_string_count() {
        // string.count self-hosted (NON-OVERLAPPING byte-scan count reusing __byte_eq):
        // count("abcabc","bc")=2, count("aaa","aa")=1 (skips past each match), count(
        // "hello","z")=0, count("ab","")=3 (empty needle = char_count+1). byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let a = string.count(\"abcabc\", \"bc\")\n  \
            let b = string.count(\"aaa\", \"aa\")\n  \
            let c = string.count(\"hello\", \"z\")\n  \
            let d = string.count(\"ab\", \"\")\n  \
            println(int.to_string(a))\n  \
            println(int.to_string(b))\n  \
            println(int.to_string(c))\n  \
            println(int.to_string(d)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.count"), "linked");
        if let Some(out) = build_and_run("string_count", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n1\n0\n3");
        }
    }

    #[test]
    fn self_hosted_string_index_of() {
        // string.index_of self-hosted (Option[Int], CODEPOINT index matching v0): index_of(
        // "abcdef","cd")=Some(2), ("abcdef","zz")=None, ("日本語","語")=Some(2) — the codepoint
        // index 2, NOT the byte offset 6. The Option result is tracked (string gate) so the
        // match executes the taken arm.
        let src = "fn main() -> Unit = {\n  \
            match string.index_of(\"abcdef\", \"cd\") {\n    \
            Some(x) => println(int.to_string(x)),\n    None => println(\"none\"),\n  }\n  \
            match string.index_of(\"abcdef\", \"zz\") {\n    \
            Some(x) => println(int.to_string(x)),\n    None => println(\"none\"),\n  }\n  \
            match string.index_of(\"日本語\", \"語\") {\n    \
            Some(x) => println(int.to_string(x)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.index_of"), "linked");
        if let Some(out) = build_and_run("string_index_of", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\nnone\n2");
        }
    }

    #[test]
    fn self_hosted_string_last_index_of() {
        // string.last_index_of self-hosted (Option[Int], codepoint index, backward scan):
        // last_index_of("abcabc","bc")=Some(4) (the LATER "bc", codepoint index), ("abcabc",
        // "z")=None, ("a日a日","日")=Some(3) (the LATER 日, codepoint index 3). byte-match v0.
        let src = "fn main() -> Unit = {\n  \
            match string.last_index_of(\"abcabc\", \"bc\") {\n    \
            Some(x) => println(int.to_string(x)),\n    None => println(\"none\"),\n  }\n  \
            match string.last_index_of(\"abcabc\", \"z\") {\n    \
            Some(x) => println(int.to_string(x)),\n    None => println(\"none\"),\n  }\n  \
            match string.last_index_of(\"a日a日\", \"日\") {\n    \
            Some(x) => println(int.to_string(x)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.last_index_of"), "linked");
        if let Some(out) = build_and_run("string_last_index_of", &render_wasm_program(&prog)) {
            assert_eq!(out, "4\nnone\n3");
        }
    }

    #[test]
    fn self_hosted_string_index_of_loop_bounded() {
        // ADVERSARIAL: index_of + match every iteration must be bounded — the per-call
        // materialized Option is freed at the iteration's end (the string gate tracks it).
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 2000 {\n    \
            match string.index_of(\"abcdef\", \"cd\") {\n      \
            Some(x) => println(int.to_string(x)),\n      None => println(\"none\"),\n    }\n    \
            i = i + 1\n  } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("index_of_loop", &render_wasm_program(&prog)) {
            assert_eq!(out.lines().count(), 2000, "every iteration prints (no OOM/leak)");
            assert!(out.lines().all(|l| l == "2"));
        }
    }

    #[test]
    fn heap_result_if_call_arm_materializes_its_arg_per_arm() {
        // SOUNDNESS: `echo("hi")` materializes the "hi" arg into a heap temp. It is freed
        // WITHIN the arm (per-arm Drop), not at function scope — so when c is false and this
        // arm never runs, there is no Drop of an uninitialized local (no garbage rc_dec
        // trap). pick2(true)=echo("hi")="ok"; pick2(false)="no" (the materializing arm not
        // taken, runs clean).
        let src = "fn echo(s: String) -> String = \"ok\"\n\
            fn pick2(c: Bool) -> String = if c then echo(\"hi\") else \"no\"\n\
            fn main() -> Unit = {\n  \
            println(pick2(true))\n  println(pick2(false)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("heap_if_mat_arg", &render_wasm_program(&prog)) {
            assert_eq!(out, "ok\nno");
        }
    }

    #[test]
    fn dynamic_string_alloc_builds_and_returns_a_decimal() {
        // The stdlib-self-host foundation: a String built at RUNTIME via prim.alloc_str
        // (Op::Alloc{DynStr} = an owned rc=1 block, cert i) + filled with decimal digits
        // via prim.store8, then RETURNED (move-out m) and printed. to_str(12345)="12345",
        // byte-matching v0's int.to_string. The owned dynamic String is freed by the caller.
        let src = "fn count_digits(n: Int, acc: Int) -> Int = if n < 10 then acc + 1 else count_digits(n / 10, acc + 1)\n\
            fn fill(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = fill(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn to_str(n: Int) -> String = {\n  \
            let len = count_digits(n, 0)\n  \
            let buf = prim.alloc_str(len)\n  \
            let _e = fill(n, prim.handle(buf) + 12)\n  \
            buf }\n\
            fn main() -> Unit = println(to_str(12345))\n";
        let prog = lower_source(src);
        let f = prog.functions.iter().find(|f| f.name == "to_str").expect("lowered fn \"to_str\" not found");
        assert!(
            f.ops.iter().any(|op| matches!(op, Op::Alloc { init: Init::DynStr { .. }, .. })),
            "to_str must allocate a DynStr, got {:?}",
            f.ops
        );
        if let Some(out) = build_and_run("dyn_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "12345");
        }
    }

    #[test]
    fn dynamic_string_in_a_loop_is_reclaimed() {
        // SOUNDNESS: a DynStr (runtime-allocated owned String) built + dropped every
        // iteration must be reclaimed (free-list reuse) — 1000 iterations allocating a
        // fresh String would OOM the single page if leaked, or trap the $rc_dec sentinel if
        // double-freed. Completing all 1000 lines proves the owned dynamic String is freed
        // exactly once per iteration and the block reused.
        let src = "fn count_digits(n: Int, acc: Int) -> Int = if n < 10 then acc + 1 else count_digits(n / 10, acc + 1)\n\
            fn fill(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = fill(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn to_str(n: Int) -> String = {\n  \
            let len = count_digits(n, 0)\n  \
            let buf = prim.alloc_str(len)\n  \
            let _e = fill(n, prim.handle(buf) + 12)\n  \
            buf }\n\
            fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 1000 {\n    println(to_str(i))\n    i = i + 1\n  } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("dyn_str_loop", &render_wasm_program(&prog)) {
            assert_eq!(out.lines().count(), 1000, "every iteration must print (no OOM)");
            assert_eq!(out.lines().next(), Some("0"));
            assert_eq!(out.lines().last(), Some("999"));
        }
    }

