
#[test]
fn matrix_swiglu_gate_byte_matches_the_canonical_fast_exp() {
    // ORACLE CHANGED (#1197): this pinned the retired promise "the wasm leg
    // reproduces v0's scalar libm exp". The leg now runs the CANONICAL fast-exp
    // — the same unfused algorithm, reduction order and scaling spelling the
    // native SIMD kernel runs — so the pinned value is the one BOTH legs
    // produce (verified by running this very program on native and wasm, and by
    // spec/wasm_cross/matrix_softmax_fastexp.almd under C-223).

    // Phase D1: swiglu_gate — g/u are LEFT-TO-RIGHT dot products, sig = 1/(1+exp(clamp(-g,
    // ±40))) via scalar rt.math_exp (= math.exp), out = (g*sig)*u. The self-host transcribes
    // the exact accumulation + op order, byte-exact vs v0 `--target wasm`.
    let src = "effect fn main() -> Unit = {\n        let x = matrix.from_lists([[1.0, 2.0, 0.0 - 1.0], [0.5, 0.0 - 3.0, 2.0]])\n        let wg = matrix.from_lists([[0.1, 0.2, 0.3], [0.0 - 0.4, 0.5, 0.0 - 0.6], [1.0, 0.0, 0.0 - 1.0], [0.2, 0.2, 0.2]])\n        let wu = matrix.from_lists([[0.5, 0.0 - 0.5, 1.0], [0.3, 0.3, 0.3], [0.0 - 1.0, 1.0, 0.0], [0.7, 0.0 - 0.2, 0.1]])\n        let ls = matrix.to_lists(matrix.swiglu_gate(x, wg, wu))\n        for row in ls { for v in row { println(float.to_string(v)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.swiglu_gate"), "swiglu self-host must link");
    if let Some(out) = build_and_run("matrix_swiglu", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 8, "2 rows × 4 out channels");
        assert_eq!(out.lines().next().unwrap(), "-0.16495019896903929");
    }
}

#[test]
fn matrix_rope_rotate_byte_matches_scalar_oracle() {
    // Phase D1: RoPE — per (row=pos, head, pair) rotate by inv_freq = exp(-(2i/head_dim)*
    // log theta), angle = pos*inv_freq, (x0*cos-x1*sin, x0*sin+x1*cos), via scalar self-hosted
    // math.{exp,log,sin,cos}. Op order transcribed exactly → byte-exact vs v0 `--target wasm`.
    let src = "effect fn main() -> Unit = {\n        let x = matrix.from_lists([\n        [1.0, 0.0, 0.5, 0.0 - 0.5, 2.0, 1.0, 0.0 - 1.0, 0.3],\n        [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]])\n        let ls = matrix.to_lists(matrix.rope_rotate(x, 2, 4, 10000.0))\n        for row in ls { for v in row { println(float.to_string(v)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.rope_rotate"), "rope self-host must link");
    if let Some(out) = build_and_run("matrix_rope", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 16, "2 rows × 8 cols");
        assert_eq!(out.lines().next().unwrap(), "1.0");
    }
}

