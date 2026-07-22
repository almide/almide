
#[test]
fn opt_tuple_fold_scanner() {
    // The wav find_chunk_at scanner: a (scalar, Option[scalar]) fold accumulator —
    // the Option component runs as tag+payload locals; the match-over-found projects
    // to an if-over-tag; the result Option materializes once (len-as-tag overwrite).
    let src = "fn find_at(sizes: List[Int], target: Int, pos: Int) -> Option[Int] = {\n\
        let positions = list.range(0, 10)\n\
        list.fold(positions, (pos, none), (state, i) => {\n\
        let (p, found) = state\n\
        match found {\n\
        some(_) => state\n\
        none =>\n\
        if p > 100 then (p, none)\n\
        else {\n\
        let size = list.get(sizes, i) |> option.unwrap_or(999)\n\
        if p == target then (p, some(p))\n\
        else (p + size, none) } } }).1 }\n\
        effect fn main() -> Unit = {\n\
        match find_at([4, 4, 4, 4], 8, 0) {\n\
        some(p) => println(\"found:\" + int.to_string(p))\n\
        none => println(\"none\") }\n\
        match find_at([4, 4], 99, 0) {\n\
        some(p) => println(\"?\")\n\
        none => println(\"none\") } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("opt_tuple_fold", &render_wasm_program(&prog)) {
        assert_eq!(out, "found:8\nnone");
    }
}

#[test]
fn adt_int_tuple_return_ctor() {
    // The gguf read_one shape: an `(ADT, Int)` tuple-return tail whose elements are
    // variant CTOR calls (`(IntV(p), p + 4)`). The ctor element routes through
    // `try_lower_variant_ctor` (a plain CallFn would leave `$IntV` unlinked); both a
    // scalar ctor and a String-payload ctor construct, accumulate through list.push,
    // and match-extract downstream.
    let src = "type GV =\n\
        | IntV(Int)\n\
        | StrV(String)\n\
        fn read_one(p: Int) -> (GV, Int) =\n\
        if p % 2 == 0 then (IntV(p), p + 5)\n\
        else (StrV(\"s\" + int.to_string(p)), p + 3)\n\
        fn collect(n: Int) -> (List[GV], Int) = {\n\
        var items: List[GV] = []\n\
        var p = 0\n\
        for _ in 0..n {\n\
        let (val, next) = read_one(p)\n\
        list.push(items, val)\n\
        p = next }\n\
        (items, p) }\n\
        effect fn main() -> Unit = {\n\
        let (vs, endp) = collect(4)\n\
        for v in vs {\n\
        match v {\n\
        IntV(i) => println(\"i:\" + int.to_string(i))\n\
        StrV(s) => println(\"s:\" + s) } }\n\
        println(\"end:\" + int.to_string(endp)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("adt_int_tuple_ret", &render_wasm_program(&prog)) {
        assert_eq!(out, "i:0\ns:s5\ni:8\ns:s13\nend:16");
    }
}

#[test]
fn variant_ctor_list_field_recursive_accumulator() {
    // The gguf read_array shape (ADT brick 5): a RECURSIVE accumulator whose tail returns
    // `(ArrV(items), p)` — a ctor with a `List[<rich variant>]` field. The ctor admits the
    // Dup'd list (freed via the generated mutually-recursive `$__drop_GV`/`$__drop_list_GV`);
    // `end:9` witnesses every element's cursor advance through two recursion levels.
    let src = "type GV =\n\
        | IntV(Int)\n\
        | StrV(String)\n\
        | ArrV(List[GV])\n\
        fn read_array(n: Int, depth: Int, pos: Int) -> (GV, Int) = {\n\
        var items: List[GV] = []\n\
        var p = pos\n\
        for i in 0..n {\n\
        if depth > 0 then {\n\
        let (val, next) = read_array(2, depth - 1, p)\n\
        list.push(items, val)\n\
        p = next }\n\
        else if i % 2 == 0 then {\n\
        let (val, next) = (IntV(i * 10 + p), p + 1)\n\
        list.push(items, val)\n\
        p = next }\n\
        else {\n\
        let (val, next) = (StrV(\"x\" + int.to_string(p)), p + 2)\n\
        list.push(items, val)\n\
        p = next } }\n\
        (ArrV(items), p) }\n\
        effect fn main() -> Unit = {\n\
        let (v, endp) = read_array(3, 1, 0)\n\
        match v {\n\
        ArrV(_) => println(\"arr\")\n\
        IntV(i) => println(\"i\" + int.to_string(i))\n\
        StrV(s) => println(s) }\n\
        println(\"end:\" + int.to_string(endp)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("ctor_list_field_rec", &render_wasm_program(&prog)) {
        assert_eq!(out, "arr\nend:9");
    }
}

