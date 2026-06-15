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