#[test]
fn matrix_multi_head_attention_byte_matches_the_canonical_fast_exp() {
    // ORACLE CHANGED (#1197): this pinned the retired promise "the wasm leg
    // reproduces v0's scalar libm exp". The leg now runs the CANONICAL fast-exp
    // — the same unfused algorithm, reduction order and scaling spelling the
    // native SIMD kernel runs — so the pinned value is the one BOTH legs
    // produce (verified by running this very program on native and wasm, and by
    // spec/wasm_cross/matrix_softmax_fastexp.almd under C-223).

    // Phase D1: MHA — per head, per query row: scaled Q·K^T (+ causal -1e9 mask), softmax
    // (scalar rt.math_exp = math.exp), weighted V-sum. Heads write DISJOINT columns so the
    // i-outer/h-inner self-host is byte-identical to v0's h-outer/i-inner `--target wasm`.
    let src = "effect fn main() -> Unit = {\n        let q = matrix.from_lists([[1.0, 0.0, 0.5, 0.0 - 0.5], [0.2, 0.3, 0.0 - 0.1, 0.4], [1.0, 1.0, 0.0, 0.0]])\n        let k = matrix.from_lists([[0.5, 0.5, 1.0, 0.0], [0.0 - 0.2, 0.1, 0.3, 0.0 - 0.4], [0.7, 0.0 - 0.3, 0.2, 0.9]])\n        let v = matrix.from_lists([[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0], [0.0 - 1.0, 0.0 - 2.0, 0.5, 0.25]])\n        for row in matrix.to_lists(matrix.multi_head_attention(q, k, v, 2)) { for x in row { println(float.to_string(x)) } }\n        for row in matrix.to_lists(matrix.masked_multi_head_attention(q, k, v, 2)) { for x in row { println(float.to_string(x)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.masked_multi_head_attention"), "masked mha self-host must link");
    if let Some(out) = build_and_run("matrix_mha", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 24, "2×(3 rows × 4 cols)");
        assert_eq!(out.lines().next().unwrap(), "1.0487146726665257");
    }
}

#[test]
fn matrix_from_q1_0_bytes_byte_matches_oracle() {
    // Phase D1 (final): Q1_0 dequant — fp16 scale decode + per-weight sign bit (1→+scale,
    // 0→-scale) over an 18-byte/128-weight block. Pure bit-ops via prim.band/bshr_u/bshl/bor
    // + bits_to_f32. Byte-exact vs v0 `--target wasm`.
    let src = "effect fn main() -> Unit = {\n\
        let b = bytes.from_list([0, 56, 170, 204, 15, 240, 0, 255, 51, 102, 129, 66, 24, 60, 195, 60, 90, 165])\n\
        let m = matrix.from_q1_0_bytes(b, 0, 2, 8)\n\
        for row in matrix.to_lists(m) { for x in row { println(float.to_string(x)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.from_q1_0_bytes"), "q1_0 self-host must link");
    if let Some(out) = build_and_run("matrix_q1_0", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 16, "2 rows × 8 cols");
        // fp16 0x3800 = 0.5; first sign byte 0xAA = 10101010 → bit0=0 → -0.5.
        assert_eq!(out.lines().next().unwrap(), "-0.5");
    }
}

#[test]
fn value_field_byte_matches_oracle() {
    // B-2 prerequisite: value.field(v, key) self-host — Object tag check + linear key scan,
    // Ok(field) / Err("missing field '<k>'") / Err("expected Object"), byte-exact vs v0.
    let src = "fn get_id(v: Value) -> Int =\n\
        match value.field(v, \"id\") { ok(fv) => value.as_int(fv) ?? 0 - 1, err(_) => 0 - 2 }\n\
        effect fn main() -> Unit = {\n\
        match json.parse(\"{\\\"id\\\":7}\") { ok(v) => println(int.to_string(get_id(v))), err(_) => println(\"perr\") } }\n";
    let prog = lower_source(&format!("import json\n{src}"));
    assert!(prog.functions.iter().any(|f| f.name == "value.field"), "value.field self-host must link");
    if let Some(out) = build_and_run("value_field", &render_wasm_program(&prog)) {
        assert_eq!(out, "6"); // (as_int ?? 0) - 1 = 6 — matches v0
    }
}

