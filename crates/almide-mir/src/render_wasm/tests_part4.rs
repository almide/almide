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
        // C1 DEFUNCTIONALIZED `list.map` over an INLINE lambda. list.map([1,2,3,4], (x) => x
        // * x) builds [1,4,9,16] via a SPECIALIZED loop at the call site — a fresh List[Int],
        // each slot filled with the INLINED body `x * x` (no runtime closure, no CallIndirect,
        // no auto-linked `list.map` combinator). Byte-matches v0 (sum + a sampled element
        // confirm the contents).
        let src = "fn main() -> Unit = {\n  \
            let ys = list.map([1, 2, 3, 4], (x) => x * x)\n  \
            let s = int.to_string(list.sum(ys))\n  println(s)\n  \
            let e = int.to_string(list.get_or(ys, 3, 0))\n  println(e) }\n";
        let prog = lower_source(src);
        // The inline lambda is defunctionalized away — no `list.map` combinator CallFn (so it
        // is NOT auto-linked) and no lifted `__lambda_*` aux.
        assert!(
            !prog.functions.iter().any(|f| f.name == "list.map"),
            "list.map is inlined, NOT auto-linked as a combinator"
        );
        assert!(
            !prog.functions.iter().any(|f| f.name.starts_with("__lambda_")),
            "the inline lambda is defunctionalized, not lifted"
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
        // C1: the inline predicate is defunctionalized — `list.filter` is inlined as a loop
        // (over-allocate, pack matches, patch len), NOT auto-linked as a combinator.
        assert!(
            !prog.functions.iter().any(|f| f.name == "list.filter"),
            "list.filter is inlined, NOT auto-linked as a combinator"
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
        // C1: the inline 2-arity closure is defunctionalized — `list.fold` is inlined as a
        // loop with a stable accumulator local, NOT auto-linked as a combinator.
        assert!(
            !prog.functions.iter().any(|f| f.name == "list.fold"),
            "list.fold is inlined, NOT auto-linked as a combinator"
        );
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
        // C1: the inline closure is defunctionalized — `list.find` is inlined as an
        // early-exit loop (try_lower_defunc_find), NOT auto-linked as a combinator.
        assert!(
            !prog.functions.iter().any(|f| f.name == "list.find"),
            "list.find is inlined, NOT auto-linked as a combinator"
        );
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
        // The cached-keys desugar (C-055) rewrites `list.sort_by(xs, f)` into
        // `list.sort_by_keys(xs, list.map(xs, f))` — the closure-free sort self-host
        // links instead of the old HOF `list.sort_by` body.
        assert!(prog.functions.iter().any(|f| f.name == "list.sort_by_keys"));
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

include!("tests_part4_f.rs");
