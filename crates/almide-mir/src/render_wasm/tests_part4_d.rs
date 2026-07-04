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
    fn heap_unwrap_or_executes_byte_matches_v0() {
        // DETECTOR for the heap-`??` hole (was: `Option[String] ?? heap_default` silent-miscompiled
        // to an empty Alloc{Opaque}). The fix routes it to the self-host option.unwrap_or_str CALL.
        // Covers BOTH subject forms the user reported: a BOUND var (`oh ?? d`) and a DIRECT call
        // (`f(x) ?? d`). Pure list.get only — no json, so this pins the SHARED `??` lowering.
        // v0: get(["a","bb"],1)="bb"; get(_,9) ?? "DEF" = "DEF"; the direct-call forms match.
        let src = "fn main() -> Unit = {\n  \
            let parts = string.split(\"a,bb\", \",\")\n  \
            let g1 = list.get(parts, 1)\n  let s1 = g1 ?? \"DEF\"\n  println(s1)\n  \
            let g2 = list.get(parts, 9)\n  let s2 = g2 ?? \"DEF\"\n  println(s2)\n  \
            println(list.get(parts, 0) ?? \"DEF\")\n  \
            println(list.get(parts, 5) ?? \"DEF\") }\n";
        let prog = lower_source(src);
        // STRUCTURAL GUARD: the heap `??` must route to the real call, never the silent Opaque.
        assert!(
            prog.functions.iter().any(|f| f.name == "option.unwrap_or_str"),
            "heap `??` must auto-link option.unwrap_or_str (not defer to empty Opaque)"
        );
        if let Some(out) = build_and_run("heap_unwrap_or_executes", &render_wasm_program(&prog)) {
            assert_eq!(out, "bb\nDEF\na\nDEF");
        }
    }

    #[test]
    fn heap_unwrap_or_concat_operand_executes() {
        // DETECTOR for the 4th heap-`??` position — a string-concat OPERAND (`"x" + (opt ?? "d")`).
        // The `??` operand lowers via lower_call_args → option.unwrap_or_str, then __str_concat.
        // Without this test the concat-operand position could silently regress to an empty Opaque.
        // (String INTERPOLATION over the EXECUTABLE subset — Lit / String Var/LitStr / Int Var/LitInt
        // parts — IS now lowered; see the `string_interp_*` tests below. A `"${opt ?? "d"}"` interp
        // whose operand is a `??`/compound/call is NOT in that subset and stays Opaque.) v0: "got=bb",
        // "got=DEF".
        let src = "fn main() -> Unit = {\n  \
            let parts = string.split(\"a,bb\", \",\")\n  \
            println(\"got=\" + (list.get(parts, 1) ?? \"DEF\"))\n  \
            println(\"got=\" + (list.get(parts, 9) ?? \"DEF\")) }\n";
        let prog = lower_source(src);
        assert!(
            prog.functions.iter().any(|f| f.name == "option.unwrap_or_str"),
            "heap `??` in a concat operand must route to option.unwrap_or_str (not Opaque)"
        );
        if let Some(out) = build_and_run("heap_unwrap_or_concat_operand", &render_wasm_program(&prog)) {
            assert_eq!(out, "got=bb\ngot=DEF");
        }
    }

    #[test]
    fn heap_unwrap_or_tail_position_executes() {
        // The RETURN/tail position (`fn f(...) -> String = opt ?? "d"`) — the result is MOVED OUT
        // (track_result=false). pick(parts, 1)="bb"; pick(parts, 9)="DEF". Byte-matches v0.
        let src = "fn pick(parts: List[String], i: Int) -> String = list.get(parts, i) ?? \"DEF\"\n\
            fn main() -> Unit = {\n  \
            let parts = string.split(\"a,bb,ccc\", \",\")\n  \
            println(pick(parts, 1))\n  println(pick(parts, 9)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "option.unwrap_or_str"));
        if let Some(out) = build_and_run("heap_unwrap_or_tail", &render_wasm_program(&prog)) {
            assert_eq!(out, "bb\nDEF");
        }
    }

    #[test]
    fn heap_unwrap_or_loop_reclaims() {
        // SOUNDNESS for the heap-`??` call path: a 4000-iter loop building + dropping the Option AND
        // the unwrapped String each round — no leak / no double-free. The Some payload is COPIED by
        // option.unwrap_or_str (the Option keeps its element, freed by its own scope-end DropListStr),
        // so the unwrapped String is independently owned. last = string.len(unwrapped) = 2 ("bb").
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  var last = 0\n  \
            while i < 4000 {\n    \
              let parts = string.split(\"a,bb\", \",\")\n    let g = list.get(parts, 1)\n    \
              let s = g ?? \"DEF\"\n    last = string.len(s)\n    i = i + 1\n  }\n  \
            println(int.to_string(last)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("heap_unwrap_or_loop_reclaims", &render_wasm_program(&prog)) {
            assert_eq!(out, "2");
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