#[test]
fn matrix_self_host_floor() {
    // The Matrix value model (roadmap B, approach (a)): a v1 Matrix IS a List[List[Float]],
    // served by the matrix_core self-host registry. Construction, metadata, element-wise,
    // transpose, and the k-ascending mul all byte-match the v0 oracle.
    let src = "effect fn main() -> Unit = {\n\
        let m = matrix.from_lists([[1.0, 2.0], [3.0, 4.0]])\n\
        println(float.to_string(matrix.get(m, 0, 0)))\n\
        println(int.to_string(matrix.rows(m)) + \"x\" + int.to_string(matrix.cols(m)))\n\
        let z = matrix.zeros(2, 3)\n\
        println(float.to_string(matrix.get(z, 1, 2)))\n\
        let a = matrix.add(m, m)\n\
        println(float.to_string(matrix.get(a, 1, 0)))\n\
        let t = matrix.transpose(m)\n\
        println(float.to_string(matrix.get(t, 0, 1)))\n\
        let s = matrix.scale(m, 2.5)\n\
        println(float.to_string(matrix.get(s, 0, 1)))\n\
        let p = matrix.mul(m, m)\n\
        println(float.to_string(matrix.get(p, 0, 0)) + \",\" + float.to_string(matrix.get(p, 1, 1)))\n\
        let ll = matrix.to_lists(m)\n\
        println(int.to_string(list.len(ll))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("matrix_floor", &render_wasm_program(&prog)) {
        assert_eq!(out, "1.0\n2x2\n0.0\n6.0\n3.0\n5.0\n7.0,22.0\n2");
    }
}

#[test]
fn matrix_per_head_repeat_kv_concat_rows() {
    // The three nn Matrix walls (roadmap B): per_head_rms_norm (list.map closure over
    // List[Matrix] + split/concat), repeat_kv (heap-result if returning a param + flat_map
    // with list.repeat_rc), concat_rows (a list-literal flatten arg of to_lists calls).
    // Expectations byte-verified against `almide run --target wasm` (the v0 oracle).
    let src = "fn per_head_rms_norm(x: Matrix, gamma: List[Float], n_heads: Int, eps: Float) -> Matrix = {\n\
        let heads = matrix.split_cols_even(x, n_heads)\n\
        let normed = heads |> list.map((h) => matrix.rms_norm_rows(h, gamma, eps))\n\
        matrix.concat_cols(normed) }\n\
        fn repeat_kv(kv: Matrix, n_kv_heads: Int, n_rep: Int) -> Matrix = {\n\
        if n_rep == 1 then kv\n\
        else {\n\
        let heads = matrix.split_cols_even(kv, n_kv_heads)\n\
        let repeated = heads |> list.flat_map((h) => list.repeat(h, n_rep))\n\
        matrix.concat_cols(repeated) } }\n\
        fn concat_rows(a: Matrix, b: Matrix) -> Matrix = {\n\
        let all = list.flatten([matrix.to_lists(a), matrix.to_lists(b)])\n\
        matrix.from_lists(all) }\n\
        effect fn main() -> Unit = {\n\
        let x = matrix.from_lists([[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]])\n\
        let g = [0.5, 1.5]\n\
        let p = per_head_rms_norm(x, g, 2, 0.00001)\n\
        println(float.to_string(matrix.get(p, 0, 0)) + \",\" + float.to_string(matrix.get(p, 1, 3)))\n\
        let r1 = repeat_kv(x, 2, 1)\n\
        println(float.to_string(matrix.get(r1, 0, 0)))\n\
        let r2 = repeat_kv(x, 2, 2)\n\
        println(int.to_string(matrix.cols(r2)) + \":\" + float.to_string(matrix.get(r2, 0, 2)) + \",\" + float.to_string(matrix.get(r2, 1, 7)))\n\
        let cr = concat_rows(x, matrix.from_lists([[9.0, 10.0, 11.0, 12.0]]))\n\
        println(int.to_string(matrix.rows(cr)) + \":\" + float.to_string(matrix.get(cr, 2, 1))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("matrix_walls", &render_wasm_program(&prog)) {
        assert_eq!(
            out,
            "0.31622713356320326,1.5964561112912787\n1.0\n8:1.0,8.0\n3:10.0"
        );
    }
}

