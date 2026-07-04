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

include!("tests_part4_b.rs");
include!("tests_part4_c.rs");
include!("tests_part4_d.rs");
include!("tests_part4_e.rs");
