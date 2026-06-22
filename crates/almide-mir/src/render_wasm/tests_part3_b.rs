    #[test]
    fn custom_variant_scalar_match_executes_on_wasmtime() {
        // A custom ADT `Tok = Num(Int) | Sym(Int) | Eof` end-to-end through v1 (ADT bricks 2+3):
        // ctor construct (the tagged value-model block — tag@slot0 + scalar field slot) in BOTH
        // arg (`val(Num(7))`) and let (`let t = Num(9)`) positions, and an N-arm tag-dispatch
        // `match` → scalar result. Byte-matches v0 (7 / 40 / -1 / 9). Scalar fields only ⇒ the
        // block frees flat, no `$__drop_value`.
        let src = "type Tok = Num(Int) | Sym(Int) | Eof\n\
            fn val(t: Tok) -> Int = match t { Num(n) => n, Sym(s) => s * 10, Eof => -1 }\n\
            fn main() -> Unit = {\n  \
              println(int.to_string(val(Num(7))))\n  \
              println(int.to_string(val(Sym(4))))\n  \
              println(int.to_string(val(Eof)))\n  \
              let t = Num(9)\n  println(int.to_string(val(t))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "val"));
        if let Some(out) = build_and_run("custom_variant", &render_wasm_program(&prog)) {
            assert_eq!(out, "7\n40\n-1\n9");
        }
    }

    #[test]
    fn custom_variant_heap_result_match_executes_on_wasmtime() {
        // A HEAP (String) result custom-variant `match` over a BORROWED param subject (ADT
        // brick 4) — each arm moves out a fresh String; the bound scalar field `n` is read from
        // the borrowed subject's slot. The shape of recursive `to_string` minus the recursion.
        // Byte-matches v0.
        let src = "type Tok = Num(Int) | Sym(Int) | Eof\n\
            fn name(t: Tok) -> String = match t {\n  \
              Num(n) => \"num:\" + int.to_string(n),\n  \
              Sym(s) => \"sym\",\n  \
              Eof    => \"eof\",\n}\n\
            fn main() -> Unit = { println(name(Num(7))); println(name(Sym(2))); println(name(Eof)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("custom_variant_heap", &render_wasm_program(&prog)) {
            assert_eq!(out, "num:7\nsym\neof");
        }
    }

    #[test]
    fn custom_variant_string_field_construct_drops_clean() {
        // A custom ADT with a LEAF heap (`String`) ctor field (`Text(String)`): construct moves
        // the String into the masked slot (ADT brick 5a), the block's scope-end drop frees that
        // slot (the String-field record's DropListStr machinery) — verified by a 1000x
        // construct+drop loop that must not leak/trap. The field is matched with a WILDCARD (the
        // heap-field BIND is a later brick); byte-matches v0.
        let src = "type Msg = Text(String) | Code(Int) | Quit\n\
            fn tag(m: Msg) -> Int = match m { Text(_) => 1, Code(c) => c, Quit => 0 }\n\
            fn main() -> Unit = {\n  \
              var t = 0\n  for i in 0..1000 { t = t + tag(Text(\"xyz\")) }\n  \
              println(int.to_string(t))\n  \
              println(int.to_string(tag(Code(7))))\n  \
              println(int.to_string(tag(Quit))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("custom_variant_strfield", &render_wasm_program(&prog)) {
            assert_eq!(out, "1000\n7\n0");
        }
    }

    #[test]
    fn custom_variant_heap_field_bind_executes_on_wasmtime() {
        // A multi-arm custom-variant `match` that BINDS a leaf-heap (`String`) ctor field (ADT
        // brick 5c): `Text(s) => s` moves it out (auto-`Dup` in lower_heap_result_arm),
        // `string.len(s)` reads it (borrow). The subject keeps ownership (its masked drop frees
        // the slot); a 1000x construct+match+drop loop must not leak. Byte-matches v0. (A
        // SINGLE-arm heap match — a 1-ctor newtype — is walled: its direct-to-ret double-move.)
        let src = "type Msg = Text(String) | Code(Int) | Quit\n\
            fn name(m: Msg) -> String = match m { Text(s) => s, Code(c) => \"code\", Quit => \"quit\" }\n\
            fn weight(m: Msg) -> Int = match m { Text(s) => string.len(s), Code(c) => c, Quit => 0 }\n\
            fn main() -> Unit = {\n  \
              println(name(Text(\"hi\")))\n  println(name(Code(7)))\n  println(name(Quit))\n  \
              var n = 0\n  for i in 0..1000 { n = n + weight(Text(\"abc\")) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("custom_variant_heapfield", &render_wasm_program(&prog)) {
            assert_eq!(out, "hi\ncode\nquit\n3000");
        }
    }

    #[test]
    fn value_get_and_as_array_unwrap_execute_on_wasmtime() {
        // THE LAYOUT BRICK read side: a heap-Result-of-Value (`value.get` → Result[Value,String]) and
        // a heap-Result-of-List (`value.as_array` → Result[List[Value],String]) round-trip through
        // BOTH a `match` (tag@16 read, @12 payload bound as a borrow) AND a `??` (routed to the
        // self-hosted result.value_unwrap_or / result.list_value_unwrap_or, the Ok arm Dup'ing @12).
        // The Err message is the byte-exact "missing field '<k>'". 2000x is the leak gate.
        let src = "import json\n\
            effect fn main() -> Unit = {\n  \
              let o = value.object([(\"a\", value.int(7)), (\"b\", value.str(\"hi\"))])\n  \
              match value.get(o, \"a\") { ok(v) => println(int.to_string(value.as_int(v) ?? 0)), err(e) => println(\"e:\" + e) }\n  \
              match value.get(o, \"zzz\") { ok(v) => println(\"got\"), err(e) => println(e) }\n  \
              let g = value.get(o, \"b\") ?? value.null()\n  println(value.stringify(g))\n  \
              let arr = value.array([value.int(10), value.int(20), value.int(30)])\n  \
              let items = value.as_array(arr) ?? []\n  \
              var s = 0\n  for it in items { s = s + (value.as_int(it) ?? 0) }\n  println(int.to_string(s))\n  \
              var n = 0\n  for i in 0..2000 { let oo = value.object([(\"k\", value.int(i))]); let gg = value.get(oo, \"k\") ?? value.null(); n = n + (value.as_int(gg) ?? 0) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "value.get"));
        if let Some(out) = build_and_run("value_get_unwrap", &render_wasm_program(&prog)) {
            assert_eq!(out, "7\nmissing field 'zzz'\n\"hi\"\n60\n1999000");
        }
    }

    #[test]
    fn parse_rows_rec_destructure_mutual_recursion_double_free() {
        // REGRESSION GUARD for the csv parse double-free, now FIXED by the TCO result accumulator: a
        // recursive `prr` whose inner-if has a SELF-recursive THEN (`prr(…)`) AND a sibling ELSE that
        // destructures an owned tuple (`let (field, np) = pf(…)`) then uses the borrowed `field` in
        // `cur + [field]`. Before the fix this TRAPPED — the TCO computed the `paf(…, cur+[field])`
        // base case in the POST-LOOP dispatch, where the loop-body-local `field` was already dead. The
        // fix carries such a base out through a result accumulator computed IN the loop (where `field`
        // is live), via the loop-carried heap slot generalized to any fresh-owned producer.
        let src = "fn pf(text: String, pos: Int, acc: String) -> (String, Int) =\n  \
              if pos >= string.len(text) then (acc, pos)\n  \
              else { let c = string.get(text, pos) ?? \"\"\n         if c == \",\" then (acc, pos) else pf(text, pos + 1, acc + c) }\n\
            fn paf(text: String, pos: Int, rows: List[List[String]], cur: List[String]) -> List[List[String]] =\n  \
              if pos >= string.len(text) then rows + [cur]\n  \
              else { let c = string.get(text, pos) ?? \"\"\n         if c == \",\" then prr(text, pos + 1, rows, cur) else prr(text, pos, rows, cur) }\n\
            fn prr(text: String, pos: Int, rows: List[List[String]], cur: List[String]) -> List[List[String]] =\n  \
              if pos >= string.len(text) then rows + [cur]\n  \
              else { let c = string.get(text, pos) ?? \"\"\n         if c == \",\" then prr(text, pos + 1, rows, cur + [\"\"]) else { let (field, np) = pf(text, pos, \"\"); paf(text, np, rows, cur + [field]) } }\n\
            fn main() -> Unit = { let r = prr(\"a,b,c\", 0, [], []); for rr in r { println(int.to_string(list.len(rr))) } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("parse_rows_rec_df", &render_wasm_program(&prog)) {
            assert_eq!(out, "3");
        }
    }

    #[test]
    fn enumerate_map_fusion_to_object_executes_on_wasmtime() {
        // parse_records' core: `header |> list.enumerate |> list.map((entry) => { let (i,key)=entry;
        // (key, value.str(list.get_or(row, i, ""))) })` → value.object. The enumerate+map FUSION
        // avoids the (Int,String) intermediate (bind i=loop-index, key=element); the body returns a
        // (String,Value) tuple (Tuple arm + str_value_elem_lists recursive drop). The OUTER
        // Value-element result (`… |> list.map(r => value.object(…))` → List[Value], value_elem_lists)
        // is exercised via a value.as_array-derived row list. 2000x is the leak gate.
        let src = "import json\n\
            effect fn main() -> Unit = {\n  \
              let row = [\"x\", \"y\"]\n  let header = [\"a\", \"b\"]\n  \
              let pairs = header |> list.enumerate |> list.map((entry) => { let (i, key) = entry; (key, value.str(list.get_or(row, i, \"\"))) })\n  \
              println(value.stringify(value.object(pairs)))\n  \
              let strs = [\"p\", \"q\"]\n  \
              let objs = strs |> list.map((s) => value.object([(s, value.int(1))]))\n  \
              println(value.stringify(value.array(objs)))\n  \
              var n = 0\n  for j in 0..2000 { let p = header |> list.enumerate |> list.map((entry) => { let (i, key) = entry; (key, value.str(list.get_or(row, i, \"\"))) }); n = n + string.len(value.stringify(value.object(p))) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("enumerate_map_fusion", &render_wasm_program(&prog)) {
            assert_eq!(out, "{\"a\":\"x\",\"b\":\"y\"}\n[{\"p\":1},{\"q\":1}]\n34000");
        }
    }

    #[test]
    fn capturing_heap_map_over_value_executes_on_wasmtime() {
        // map-closure-over-Value: a CAPTURING list.map closure over a HEAP-element list, inlined as a
        // specialized loop (defunctionalization extended to heap source + heap result). The closure
        // CAPTURES `obj`/`row` (resolved through value_of) and calls value.get/value.as_string/`??` on
        // it — the lift path can't represent that env. Exercises the csv stringify_records shape: an
        // OUTER map over List[Value] whose body is an INNER map over List[String] (capturing the row)
        // + list.join, with value.get + value.as_string ?? "" + escape_cell-style quoting. 2000x leaks.
        let src = "import json\n\
            fn escape_cell(s: String) -> String = if string.contains(s, \",\") then \"\\\"\" + s + \"\\\"\" else s\n\
            fn rows_to_csv(v: Value) -> String = {\n  \
              let rows = value.as_array(v) ?? []\n  \
              let header = [\"name\", \"city\"]\n  \
              let lines = rows |> list.map((row) =>\n    \
                header |> list.map((h) => escape_cell(value.as_string(value.get(row, h) ?? value.null()) ?? \"\")) |> list.join(\",\"))\n  \
              lines |> list.join(\"\\n\")\n }\n\
            effect fn main() -> Unit = {\n  \
              let recs = value.array([\n    \
                value.object([(\"name\", value.str(\"alice\")), (\"city\", value.str(\"nyc\"))]),\n    \
                value.object([(\"name\", value.str(\"bob\")), (\"city\", value.str(\"LA, CA\"))])\n  \
              ])\n  \
              println(rows_to_csv(recs))\n  \
              var n = 0\n  for i in 0..2000 { let o = value.object([(\"k\", value.str(\"x\"))]); let cs = [\"k\"] |> list.map((h) => value.as_string(value.get(o, h) ?? value.null()) ?? \"\"); n = n + string.len(cs |> list.join(\",\")) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("capturing_heap_map", &render_wasm_program(&prog)) {
            assert_eq!(out, "alice,nyc\nbob,\"LA, CA\"\n2000");
        }
    }

    #[test]
    fn nested_heap_list_get_drop_over_list_of_list_string() {
        // list.get / list.drop over a List[List[String]] — the csv.parse_records shape: a recursive
        // parser builds `rows` (a List of inner String-lists), then `header = get(rows,0)`, `data =
        // drop(rows,1)`. The element is itself a heap list — the `_str` variants would deep-copy it via
        // string.repeat (reading the inner list's length word as a byte count: a silent miscompile for
        // get, a double-free trap for drop). The `_liststr` accessors SHARE each inner list by handle
        // (rc_inc + raw store64, like __varr_copy); co-owned, freed once at the last ref (leak-verified
        // separately at 100000×). `rows` is built by a CALL (a list-of-lists literal is a separate gap).
        let src = "fn pfield(text: String, pos: Int, acc: String) -> (String, Int) =\n  \
              if pos >= string.len(text) then (acc, pos)\n  \
              else { let c = string.get(text, pos) ?? \"\"\n         if c == \",\" or c == \"\\n\" then (acc, pos) else pfield(text, pos + 1, acc + c) }\n\
            fn pafter(text: String, pos: Int, rows: List[List[String]], cur: List[String]) -> List[List[String]] =\n  \
              if pos >= string.len(text) then rows + [cur]\n  \
              else { let c = string.get(text, pos) ?? \"\"\n         if c == \",\" then prows(text, pos + 1, rows, cur) else if c == \"\\n\" then prows(text, pos + 1, rows + [cur], []) else prows(text, pos, rows, cur) }\n\
            fn prows(text: String, pos: Int, rows: List[List[String]], cur: List[String]) -> List[List[String]] =\n  \
              if pos >= string.len(text) then rows + [cur]\n  \
              else { let c = string.get(text, pos) ?? \"\"\n         if c == \",\" then prows(text, pos + 1, rows, cur + [\"\"]) else if c == \"\\n\" then prows(text, pos + 1, rows + [cur], []) else { let (f, np) = pfield(text, pos, \"\"); pafter(text, np, rows, cur + [f]) } }\n\
            fn pick(rows: List[List[String]]) -> Int = {\n  \
              let header = list.get(rows, 0) ?? []\n  \
              let data = list.drop(rows, 1)\n  \
              list.len(header) + list.len(data) }\n\
            effect fn main() -> Unit = {\n  \
              let rows = prows(\"a,b\\nc\\nd,e,f\", 0, [], [])\n  \
              println(int.to_string(pick(rows))) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.get_liststr"));
        assert!(prog.functions.iter().any(|f| f.name == "list.drop_liststr"));
        if let Some(out) = build_and_run("nested_heap_list_get_drop", &render_wasm_program(&prog)) {
            // rows = [[a,b],[c],[d,e,f]] → len(get(rows,0)=[a,b]) + len(drop(rows,1)=[[c],[d,e,f]]) = 2+2 = 4
            assert_eq!(out, "4");
        }
    }

    #[test]
    fn record_call_result_field_read_and_spread_return() {
        // A record returned from a CALL (`let p = bump(mk(5))`) must be tracked as a materialized
        // aggregate so a heap-field read (`p.y`) loads the real slot — not the container-grain Dup that
        // returned the whole record (the `mk(5).y` empty-string miscompile). And a SPREAD record
        // RETURNED from a fn (`fn bump(p) = { ...p, x: p.x + 1 }`, the svg element-builder shape) builds
        // + moves out a fresh same-layout block (scalar Load + heap Dup of the un-overridden fields).
        let src = "type P = { x: Int, y: String }\n\
            fn mk(n: Int) -> P = P { x: n, y: \"hi\" }\n\
            fn bump(p: P) -> P = { ...p, x: p.x + 1 }\n\
            effect fn main() -> Unit = {\n  \
              let p = bump(mk(5))\n  \
              println(int.to_string(p.x))\n  \
              println(p.y) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("record_call_spread", &render_wasm_program(&prog)) {
            assert_eq!(out, "6\nhi");
        }
    }

    #[test]
    fn record_list_concat_in_spread_override_leak_free() {
        // The svg `add_child` shape: `{ ...parent, kids: parent.kids + [child] }` — a List[Record]
        // CONCAT as a spread-override. lower_owned_heap_field now has a ConcatList arm; try_lower_concat_list
        // rc-incs each record (`__list_concat_rc`) + routes the result to `$__drop_list_<R>`. Was walled
        // "heap-result SpreadRecord". The 10000x loop is the leak gate.
        let src = "type E = { tag: String, kids: List[E] }\n\
            fn leaf(t: String) -> E = E { tag: t, kids: [] }\n\
            fn addk(p: E, c: E) -> E = { ...p, kids: p.kids + [c] }\n\
            effect fn main() -> Unit = {\n  \
              let root = addk(addk(leaf(\"r\"), leaf(\"a\")), leaf(\"b\"))\n  \
              println(int.to_string(list.len(root.kids)))\n  \
              var n = 0\n  \
              for i in 0..10000 { let r = addk(addk(leaf(\"r\"), leaf(\"a\")), leaf(\"b\")); n = n + list.len(r.kids) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("record_list_concat", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n20000");
        }
    }

    #[test]
    fn record_list_literal_materializes_and_drops_leak_free() {
        // A `List[Record]` LITERAL (`[leaf("a"), leaf("b")]` — the svg `group([rect(…), …])` shape):
        // build a list block storing each Element handle (moved in via lower_owned_heap_field), routed
        // to the generated `$__drop_list_<R>` (each element freed recursively via `$__drop_<R>`). The
        // 10000x loop is the leak gate. Was walled "non-empty List[heap] literal".
        let src = "type E = { tag: String, kids: List[E] }\n\
            fn leaf(t: String) -> E = E { tag: t, kids: [] }\n\
            effect fn main() -> Unit = {\n  \
              let xs = [leaf(\"a\"), leaf(\"b\"), leaf(\"c\")]\n  \
              println(int.to_string(list.len(xs)))\n  \
              var n = 0\n  \
              for i in 0..10000 { let ys = [leaf(\"x\"), leaf(\"y\")]; n = n + list.len(ys) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "__drop_list_E"), "the recursive list-of-record drop must be generated");
        if let Some(out) = build_and_run("record_list_literal", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n20000");
        }
    }

    #[test]
    fn map_entries_render_loop_leak_free() {
        // The svg render-in-a-loop leak gate: a map.entries → list.map → list.join chain in a 10000x
        // loop. The map.entries result `List[(String,String)]` must drop via DropListStrStr (frees
        // each tuple's two Strings); tracked as heap_elem_lists it dropped flat (DropListStr), leaking
        // the Strings → OOM. is_list_str_str_ty now reclassifies the bound result. The loop OOMs if it
        // leaks.
        let src = "fn rattr(m: Map[String, String]) -> String =\n  \
            map.entries(m) |> list.map((p) => { let (k, v) = p; \"${k}=${v}\" }) |> list.join(\" \")\n\
            effect fn main() -> Unit = {\n  \
              var m: Map[String, String] = [:]\n  \
              m = map.set(m, \"a\", \"1\")\n  \
              m = map.set(m, \"b\", \"2\")\n  \
              var n = 0\n  \
              for i in 0..10000 { n = n + string.len(rattr(m)) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("map_entries_loop_leak", &render_wasm_program(&prog)) {
            assert_eq!(out, "70000");
        }
    }

    #[test]
    fn map_entries_str_map_and_tuple_destructure() {
        // The svg render_attrs shape: `map.entries(attrs) |> list.map((p) => { let (k,v)=p; … })`.
        // map.entries on Map[String,String] → List[(String,String)] (map_entries_str), the defunc
        // list.map over the (String,String) tuple-list, and the `let (k,v)=p` destructure (which
        // requires the borrowed tuple element to be a tracked materialized aggregate). Was the wall:
        // map.entries unlinked + the tuple-element destructure read garbage.
        let src = "effect fn main() -> Unit = {\n  \
            var m: Map[String, String] = [:]\n  \
            m = map.set(m, \"a\", \"1\")\n  \
            m = map.set(m, \"b\", \"2\")\n  \
            let s = map.entries(m) |> list.map((p) => { let (k, v) = p; \"${k}=${v}\" }) |> list.join(\" \")\n  \
            println(s) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("map_entries_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "a=1 b=2");
        }
    }

    #[test]
    fn not_bool_call_bound_in_let() {
        // `let hc = not list.is_empty(xs)` — a UnOp(Not) over a Bool CALL, bound to a let. The
        // lower_bind scalar path had no UnOp arm, so it fell to the deferred Const (the operand call
        // unemitted, the var silently 0) → `not list.is_empty` always read false (the render_el
        // `<g/>`-for-a-nonempty-group miscompile). Now routes through lower_scalar_value.
        let src = "fn pick(xs: List[String]) -> String = {\n  \
            let hc = not list.is_empty(xs)\n  \
            if hc then \"has\" else \"none\" }\n\
            effect fn main() -> Unit = {\n  \
            println(pick([\"a\"]))\n  \
            println(pick([])) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("not_bool_call_let", &render_wasm_program(&prog)) {
            assert_eq!(out, "has\nnone");
        }
    }

    #[test]
    fn defunc_map_self_recursive_record_render() {
        // The svg render_el shape: a defunctionalized `children |> list.map((c) => rend(c, d+1))`
        // whose body is a SELF-RECURSIVE call. The heap-result-arm self-call gate WALLED every
        // self-call (the unbounded-TCO guard); inside a defunc-map body the recursion is bounded by
        // the tree, so it is admitted (in_defunc_body). Without this the map fell back to a wrong
        // list.map_str dispatch (invalid wasm / garbage).
        let src = "type E = { tag: String, kids: List[E] }\n\
            fn leaf(t: String) -> E = E { tag: t, kids: [] }\n\
            local fn rend(e: E, d: Int) -> String = {\n  \
              let body = e.kids |> list.map((c) => rend(c, d + 1)) |> list.join(\",\")\n  \
              \"${e.tag}[${body}]\" }\n\
            effect fn main() -> Unit = {\n  \
              let root = E { tag: \"r\", kids: [leaf(\"a\"), leaf(\"b\")] }\n  \
              println(rend(root, 0)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("defunc_map_self_rec", &render_wasm_program(&prog)) {
            assert_eq!(out, "r[a[],b[]]");
        }
    }

    #[test]
    fn record_call_arg_with_map_field_drops_leak_free() {
        // A record passed as a CALL ARGUMENT (`withattr(mk("x"), …)`) must drop via its masked/recursive
        // drop, NOT the flat `Op::Drop` that rc_dec's only the record block and LEAKS its heap fields
        // (the `f(mk(x))`-in-a-loop OOM). materialized_call_arg now seeds the arg's record_masks +
        // (for a Map/List[heap]/record field) variant_drop_handles. Map[String,String] (map_str,
        // interleaved owned key+value) is freed by $__drop_map_ss. The 10000x loop is the leak gate.
        let src = "type R = { name: String, attrs: Map[String, String] }\n\
            fn mk(n: String) -> R = R { name: n, attrs: [:] }\n\
            fn withattr(r: R, k: String, v: String) -> R = { ...r, attrs: map.set(r.attrs, k, v) }\n\
            effect fn main() -> Unit = {\n  \
              let r = withattr(mk(\"hi\"), \"a\", \"1\")\n  \
              println(r.name)\n  \
              println(int.to_string(map.len(r.attrs)))\n  \
              var n = 0\n  \
              for i in 0..10000 { let r2 = withattr(mk(\"x\"), \"k\", \"v\"); n = n + map.len(r2.attrs) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("record_call_arg_map", &render_wasm_program(&prog)) {
            assert_eq!(out, "hi\n1\n10000");
        }
    }

    #[test]
    fn record_recursive_drop_frees_heap_fields_leak_free() {
        // The recursive record drop: a record with String + List[String] heap fields is freed by the
        // GENERATED `$__drop_R` (rc_dec the String, `$__drop_list_str` the list) — NOT the flat masked
        // DropListStr that would leak the list's element Strings. The 10000x loop is the leak gate:
        // each per-iteration record is fully reclaimed (no OOM). `$__drop_R` is routed via DropVariant
        // (record_drop_type_name → variant_drop_handles) and appended by generate_record_drop_sources.
        let src = "type R = { name: String, tags: List[String] }\n\
            fn mk(n: String, t: List[String]) -> R = R { name: n, tags: t }\n\
            effect fn main() -> Unit = {\n  \
              let r = mk(\"x\", [\"a\", \"b\"])\n  \
              println(r.name)\n  \
              println(int.to_string(list.len(r.tags)))\n  \
              var n = 0\n  \
              for i in 0..10000 { let r2 = mk(\"y\", [\"p\", \"q\", \"z\"]); n = n + list.len(r2.tags) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "__drop_R"), "the recursive record drop must be generated + linked");
        if let Some(out) = build_and_run("record_recursive_drop", &render_wasm_program(&prog)) {
            assert_eq!(out, "x\n2\n30000");
        }
    }

    #[test]
    fn record_with_empty_map_and_list_fields_constructs() {
        // The svg `el` shape: a record whose fields include an EMPTY Map (`attrs: [:]`) and an EMPTY
        // recursive List (`children: []`). `lower_owned_heap_field` now materializes an empty heap
        // container (a 0-length layout-agnostic block) as a record field, so the construct lowers and
        // the field reads (`map.len(e.attrs)`, `list.len(e.children)`) read the real empty slots.
        let src = "type E = { tag: String, attrs: Map[String, String], children: List[E], content: String }\n\
            fn el(tag: String) -> E = E { tag: tag, attrs: [:], children: [], content: \"\" }\n\
            effect fn main() -> Unit = {\n  \
              let e = el(\"rect\")\n  \
              println(e.tag)\n  \
              println(int.to_string(map.len(e.attrs)))\n  \
              println(int.to_string(list.len(e.children))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("record_empty_map_list", &render_wasm_program(&prog)) {
            assert_eq!(out, "rect\n0\n0");
        }
    }

    #[test]
    fn list_get_value_option_unwrap_executes_on_wasmtime() {
        // Option-of-Value read (`list.get(rows, i) ?? d` — the stringify_records row accessor). list.get
        // on a List[Value] dispatches to list.get_value (NOT the `_str` variant, which deep-copies the
        // element as a String — corrupting an Object to {}); it SHARES the element via Some(Dup), and the
        // `??` routes to option.value_unwrap_or (prim-based, since the value-match Some-arm rejects a heap
        // payload). The 2000x loop is the leak gate: value.as_array's OWNED list drops recursively
        // (value_result_lists), the shared element Values flat (co-owned). Was returning {} + OOM-leaking.
        let src = "import json\n\
            effect fn main() -> Unit = {\n  \
              let rows = value.as_array(value.array([value.object([(\"a\", value.int(1))]), value.str(\"x\")])) ?? []\n  \
              println(value.stringify(list.get(rows, 0) ?? value.null()))\n  \
              println(value.stringify(list.get(rows, 1) ?? value.null()))\n  \
              println(value.stringify(list.get(rows, 9) ?? value.object([])))\n  \
              match list.get(rows, 0) { some(v) => println(value.stringify(v)), none => println(\"none\") }\n  \
              var n = 0\n  for i in 0..2000 { let rs = value.as_array(value.array([value.object([(\"k\", value.int(i))])])) ?? []; let g = list.get(rs, 0) ?? value.null(); n = n + string.len(value.stringify(g)) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.get_value"));
        if let Some(out) = build_and_run("list_get_value_opt", &render_wasm_program(&prog)) {
            assert_eq!(out, "{\"a\":1}\n\"x\"\n{}\n{\"a\":1}\n18890");
        }
    }

    #[test]
    fn value_as_string_unwrap_executes_on_wasmtime() {
        // The String-payload Result `??` (`value.as_string(x) ?? "fb"` — Result[String,String]):
        // routed to the self-hosted result.str_unwrap_or, completing the Result-`??` family
        // (Value / List[Value] / String). Ok → the inner String, a tag-mismatch Err → the fallback.
        let src = "import json\n\
            effect fn main() -> Unit = {\n  \
              println(value.as_string(value.str(\"hello\")) ?? \"fb\")\n  \
              println(value.as_string(value.int(5)) ?? \"fb\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("value_as_string_unwrap", &render_wasm_program(&prog)) {
            assert_eq!(out, "hello\nfb");
        }
    }

    #[test]
    fn value_stringify_executes_on_wasmtime() {
        // The recursive JSON serializer, self-hosted in value_core, byte-identical to v0's
        // `almide_rt_value_stringify`: scalars direct, Str quoted+escaped (\ first), Array/Object
        // joined with "," via a String accumulator (the separator is `string.repeat(",", k)` with a
        // SCALAR-if k, sidestepping a heap-result-if in the loop body). 2000x is the leak gate — the
        // `prim.load_str` Str payload is a BORROW (not dropped as a call arg → no double-free).
        let src = "import json\n\
            effect fn main() -> Unit = {\n  \
              println(value.stringify(value.int(42)))\n  \
              println(value.stringify(value.bool(true)))\n  \
              println(value.stringify(value.null()))\n  \
              println(value.stringify(value.str(\"hi\\\"x\")))\n  \
              println(value.stringify(value.array([value.int(1), value.int(2), value.str(\"a\")])))\n  \
              println(value.stringify(value.object([(\"k\", value.int(1)), (\"s\", value.str(\"v\"))])))\n  \
              var n = 0\n  for i in 0..2000 { let s = value.stringify(value.object([(\"x\", value.str(\"v\")), (\"n\", value.int(i))])); n = n + string.len(s) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("value_stringify", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\ntrue\nnull\n\"hi\\\"x\"\n[1,2,\"a\"]\n{\"k\":1,\"s\":\"v\"}\n34890");
        }
    }

    #[test]
    fn value_object_and_json_keys_execute_on_wasmtime() {
        // The dynamic Value OBJECT (tag 6) self-host: `value.object(pairs)` builds a 2-slot-per-pair
        // block (key String + value Value, each rc_inc'd in — the Object co-owns them, freed by the
        // recursive __vdrop_obj at the last ref via __drop_value). `json.keys` reads them back. The
        // SLOT count (@8 = 2*pairs) is what the freelist reclaims — storing the pair count there
        // leaked 2 slots/iter (the 2-pair OOM this caught). 2000x is the leak gate (multi-pair).
        let src = "import json\n\
            effect fn main() -> Unit = {\n  \
              let o = value.object([(\"a\", value.int(1)), (\"bb\", value.str(\"x\"))])\n  \
              println(int.to_string(list.len(json.keys(o))))\n  \
              var k = 0\n  for i in 0..2000 { let p = value.object([(\"a\", value.int(i)), (\"b\", value.int(i))]); k = k + list.len(json.keys(p)) }\n  \
              println(int.to_string(k)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("value_object", &render_wasm_program(&prog)) {
            assert_eq!(out, "2\n4000");
        }
    }

    #[test]
    fn result_value_ok_wrapper_executes_on_wasmtime() {
        // The csv `parse` shape: a `Result[Value, String]` constructed by `ok(<Value>)` / `err(msg)`.
        // The Ok payload is a dynamic Value (materialized via lower_owned_heap_field), stored in the
        // len-1 + tag@16 block; marked `value_result_results` so the scope-end drop is the recursive
        // `Op::DropResultValue` ($__drop_value the Ok Value, rc_dec the Err String) — a flat
        // DropListStr would leak the Ok Value's nested payload. Round-trips: construct (ok/err),
        // match-read (ok(v)/err(e)), and the recursive drop at scope end.
        let src = "import json\n\
            effect fn wrap(n: Int) -> Result[Value, String] = if n < 0 then err(\"neg\") else ok(value.int(n))\n\
            effect fn main() -> Unit = {\n  \
              match wrap(42) { ok(v) => println(int.to_string(value.as_int(v) ?? 0)), err(e) => println(\"E:\" + e) }\n  \
              match wrap(0 - 1) { ok(v) => println(int.to_string(value.as_int(v) ?? 0)), err(e) => println(\"E:\" + e) } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("result_value_ok", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\nE:neg");
        }
    }

    #[test]
    fn empty_list_heap_result_if_arm_executes_on_wasmtime() {
        // A heap-result `if` with an EMPTY-list `[]` arm (`if cond then [] else <list>` — the parser
        // entry's empty-or-recurse split: `parse_rows = if is_empty(t) then [] else parse_rows_rec(...)`).
        // lower_heap_result_arm now materializes an empty `[]` arm (a fresh empty list block) +
        // Consumes it, alongside the populated-list-literal and call arms. Closes csv's parse_rows.
        let src = "fn gen(flag: Bool) -> List[String] = if flag then [] else [\"a\", \"b\"]\n\
            fn seq(n: Int) -> List[String] = if n <= 0 then [] else seq(n - 1) + [int.to_string(n)]\n\
            fn pick(flag: Bool, n: Int) -> List[String] = if flag then [] else seq(n)\n\
            fn main() -> Unit = {\n  \
              println(int.to_string(list.len(gen(true))) + \",\" + int.to_string(list.len(gen(false))))\n  \
              println(int.to_string(list.len(pick(true, 5))) + \",\" + int.to_string(list.len(pick(false, 5)))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("empty_list_arm", &render_wasm_program(&prog)) {
            assert_eq!(out, "0,2\n0,5");
        }
    }

    #[test]
    fn nested_list_of_lists_recursive_drop_executes_on_wasmtime() {
        // THE BOSS (csv `rows: List[List[String]]`): a list whose elements are owned `List[String]`
        // rows. Three pieces meet: (1) the list-of-lists CONCAT `rows + [cur]` (admit a List[String]
        // element via `__list_concat_rc`); (2) the singleton `[cur]` materialization; (3) the
        // RECURSIVE `Op::DropListListStr` — a NESTED wasm loop freeing each row's cell Strings, then
        // each row, then the outer block. A flat `DropListStr` would only `rc_dec` each row HANDLE,
        // leaking the cells. EVERY value of this type (concat result, call result, accumulator slot)
        // routes to `list_list_str_lists` so its drop is the nested one. The 2000x build+drop is the
        // LEAK GATE (an under-free OOMs the freelist as an OOB trap — exactly what this caught first).
        let src = "fn scan(text: String, pos: Int, rows: List[List[String]], cur: List[String]) -> List[List[String]] = {\n  \
              if pos >= string.len(text) then rows + [cur]\n  \
              else { let c = string.get(text, pos) ?? \"\"\n    \
                if c == \",\" then scan(text, pos + 1, rows, cur + [c])\n    \
                else if c == \"\\n\" then scan(text, pos + 1, rows + [cur], [])\n    \
                else scan(text, pos + 1, rows, cur + [c]) } }\n\
            fn main() -> Unit = {\n  \
              println(int.to_string(list.len(scan(\"ab,cd\\nef,gh\\n\", 0, [], []))))\n  \
              var n = 0\n  for i in 0..2000 { n = n + list.len(scan(\"ab,cd\\nef,gh\\n\", 0, [], [])) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("nested_list_drop", &render_wasm_program(&prog)) {
            assert_eq!(out, "3\n6000");
        }
    }

    #[test]
    fn scalar_var_list_literal_materializes_on_wasmtime() {
        // A `List[Int/Float/Bool]` literal with a VARIABLE element (`[n]`, `[a, b]`) in a value /
        // call-arg position. An all-LITERAL list folds to an `Init::IntList`, but a computed element
        // forced `alloc_init` to `Init::Opaque` (an empty list) → walled as unfaithful. Now the
        // call-arg path also tries `try_lower_scalar_list_construct` (flat `DynList` + `store64` each
        // element). This unblocks the append-accumulator element `acc + [n]` (the parser-row shape that
        // accumulates a scalar per step). Scalar elements own no heap, so the scope-end drop is flat.
        // 2000x is the leak gate.
        let src = "fn build(n: Int, acc: List[Int]) -> List[Int] =\n  \
              if n >= 8 then acc else build(n + 1, acc + [n * n])\n\
            fn main() -> Unit = {\n  \
              println(int.to_string(list.sum(build(0, []))))\n  \
              var s = 0\n  for i in 0..2000 { s = s + list.sum(build(0, [])) }\n  \
              println(int.to_string(s)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("scalar_var_list", &render_wasm_program(&prog)) {
            assert_eq!(out, "140\n280000");
        }
    }

    #[test]
    fn multi_accumulator_reset_and_cross_read_tco_executes_on_wasmtime() {
        // The csv-row shape: TWO heap accumulators where one's new value READS the other
        // (`out = out + cur`) while that other is RESET (`cur = ""`) in the same self-call. The TCO
        // append-accumulator now (1) admits a RESET to a fresh empty (`""`/`[]`) as a loop-carried
        // slot update, and (2) emits the per-iteration heap assigns in READ-DEPENDENCY order (the
        // reader `out` before the reset of `cur`), so `out` sees the OLD `cur`. A cyclic read
        // (`a=a+b; b=b+a`) still walls. 2000x is the leak gate (each slot's drop-old/alloc-new).
        let src = "fn scan(text: String, pos: Int, out: String, cur: String) -> String = {\n  \
              if pos >= string.len(text) then out + cur\n  \
              else { let c = string.get(text, pos) ?? \"\"; if c == \",\" then scan(text, pos + 1, out + cur, \"\") else scan(text, pos + 1, out, cur + c) } }\n\
            fn main() -> Unit = {\n  \
              println(scan(\"ab,cd,ef\", 0, \"\", \"\"))\n  \
              var n = 0\n  for i in 0..2000 { n = n + string.len(scan(\"ab,cd,ef\", 0, \"\", \"\")) }\n  \
              println(int.to_string(n)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("multi_acc_tco", &render_wasm_program(&prog)) {
            assert_eq!(out, "abcdef\n12000");
        }
    }