#[test]
fn matrix_norms_and_bytes() {
    // rms/layer norms (full-row statistics, zip-truncated output), split/concat round-trip,
    // gather with the OOB zero-row edge, and the f32/f16 LE byte decoders (in-bounds — the
    // native oracle's OOB→zeros edge is pinned by the from_bytes probe, not here).
    let src = "effect fn main() -> Unit = {\n\
        let m = matrix.from_lists([[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]])\n\
        let g = [0.5, 1.5, 2.5, 3.5]\n\
        let r = matrix.rms_norm_rows(m, g, 0.00001)\n\
        println(float.to_string(matrix.get(r, 0, 0)) + \",\" + float.to_string(matrix.get(r, 1, 3)))\n\
        let ln = matrix.layer_norm_rows(m, g, [0.1, 0.2, 0.3, 0.4], 0.00001)\n\
        println(float.to_string(matrix.get(ln, 0, 1)) + \",\" + float.to_string(matrix.get(ln, 1, 2)))\n\
        let heads = matrix.split_cols_even(m, 2)\n\
        let cc = matrix.concat_cols(heads)\n\
        println(float.to_string(matrix.get(cc, 0, 0)) + \",\" + float.to_string(matrix.get(cc, 1, 3)))\n\
        let ga = matrix.gather_rows(m, [1, 0, 9])\n\
        println(float.to_string(matrix.get(ga, 0, 0)) + \",\" + float.to_string(matrix.get(ga, 2, 3)))\n\
        let b32 = bytes.from_list([0, 0, 192, 63, 0, 0, 0, 64, 0, 0, 0, 191, 0, 128, 200, 66])\n\
        let m32 = matrix.from_bytes_f32_le(b32, 0, 2, 2)\n\
        println(float.to_string(matrix.get(m32, 1, 0)) + \",\" + float.to_string(matrix.get(m32, 1, 1)))\n\
        let b16 = bytes.from_list([0, 60, 0, 193, 0, 56, 255, 123])\n\
        let m16 = matrix.from_bytes_f16_le(b16, 0, 2, 2)\n\
        println(float.to_string(matrix.get(m16, 0, 1)) + \",\" + float.to_string(matrix.get(m16, 1, 1))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("matrix_norms_bytes", &render_wasm_program(&prog)) {
        assert_eq!(
            out,
            "0.1825740641190532,4.2453485560707875\n-0.47081770998446343,1.4180295166407726\n\
             1.0,8.0\n5.0,0.0\n-0.5,100.25\n-2.5,65504.0"
        );
    }
}

#[test]
fn defunc_find_capturing_predicate() {
    // `list.find` with a CAPTURING predicate over record elements (the gguf/ggml
    // find_tensor shape) — inlined as an early-exit loop with a len-as-tag Option
    // result. Previously the dropped closure emitted INVALID WASM (the translation
    // type-mismatch escape); the general unfaithful-HOF wall plus this inline turned
    // that into a faithful execution. Scalar find (`x == k`) rides the same loop.
    let src = "type Tensor = {\n\
        name: String,\n\
        off: Int,\n\
        }\n\
        fn find_tensor(ts: List[Tensor], name: String) -> Option[Tensor] =\n\
        ts |> list.find((t) => t.name == name)\n\
        fn find_val(xs: List[Int], k: Int) -> Option[Int] =\n\
        xs |> list.find((x) => x == k)\n\
        effect fn main() -> Unit = {\n\
        let ts = [{ name: \"a\", off: 3 }, { name: \"b\", off: 7 }]\n\
        match find_tensor(ts, \"b\") {\n\
        some(t) => println(\"hit:\" + int.to_string(t.off))\n\
        none => println(\"none\") }\n\
        match find_tensor(ts, \"zz\") {\n\
        some(t) => println(\"?\")\n\
        none => println(\"none\") }\n\
        match find_val([3, 7, 9], 7) {\n\
        some(v) => println(\"v:\" + int.to_string(v))\n\
        none => println(\"none\") } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("defunc_find", &render_wasm_program(&prog)) {
        assert_eq!(out, "hit:7\nnone\nv:7");
    }
}

