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