#[test]
fn derived_codec_decode_chain_lowers_and_byte_matches() {
    // B-2: the derived Codec `T.decode(v)` chain — `let f = value.as_T(value.field(v,k)?)?; …;
    // ok(T{…})`. Two fixes compose: (1) the nested call-arg `?` (Try) is lifted to a separate
    // bind so the proven nested value-Result match lowers (extract_first_callarg_unwrap), and
    // (2) the derive tags each record-field value with its DECLARED type (not Ty::Unknown) so
    // the v1 record builder stores a scalar `Int` field directly instead of the rc_inc +
    // i64.extend_i32_u heap path that emitted invalid wasm. Single, multi, and NESTED-record
    // fields all byte-match v0 `--target wasm`.
    let src = "type Inner: Codec = { x: Int, y: Int }\n\
        type Config: Codec = { host: String, port: Int, inner: Inner }\n\
        effect fn main() -> Unit = {\n\
        let text = \"{\\\"host\\\":\\\"h\\\",\\\"port\\\":8080,\\\"inner\\\":{\\\"x\\\":1,\\\"y\\\":2}}\"\n\
        match json.parse(text) {\n\
        ok(v) => match Config.decode(v) {\n\
        ok(c) => println(c.host + \":\" + int.to_string(c.port) + \" \" + int.to_string(c.inner.x))\n\
        err(e) => println(\"e:\" + e) }\n\
        err(_) => println(\"perr\") } }\n";
    let prog = lower_source(&format!("import json\n{src}"));
    assert!(prog.functions.iter().any(|f| f.name == "Config.decode"), "Config.decode must link");
    assert!(prog.functions.iter().any(|f| f.name == "Inner.decode"), "nested Inner.decode must link");
    if let Some(out) = build_and_run("codec_decode", &render_wasm_program(&prog)) {
        assert_eq!(out, "h:8080 1");
    }
}

#[test]
fn derived_codec_list_and_default_fields_decode() {
    // B-2 extension: Codec `List[T]` fields (self-hosted __decode_list_T / __encode_list_T
    // over value.as_T / value.array) and DEFAULT fields (__decode_default_T: absent/Null →
    // default). Both decode + the generated encode method byte-match v0 `--target wasm`.
    let src = "type Rec: Codec = { id: Int, tags: List[Int], names: List[String] }\n\
        type Cfg: Codec = { host: String = \"localhost\", port: Int = 8080, tags: List[String] }\n\
        effect fn main() -> Unit = {\n\
        match json.parse(\"{\\\"id\\\":5,\\\"tags\\\":[1,2,3],\\\"names\\\":[\\\"a\\\",\\\"b\\\"]}\") {\n\
        ok(v) => match Rec.decode(v) { ok(r) => println(int.to_string(r.id) + \" \" + int.to_string(list.len(r.tags)) + \" \" + int.to_string(list.len(r.names))), err(_e) => println(\"e\") }\n\
        err(_) => println(\"perr\") }\n\
        match json.parse(\"{\\\"tags\\\":[\\\"x\\\"]}\") {\n\
        ok(v) => match Cfg.decode(v) { ok(c) => println(c.host + \" \" + int.to_string(c.port)), err(_e) => println(\"e\") }\n\
        err(_) => println(\"perr\") } }\n";
    let prog = lower_source(&format!("import json\n{src}"));
    assert!(prog.functions.iter().any(|f| f.name == "__decode_list_int"), "list decode helper must link");
    assert!(prog.functions.iter().any(|f| f.name == "__decode_default_int"), "default decode helper must link");
    if let Some(out) = build_and_run("codec_list_default", &render_wasm_program(&prog)) {
        assert_eq!(out, "5 3 2\nlocalhost 8080");
    }
}