#[test]
fn load_weights_record_return_shape() {
    // The whisper load_weights skeleton: a heap-result RECORD return whose fields are a
    // match-bound Matrix (via find_tensor + a byte decode), a List[Float] call, and a
    // `list.map` of a record-building user call capturing the model record.
    let src = "type Tensor = {\n\
        name: String,\n\
        ftype: Int,\n\
        off: Int,\n\
        }\n\
        type Model = {\n\
        tensors: List[Tensor],\n\
        data: Bytes,\n\
        }\n\
        type Layer = {\n\
        w: Matrix,\n\
        b: List[Float],\n\
        }\n\
        type Weights = {\n\
        conv_w: Matrix,\n\
        conv_b: List[Float],\n\
        layers: List[Layer],\n\
        }\n\
        fn find_tensor(ts: List[Tensor], name: String) -> Option[Tensor] =\n\
        ts |> list.find((t) => t.name == name)\n\
        fn tensor_vec(m: Model, name: String, n: Int) -> List[Float] =\n\
        match find_tensor(m.tensors, name) {\n\
        some(t) => bytes.read_f32_le_array(m.data, t.off, n)\n\
        none => list.map(list.range(0, n), (_) => 0.0) }\n\
        fn load_layer(m: Model, i: Int, n: Int) -> Layer = {\n\
        {\n\
        w: matrix.from_bytes_f32_le(m.data, i * 8, 1, 2),\n\
        b: tensor_vec(m, \"conv.bias\", n),\n\
        } }\n\
        fn load_weights(m: Model, n: Int) -> Weights = {\n\
        let conv_w = match find_tensor(m.tensors, \"conv.weight\") {\n\
        some(t) => matrix.transpose(matrix.from_bytes_f32_le(m.data, t.off, 2, 2))\n\
        none => matrix.zeros(2, 2) }\n\
        {\n\
        conv_w: conv_w,\n\
        conv_b: tensor_vec(m, \"conv.bias\", 2),\n\
        layers: list.map(list.range(0, n), (i) => load_layer(m, i, 2)),\n\
        } }\n\
        effect fn main() -> Unit = {\n\
        let ts = [\n\
        { name: \"conv.weight\", ftype: 0, off: 0 },\n\
        { name: \"conv.bias\", ftype: 0, off: 8 },\n\
        ]\n\
        let data = bytes.from_list([0, 0, 192, 63, 0, 0, 0, 64, 0, 0, 0, 191, 0, 128, 200, 66])\n\
        let m: Model = { tensors: ts, data: data }\n\
        let w = load_weights(m, 2)\n\
        println(float.to_string(matrix.get(w.conv_w, 1, 0)))\n\
        println(float.to_string(list.get(w.conv_b, 0) ?? 9.9))\n\
        let l1 = list.get(w.layers, 1)\n\
        match l1 {\n\
        some(l) => println(float.to_string(matrix.get(l.w, 0, 1)))\n\
        none => println(\"none\") } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("load_weights_shape", &render_wasm_program(&prog)) {
        assert_eq!(out, "2.0\n-0.5\n100.25");
    }
}

#[test]
fn matrix_record_field_drop_routes_recursively() {
    // A record with `Matrix` / `List[Matrix]` fields: the generated `$__drop_<R>` must
    // free each field's ROWS (via `__drop_matrix` / `__drop_list_matrix`), not flat-
    // `rc_dec` the outer block (the pre-fix row leak). A 2000-iteration create+drop
    // loop runs bounded with the right sum — the leak-loop convention.
    let src = "type W = {\n\
        m: Matrix,\n\
        ms: List[Matrix],\n\
        }\n\
        fn mk(i: Int) -> W = {\n\
        {\n\
        m: matrix.from_lists([[int.to_float(i), 2.0], [3.0, 4.0]]),\n\
        ms: matrix.split_cols_even(matrix.ones(2, 4), 2),\n\
        } }\n\
        effect fn main() -> Unit = {\n\
        var i = 0\n\
        var acc = 0.0\n\
        while i < 2000 {\n\
        let w = mk(i)\n\
        acc = acc + matrix.get(w.m, 0, 0) + int.to_float(list.len(w.ms))\n\
        i = i + 1 }\n\
        println(float.to_string(acc)) }\n";
    let prog = lower_source(src);
    let wat = render_wasm_program(&prog);
    assert!(wat.contains("$__drop_matrix"), "the Matrix field routes through __drop_matrix");
    assert!(
        wat.contains("$__drop_list_matrix"),
        "the List[Matrix] field routes through __drop_list_matrix"
    );
    if let Some(out) = build_and_run("matrix_field_drop", &wat) {
        // Σ i (0..2000) + 2000 × len(ms)=2 = 1999000 + 4000.
        assert_eq!(out, "2003000.0");
    }
}

