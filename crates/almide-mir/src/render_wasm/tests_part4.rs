// render_wasm test suite — part 4 of 4 (self-hosted stdlib e2e, continued).
// Textually included by render_wasm/tests.rs (one module: helpers/tests share scope).

    #[test]
    fn lifted_lambda_executes_via_call_indirect() {
        // THE closures-machinery floor (the path to higher-order self-host: list.map/filter/
        // fold). A non-capturing `let f = (x) => x + 1` LIFTS to a fresh `__lambda_*`
        // function bound via Op::FuncRef; `f(5)` lowers to Op::CallIndirect through that slot
        // and EXECUTES, computing 6 — byte-matching v0. End-to-end: lower (lift + FuncRef +
        // CallIndirect) → render (function table) → wasm → wasmtime.
        let src = "fn main() -> Unit = {\n  \
            let f = (x) => x + 1\n  let y = f(5)\n  let s = int.to_string(y)\n  println(s) }\n";
        let prog = lower_source(src);
        assert!(
            prog.functions.iter().any(|f| f.name.starts_with("__lambda_")),
            "the non-capturing lambda must be lifted to a __lambda_* function"
        );
        if let Some(out) = build_and_run("lifted_lambda_call_indirect", &render_wasm_program(&prog)) {
            assert_eq!(out, "6");
        }
    }

    #[test]
    fn user_higher_order_function_executes() {
        // The full closures machinery end-to-end (the path to list.map/filter/fold). A
        // user-defined higher-order `apply(f, x) = f(x)`: `f` is a FUNCTION-typed PARAM
        // (a scalar table slot, NOT a heap value) invoked via Op::CallIndirect; the call
        // site `apply((n) => n + 10, 5)` LIFTS the lambda argument to a FuncRef slot passed
        // BY VALUE. Computes 15, byte-matching v0. Proves (A) lambda-arg lift + (B) Fn-param
        // scalar repr + (C) CallIndirect through a function-typed param all compose.
        let src = "fn apply(f: (Int) -> Int, x: Int) -> Int = {\n  let r = f(x)\n  r\n}\n\
            fn main() -> Unit = {\n  let v = apply((n) => n + 10, 5)\n  let s = int.to_string(v)\n  println(s) }\n";
        let prog = lower_source(src);
        assert!(
            prog.functions.iter().any(|f| f.name.starts_with("__lambda_")),
            "the lambda argument must be lifted to a __lambda_* function"
        );
        if let Some(out) = build_and_run("user_higher_order", &render_wasm_program(&prog)) {
            assert_eq!(out, "15");
        }
    }

    #[test]
    fn self_hosted_list_map() {
        // SELF-HOSTED `list.map` (the first higher-order stdlib fn). list.map([1,2,3,4],
        // (x) => x * x) builds [1,4,9,16] over the prim floor: a fresh List[Int], each slot
        // filled with f(elem) invoked via CallIndirect through the lifted lambda's slot.
        // Byte-matches v0 (sum + a sampled element confirm the contents).
        let src = "fn main() -> Unit = {\n  \
            let ys = list.map([1, 2, 3, 4], (x) => x * x)\n  \
            let s = int.to_string(list.sum(ys))\n  println(s)\n  \
            let e = int.to_string(list.get_or(ys, 3, 0))\n  println(e) }\n";
        let prog = lower_source(src);
        assert!(
            prog.functions.iter().any(|f| f.name == "list.map"),
            "list.map must be auto-linked from the self-host registry"
        );
        if let Some(out) = build_and_run("self_hosted_list_map", &render_wasm_program(&prog)) {
            // 1+4+9+16 = 30 ; ys[3] = 16
            assert_eq!(out, "30\n16");
        }
    }

    #[test]
    fn self_hosted_list_filter() {
        // SELF-HOSTED `list.filter` (variable-length higher-order). filter([1..6], (x) => x %
        // 2 == 0) keeps the evens [2,4,6]: over-allocate, pack matches via CallIndirect on
        // the predicate (called ONCE per element = byte-matches v0), patch the result len.
        let src = "fn main() -> Unit = {\n  \
            let ys = list.filter([1, 2, 3, 4, 5, 6], (x) => x % 2 == 0)\n  \
            let s = int.to_string(list.len(ys))\n  println(s)\n  \
            let t = int.to_string(list.sum(ys))\n  println(t)\n  \
            let e = int.to_string(list.get_or(ys, 0, 0))\n  println(e) }\n";
        let prog = lower_source(src);
        assert!(
            prog.functions.iter().any(|f| f.name == "list.filter"),
            "list.filter must be auto-linked from the self-host registry"
        );
        if let Some(out) = build_and_run("self_hosted_list_filter", &render_wasm_program(&prog)) {
            // [2,4,6]: len 3, sum 12, ys[0] = 2
            assert_eq!(out, "3\n12\n2");
        }
    }

    #[test]
    fn self_hosted_list_any_all() {
        // SELF-HOSTED `list.any` / `list.all` (predicate → Bool, short-circuiting). any([1,2,
        // 3], x>2)=true, any(_, x>9)=false; all([2,4,6], even)=true, all([2,3,4], even)=false.
        // The predicate is a lifted lambda invoked via CallIndirect. Booleans printed as 1/0.
        let src = "fn main() -> Unit = {\n  \
            let a1 = list.any([1, 2, 3], (x) => x > 2)\n  let n1 = if a1 then 1 else 0\n  println(int.to_string(n1))\n  \
            let a2 = list.any([1, 2, 3], (x) => x > 9)\n  let n2 = if a2 then 1 else 0\n  println(int.to_string(n2))\n  \
            let b1 = list.all([2, 4, 6], (x) => x % 2 == 0)\n  let n3 = if b1 then 1 else 0\n  println(int.to_string(n3))\n  \
            let b2 = list.all([2, 3, 4], (x) => x % 2 == 0)\n  let n4 = if b2 then 1 else 0\n  println(int.to_string(n4)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.any"));
        assert!(prog.functions.iter().any(|f| f.name == "list.all"));
        if let Some(out) = build_and_run("self_hosted_list_any_all", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n0\n1\n0");
        }
    }

    #[test]
    fn self_hosted_list_count_and_while() {
        // SELF-HOSTED `list.count` / `list.take_while` / `list.drop_while` (1-arity predicate,
        // closures machinery). count([1..6], even)=3; take_while([2,4,5,6], even)=[2,4] (stops
        // at 5); drop_while([2,4,5,6], even)=[5,6]. Predicate via CallIndirect, byte-match v0.
        let src = "fn main() -> Unit = {\n  \
            let c = list.count([1, 2, 3, 4, 5, 6], (x) => x % 2 == 0)\n  println(int.to_string(c))\n  \
            let tw = list.take_while([2, 4, 5, 6], (x) => x % 2 == 0)\n  println(int.to_string(list.len(tw)))\n  println(int.to_string(list.sum(tw)))\n  \
            let dw = list.drop_while([2, 4, 5, 6], (x) => x % 2 == 0)\n  println(int.to_string(list.len(dw)))\n  println(int.to_string(list.sum(dw))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.count"));
        assert!(prog.functions.iter().any(|f| f.name == "list.take_while"));
        assert!(prog.functions.iter().any(|f| f.name == "list.drop_while"));
        if let Some(out) = build_and_run("self_hosted_list_count_while", &render_wasm_program(&prog)) {
            // count=3 ; take_while=[2,4] len2 sum6 ; drop_while=[5,6] len2 sum11
            assert_eq!(out, "3\n2\n6\n2\n11");
        }
    }

    #[test]
    fn self_hosted_list_fold() {
        // SELF-HOSTED higher-order `list.fold` — the FIRST two-arity closure (f: (Acc, Int) ->
        // Acc via $closure_fn2). fold([1,2,3,4], 0, (a,x)=>a+x)=10; fold([1,2,3,4], 1, (a,x)=>
        // a*x)=24. The 2-arg closure is invoked via CallIndirect through the new per-arity
        // closure type; byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let s = list.fold([1, 2, 3, 4], 0, (a, x) => a + x)\n  println(int.to_string(s))\n  \
            let p = list.fold([1, 2, 3, 4], 1, (a, x) => a * x)\n  println(int.to_string(p)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.fold"));
        if let Some(out) = build_and_run("self_hosted_list_fold", &render_wasm_program(&prog)) {
            assert_eq!(out, "10\n24");
        }
    }

    #[test]
    fn self_hosted_list_find() {
        // SELF-HOSTED `list.find` — Option-returning higher-order. find([1,2,3,4], x>2)=Some(3),
        // find(_, x>9)=None. Some/None built in tail position (the call-arm Option pattern,
        // like list.max), consumed via `??`; byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let a = list.find([1, 2, 3, 4], (x) => x > 2) ?? 0\n  println(int.to_string(a))\n  \
            let b = list.find([1, 2, 3, 4], (x) => x > 9) ?? 0\n  println(int.to_string(b)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.find"));
        if let Some(out) = build_and_run("self_hosted_list_find", &render_wasm_program(&prog)) {
            // find(x>2)=Some(3)→3 ; find(x>9)=None→0 (fallback)
            assert_eq!(out, "3\n0");
        }
    }

    #[test]
    fn self_hosted_list_reduce() {
        // SELF-HOSTED `list.reduce` — seedless fold (two-arity closure + Option result). reduce
        // ([3,1,4,1,5], max)=Some(5); reduce([], +)=None. Combines $closure_fn2 with the
        // call-arm materialized-Option; consumed via ??, byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let m = list.reduce([3, 1, 4, 1, 5], (a, x) => if a > x then a else x) ?? 0\n  println(int.to_string(m))\n  \
            let e = list.reduce([], (a, x) => a + x) ?? 99\n  println(int.to_string(e)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.reduce"));
        if let Some(out) = build_and_run("self_hosted_list_reduce", &render_wasm_program(&prog)) {
            // reduce(max)=5 ; reduce([])=None → fallback 99
            assert_eq!(out, "5\n99");
        }
    }

    #[test]
    fn self_hosted_list_find_index_and_scan() {
        // find_index([10,20,30,40], x>25)=Some(2) (30 is index 2), find_index(_, x>99)=None.
        // scan([1,2,3,4], 0, +)=[1,3,6,10] (running sums; len 4, sum 20, ys[3]=10). find_index
        // = the find pattern with the INDEX payload; scan = a 2-arity fold emitting each acc.
        let src = "fn main() -> Unit = {\n  \
            let i = list.find_index([10, 20, 30, 40], (x) => x > 25) ?? 99\n  println(int.to_string(i))\n  \
            let j = list.find_index([10, 20, 30, 40], (x) => x > 99) ?? 99\n  println(int.to_string(j))\n  \
            let ys = list.scan([1, 2, 3, 4], 0, (a, x) => a + x)\n  println(int.to_string(list.len(ys)))\n  println(int.to_string(list.sum(ys)))\n  println(int.to_string(list.get_or(ys, 3, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.find_index"));
        assert!(prog.functions.iter().any(|f| f.name == "list.scan"));
        if let Some(out) = build_and_run("self_hosted_list_find_index_scan", &render_wasm_program(&prog)) {
            // find_index=2 ; none→99 ; scan len4 sum20 ys[3]=10
            assert_eq!(out, "2\n99\n4\n20\n10");
        }
    }

    #[test]
    fn self_hosted_list_zip_with() {
        // SELF-HOSTED `list.zip_with` — two lists + a two-arity closure ($closure_fn2), result
        // length = min. zip_with([1,2,3],[10,20,30,40], +)=[11,22,33] (stops at 3). len 3, sum
        // 66, ys[2]=33. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let ys = list.zip_with([1, 2, 3], [10, 20, 30, 40], (a, b) => a + b)\n  \
            println(int.to_string(list.len(ys)))\n  println(int.to_string(list.sum(ys)))\n  \
            println(int.to_string(list.get_or(ys, 2, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.zip_with"));
        if let Some(out) = build_and_run("self_hosted_list_zip_with", &render_wasm_program(&prog)) {
            // [11,22,33]: len3 sum66 ys[2]=33
            assert_eq!(out, "3\n66\n33");
        }
    }

    #[test]
    fn self_hosted_list_sort_by() {
        // SELF-HOSTED `list.sort_by` — STABLE sort by a cached scalar key. Key = x%2 (even=0,
        // odd=1): sort_by([3,1,4,1,5], x%2) puts the even (4) first, then the odds in ORIGINAL
        // order [3,1,1,5] → [4,3,1,1,5]. Confirms by-key ordering AND stability; byte-matches
        // v0's sort_by_cached_key. ys[0]=4, ys[1]=3 (first odd), sum=14, len=5.
        let src = "fn main() -> Unit = {\n  \
            let ys = list.sort_by([3, 1, 4, 1, 5], (x) => x % 2)\n  \
            println(int.to_string(list.len(ys)))\n  \
            println(int.to_string(list.get_or(ys, 0, 0)))\n  \
            println(int.to_string(list.get_or(ys, 1, 0)))\n  \
            println(int.to_string(list.sum(ys))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.sort_by"));
        if let Some(out) = build_and_run("self_hosted_list_sort_by", &render_wasm_program(&prog)) {
            // [4,3,1,1,5]: len5 ys[0]=4 ys[1]=3 sum14
            assert_eq!(out, "5\n4\n3\n14");
        }
    }

    #[test]
    fn self_hosted_list_unique_by() {
        // SELF-HOSTED `list.unique_by` — keep the first element of each distinct key. key = x%3:
        // [1,2,3,4,5,6] → keys [1,2,0,1,2,0] → keep first of each: 1(k1),2(k2),3(k0) → [1,2,3].
        // len 3, sum 6, ys[2]=3. Byte-matches v0 for a pure key.
        let src = "fn main() -> Unit = {\n  \
            let ys = list.unique_by([1, 2, 3, 4, 5, 6], (x) => x % 3)\n  \
            println(int.to_string(list.len(ys)))\n  \
            println(int.to_string(list.sum(ys)))\n  \
            println(int.to_string(list.get_or(ys, 2, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.unique_by"));
        if let Some(out) = build_and_run("self_hosted_list_unique_by", &render_wasm_program(&prog)) {
            // [1,2,3]: len3 sum6 ys[2]=3
            assert_eq!(out, "3\n6\n3");
        }
    }

    #[test]
    fn self_hosted_list_filter_map() {
        // SELF-HOSTED `list.filter_map` — the FIRST heap-returning closure (f: (Int) ->
        // Option[Int] via $closure_fn1_h). Keep doubled evens: filter_map([1,2,3,4,5], x =>
        // if x%2==0 then Some(x*2) else None) = [4,8] (from 2,4). len 2, sum 12, ys[0]=4.
        // Each closure result Option is owned and dropped — byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let ys = list.filter_map([1, 2, 3, 4, 5], (x) => if x % 2 == 0 then Some(x * 2) else None)\n  \
            println(int.to_string(list.len(ys)))\n  \
            println(int.to_string(list.sum(ys)))\n  \
            println(int.to_string(list.get_or(ys, 0, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.filter_map"));
        if let Some(out) = build_and_run("self_hosted_list_filter_map", &render_wasm_program(&prog)) {
            // [4,8]: len2 sum12 ys[0]=4
            assert_eq!(out, "2\n12\n4");
        }
    }

    #[test]
    fn filter_map_closure_results_do_not_leak() {
        // BOUNDED-LOOP LEAK GUARD for the heap-returning closure (memory is a fixed 1 page =
        // 64 KiB). filter_map over 3000 elems allocates 3000 owned Option results. With each
        // dropped + reused (O(1), __fm_step frees `o` before the next), peak ≈ xs+buf ≈ 48 KiB
        // — FITS. If a closure result LEAKED, the 3000 un-freed Options would ≈ double memory
        // to ~96 KiB and trap (out of bounds). Completing with the right count is the proof.
        let src = "fn main() -> Unit = {\n  \
            let xs = list.range(0, 3000)\n  \
            let ys = list.filter_map(xs, (x) => if x % 3 == 0 then Some(x) else None)\n  \
            println(int.to_string(list.len(ys))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("filter_map_no_leak", &render_wasm_program(&prog)) {
            // multiples of 3 in [0,3000): 0,3,…,2997 = 1000
            assert_eq!(out, "1000");
        }
    }

    #[test]
    fn self_hosted_float_core() {
        // SELF-HOSTED `float.*` over the FLOAT prim floor (f64 bits in the i64-uniform value).
        // abs(-5.0)=5, sqrt(16.0)=4, floor(2.5)=2, ceil(2.5)=3, min(3.0,7.0)=3, max=7. Results
        // converted to Int (float.to_int) for printing; from_int builds the Float inputs.
        let src = "fn main() -> Unit = {\n  \
            let n5 = float.from_int(0 - 5)\n  let a = float.to_int(float.abs(n5))\n  println(int.to_string(a))\n  \
            let s16 = float.from_int(16)\n  let s = float.to_int(float.sqrt(s16))\n  println(int.to_string(s))\n  \
            let f = float.to_int(float.floor(2.5))\n  println(int.to_string(f))\n  \
            let c = float.to_int(float.ceil(2.5))\n  println(int.to_string(c))\n  \
            let lo = float.to_int(float.min(float.from_int(3), float.from_int(7)))\n  println(int.to_string(lo))\n  \
            let hi = float.to_int(float.max(float.from_int(3), float.from_int(7)))\n  println(int.to_string(hi)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.abs"));
        assert!(prog.functions.iter().any(|f| f.name == "float.sqrt"));
        if let Some(out) = build_and_run("self_hosted_float_core", &render_wasm_program(&prog)) {
            // abs(-5)=5 sqrt(16)=4 floor(2.5)=2 ceil(2.5)=3 min=3 max=7
            assert_eq!(out, "5\n4\n2\n3\n3\n7");
        }
    }

    #[test]
    fn self_hosted_float_extra() {
        // SELF-HOSTED float.clamp / float.is_nan. clamp(5,0,3)=3, clamp(-1,0,3)=0, clamp(2,0,3)
        // =2 (to_int printed). is_nan(sqrt(-1))=1 (NaN), is_nan(5.0)=0. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let z = float.from_int(0)\n  let th = float.from_int(3)\n  \
            let c1 = float.to_int(float.clamp(float.from_int(5), z, th))\n  println(int.to_string(c1))\n  \
            let c2 = float.to_int(float.clamp(float.from_int(0 - 1), z, th))\n  println(int.to_string(c2))\n  \
            let c3 = float.to_int(float.clamp(float.from_int(2), z, th))\n  println(int.to_string(c3))\n  \
            let nan = float.sqrt(float.from_int(0 - 1))\n  let b1 = float.is_nan(nan)\n  let i1 = if b1 then 1 else 0\n  println(int.to_string(i1))\n  \
            let five = float.from_int(5)\n  let b2 = float.is_nan(five)\n  let i2 = if b2 then 1 else 0\n  println(int.to_string(i2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.clamp"));
        assert!(prog.functions.iter().any(|f| f.name == "float.is_nan"));
        if let Some(out) = build_and_run("self_hosted_float_extra", &render_wasm_program(&prog)) {
            // clamp: 3,0,2 ; is_nan: 1,0
            assert_eq!(out, "3\n0\n2\n1\n0");
        }
    }

    #[test]
    fn self_hosted_float_sign() {
        // SELF-HOSTED float.sign = f64::signum (sign-bit based via copysign). sign(5)=1,
        // sign(-5)=-1, sign(+0.0)=1 (signum of +0.0 is 1.0, NOT 0). to_int printed.
        let src = "fn main() -> Unit = {\n  \
            let a = float.to_int(float.sign(float.from_int(5)))\n  println(int.to_string(a))\n  \
            let b = float.to_int(float.sign(float.from_int(0 - 5)))\n  println(int.to_string(b))\n  \
            let c = float.to_int(float.sign(float.from_int(0)))\n  println(int.to_string(c)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.sign"));
        if let Some(out) = build_and_run("self_hosted_float_sign", &render_wasm_program(&prog)) {
            // sign(5)=1, sign(-5)=-1, sign(+0.0)=1
            assert_eq!(out, "1\n-1\n1");
        }
    }

    #[test]
    fn self_hosted_float_is_infinite() {
        // SELF-HOSTED float.is_infinite — |n| == +inf (inf built as 1.0/0.0). is_infinite(1e400)
        // =true (1e400 overflows the f64 to +inf), is_infinite(5.0)=false. Booleans as 1/0.
        let src = "fn main() -> Unit = {\n  \
            let big = 1e400\n  let i1b = float.is_infinite(big)\n  let i1 = if i1b then 1 else 0\n  println(int.to_string(i1))\n  \
            let five = float.from_int(5)\n  let i2b = float.is_infinite(five)\n  let i2 = if i2b then 1 else 0\n  println(int.to_string(i2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.is_infinite"));
        if let Some(out) = build_and_run("self_hosted_float_is_infinite", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n0");
        }
    }

    #[test]
    fn self_hosted_float_bits() {
        // SELF-HOSTED float.to_bits / int.bits_to_float = identity bit reinterpret (the value
        // IS the f64 bits). to_bits(2.0)=0x4000000000000000=4611686018427387904; round-trip
        // bits_to_float(that) back to 2.0 → to_int 2.
        let src = "fn main() -> Unit = {\n  \
            let bits = float.to_bits(2.0)\n  println(int.to_string(bits))\n  \
            let back = int.bits_to_float(bits)\n  println(int.to_string(float.to_int(back))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "float.to_bits"));
        if let Some(out) = build_and_run("self_hosted_float_bits", &render_wasm_program(&prog)) {
            assert_eq!(out, "4611686018427387904\n2");
        }
    }

    #[test]
    fn self_hosted_int_rotate() {
        // SELF-HOSTED int.rotate_left/right (width-parameterized, logical-shift via prim.bshr_u).
        // rotate_left(1,4,8)=16, rotate_right(16,4,8)=1, rotate_left(128,1,8)=1 (high bit wraps
        // to bit 0), rotate_left(1,4,32)=16. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(int.rotate_left(1, 4, 8)))\n  \
            println(int.to_string(int.rotate_right(16, 4, 8)))\n  \
            println(int.to_string(int.rotate_left(128, 1, 8)))\n  \
            println(int.to_string(int.rotate_left(1, 4, 32))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.rotate_left"));
        assert!(prog.functions.iter().any(|f| f.name == "int.rotate_right"));
        if let Some(out) = build_and_run("self_hosted_int_rotate", &render_wasm_program(&prog)) {
            assert_eq!(out, "16\n1\n1\n16");
        }
    }

    #[test]
    fn self_hosted_string_split() {
        // SELF-HOSTED string.split — Machinery 2 (List[String], nested ownership). Verified by
        // the PIECE COUNT (list.len, element-repr-agnostic) over the separator-finding edge
        // cases: "a,bb,ccc"->3, "x"->1 (no sep), "a,,b"->3 (empty middle), ""->1 (empty src),
        // "a::b::c" w/ a MULTI-BYTE sep ->3. (Byte-copy of each piece is the proven alloc_str
        // + __copy_bytes path; element CONTENT access needs a repr-aware List[String] get.)
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(list.len(string.split(\"a,bb,ccc\", \",\"))))\n  \
            println(int.to_string(list.len(string.split(\"x\", \",\"))))\n  \
            println(int.to_string(list.len(string.split(\"a,,b\", \",\"))))\n  \
            println(int.to_string(list.len(string.split(\"\", \",\"))))\n  \
            println(int.to_string(list.len(string.split(\"a::b::c\", \"::\")))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.split"));
        if let Some(out) = build_and_run("self_hosted_string_split", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n1\n3\n1\n3");
        }
    }

    #[test]
    fn string_split_pieces_do_not_leak() {
        // BOUNDED-LOOP LEAK GUARD for Machinery 2 (1-page memory). A while loop splits 2000
        // times; each split's result List[String] (3 owned Strings) MUST be recursively freed
        // (DropListStr) at the loop-iteration scope end. If the element Strings or the list
        // leaked, $alloc would exhaust memory and trap. Completing + the right last count proves
        // the recursive free works.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 2000 {\n  \
              let p = string.split(\"a,bb,ccc\", \",\")\n  last = list.len(p)\n  i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_split_no_leak", &render_wasm_program(&prog)) {
            assert_eq!(out, "3");
        }
    }

    #[test]
    fn self_hosted_string_chars() {
        // SELF-HOSTED string.chars (Machinery 2). Verified by the codepoint COUNT (list.len):
        // chars("abc")=3, chars("")=0, chars("a日c")=3 (the multibyte CJK char is ONE element),
        // chars("日本語")=3. Each codepoint's bytes (1-4, from the UTF-8 lead) are copied out.
        let src = "fn main() -> Unit = {\n  \
            println(int.to_string(list.len(string.chars(\"abc\"))))\n  \
            println(int.to_string(list.len(string.chars(\"\"))))\n  \
            println(int.to_string(list.len(string.chars(\"a日c\"))))\n  \
            println(int.to_string(list.len(string.chars(\"日本語\")))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "string.chars"));
        if let Some(out) = build_and_run("self_hosted_string_chars", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n0\n3\n3");
        }
    }

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
        assert!(prog.functions.iter().any(|f| f.name == "list.fold_str"));
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
        assert!(prog.functions.iter().any(|f| f.name == "list.find_str"));
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

    #[test]
    fn self_hosted_result_flatten() {
        // SELF-HOSTED result.flatten → Ok(inner) collapses to the inner Result, Err(e) propagates. The
        // input is a heap-Ok Result[Result[Int,String],String] (tag @16, inner handle @12). v0-match.
        // (Ok(Err(..)) input leaks the inner String once — harmless for a single run.)
        let src = "fn main() -> Unit = {\n  \
            let i1: Result[Int, String] = Ok(9)\n  let r1: Result[Result[Int, String], String] = Ok(i1)\n  let f1 = result.flatten(r1)\n  \
            match f1 { Ok(v) => println(int.to_string(v)), Err(e) => println(e), }\n  \
            let r2: Result[Result[Int, String], String] = Err(\"outer\")\n  let f2 = result.flatten(r2)\n  \
            match f2 { Ok(v) => println(int.to_string(v)), Err(e) => println(e), }\n  \
            let i3: Result[Int, String] = Err(\"inner\")\n  let r3: Result[Result[Int, String], String] = Ok(i3)\n  let f3 = result.flatten(r3)\n  \
            match f3 { Ok(v) => println(int.to_string(v)), Err(e) => println(e), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.flatten"));
        if let Some(out) = build_and_run("self_hosted_result_flatten", &render_wasm_program(&prog)) {
            assert_eq!(out, "9\nouter\ninner");
        }
    }

    #[test]
    fn self_hosted_result_flatten_loop_reclaims() {
        // SOUNDNESS: a bounded loop flattening Ok(Ok(i)) (no inner String → no leak) — reclaimed each
        // iter. 4000 iters; `last` = the flattened value.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let inr: Result[Int, String] = Ok(i)\n    let r: Result[Result[Int, String], String] = Ok(inr)\n    let f = result.flatten(r)\n    \
              match f { Ok(v) => { last = v }, Err(e) => { last = 0 }, }\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_result_flatten_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "3999");
        }
    }

    #[test]
    fn self_hosted_error_chain() {
        // SELF-HOSTED error.chain → "{outer}\\ncaused by: {cause}" (v0's format!). A 3-way byte
        // concat over the prim floor. Byte-matches v0.
        let src = "fn main() -> Unit = { println(error.chain(\"disk full\", \"out of space\")) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "error.chain"));
        if let Some(out) = build_and_run("self_hosted_error_chain", &render_wasm_program(&prog)) {
            assert_eq!(out, "disk full\ncaused by: out of space");
        }
    }

    #[test]
    fn self_hosted_error_chain_loop_reclaims() {
        // SOUNDNESS: a bounded loop building error.chain (a fresh String each iter) — no leak. 5000
        // iters; `last` = the result byte length (codepoints).
        let src = "fn main() -> Unit = {\n  var i = 0\n  var last = 0\n  \
            while i < 5000 {\n    let s = error.chain(\"ab\", \"cd\")\n    last = string.len(s)\n    i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_error_chain_loop_reclaims", &render_wasm_program(&prog)) {
            // "ab" + "\ncaused by: " (12) + "cd" = 2 + 12 + 2 = 16.
            assert_eq!(out, "16");
        }
    }

    #[test]
    fn self_hosted_error_message() {
        // SELF-HOSTED error.message → Ok → "" (empty), Err → a copy of the message. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let a: Result[Int, String] = Ok(5)\n  let ma = error.message(a)\n  println(int.to_string(string.len(ma)))\n  \
            let b: Result[Int, String] = Err(\"boom\")\n  let mb = error.message(b)\n  println(mb) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "error.message"));
        if let Some(out) = build_and_run("self_hosted_error_message", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\nboom");
        }
    }

    #[test]
    fn self_hosted_error_message_loop_reclaims() {
        // SOUNDNESS: a bounded loop copying an Err message (a fresh String each iter) — no leak. 5000
        // iters; `last` = the message length.
        let src = "fn main() -> Unit = {\n  var i = 0\n  var last = 0\n  \
            while i < 5000 {\n    let b: Result[Int, String] = Err(\"boom\")\n    let m = error.message(b)\n    last = string.len(m)\n    i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_error_message_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "4");
        }
    }

    #[test]
    fn self_hosted_error_context() {
        // SELF-HOSTED error.context → Ok kept, Err(e) → Err("{ctx}: {e}"). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let a: Result[Int, String] = Ok(5)\n  let ra = error.context(a, \"reading\")\n  \
            match ra { Ok(v) => println(int.to_string(v)), Err(e) => println(e), }\n  \
            let b: Result[Int, String] = Err(\"disk full\")\n  let rb = error.context(b, \"reading config\")\n  \
            match rb { Ok(v) => println(int.to_string(v)), Err(e) => println(e), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "error.context"));
        if let Some(out) = build_and_run("self_hosted_error_context", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\nreading config: disk full");
        }
    }

    #[test]
    fn self_hosted_error_context_loop_reclaims() {
        // SOUNDNESS + the scalar-call-reassign-in-loop fix: a bounded loop adding context to an Err
        // (a fresh concatenated String each iter; `last = string.len(e)` is a scalar CALL reassign, so
        // the loop runs for real — not the model-one-iteration fallback that would mask a leak). No
        // leak/double-free. 4000 iters; the contextualized message is "ctx: x" = 6 codepoints.
        let src = "fn main() -> Unit = {\n  var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    let b: Result[Int, String] = Err(\"x\")\n    let rb = error.context(b, \"ctx\")\n    \
            match rb { Ok(v) => { last = 0 }, Err(e) => { last = string.len(e) }, }\n    i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_error_context_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "6");
        }
    }

    #[test]
    fn self_hosted_option_collect() {
        // SELF-HOSTED option.collect → Some([all values]) when every element is Some, else None. The
        // List[Option[Int]] input is built via list.map (an Option literal-list does not construct).
        let src = "fn main() -> Unit = {\n  \
            let ns = [10, 20, 30]\n  let alls = list.map(ns, (x) => Some(x))\n  let ra = option.collect(alls)\n  \
            match ra { Some(lst) => println(int.to_string(list.sum(lst))), None => println(\"none\"), }\n  \
            let mix = list.map(ns, (x) => if x == 20 then None else Some(x))\n  let rb = option.collect(mix)\n  \
            match rb { Some(lst) => println(int.to_string(list.len(lst))), None => println(\"none\"), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.collect"));
        if let Some(out) = build_and_run("self_hosted_option_collect", &render_wasm_program(&prog)) {
            assert_eq!(out, "60\nnone");
        }
    }

    #[test]
    fn self_hosted_option_collect_loop_reclaims() {
        // SOUNDNESS: a bounded loop building option.collect (a fresh List[Int] in Some, freed by
        // DropListStr each iter; `last = list.len(lst)` is a scalar-CALL reassign → a real loop, not
        // the model-one-iteration fallback that would mask a leak). 4000 iters; `last` = the length.
        let src = "fn main() -> Unit = {\n  var i = 0\n  var last = 0\n  let ns = [1, 2, 3, 4]\n  \
            while i < 4000 {\n    let alls = list.map(ns, (x) => Some(x))\n    let r = option.collect(alls)\n    \
            match r { Some(lst) => { last = list.len(lst) }, None => { last = 0 }, }\n    i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_option_collect_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "4");
        }
    }

    #[test]
    fn self_hosted_bytes_skip() {
        // SELF-HOSTED bytes.skip → advance pos by n, clamped to the byte length. Pure scalar over the
        // Bytes header (len@4). Byte-matches v0's `let np = pos + n; if np > len then len else np`.
        let src = "fn main() -> Unit = {\n  \
            let b = bytes.from_string(\"hello\")\n  \
            println(int.to_string(bytes.skip(b, 0, 3)))\n  \
            println(int.to_string(bytes.skip(b, 2, 2)))\n  \
            println(int.to_string(bytes.skip(b, 3, 10)))\n  \
            println(int.to_string(bytes.skip(b, 0, 5))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.skip"));
        if let Some(out) = build_and_run("self_hosted_bytes_skip", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n4\n5\n5");
        }
    }

    #[test]
    fn self_hosted_result_to_list() {
        // SELF-HOSTED result.to_list → a 0-or-1-element List[Int] built over the prim floor (Ok→[v]
        // len 1, Err→[] len 0). Byte-matches v0's `match r { Ok(v) => [v], Err(_) => [] }`.
        let src = "fn main() -> Unit = {\n  \
            let r: Result[Int, String] = Ok(7)\n  let xs = result.to_list(r)\n  \
            println(int.to_string(list.len(xs)))\n  \
            let e: Result[Int, String] = Err(\"bad\")\n  let ys = result.to_list(e)\n  \
            println(int.to_string(list.len(ys))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.to_list"));
        if let Some(out) = build_and_run("self_hosted_result_to_list", &render_wasm_program(&prog)) {
            assert_eq!(out, "1\n0");
        }
    }

    #[test]
    fn self_hosted_result_to_list_loop_reclaims() {
        // SOUNDNESS: a bounded loop building + dropping the 0/1-element List[Int] — no leak/double-
        // free (the flat List block is reclaimed each iteration). 4000 iters alternating Ok/Err by
        // parity; `last` accumulates the Ok-list length so the result is observable.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let r: Result[Int, String] = Ok(i)\n    let xs = result.to_list(r)\n    \
              last = list.len(xs)\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_result_to_list_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "1");
        }
    }

    #[test]
    fn self_hosted_value_as_string() {
        // SELF-HOSTED value.as_string → the HEAP-Ok Result[String, String] (1-slot DynListStr; the
        // String handle in slot 0's low 32 bits, the Ok/Err tag in its high 32 bits). as_string(str
        // "hello")=Ok("hello") match→"hello"; as_string(int 5)=Err("expected Str") match→the message.
        // Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let vs = value.str(\"hello\")\n  let r = value.as_string(vs)\n  \
            match r { Ok(s) => println(s), Err(e) => println(e), }\n  \
            let vi = value.int(5)\n  let r2 = value.as_string(vi)\n  \
            match r2 { Ok(s) => println(s), Err(e) => println(e), } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "value.as_string"));
        if let Some(out) = build_and_run("self_hosted_value_as_string", &render_wasm_program(&prog)) {
            assert_eq!(out, "hello\nexpected Str");
        }
    }

    #[test]
    fn self_hosted_value_as_string_loop_reclaims() {
        // SOUNDNESS for the heap-Ok Result path: a bounded loop building + dropping a Result[String,
        // String] (the slot-0 String reclaimed by DropListStr regardless of Ok/Err) — no leak/double-
        // free. 4000 iters; the Ok arm uses string.len on the borrowed payload (5).
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let vs = value.str(\"abcde\")\n    let r = value.as_string(vs)\n    \
              match r { Ok(s) => { let n = string.len(s)\n last = n }, Err(e) => { last = 0 }, }\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_value_as_string_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "5");
        }
    }

    #[test]
    fn self_hosted_value_str() {
        // SELF-HOSTED value.str — a heap-payload Value (tag 4) owning a deep-copied String at +12.
        // value.str("hello") → tag 4, payload "hello". Verified by reading the block via prim.
        let src = "fn main() -> Unit = {\n  \
            let v = value.str(\"hello\")\n  let h = prim.handle(v)\n  \
            println(int.to_string(prim.load32(h + 4)))\n  \
            let payload = prim.load_str(h + 12)\n  println(payload) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "value.str"));
        if let Some(out) = build_and_run("self_hosted_value_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "4\nhello");
        }
    }

    #[test]
    fn self_hosted_value_str_loop_reclaims() {
        // SOUNDNESS for the new DropValue heap path: a bounded loop building + dropping a fresh
        // heap-payload Value (value.str) each iteration must reclaim the payload String + the block
        // (the runtime-tag DropValue) — no leak (OOM) / double-free (trap). 4000 iters, prints the
        // last Value's tag (4).
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let v = value.str(\"payload-string\")\n    \
              last = prim.load32(prim.handle(v) + 4)\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_value_str_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "4");
        }
    }

    #[test]
    fn self_hosted_set_string_loop_reclaims() {
        // SOUNDNESS for the Set[String] nested-ownership path: a bounded loop building + dropping a
        // fresh Set[String] each iteration must reclaim each element String + the block (DropListStr)
        // — no leak (OOM) / double-free (trap). 3000 iterations, prints the last set's len (2).
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 3000 {\n    \
              let s = set.from_list(string.split(\"xx,yy,xx\", \",\"))\n    \
              last = set.len(s)\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_set_string_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "2");
        }
    }

    #[test]
    fn self_hosted_map_core_loop_reclaims() {
        // SOUNDNESS for the new Map[Int,Int] heap path: a bounded loop building + dropping a
        // fresh Map every iteration must reclaim each (plain non-nested drop) — no leak/double-free.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let m0 = map.new()\n    let m1 = map.set(m0, 3, 30)\n    let m = map.set(m1, 4, 40)\n    \
              last = map.get_or(m, 4, 0)\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_map_core_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "40");
        }
    }

    #[test]
    fn self_hosted_set_core_loop_reclaims() {
        // SOUNDNESS for the new Set[Int] heap path: a bounded loop building + dropping a fresh
        // Set[Int] every iteration must reclaim each (plain non-nested drop) — no leak (OOM) or
        // double-free (trap). 4000 iterations, prints the last set's len (2).
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let s = set.from_list([7, 7, 8])\n    \
              last = set.len(s)\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_set_core_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "2");
        }
    }

    #[test]
    fn self_hosted_bytes_read_f64_array_loop_reclaims() {
        // SOUNDNESS for the new List[Float] heap path: a bounded loop that allocates a fresh
        // List[Float] (read_f64_le_array) every iteration must RECLAIM each one (plain non-nested
        // drop) — no leak (would OOM) and no double-free (would trap). 4000 iterations runs to
        // completion and prints the last element's bits (1.0 = 4607182418800017408).
        let src = "fn main() -> Unit = {\n  \
            let b = bytes.from_list([0, 0, 0, 0, 0, 0, 240, 63])\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let a = bytes.read_f64_le_array(b, 0, 1)\n    \
              last = prim.load64(prim.handle(a) + 12)\n    \
              i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("self_hosted_bytes_read_f64_array_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "4607182418800017408");
        }
    }

    #[test]
    fn self_hosted_bytes_read_array_be_and_wide() {
        // SELF-HOSTED big-endian + i16/i64 array reads, each reusing its self-hosted scalar read.
        // u16_be [0,1,0,2]@0×2=[1,2] sum 3; i16_le [255,255]@0×1=-1 (negated 1);
        // i32_be [0,0,0,7]@0×1=[7]; i64_le [5,0,0,0,0,0,0,0]@0×1=[5]. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let b = bytes.from_list([0, 1, 0, 2])\n  let a = bytes.read_u16_be_array(b, 0, 2)\n  \
            println(int.to_string(list.sum(a)))\n  \
            let b2 = bytes.from_list([255, 255])\n  let a2 = bytes.read_i16_le_array(b2, 0, 1)\n  \
            let v = list.get_or(a2, 0, 0)\n  let nv = 0 - v\n  println(int.to_string(nv))\n  \
            let b3 = bytes.from_list([0, 0, 0, 7])\n  let a3 = bytes.read_i32_be_array(b3, 0, 1)\n  \
            println(int.to_string(list.get_or(a3, 0, 0)))\n  \
            let b4 = bytes.from_list([5, 0, 0, 0, 0, 0, 0, 0])\n  let a4 = bytes.read_i64_le_array(b4, 0, 1)\n  \
            println(int.to_string(list.get_or(a4, 0, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "bytes.read_u16_be_array"));
        if let Some(out) = build_and_run("self_hosted_bytes_read_array_be_and_wide", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n1\n7\n5");
        }
    }

    #[test]
    fn self_hosted_list_get_first_last_str() {
        // SELF-HOSTED list.get / list.first / list.last over a List[String] → Option[String] (the
        // repr-poly _str accessors). An in-bounds element is returned as Some(a deep copy); out of
        // bounds is None. get(["a","bb","ccc"],1)=Some("bb"); get(_,9)=None; first=Some("a"); last=
        // Some("ccc"). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let parts = string.split(\"a,bb,ccc\", \",\")\n  \
            let g1 = list.get(parts, 1)\n  match g1 {\n    Some(v) => println(v),\n    None => println(\"none\"),\n  }\n  \
            let g2 = list.get(parts, 9)\n  match g2 {\n    Some(v) => println(v),\n    None => println(\"none\"),\n  }\n  \
            let f = list.first(parts)\n  match f {\n    Some(v) => println(v),\n    None => println(\"none\"),\n  }\n  \
            let l = list.last(parts)\n  match l {\n    Some(v) => println(v),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.get_str"));
        assert!(prog.functions.iter().any(|f| f.name == "list.first_str"));
        assert!(prog.functions.iter().any(|f| f.name == "list.last_str"));
        if let Some(out) = build_and_run("self_hosted_list_get_first_last_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "bb\nnone\na\nccc");
        }
    }

    #[test]
    fn self_hosted_list_take_drop_str() {
        // SELF-HOSTED list.take / list.drop over a List[String] (the repr-poly _str variants). Same
        // clamping as the List[Int] version; each kept element is deep-copied. take(["a","b","c","d"],
        // 2)=["a","b"]; drop(_,2)=["c","d"]; drop(["a","b"],9) (n>len) = []. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let parts = string.split(\"a,b,c,d\", \",\")\n  \
            let t = list.take(parts, 2)\n  println(int.to_string(list.len(t)))\n  println(list.join(t, \"-\"))\n  \
            let d = list.drop(parts, 2)\n  println(int.to_string(list.len(d)))\n  println(list.join(d, \"-\"))\n  \
            let p2 = string.split(\"a,b\", \",\")\n  let dn = list.drop(p2, 9)\n  println(int.to_string(list.len(dn))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.take_str"));
        assert!(prog.functions.iter().any(|f| f.name == "list.drop_str"));
        if let Some(out) = build_and_run("self_hosted_list_take_drop_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\na-b\n2\nc-d\n0");
        }
    }

    #[test]
    fn self_hosted_list_reverse_str() {
        // SELF-HOSTED list.reverse over a List[String] (the repr-poly _str variant). Each element is
        // DEEP-COPIED into the mirrored slot. reverse(["a","b","c"])=["c","b","a"]; verified via
        // list.join + list.len. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let parts = string.split(\"a,b,c\", \",\")\n  \
            let rev = list.reverse(parts)\n  \
            println(int.to_string(list.len(rev)))\n  \
            println(list.join(rev, \"-\")) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.reverse_str"));
        if let Some(out) = build_and_run("self_hosted_list_reverse_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\nc-b-a");
        }
    }

    #[test]
    fn self_hosted_list_filter_str() {
        // SELF-HOSTED list.filter over a List[String] (the repr-poly _str variant). Single pass: keep
        // each element whose predicate is true, DEEP-COPYING it into the result; the len header is
        // patched to the match count. filter(["a","bb","c","dd"], len>1) = ["bb","dd"]; verified via
        // list.len + list.join. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let parts = string.split(\"a,bb,c,dd\", \",\")\n  \
            let kept = list.filter(parts, (x) => { let n = string.len(x)\n n > 1 })\n  \
            println(int.to_string(list.len(kept)))\n  \
            println(list.join(kept, \"-\")) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.filter_str"));
        if let Some(out) = build_and_run("self_hosted_list_filter_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\nbb-dd");
        }
    }

    #[test]
    fn self_hosted_list_map_str() {
        // SELF-HOSTED list.map over a List[String] (the repr-poly _str variant, auto-dispatched on a
        // List[heap] result). Each element is borrowed (prim.load_str), passed to the closure over
        // the heap-arg ABI, and the fresh result moved into a DynListStr. map(split"a,b,c", repeat·2)
        // = ["aa","bb","cc"], verified via list.join + list.len. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let parts = string.split(\"a,b,c\", \",\")\n  \
            let mapped = list.map(parts, (x) => string.repeat(x, 2))\n  \
            println(int.to_string(list.len(mapped)))\n  \
            println(list.join(mapped, \"-\")) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.map_str"));
        if let Some(out) = build_and_run("self_hosted_list_map_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\naa-bb-cc");
        }
    }

    #[test]
    fn list_map_str_loop_is_bounded() {
        // ADVERSARIAL leak/double-free guard: a loop mapping a List[String] through a closure each
        // iteration must run in BOUNDED memory — the input list + elements (borrowed, freed by their
        // own DropListStr), the closure's fresh element Strings and the result DynListStr are all
        // freed once. Short uniform strings keep every block one-slot (20 bytes) for free-list reuse.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 3000 {\n    \
              let parts = string.split(\"a,b\", \",\")\n    let m = list.map(parts, (x) => string.repeat(x, 1))\n    \
              let _l = list.len(m)\n    i = i + 1\n  }\n  \
            println(\"done\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("list_map_str_loop_bounded", &render_wasm_program(&prog)) {
            assert!(out.ends_with("done"), "loop must terminate (bounded memory)");
        }
    }

    #[test]
    fn self_hosted_list_windows() {
        // SELF-HOSTED list.windows / list.window — all CONTIGUOUS (overlapping) sub-slices of length
        // n (v0's xs.windows(n): n>len → [], else len-n+1 windows). windows([1,2,3,4],2)=[[1,2],[2,3],
        // [3,4]]: 3 windows, flatten len 6 sum 15. windows(_,5) over 2 elems = []. window is an alias.
        let src = "fn main() -> Unit = {\n  \
            let ws = list.windows([1, 2, 3, 4], 2)\n  println(int.to_string(list.len(ws)))\n  \
            let fl = list.flatten(ws)\n  println(int.to_string(list.len(fl)))\n  \
            println(int.to_string(list.sum(fl)))\n  \
            let empty = list.windows([1, 2], 5)\n  println(int.to_string(list.len(empty)))\n  \
            let wv = list.window([1, 2, 3], 2)\n  println(int.to_string(list.len(wv))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.windows"));
        assert!(prog.functions.iter().any(|f| f.name == "list.window"));
        if let Some(out) = build_and_run("self_hosted_list_windows", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n6\n15\n0\n2");
        }
    }

    #[test]
    fn self_hosted_list_chunk() {
        // SELF-HOSTED list.chunk(xs, n): split into consecutive chunks of n (last may be smaller),
        // a NESTED List[List[Int]] built via prim.alloc_list_str + per-chunk alloc_list. Verified by
        // list.len (chunk count) and list.flatten/sum (contents). chunk([1..5],2)=[[1,2],[3,4],[5]]:
        // 3 chunks, flatten len 5 sum 15 [2]=3 ; chunk([1..4],2)=2 chunks, flatten sum 10.
        let src = "fn main() -> Unit = {\n  \
            let cs = list.chunk([1, 2, 3, 4, 5], 2)\n  println(int.to_string(list.len(cs)))\n  \
            let fl = list.flatten(cs)\n  println(int.to_string(list.len(fl)))\n  \
            println(int.to_string(list.sum(fl)))\n  println(int.to_string(list.get_or(fl, 2, 0)))\n  \
            let ev = list.chunk([1, 2, 3, 4], 2)\n  println(int.to_string(list.len(ev)))\n  \
            let fe = list.flatten(ev)\n  println(int.to_string(list.sum(fe))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.chunk"));
        if let Some(out) = build_and_run("self_hosted_list_chunk", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n5\n15\n3\n2\n10");
        }
    }

    #[test]
    fn self_hosted_list_flatten() {
        // SELF-HOSTED list.flatten: List[List[Int]] -> List[Int], concatenating sublists. The nested
        // input is built via prim.alloc_list_str + prim.store_str (now generic over the heap element
        // type, here List[Int]). flatten([[1,2],[3,4,5]]) = [1,2,3,4,5]: len 5, sum 15, [0]=1, [4]=5.
        let src = "fn mk() -> List[List[Int]] = {\n  \
            let outer: List[List[Int]] = prim.alloc_list_str(2)\n  \
            let a = list.range(1, 3)\n  let b = list.range(3, 6)\n  \
            let oh = prim.handle(outer)\n  \
            prim.store_str(oh + 12, a)\n  prim.store_str(oh + 20, b)\n  outer\n}\n\
                   fn main() -> Unit = {\n  \
            let nested = mk()\n  let flat = list.flatten(nested)\n  \
            println(int.to_string(list.len(flat)))\n  \
            println(int.to_string(list.sum(flat)))\n  \
            println(int.to_string(list.get_or(flat, 0, 0)))\n  \
            println(int.to_string(list.get_or(flat, 4, 0))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.flatten"));
        if let Some(out) = build_and_run("self_hosted_list_flatten", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\n15\n1\n5");
        }
    }

    #[test]
    fn self_hosted_option_flatten() {
        // SELF-HOSTED option.flatten(o) = o.flatten(): Some(inner) → inner, None → None. The outer
        // Option[Option[Int]] is built via `Some(list.first/get(..))` (the new heap-`Some` materialize),
        // and flatten re-reads the inner's tag/value. flatten(Some(Some(5)))=Some(5); flatten(Some(
        // None))=None. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let inner = list.first([5, 6])\n  let oo = Some(inner)\n  let f1 = option.flatten(oo)\n  \
            match f1 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let inner2 = list.get([5], 9)\n  let oo2 = Some(inner2)\n  let f2 = option.flatten(oo2)\n  \
            match f2 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.flatten"));
        if let Some(out) = build_and_run("self_hosted_option_flatten", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\nnone");
        }
    }

    #[test]
    fn self_hosted_result_map_err() {
        // SELF-HOSTED result.map_err(r, f): Ok(x) → Ok(x), Err(e) → Err(f(e)). The closure maps the
        // Err message (HEAP String arg → HEAP String result), invoked ONLY on the Err arm over a
        // deep copy. map_err(Ok(5), repeat·2)=Ok(5); map_err(Err"oops", repeat·2)=Err("oopsoops").
        let src = "fn main() -> Unit = {\n  \
            let r1 = int.parse(\"5\")\n  let m1 = result.map_err(r1, (e) => string.repeat(e, 2))\n  \
            match m1 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  }\n  \
            let r2 = int.parse(\"abc\")\n  let m2 = result.map_err(r2, (e) => string.repeat(e, 2))\n  \
            match m2 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.map_err"));
        if let Some(out) = build_and_run("self_hosted_result_map_err", &render_wasm_program(&prog)) {
            // Ok(5) preserved; Err message "invalid digit found in string" repeated twice
            assert_eq!(out, "5\ninvalid digit found in stringinvalid digit found in string");
        }
    }

    #[test]
    fn result_map_err_loop_is_bounded() {
        // ADVERSARIAL leak guard: a loop mapping a short Err message through a closure and matching
        // the result must run in BOUNDED memory — the deep copy `e`, the closure's new String and
        // the Result blocks are freed each iteration (no leak / double-free of the borrowed input).
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 4000 {\n    \
              let er = option.to_result(list.get([5], 9), \"e\")\n    let m = result.map_err(er, (e) => string.repeat(e, 1))\n    \
              match m {\n      Ok(v) => println(\"k\"),\n      Err(e) => println(e),\n    }\n    \
              i = i + 1\n  }\n  \
            println(\"done\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("result_map_err_loop_bounded", &render_wasm_program(&prog)) {
            assert!(out.ends_with("done"), "loop must terminate (bounded memory)");
        }
    }

    #[test]
    fn self_hosted_result_unwrap_or_else() {
        // SELF-HOSTED result.unwrap_or_else(r, f): Ok(x) → x, Err(e) → f(e). The closure takes the
        // Err message (a HEAP String arg — the new uniform-i64 closure ABI) and is invoked ONLY on
        // the Err arm. unwrap_or_else(Ok(5), e=>len e)=5 (f not called); unwrap_or_else(Err"invalid
        // digit found in string", e=>len e)=29 (len of the message). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let r1 = int.parse(\"5\")\n  let v1 = result.unwrap_or_else(r1, (e) => string.len(e))\n  println(int.to_string(v1))\n  \
            let r2 = int.parse(\"abc\")\n  let v2 = result.unwrap_or_else(r2, (e) => string.len(e))\n  println(int.to_string(v2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.unwrap_or_else"));
        if let Some(out) = build_and_run("self_hosted_result_unwrap_or_else", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\n29");
        }
    }

    #[test]
    fn self_hosted_result_to_err_option() {
        // SELF-HOSTED result.to_err_option(r) = r.err(): Ok → None, Err(e) → Some(e). Builds an
        // Option[String] with the Err message DEEP-COPIED into Some (v0 clones it). Extracted via a
        // Some(e) heap match. to_err_option(Ok(5))=None; to_err_option(Err"...")=Some("..."). v0-match.
        let src = "fn main() -> Unit = {\n  \
            let r1 = int.parse(\"5\")\n  let o1 = result.to_err_option(r1)\n  \
            match o1 {\n    Some(e) => println(e),\n    None => println(\"none\"),\n  }\n  \
            let r2 = int.parse(\"abc\")\n  let o2 = result.to_err_option(r2)\n  \
            match o2 {\n    Some(e) => println(e),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.to_err_option"));
        if let Some(out) = build_and_run("self_hosted_result_to_err_option", &render_wasm_program(&prog)) {
            assert_eq!(out, "none\ninvalid digit found in string");
        }
    }

    #[test]
    fn result_to_err_option_loop_is_bounded() {
        // ADVERSARIAL leak guard: a loop turning a short-message Err into Some(copy) and matching it
        // must run in BOUNDED memory — all blocks are one-slot (20-byte). The copy is freed once.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 4000 {\n    \
              let er = option.to_result(list.get([5], 9), \"e\")\n    let o = result.to_err_option(er)\n    \
              match o {\n      Some(e) => println(e),\n      None => println(\"n\"),\n    }\n    \
              i = i + 1\n  }\n  \
            println(\"done\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("result_to_err_option_loop_bounded", &render_wasm_program(&prog)) {
            assert!(out.ends_with("done"), "loop must terminate (bounded memory)");
        }
    }

    #[test]
    fn self_hosted_result_flat_map() {
        // SELF-HOSTED result.flat_map(r, f) = r.and_then(f): Ok(x) → f(x) (a Result itself), Err(e)
        // → Err(e) (message preserved by deep copy). The closure RETURNS a Result (heap-result
        // CallIndirect), invoked ONLY on the Ok arm. flat_map(Ok(5), x=>Ok(x*2))=Ok(10); flat_map(
        // Ok(5), x=>Err"bad")=Err"bad"; flat_map(Err"...", f) keeps the message. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let r1 = int.parse(\"5\")\n  let m1 = result.flat_map(r1, (x) => Ok(x * 2))\n  \
            match m1 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  }\n  \
            let r2 = int.parse(\"5\")\n  let m2 = result.flat_map(r2, (x) => Err(\"bad\"))\n  \
            match m2 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  }\n  \
            let r3 = int.parse(\"abc\")\n  let m3 = result.flat_map(r3, (x) => Ok(x * 2))\n  \
            match m3 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.flat_map"));
        if let Some(out) = build_and_run("self_hosted_result_flat_map", &render_wasm_program(&prog)) {
            assert_eq!(out, "10\nbad\ninvalid digit found in string");
        }
    }

    #[test]
    fn self_hosted_result_map() {
        // SELF-HOSTED result.map(r, f) = r.map(f): Ok(x) → Ok(f(x)) (f applied ONLY on the Ok arm),
        // Err(e) → Err(e) (the message PRESERVED by deep copy). map(Ok(5), *10)=Ok(50); map(Err"...",
        // *10) keeps the message. Built on int.parse Results; the Err message extracted via match.
        let src = "fn main() -> Unit = {\n  \
            let r1 = int.parse(\"5\")\n  let m1 = result.map(r1, (x) => x * 10)\n  \
            match m1 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  }\n  \
            let r2 = int.parse(\"abc\")\n  let m2 = result.map(r2, (x) => x * 10)\n  \
            match m2 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "result.map"));
        if let Some(out) = build_and_run("self_hosted_result_map", &render_wasm_program(&prog)) {
            assert_eq!(out, "50\ninvalid digit found in string");
        }
    }

    #[test]
    fn result_map_loop_is_bounded() {
        // ADVERSARIAL leak guard: a loop mapping an Err Result (deep-copying the short message each
        // iteration) and matching it must run in BOUNDED memory — input Result, the mapped Result
        // and the one-slot message copy are all 20-byte. The copy is freed once (no double-free).
        // A short-message Err is built via option.to_result(None, "e") (int.parse messages are long,
        // which would hit the head-only free-list's mixed-size fragmentation — a separate property).
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 4000 {\n    \
              let o = list.get([5], 9)\n    let r = option.to_result(o, \"e\")\n    let m = result.map(r, (x) => x + 1)\n    \
              match m {\n      Ok(v) => println(\"k\"),\n      Err(e) => println(e),\n    }\n    \
              i = i + 1\n  }\n  \
            println(\"done\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("result_map_loop_bounded", &render_wasm_program(&prog)) {
            assert!(out.ends_with("done"), "loop must terminate (bounded memory)");
        }
    }

    #[test]
    fn self_hosted_option_to_result() {
        // SELF-HOSTED option.to_result(o, msg) = o.ok_or(msg.to_string()): Some(x) → Ok(x), None →
        // Err(a fresh COPY of msg). v0 copies the message, so the borrowed msg is deep-copied (not
        // moved) into the Err — no double-free. The Err message is extracted via match and printed,
        // byte-matching. to_result(Some(5),"missing")=Ok(5); to_result(None,"missing")=Err("missing").
        let src = "fn main() -> Unit = {\n  \
            let o1 = list.first([5, 6])\n  let r1 = option.to_result(o1, \"missing\")\n  \
            match r1 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  }\n  \
            let o2 = list.get([5], 9)\n  let r2 = option.to_result(o2, \"missing\")\n  \
            match r2 {\n    Ok(v) => println(int.to_string(v)),\n    Err(e) => println(e),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.to_result"));
        if let Some(out) = build_and_run("self_hosted_option_to_result", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\nmissing");
        }
    }

    #[test]
    fn option_to_result_loop_is_bounded() {
        // ADVERSARIAL leak guard: a loop building Err(copy-of-msg) Results (each a fresh deep copy)
        // and matching them must run in BOUNDED memory — the input None, the Err Result and the
        // one-slot msg copy are all 20-byte so the head-only free-list reuses them. The deep copy is
        // freed by the Result's scope-end DropListStr exactly once (no double-free with the borrowed msg).
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 4000 {\n    \
              let o = list.get([5], 9)\n    let r = option.to_result(o, \"e\")\n    \
              match r {\n      Ok(v) => println(\"k\"),\n      Err(e) => println(e),\n    }\n    \
              i = i + 1\n  }\n  \
            println(\"done\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("option_to_result_loop_bounded", &render_wasm_program(&prog)) {
            assert!(out.ends_with("done"), "loop must terminate (bounded memory)");
        }
    }

    #[test]
    fn self_hosted_option_or_else() {
        // SELF-HOSTED option.or_else(o, f): Some(x) → Some(x) (kept), None → f() (a 0-arg thunk
        // returning an Option, invoked ONLY on the None arm via a heap-result CallIndirect).
        // or_else(Some(5), ()=>Some(99))=Some(5) ; or_else(None, ()=>Some(99))=Some(99) ; or_else(
        // None, ()=>None)=None. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let o1 = list.first([5, 6])\n  let m1 = option.or_else(o1, () => Some(99))\n  \
            match m1 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let o2 = list.get([5], 9)\n  let m2 = option.or_else(o2, () => Some(99))\n  \
            match m2 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let o3 = list.get([5], 9)\n  let m3 = option.or_else(o3, () => None)\n  \
            match m3 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.or_else"));
        if let Some(out) = build_and_run("self_hosted_option_or_else", &render_wasm_program(&prog)) {
            // or_else(Some(5),..)=5 ; or_else(None,()=>Some(99))=99 ; or_else(None,()=>None)=none
            assert_eq!(out, "5\n99\nnone");
        }
    }

    #[test]
    fn self_hosted_option_unwrap_or_else() {
        // SELF-HOSTED option.unwrap_or_else(o, f): Some(x) → x, None → f() (a 0-arg thunk invoked via
        // CallIndirect ONLY on the None arm). unwrap_or_else(Some(5), ()=>99)=5 ; unwrap_or_else(None,
        // ()=>99)=99. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let o1 = list.first([5, 6])\n  let v1 = option.unwrap_or_else(o1, () => 99)\n  println(int.to_string(v1))\n  \
            let o2 = list.get([5], 9)\n  let v2 = option.unwrap_or_else(o2, () => 99)\n  println(int.to_string(v2)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.unwrap_or_else"));
        if let Some(out) = build_and_run("self_hosted_option_unwrap_or_else", &render_wasm_program(&prog)) {
            assert_eq!(out, "5\n99");
        }
    }

    #[test]
    fn self_hosted_list_with_capacity() {
        // SELF-HOSTED list.with_capacity(cap) -> an EMPTY list (v0's Vec::with_capacity has len 0;
        // the reserved capacity is not observable). with_capacity(5)/with_capacity(0) both have len
        // 0 and sum 0. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let a = list.with_capacity(5)\n  println(int.to_string(list.len(a)))\n  \
            println(int.to_string(list.sum(a)))\n  \
            let b = list.with_capacity(0)\n  println(int.to_string(list.len(b))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.with_capacity"));
        if let Some(out) = build_and_run("self_hosted_list_with_capacity", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\n0\n0");
        }
    }

    #[test]
    fn self_hosted_option_flat_map() {
        // SELF-HOSTED option.flat_map(o, f) = o.and_then(f): Some(x) → f(x) (an Option itself),
        // None → None. The closure RETURNS an Option (heap-result CallIndirect), invoked ONLY on the
        // Some arm. flat_map(Some(5), x=>Some(x*2))=Some(10); flat_map(Some(5), x=>None)=None;
        // flat_map(None, x=>Some(x*2))=None. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let o1 = list.first([5, 6])\n  let m1 = option.flat_map(o1, (x) => Some(x * 2))\n  \
            match m1 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let o2 = list.first([5, 6])\n  let m2 = option.flat_map(o2, (x) => None)\n  \
            match m2 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let o3 = list.get([5], 9)\n  let m3 = option.flat_map(o3, (x) => Some(x * 2))\n  \
            match m3 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.flat_map"));
        if let Some(out) = build_and_run("self_hosted_option_flat_map", &render_wasm_program(&prog)) {
            // flat_map(Some(5),x=>Some(x*2))=10 ; flat_map(Some(5),x=>None)=none ; flat_map(None,..)=none
            assert_eq!(out, "10\nnone\nnone");
        }
    }

    #[test]
    fn option_flat_map_loop_is_bounded() {
        // ADVERSARIAL leak guard: a loop flat-mapping an Option through a heap-result closure (which
        // allocates a fresh inner Option each Some iteration) and matching it must run in BOUNDED
        // memory — the input Option, the closure's fresh Option and the "k" string are all one-slot.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 4000 {\n    \
              let o = list.first([5, 6])\n    let m = option.flat_map(o, (x) => Some(x + 1))\n    \
              match m {\n      Some(v) => println(\"k\"),\n      None => println(\"n\"),\n    }\n    \
              i = i + 1\n  }\n  \
            println(\"done\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("option_flat_map_loop_bounded", &render_wasm_program(&prog)) {
            assert!(out.ends_with("done"), "loop must terminate (bounded memory)");
        }
    }

    #[test]
    fn self_hosted_option_filter() {
        // SELF-HOSTED option.filter(o, pred) = o.filter(pred): keep Some(x) iff pred(x), else None;
        // pred invoked (CallIndirect) ONLY on the Some arm. filter(Some(5), >3)=Some(5);
        // filter(Some(2), >3)=None; filter(None, >3)=None. Byte-matches v0.
        let src = "fn main() -> Unit = {\n  \
            let o1 = list.first([5, 6])\n  let m1 = option.filter(o1, (x) => x > 3)\n  \
            match m1 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let o2 = list.first([2, 9])\n  let m2 = option.filter(o2, (x) => x > 3)\n  \
            match m2 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  }\n  \
            let o3 = list.get([5], 9)\n  let m3 = option.filter(o3, (x) => x > 3)\n  \
            match m3 {\n    Some(v) => println(int.to_string(v)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.filter"));
        if let Some(out) = build_and_run("self_hosted_option_filter", &render_wasm_program(&prog)) {
            // filter(Some(5),>3)=5 ; filter(Some(2),>3)=none ; filter(None,>3)=none
            assert_eq!(out, "5\nnone\nnone");
        }
    }

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