#[test]
fn derived_variant_codec_decode_all_payload_shapes() {
    // Derived-Codec DECODE of tagged variants across every payload shape the trust-spine handles:
    // a nested scalar-record field (Wrap(Color)), a record-shaped case with a String + nested record
    // (Tag), a List field (Multi), a tuple with scalar/String fields (Pair), and unit (Plain). The
    // decode reads the tag as a plain String (value.keys |> list.get ?? "") + the payload via
    // value.field — NOT a (String, Value) tuple the trust-spine walls — then `ok(Ctor(..))`
    // materializes the variant (a nested scalar-record field stored + freed by the masked rc_dec).
    let src = "type Color: Codec = { r: Int, g: Int, b: Int }\n\
        type Labeled: Codec = { label: String, n: Int }\n\
        type Shape: Codec = | Wrap(Color) | Boxed(Labeled) | Tag { name: String, c: Color } | Multi(List[Int]) | Pair(Int, String) | Plain\n\
        effect fn main() -> Unit = {\n\
        match Shape.decode(Shape.encode(Wrap({ r: 1, g: 2, b: 3 }))) { ok(s) => match s { Wrap(c) => println(int.to_string(c.g)), _ => println(\"?\") }, err(e) => println(e) }\n\
        match Shape.decode(Shape.encode(Boxed({ label: \"z\", n: 8 }))) { ok(s) => match s { Boxed(i) => println(i.label + \" \" + int.to_string(i.n)), _ => println(\"?\") }, err(e) => println(e) }\n\
        match Shape.decode(Shape.encode(Tag { name: \"hi\", c: { r: 4, g: 5, b: 6 } })) { ok(s) => match s { Tag { name, c } => println(name + \" \" + int.to_string(c.b)), _ => println(\"?\") }, err(e) => println(e) }\n\
        match Shape.decode(Shape.encode(Multi([1, 2, 3]))) { ok(s) => match s { Multi(xs) => println(int.to_string(list.len(xs))), _ => println(\"?\") }, err(e) => println(e) }\n\
        match Shape.decode(Shape.encode(Pair(7, \"x\"))) { ok(s) => match s { Pair(n, t) => println(int.to_string(n) + t), _ => println(\"?\") }, err(e) => println(e) }\n\
        match Shape.decode(Shape.encode(Plain)) { ok(s) => match s { Plain => println(\"plain\"), _ => println(\"?\") }, err(e) => println(e) } }\n";
    let prog = lower_source(&format!("import json\n{src}"));
    assert!(prog.functions.iter().any(|f| f.name == "Shape.decode"), "Shape.decode must link");
    if let Some(out) = build_and_run("variant_codec_decode", &render_wasm_program(&prog)) {
        assert_eq!(out, "2\nz 8\nhi 6\n3\n7x\nplain");
    }
}

#[test]
fn option_interp_self_hosts_per_element_type() {
    // `${Option[Int]}` / `${Option[Bool]}` render v0's `some(<v>)` / `none` via a per-element self-host
    // (`option.to_string` / `option.to_string_b` — a 2-arm Option match + string concat), routed by
    // element type exactly like the List family. A String/Float element stays an unlinked clean wall.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[Int] = some(42) let b: Option[Int] = some(-7) let c: Option[Int] = none\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"v=${a}!\")\n\
        let s: Option[String] = some(\"hi\") let q: Option[String] = some(\"a \\\"b\\\"\") let sn: Option[String] = none\n\
        println(\"${s}\") println(\"${q}\") println(\"${sn}\")\n\
        let fa: Option[Float] = some(3.5) let fb: Option[Float] = some(3.0)\n\
        println(\"${fa}\") println(\"${fb}\")\n\
        let t: Option[Bool] = some(true) let f: Option[Bool] = none\n\
        println(\"${t}\") println(\"${f}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string"), "Option[Int] interp must auto-link option.to_string");
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_s"), "Option[String] interp must auto-link option.to_string_s");
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_f"), "Option[Float] interp must auto-link option.to_string_f");
    if let Some(out) = build_and_run("option_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "some(42)\nsome(-7)\nnone\nv=some(42)!\nsome(\"hi\")\nsome(\"a \\\"b\\\"\")\nnone\nsome(3.5)\nsome(3)\nsome(true)\nnone");
    }
}