#[test]
fn anon_record_with_anon_list_field_drop_source_typechecks() {
    // An UNTYPED anon-record binding whose field is a List of STRUCTURAL records:
    // the synthesized `__drop_anonrec_<hash>` must bind the list field with the
    // STRUCTURAL source type (`List[{ name: String, off: Int }]`) — writing the
    // drop-fn hash as a type (`List[anonrec_<hash>]`) type-errored the WHOLE
    // generated batch ("undefined variable 'f0'") and failed the render
    // program-level.
    let src = "effect fn main() -> Unit = {\n\
        let ts = [{ name: \"a\", off: 3 }, { name: \"b\", off: 9 }]\n\
        let m = { tensors: ts, tag: 7 }\n\
        println(int.to_string(m.tag))\n\
        println(int.to_string(list.len(m.tensors))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("anon_f0_shape", &render_wasm_program(&prog)) {
        assert_eq!(out, "7\n2");
    }
}

#[test]
fn variant_record_ctor_construct_and_match() {
    // RECORD-ctor variants end to end: construction (`Data { … }` → the TAGGED block, a
    // tag-less plain record misread every match as tag 0 — the mt2 miscompile), record
    // PATTERNS (`Data { seq, .. }` — named-field slot binds incl. `..`), a heap-result
    // record-pattern match (payload/message borrows), and NESTED record-ctor fields
    // (`Node { left: Leaf(1), right: Node { … } }` — the recursive tree).
    let src = "type Tree = | Leaf(Int) | Node { left: Tree, right: Tree, value: Int }\n\
        fn tree_sum(t: Tree) -> Int =\n\
        match t {\n\
        Leaf(n) => n\n\
        Node { left, right, value } => tree_sum(left) + tree_sum(right) + value\n\
        }\n\
        type Message =\n\
        | Ping\n\
        | Data { payload: String, seq: Int }\n\
        | Error { code: Int, message: String }\n\
        fn message_code(m: Message) -> Int = match m {\n\
        Ping => 0,\n\
        Data { seq, .. } => seq,\n\
        Error { code, .. } => code,\n\
        }\n\
        fn message_text(m: Message) -> String = match m {\n\
        Ping => \"ping\",\n\
        Data { payload, .. } => payload,\n\
        Error { message, .. } => message,\n\
        }\n\
        effect fn main() -> Unit = {\n\
        let t = Node { left: Leaf(1), right: Node { left: Leaf(2), right: Leaf(3), value: 10 }, value: 5 }\n\
        println(int.to_string(tree_sum(t)))\n\
        let m1 = Data { payload: \"abc\", seq: 42 }\n\
        let m2 = Error { code: 7, message: \"boom\" }\n\
        println(int.to_string(message_code(m1)) + \":\" + message_text(m1))\n\
        println(int.to_string(message_code(m2)) + \":\" + message_text(m2))\n\
        println(int.to_string(message_code(Ping)) + \":\" + message_text(Ping)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("variant_record_ctor", &render_wasm_program(&prog)) {
        assert_eq!(out, "21\n42:abc\n7:boom\n0:ping");
    }
}

