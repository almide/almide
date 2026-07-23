
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
        // The BLOCK-bodied lambda now DEFUNCTIONALIZES (inlined as a specialized loop —
        // the scalar-block body arm); the self-host link is only needed when the defunc
        // declines. Either way the OUTPUT is the claim.
        if let Some(out) = build_and_run("self_hosted_list_filter_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\nbb-dd");
        }
    }

    #[test]
    fn self_hosted_list_map_str() {
        // list.map over a List[String] with an INLINE lambda — now DEFUNCTIONALIZED as a specialized
        // loop (the heap-element extension of #67): the element is read by LoadHandle, the body
        // (`string.repeat(x, 2)`) lowers per element to a fresh owned String moved into a DynListStr.
        // map(split"a,b,c", repeat·2) = ["aa","bb","cc"], verified via list.join + list.len; byte-v0.
        let src = "fn main() -> Unit = {\n  \
            let parts = string.split(\"a,b,c\", \",\")\n  \
            let mapped = list.map(parts, (x) => string.repeat(x, 2))\n  \
            println(int.to_string(list.len(mapped)))\n  \
            println(list.join(mapped, \"-\")) }\n";
        let prog = lower_source(src);
        // The closure was specialized away (inlined) — NO `list.map_str` lift function. This is the
        // preferred path AND avoids the lift's nested-map silent miscompile (csv `stringify`).
        assert!(prog.functions.iter().all(|f| f.name != "list.map_str"));
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