#[test]
fn nonempty_map_literal_materializes_via_from_list() {
    // A non-empty map literal used to lower to a DEFERRED-Opaque empty block, so `map.len`/`map.get`
    // silently read 0 (a miscompile). Routing it through `map.from_list` materializes a real map, so
    // the ops byte-match v0. (Regression guard for the silent-miscompile fix.)
    let src = "fn probe(m: Map[String, Int]) -> String = {\n\
        \"len=\" + int.to_string(map.len(m)) + \" x=\" + int.to_string(map.get(m, \"x\") ?? -1)\n\
        }\n\
        effect fn main() -> Unit = {\n\
        let a: Map[String, Int] = [\"x\": 1, \"y\": 2, \"z\": 3]\n\
        println(probe(a)) println(int.to_string(map.len(a))) }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("map_literal", &render_wasm_program(&prog)) {
        assert_eq!(out, "len=3 x=1\n3");
    }
}

#[test]
fn map_interp_self_hosts_via_keys_values() {
    // `${Map[String, Int]}` renders v0's `["k": v, …]` (empty → `[:]`; keys quoted). `map.to_string`
    // reads keys/values via the callable `map.keys`/`map.values` (unblocked by the map-literal
    // materialization fix) and renders each entry inline; both owned lists drop at scope end.
    let src = "effect fn main() -> Unit = {\n\
        let a: Map[String, Int] = [\"x\": 1, \"y\": 2] let b: Map[String, Int] = [:] let c: Map[String, Int] = [\"n\": -5]\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"m=${a}!\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "map.to_string"), "Map[String,Int] interp must auto-link map.to_string");
    if let Some(out) = build_and_run("map_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "[\"x\": 1, \"y\": 2]\n[:]\n[\"n\": -5]\nm=[\"x\": 1, \"y\": 2]!");
    }
}

#[test]
fn set_interp_self_hosts_via_to_list() {
    // `${Set[Int]}` renders v0's `set.from_list([<elems>])` (insertion order, dedup). `set.to_string`
    // reads the elements via the callable `set.to_list` and renders the body inline like
    // `list.to_string`; the owned `set.to_list` result is dropped at scope end (no leak).
    let src = "effect fn main() -> Unit = {\n\
        let a: Set[Int] = set.from_list([3, 1, 2, 1]) let b: Set[Int] = set.from_list([]) let c: Set[Int] = set.from_list([-5, 10])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"s=${a}!\")\n\
        let sa: Set[String] = set.from_list([\"b\", \"a\", \"b\"]) let sc: Set[String] = set.from_list([\"q\"])\n\
        println(\"${sa}\") println(\"${sc}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "set.to_string"), "Set[Int] interp must auto-link set.to_string");
    assert!(prog.functions.iter().any(|f| f.name == "set.to_string_s"), "Set[String] interp must auto-link set.to_string_s");
    if let Some(out) = build_and_run("set_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "set.from_list([3, 1, 2])\nset.from_list([])\nset.from_list([-5, 10])\ns=set.from_list([3, 1, 2])!\nset.from_list([\"b\", \"a\"])\nset.from_list([\"q\"])");
    }
}

#[test]
fn result_list_str_interp() {
    // `${Result[List[String], String]}` → `ok(["a", "b"])` / `err("<quoted>")`. `result.to_string_ls`
    // renders the Ok string-list (each element quoted+escaped) reusing `result_to_string`'s `__rts_esc_*`.
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[List[String], String] = ok([\"a\", \"b\"]) let b: Result[List[String], String] = err(\"boom\") let c: Result[List[String], String] = ok([])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_ls"), "must auto-link result.to_string_ls");
    if let Some(out) = build_and_run("result_list_str_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok([\"a\", \"b\"])\nerr(\"boom\")\nok([])");
    }
}

#[test]
fn result_list_int_interp_and_construction() {
    // `${Result[List[Int], String]}` → `ok([1, 2, 3])` / `err("<quoted>")`. The ResultOk heap
    // materializer admits a scalar-list literal (incl empty `ok([])`); `result.to_string_li` renders.
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[List[Int], String] = ok([4, 5]) let b: Result[List[Int], String] = err(\"boom\") let c: Result[List[Int], String] = ok([])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_li"), "must auto-link result.to_string_li");
    if let Some(out) = build_and_run("result_list_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok([4, 5])\nerr(\"boom\")\nok([])");
    }
}