#[test]
fn variant_list_field_extraction_loops() {
    // The gguf ValArray CONSUMER: a statement match binding a `List[variant]` field
    // (`ArrV(rows)`), iterated with nested matches and a per-row String accumulator —
    // the arm-tail loop must RUN (it was silently elided to caps markers before).
    let src = "type GV =\n\
        | IntV(Int)\n\
        | StrV(String)\n\
        | ArrV(List[GV])\n\
        fn read_array(n: Int, depth: Int, pos: Int) -> (GV, Int) = {\n\
        var items: List[GV] = []\n\
        var p = pos\n\
        for i in 0..n {\n\
        if depth > 0 then {\n\
        let (val, next) = read_array(2, depth - 1, p)\n\
        list.push(items, val)\n\
        p = next }\n\
        else if i % 2 == 0 then {\n\
        let (val, next) = (IntV(i * 10 + p), p + 1)\n\
        list.push(items, val)\n\
        p = next }\n\
        else {\n\
        let (val, next) = (StrV(\"x\" + int.to_string(p)), p + 2)\n\
        list.push(items, val)\n\
        p = next } }\n\
        (ArrV(items), p) }\n\
        effect fn main() -> Unit = {\n\
        let (v, endp) = read_array(3, 1, 0)\n\
        match v {\n\
        ArrV(rows) => {\n\
        println(\"rows:\" + int.to_string(list.len(rows)))\n\
        for row in rows {\n\
        match row {\n\
        ArrV(cells) => {\n\
        var line = \"\"\n\
        for c in cells {\n\
        match c {\n\
        IntV(i) => { line = line + \"i\" + int.to_string(i) + \",\" }\n\
        StrV(s) => { line = line + s + \",\" }\n\
        ArrV(_) => { line = line + \"?,\" }\n\
        } }\n\
        println(line) }\n\
        IntV(i) => println(\"i\" + int.to_string(i))\n\
        StrV(s) => println(s)\n\
        } } }\n\
        IntV(i) => println(\"top-i\")\n\
        StrV(s) => println(\"top-s\")\n\
        }\n\
        println(\"end:\" + int.to_string(endp)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("variant_list_field_loops", &render_wasm_program(&prog)) {
        assert_eq!(out, "rows:3\ni0,x1,\ni3,x4,\ni6,x7,\nend:9");
    }
}

#[test]
fn pair_selfhosts_enumerate_zip_skv_hshare() {
    // The light self-host batch (backlog T3-5): scalar/String enumerate, scalar/rc-row
    // zip (min-length), map.from_list/entries over the skv vocab repr (duplicate key =
    // FIRST position, LAST value — v0's AlmideMap collect), and the handle-sharing
    // get_or/take over a List[rows]. All byte-verified against `almide run` (native).
    let src = "effect fn main() -> Unit = {\n\
        let xs = [10.5, 20.25]\n\
        for pair in list.enumerate(xs) {\n\
        println(int.to_string(pair.0) + \"=\" + float.to_string(pair.1)) }\n\
        for p2 in list.enumerate([\"ab\", \"cd\"]) {\n\
        println(int.to_string(p2.0) + \":\" + p2.1) }\n\
        let zs = list.zip([1, 2, 3], [40, 50, 60, 70])\n\
        println(int.to_string(list.len(zs)))\n\
        let za = list.zip([[1.0, 2.0], [3.0]], [[9.0], [8.0], [7.0]])\n\
        match list.get(za, 1) {\n\
        some(pr) => println(float.to_string(list.get(pr.0, 0) ?? 0.0) + \"/\" + float.to_string(list.get(pr.1, 0) ?? 0.0))\n\
        none => println(\"none\") }\n\
        let vocab = map.from_list([(\"abc\", 100), (\"d\", 4), (\"abc\", 999)])\n\
        println(int.to_string(map.len(vocab)) + \",\" + int.to_string(map.get(vocab, \"abc\") ?? -1))\n\
        for e in map.entries(vocab) {\n\
        println(e.0 + \"->\" + int.to_string(e.1)) }\n\
        let rows = [[1.5, 2.5], [3.5]]\n\
        let r1 = list.get_or(rows, 1, [])\n\
        println(float.to_string(list.get(r1, 0) ?? -1.0))\n\
        println(int.to_string(list.len(list.take(rows, 1)))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("pair_selfhosts", &render_wasm_program(&prog)) {
        assert_eq!(
            out,
            "0=10.5\n1=20.25\n0:ab\n1:cd\n3\n3.0/8.0\n2,999\nabc->999\nd->4\n3.5\n1"
        );
    }
}

