// render_wasm test suite — part 4 of 4 (self-hosted stdlib e2e, continued).
// Textually included by render_wasm/tests.rs (one module: helpers/tests share scope).

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
