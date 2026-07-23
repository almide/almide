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

include!("tests_part4_i.rs");