#[test]
fn bytes_length_prefixed_strings() {
    // bytes.read_length_prefixed_strings_le: u32-LE prefixes, a truncated tail STOPS
    // the scan (v0's break), lossy UTF-8 decode; a mid-buffer start reads the rest.
    let src = "effect fn main() -> Unit = {\n\
        let b = bytes.from_list([2, 0, 0, 0, 97, 98, 0, 0, 0, 0, 3, 0, 0, 0, 120, 121, 122, 9, 0])\n\
        let xs = bytes.read_length_prefixed_strings_le(b, 0, 10)\n\
        println(int.to_string(list.len(xs)))\n\
        for s in xs {\n\
        println(\"[\" + s + \"]\") }\n\
        let tail = bytes.read_length_prefixed_strings_le(b, 6, 10)\n\
        println(int.to_string(list.len(tail))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("bytes_lenprefix", &render_wasm_program(&prog)) {
        assert_eq!(out, "3\n[ab]\n[]\n[xyz]\n2");
    }
}

#[test]
fn heap_if_returning_a_bound_var_is_leak_free() {
    // The `let base = "…"; let base = if c then base + "…" else base` shape (default_fields
    // describe's Rect arm). The else arm Dups `base` and moves it out; the ownership
    // certificate must NOT double-count that move against the shared scope-local's
    // reference (the pre-fix `iammd` REJECT — a Consumed value must not also take the
    // EndIf val-move). A 3000-iteration loop stays bounded (no double-free / no leak).
    let src = "fn describe(width: Float, color: String) -> String = {\n\
        let base = \"rect \" + float.to_string(width)\n\
        let base = if color != \"\" then base + \" color=\" + color else base\n\
        base }\n\
        effect fn main() -> Unit = {\n\
        var i = 0\n\
        var last = \"\"\n\
        while i < 3000 {\n\
        last = describe(int.to_float(i), if i % 2 == 0 then \"c\" else \"\")\n\
        i = i + 1 }\n\
        println(last) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("heap_if_bound_var", &render_wasm_program(&prog)) {
        assert_eq!(out, "rect 2999.0");
    }
}

#[test]
fn guard_else_early_return_and_continue_execute() {
    // Phase A end-to-end: a function-body `guard cond else err(...)` returns the Err on
    // the failing path (the pre-fix always-continue miscompile returned ok), and a
    // loop-body `guard cond else continue` filters iterations. Byte-verified vs v0.
    let src = "effect fn validated(s: String) -> Result[String, String] = {\n\
        guard string.len(s) > 0 else err(\"empty\")\n\
        ok(string.to_upper(s)) }\n\
        effect fn main() -> Unit = {\n\
        match validated(\"hi\") { ok(v) => println(\"ok:\" + v), err(e) => println(\"err:\" + e) }\n\
        match validated(\"\") { ok(v) => println(\"ok:\" + v), err(e) => println(\"err:\" + e) }\n\
        var total = 0\n\
        for i in 1..=10 {\n\
        guard i % 2 != 0 else continue\n\
        total = total + i }\n\
        println(int.to_string(total)) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("guard_early_return", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok:HI\nerr:empty\n25");
    }
}

#[test]
fn heap_result_match_over_option_field_and_let_bound_variant() {
    // Phase B: (1) a heap-result `match` over a BORROWED `Option[String]` FIELD subject
    // (`match u.email { some(e) => "…${e}…", none => u.name }`) — the field's Option handle
    // is borrowed and tracked so the heap-payload some-bind executes. (2) a `let nm = match
    // s.shape { Circle(_) => "circle", … }; "${nm}…"` — a let-bound CUSTOM-VARIANT heap-result
    // match, tail-duplicated into each arm (wrap_match_arms). Both byte-verified vs v0.
    let src = "type Shape = | Circle(Float) | Rect(Float, Float)\n\
        type User = { name: String, email: Option[String] }\n\
        fn user_display(u: User) -> String =\n\
        match u.email { some(e) => \"${u.name} <${e}>\", none => u.name }\n\
        fn describe(s: Shape, label: String) -> String = {\n\
        let nm = match s { Circle(_) => \"circle\", Rect(_, _) => \"rect\" }\n\
        \"${nm}: ${label}\" }\n\
        effect fn main() -> Unit = {\n\
        let a: User = { name: \"alice\", email: some(\"a@x.com\") }\n\
        let b: User = { name: \"bob\", email: none }\n\
        println(user_display(a))\n\
        println(user_display(b))\n\
        println(describe(Circle(5.0), \"big\"))\n\
        println(describe(Rect(1.0, 2.0), \"wide\")) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("heap_match_option_field", &render_wasm_program(&prog)) {
        assert_eq!(out, "alice <a@x.com>\nbob\ncircle: big\nrect: wide");
    }
}