#[test]
fn option_option_int_interp() {
    // `${Option[Option[Int]]}` → `some(some(5))` / `some(none)` / `none` (nested Option interp), the
    // self-host `option.to_string_oi` over the already-materializing nested-Option construction.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[Option[Int]] = some(some(5)) let b: Option[Option[Int]] = some(none) let c: Option[Option[Int]] = none\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_oi"), "must auto-link option.to_string_oi");
    if let Some(out) = build_and_run("option_option_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "some(some(5))\nsome(none)\nnone");
    }
}

#[test]
fn nested_interp_batch2_compositions_and_result_map() {
    // Option[Option[List[Int]]], Option[Result[List[Int],String]], Result[Option[List[String]],String],
    // Result[Map[String,Int],String] (the last with a ResultOk map materialization).
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[Option[List[Int]]] = some(some([1, 2])) let b: Option[Result[List[Int], String]] = some(ok([3, 4]))\n\
        let c: Result[Option[List[String]], String] = ok(some([\"x\"])) let d: Result[Map[String, Int], String] = ok([\"k\": 5])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_msi"), "must auto-link result.to_string_msi");
    if let Some(out) = build_and_run("nested_batch2", &render_wasm_program(&prog)) {
        assert_eq!(out, "some(some([1, 2]))\nsome(ok([3, 4]))\nok(some([\"x\"]))\nok([\"k\": 5])");
    }
}

#[test]
fn option_map_string_int_interp() {
    // `${Option[Map[String,Int]]}` — the non-empty map is a map.from_list computed payload materialized
    // into the Some slot; rendered via map.keys/map.values wrapped in some(…).
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[Map[String, Int]] = some([\"a\": 1, \"b\": 2]) let b: Option[Map[String, Int]] = none\n\
        println(\"${a}\") println(\"${b}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_msi"), "must auto-link option.to_string_msi");
    if let Some(out) = build_and_run("option_map", &render_wasm_program(&prog)) {
        assert_eq!(out, "some([\"a\": 1, \"b\": 2])\nnone");
    }
}

#[test]
fn float_list_option_and_result_interp() {
    // Option[List[Float]] / Result[List[Float],String] — each element float.to_string with drop-.0.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[List[Float]] = some([1.5, 2.0]) let b: Option[List[Float]] = some([])\n\
        let c: Result[List[Float], String] = ok([100.0, 0.5]) let d: Result[List[Float], String] = err(\"x\")\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_lf"), "must auto-link option.to_string_lf");
    if let Some(out) = build_and_run("float_list", &render_wasm_program(&prog)) {
        assert_eq!(out, "some([1.5, 2])\nsome([])\nok([100, 0.5])\nerr(\"x\")");
    }
}

#[test]
fn nested_interp_float_deep_option_and_result_option_list() {
    // Result[Float,String] (float drop-.0), Option[Option[Option[Int]]] (3-deep), and
    // Result[Option[List[Int]],String] (int-list under ok(some …)).
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[Float, String] = ok(3.5) let b: Result[Float, String] = ok(4.0)\n\
        let c: Option[Option[Option[Int]]] = some(some(some(5))) let d: Option[Option[Option[Int]]] = some(none)\n\
        let e: Result[Option[List[Int]], String] = ok(some([1, 2])) let f: Result[Option[List[Int]], String] = ok(none)\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") println(\"${e}\") println(\"${f}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_f"), "must auto-link result.to_string_f");
    if let Some(out) = build_and_run("nested_more", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok(3.5)\nok(4)\nsome(some(some(5)))\nsome(none)\nok(some([1, 2]))\nok(none)");
    }
}