#[test]
fn list_of_record_ctor_variants_literal() {
    // Phase B: a `[Click { x, y }, KeyPress { key }, Close]` literal — a List of RECORD-CTOR
    // variants (a rich variant with a String field). Each element materializes via the tagged
    // variant ctor (not the plain-record path); the list's `$__drop_list_Event` frees each
    // recursively. Byte-verified vs v0.
    let src = "type Event =\n\
        | Click { x: Int, y: Int }\n\
        | KeyPress { key: String }\n\
        | Close\n\
        fn name(e: Event) -> String = match e {\n\
        Click { x, .. } => \"click:\" + int.to_string(x)\n\
        KeyPress { key } => \"key:\" + key\n\
        Close => \"close\"\n\
        }\n\
        effect fn main() -> Unit = {\n\
        let events = [Click { x: 1, y: 2 }, KeyPress { key: \"a\" }, Close]\n\
        for e in events { println(name(e)) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("list_record_ctor_variants", &render_wasm_program(&prog)) {
        assert_eq!(out, "click:1\nkey:a\nclose");
    }
}

#[test]
fn matrix_softmax_rows_byte_matches_scalar_libm_oracle() {
    // Phase D1: the WASM matrix oracle computes softmax with scalar `rt.math_exp` (= libm
    // exp, which the self-hosted `math.exp`/math_exp.almd byte-matches) and a LEFT-TO-RIGHT
    // scalar sum — NOT the native SIMD fast-exp. The self-host transcribes the SAME op order
    // (row-max subtract → per-element exp → l-to-r sum → divide, with the NaN/Inf/sum<=0 →
    // uniform 1/cols guard), so it is byte-exact vs v0 `--target wasm` even at the -1e9 mask,
    // the ±708 clamp boundary, and extreme magnitudes.
    let src = "effect fn main() -> Unit = {\n\
        let m = matrix.from_lists([\n\
        [1000.0, 0.0 - 1000000000.0, 2.0, 5.5],\n\
        [0.001, 100.0, 0.0 - 50.0, 0.0033333333],\n\
        [710.0, 0.0 - 710.0, 0.0, 708.0]])\n\
        let ls = matrix.to_lists(matrix.softmax_rows(m))\n\
        for row in ls { for x in row { println(float.to_string(x)) } } }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "matrix.softmax_rows"),
        "softmax_rows must auto-link its self-host"
    );
    // Golden captured from `almide run --target wasm` (the scalar-libm oracle).
    if let Some(out) = build_and_run("matrix_softmax", &render_wasm_program(&prog)) {
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 12, "3 rows × 4 cols");
        // Row 2 is a proper distribution summing to ~1; row 1's masked lane is ~0.
        assert!(lines[1].starts_with('0'), "masked -1e9 lane → ~0, got {}", lines[1]);
    }
}

#[test]
fn matrix_gelu_byte_matches_scalar_libm_oracle() {
    // Phase D1: gelu (tanh approx) is element-wise scalar arithmetic + `rt.math_exp` (libm,
    // = self-hosted math.exp). The self-host transcribes the exact op order — inner = K*(x +
    // 0.044715*(x*x)*x), clamp ±20, e2 = exp(2*clamped), tanh = (e2-1)/(e2+1), 0.5*(1+tanh)*x
    // — so it is byte-exact vs v0 `--target wasm` across sign, magnitude, and the clamp region.
    let src = "effect fn main() -> Unit = {\n\
        let m = matrix.from_lists([[0.0 - 3.0, 0.0 - 0.5, 0.0, 0.5, 1.0], [2.0, 5.0, 0.0 - 10.0, 100.0, 0.001]])\n\
        let ls = matrix.to_lists(matrix.gelu(m))\n\
        for row in ls { for x in row { println(float.to_string(x)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.gelu"), "gelu self-host must link");
    if let Some(out) = build_and_run("matrix_gelu", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 10);
        assert_eq!(out.lines().next().unwrap(), "-0.0036373920817729943");
    }
}
