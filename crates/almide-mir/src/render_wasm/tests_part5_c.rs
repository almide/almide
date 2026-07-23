
#[test]
fn matrix_swiglu_gate_byte_matches_scalar_libm_oracle() {
    // Phase D1: swiglu_gate — g/u are LEFT-TO-RIGHT dot products, sig = 1/(1+exp(clamp(-g,
    // ±40))) via scalar rt.math_exp (= math.exp), out = (g*sig)*u. The self-host transcribes
    // the exact accumulation + op order, byte-exact vs v0 `--target wasm`.
    let src = "effect fn main() -> Unit = {\n        let x = matrix.from_lists([[1.0, 2.0, 0.0 - 1.0], [0.5, 0.0 - 3.0, 2.0]])\n        let wg = matrix.from_lists([[0.1, 0.2, 0.3], [0.0 - 0.4, 0.5, 0.0 - 0.6], [1.0, 0.0, 0.0 - 1.0], [0.2, 0.2, 0.2]])\n        let wu = matrix.from_lists([[0.5, 0.0 - 0.5, 1.0], [0.3, 0.3, 0.3], [0.0 - 1.0, 1.0, 0.0], [0.7, 0.0 - 0.2, 0.1]])\n        let ls = matrix.to_lists(matrix.swiglu_gate(x, wg, wu))\n        for row in ls { for v in row { println(float.to_string(v)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.swiglu_gate"), "swiglu self-host must link");
    if let Some(out) = build_and_run("matrix_swiglu", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 8, "2 rows × 4 out channels");
        assert_eq!(out.lines().next().unwrap(), "-0.1649501991937434");
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
fn matrix_multi_head_attention_byte_matches_scalar_oracle() {
    // Phase D1: MHA — per head, per query row: scaled Q·K^T (+ causal -1e9 mask), softmax
    // (scalar rt.math_exp = math.exp), weighted V-sum. Heads write DISJOINT columns so the
    // i-outer/h-inner self-host is byte-identical to v0's h-outer/i-inner `--target wasm`.
    let src = "effect fn main() -> Unit = {\n        let q = matrix.from_lists([[1.0, 0.0, 0.5, 0.0 - 0.5], [0.2, 0.3, 0.0 - 0.1, 0.4], [1.0, 1.0, 0.0, 0.0]])\n        let k = matrix.from_lists([[0.5, 0.5, 1.0, 0.0], [0.0 - 0.2, 0.1, 0.3, 0.0 - 0.4], [0.7, 0.0 - 0.3, 0.2, 0.9]])\n        let v = matrix.from_lists([[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0], [0.0 - 1.0, 0.0 - 2.0, 0.5, 0.25]])\n        for row in matrix.to_lists(matrix.multi_head_attention(q, k, v, 2)) { for x in row { println(float.to_string(x)) } }\n        for row in matrix.to_lists(matrix.masked_multi_head_attention(q, k, v, 2)) { for x in row { println(float.to_string(x)) } } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "matrix.masked_multi_head_attention"), "masked mha self-host must link");
    if let Some(out) = build_and_run("matrix_mha", &render_wasm_program(&prog)) {
        assert_eq!(out.lines().count(), 24, "2×(3 rows × 4 cols)");
        assert_eq!(out.lines().next().unwrap(), "1.0487146726713201");
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

#[test]
fn nested_interp_min_int_and_computed_list_payloads() {
    // Two adversarial-fuzz regressions: (A) i64::MIN in a list interp rendered "-0" (negate overflow),
    // (B) some/ok of a COMPUTED list read none/ok([]). Both fixed.
    let src = "effect fn main() -> Unit = {\n\
        let mn: List[Int] = [0 - 9223372036854775807 - 1, 7] println(\"${mn}\")\n\
        let a: Option[List[Int]] = some(list.map([1, 2, 3], (n) => n * 2)) println(\"${a}\")\n\
        let b: Result[List[Int], String] = ok([1, 2] + [3]) println(\"${b}\")\n\
        let c: Option[List[Bool]] = some(list.map([1, 2], (n) => n > 1)) println(\"${c}\") }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("nested_edgecases", &render_wasm_program(&prog)) {
        assert_eq!(out, "[-9223372036854775808, 7]\nsome([2, 4, 6])\nok([1, 2, 3])\nsome([false, true])");
    }
}

#[test]
fn result_outer_nested_interp() {
    // Result-outer nested `${…}`: the ResultOk heap materializer admits a nested Option/Result ctor
    // Ok payload (construction) and the nested-payload bind seeds its read-shape (inner match).
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[Bool, String] = ok(true) let b: Result[List[Bool], String] = ok([true, false])\n\
        let c: Result[Option[Int], String] = ok(some(5)) let d: Result[Option[Int], String] = ok(none)\n\
        let e: Result[Result[Int, String], String] = ok(err(\"x\"))\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") println(\"${e}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_b"), "must auto-link result.to_string_b");
    if let Some(out) = build_and_run("result_nested", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok(true)\nok([true, false])\nok(some(5))\nok(none)\nok(err(\"x\"))");
    }
}

#[test]
fn option_outer_nested_interp() {
    // Option-outer nested `${…}`, incl a cap-as-tag inner Result[String,String] (the seed_variant_param
    // nested-payload fix) and a bool-list inner.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[Option[Bool]] = some(some(true)) let b: Option[Option[String]] = some(some(\"a\"))\n\
        let c: Option[Result[Int, String]] = some(ok(5)) let d: Option[Result[String, String]] = some(ok(\"q\"))\n\
        let e: Option[List[Bool]] = some([false, true])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") println(\"${e}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_rs"), "must auto-link option.to_string_rs");
    if let Some(out) = build_and_run("option_nested", &render_wasm_program(&prog)) {
        assert_eq!(out, "some(some(true))\nsome(some(\"a\"))\nsome(ok(5))\nsome(ok(\"q\"))\nsome([false, true])");
    }
}

#[test]
fn option_list_str_interp() {
    // `${Option[List[String]]}` → `some(["a", "b"])` / `none` — a HEAP-element inner list. The self-host
    // `option.to_string_ls` inlines the string quote+escape (\ " \n \r \t) since self-hosts can't call
    // each other. Escaping is exercised by the embedded quote/backslash.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[List[String]] = some([\"a\", \"b\"]) let b: Option[List[String]] = none let c: Option[List[String]] = some([])\n\
        let d: Option[List[String]] = some([\"q\\\"x\"])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_ls"), "must auto-link option.to_string_ls");
    if let Some(out) = build_and_run("option_list_str_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "some([\"a\", \"b\"])\nnone\nsome([])\nsome([\"q\\\"x\"])");
    }
}

#[test]
fn option_list_int_interp_and_construction() {
    // `${Option[List[Int]]}` → `some([1, 2, 3])` / `none` (nested compound). Two gaps close: the
    // OptionSome heap materializer now admits a scalar-list literal (incl the empty `some([])`), and
    // the self-host `option.to_string_li` renders it. A constructed Some list is also matchable.
    let src = "fn describe(o: Option[List[Int]]) -> String = match o { some(v) => int.to_string(list.len(v)), none => \"none\" }\n\
        effect fn main() -> Unit = {\n\
        let a: Option[List[Int]] = some([1, 2, 3]) let b: Option[List[Int]] = none let c: Option[List[Int]] = some([])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\")\n\
        println(describe(a) + \",\" + describe(c)) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_li"), "Option[List[Int]] interp must auto-link option.to_string_li");
    if let Some(out) = build_and_run("option_list_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "some([1, 2, 3])\nnone\nsome([])\n3,0");
    }
}

#[test]
fn noncapturing_lambda_returned_as_funcref() {
    // A function RETURNING a non-capturing lambda / a bare fn reference — the trust-spine lifts it to
    // a table slot and returns the scalar funcref; the caller tracks the bound result so `f(args)`
    // dispatches through CallIndirect. A capturing closure still walls (a real env is a later brick).
    let src = "fn inc() -> (Int) -> Int = (x) => x + 1\n\
        fn tp(x: Int) -> Int = x * 2 + 3\n\
        fn getter() -> (Int) -> Int = tp\n\
        effect fn main() -> Unit = {\n\
        let f = inc() println(int.to_string(f(5))) println(int.to_string(f(41)))\n\
        let h = getter() println(int.to_string(h(6))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "inc"), "inc must lower");
    if let Some(out) = build_and_run("closure_return", &render_wasm_program(&prog)) {
        assert_eq!(out, "6\n42\n15");
    }
}

#[test]
fn fan_race_and_any_inline_literal_thunk_lists() {
    // `fan.race`/`fan.any` over a LITERAL thunk list, deterministic on wasm: race takes thunk[0]'s
    // result (head even if it errs); any takes the FIRST Ok in order (else v0's fixed `fan.any: all
    // candidates failed`). Both inline into a plain match chain, avoiding a List[funcref] — race the
    // first thunk's body, any the outer arms folded into each thunk level.
    let src = "effect fn okn(n: Int) -> Result[Int, String] = ok(n * 3)\n\
        effect fn failing() -> Result[Int, String] = err(\"boom\")\n\
        effect fn main() -> Unit = {\n\
        match fan.race([() => okn(10), () => okn(20)]) { ok(v) => println(\"r=\" + int.to_string(v)), err(e) => println(e) }\n\
        match fan.race([() => failing(), () => okn(9)]) { ok(v) => println(\"ok\"), err(e) => println(\"e=\" + e) }\n\
        match fan.any([() => failing(), () => okn(7)]) { ok(v) => println(\"any=\" + int.to_string(v)), err(e) => println(e) }\n\
        match fan.any([() => failing(), () => failing()]) { ok(v) => println(\"ok\"), err(e) => println(\"af=\" + e) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("fan_race_any", &render_wasm_program(&prog)) {
        assert_eq!(out, "r=30\ne=boom\nany=21\naf=fan.any: all candidates failed");
    }
}

#[test]
fn fan_map_int_lowers_to_self_host_traverse() {
    // `fan.map` over List[Int] with an (Int) -> Result[Int, String] callback — the compiler intrinsic
    // routed to the self-host `fan_map` (a fallible traverse invoking the lifted callback via
    // CallIndirect), collecting ok values in list order and short-circuiting on the first err. The
    // result is matched / auto-`!`-unwrapped.
    let src = "effect fn dbl(x: Int) -> Result[Int, String] = ok(x * 2)\n\
        effect fn checked(x: Int) -> Result[Int, String] = if x < 0 then err(\"neg\") else ok(x)\n\
        effect fn main() -> Unit = {\n\
        let doubled = fan.map([1, 2, 3], (x) => dbl(x))\n\
        println(int.to_string(doubled[0]) + \",\" + int.to_string(doubled[2]))\n\
        match fan.map([1, -2, 3], (x) => checked(x)) { ok(ys) => println(\"ok\"), err(e) => println(\"short:\" + e) } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "fan.map"), "fan.map must auto-link the fan_map self-host");
    if let Some(out) = build_and_run("fan_map_int", &render_wasm_program(&prog)) {
        assert_eq!(out, "2,6\nshort:neg");
    }
}

#[test]
fn higher_order_result_traverse_matches_call_indirect() {
    // A fallible list traverse over a funcref callback — `match f(x) { ok => .., err => .. }` where
    // `f` is invoked via CallIndirect. The trust-spine seeds the CallIndirect result's read-shape and
    // hoists a computed-call match subject, so the traverse (short-circuit on first err) lowers. The
    // sequential `fan.map` semantics on wasm.
    let src = "fn go(xs: List[Int], f: (Int) -> Result[Int, String], i: Int, acc: List[Int]) -> Result[List[Int], String] =\n\
        if i >= list.len(xs) then ok(acc)\n\
        else match f(list.get(xs, i) ?? 0) { ok(y) => go(xs, f, i + 1, acc + [y]), err(e) => err(e) }\n\
        fn traverse(xs: List[Int], f: (Int) -> Result[Int, String]) -> Result[List[Int], String] = go(xs, f, 0, [])\n\
        fn show(r: Result[List[Int], String]) -> String = match r { ok(ys) => \"ok:\" + int.to_string(list.sum(ys)), err(e) => \"err:\" + e }\n\
        effect fn main() -> Unit = {\n\
        println(show(traverse([1, 2, 3, 4], (x) => ok(x * 2))))\n\
        println(show(traverse([1, -2, 3], (x) => if x > 0 then ok(x) else err(\"neg\")))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "traverse"), "traverse must lower");
    if let Some(out) = build_and_run("ho_traverse", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok:20\nerr:neg");
    }
}

#[test]
fn higher_order_heap_return_via_call_indirect() {
    // `fn apply(g, x) = g(x)` returning a heap value (Result / String) through a known funcref used to
    // wall a tail heap-result computed call; it now executes via `Op::CallIndirect` and moves the
    // owned result out. Opens higher-order functions returning heap values (the fan.map foundation).
    let src = "fn apply_r(f: (Int) -> Result[Int, String], x: Int) -> Result[Int, String] = f(x)\n\
        fn apply_s(f: (Int) -> String, x: Int) -> String = f(x)\n\
        fn show(r: Result[Int, String]) -> String = match r { ok(v) => \"ok:\" + int.to_string(v), err(e) => \"err:\" + e }\n\
        effect fn main() -> Unit = {\n\
        println(show(apply_r((y) => ok(y * 2), 5)))\n\
        println(show(apply_r((y) => if y > 0 then ok(y) else err(\"neg\"), -3)))\n\
        println(apply_s((y) => \"v\" + int.to_string(y), 7)) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "apply_r"), "apply_r must lower");
    if let Some(out) = build_and_run("higher_order_heap", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok:10\nerr:neg\nv7");
    }
}

#[test]
fn result_ok_err_concat_payload_materializes() {
    // `ok("n" + int.to_string(x))` / `err("bad " + …)` — a computed (ConcatStr) String payload the
    // trust-spine used to wall (only literal/Var/call payloads were handled). It now materializes the
    // concat and moves it into the Result, dropping the borrowed operand temps.
    let src = "fn classify(x: Int) -> Result[String, String] =\n\
        if x > 0 then ok(\"pos \" + int.to_string(x)) else err(\"neg \" + int.to_string(x))\n\
        fn show(r: Result[String, String]) -> String = match r { ok(s) => \"OK:\" + s, err(e) => \"ERR:\" + e }\n\
        effect fn main() -> Unit = { println(show(classify(7))) println(show(classify(-3))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "classify"), "classify must lower");
    if let Some(out) = build_and_run("result_concat", &render_wasm_program(&prog)) {
        assert_eq!(out, "OK:pos 7\nERR:neg -3");
    }
}

#[test]
fn result_interp_self_hosts_per_element_pair() {
    // `${Result[Int, String]}` / `${Result[String, String]}` render v0's `ok(<T>)` / `err(<E>)` via a
    // per-(T,E) self-host (`result.to_string` / `result.to_string_ss`); a String payload is quoted +
    // escaped. Any other pairing stays an unlinked clean wall.
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[Int, String] = ok(42) let b: Result[Int, String] = err(\"bad\")\n\
        println(\"${a}\") println(\"${b}\") println(\"r=${a}!\")\n\
        let c: Result[String, String] = ok(\"hi\") let d: Result[String, String] = err(\"x\")\n\
        println(\"${c}\") println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string"), "Result[Int,String] interp must auto-link result.to_string");
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_ss"), "Result[String,String] interp must auto-link result.to_string_ss");
    if let Some(out) = build_and_run("result_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok(42)\nerr(\"bad\")\nr=ok(42)!\nok(\"hi\")\nerr(\"x\")");
    }
}

#[test]
fn tuple_multifield_and_single_ctor_matches_lower() {
    // A TUPLE-subject match (`desugar_tuple_match`), a MULTI-FIELD variant match (regrouped into a
    // tuple payload sub-match), and a SINGLE-CTOR newtype match (routed through an IfThen merge with
    // an unreachable empty-heap else — no double-move). Every arm's literal/column must select the
    // exact result, byte-identical to v0.
    let src = "type Ev = KV(String, Int) | Tag(String)\n\
        type Rec = Pair(Int, String)\n\
        type Boxed = B(Int)\n\
        fn tup(t: (String, Int)) -> String = match t { (\"a\", 1) => \"A1\", (\"a\", _) => \"AX\", (_, 0) => \"X0\", (_, _) => \"XX\" }\n\
        fn ev(e: Ev) -> String = match e { KV(\"count\", n) => \"C\" + int.to_string(n), KV(_, n) => \"K\" + int.to_string(n), Tag(_) => \"T\" }\n\
        fn rec(r: Rec) -> String = match r { Pair(1, \"one\") => \"1ONE\", Pair(1, _) => \"1X\", Pair(_, _) => \"XX\" }\n\
        fn unbox(b: Boxed) -> String = match b { B(n) => \"b\" + int.to_string(n) }\n\
        effect fn main() -> Unit = {\n\
        println(tup((\"a\", 1))) println(tup((\"z\", 0))) println(tup((\"z\", 5)))\n\
        println(ev(KV(\"count\", 3))) println(ev(KV(\"x\", 7))) println(ev(Tag(\"t\")))\n\
        println(rec(Pair(1, \"one\"))) println(rec(Pair(1, \"z\"))) println(rec(Pair(2, \"z\")))\n\
        println(unbox(B(42))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "unbox"), "single-ctor unbox must lower");
    if let Some(out) = build_and_run("tuple_multifield_match", &render_wasm_program(&prog)) {
        assert_eq!(out, "A1\nX0\nXX\nC3\nK7\nT\n1ONE\n1X\nXX\nb42");
    }
}

#[test]
fn guarded_option_result_match_regroups_and_lowers() {
    // A heap-result `match` over an Option / Result subject whose arms carry GUARDS + LITERAL
    // payloads regroups into constructor dispatch + a scalar payload sub-match (`some(n) if g` →
    // `some($p) => match $p { n if g => .. }`), so the guarded-variant case reduces to the proven
    // variant-tag dispatch + scalar guard/literal chain. A guard/literal MUST select the exact arm.
    let src = "type Tok = Word(String) | Num(Int)\n\
        fn olabel(x: Option[Int]) -> String = match x { some(n) if n > 100 => \"big\", some(n) if n > 0 => \"pos\", some(0) => \"zero\", some(_) => \"neg\", none => \"none\" }\n\
        fn rlabel(r: Result[Int, String]) -> String = match r { ok(v) if v > 0 => \"ok+\", ok(0) => \"ok0\", ok(_) => \"ok-\", err(e) if string.len(e) > 5 => \"eL\", err(_) => \"eS\" }\n\
        fn tclass(t: Tok) -> String = match t { Word(\"hi\") => \"HI\", Word(_) => \"W\", Num(7) => \"SEVEN\", Num(_) => \"N\" }\n\
        effect fn main() -> Unit = {\n\
        println(olabel(some(200))) println(olabel(some(5))) println(olabel(some(0))) println(olabel(none))\n\
        println(rlabel(ok(7))) println(rlabel(ok(0))) println(rlabel(err(\"longmsg\"))) println(rlabel(err(\"no\")))\n\
        println(tclass(Word(\"hi\"))) println(tclass(Word(\"z\"))) println(tclass(Num(7))) println(tclass(Num(3))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "olabel"), "olabel must lower");
    assert!(prog.functions.iter().any(|f| f.name == "tclass"), "tclass must lower");
    if let Some(out) = build_and_run("guarded_variant_match", &render_wasm_program(&prog)) {
        assert_eq!(out, "big\npos\nzero\nnone\nok+\nok0\neL\neS\nHI\nW\nSEVEN\nN");
    }
}

#[test]
fn variant_ctor_in_result_ok_materializes() {
    // `ok(<user-variant ctor>)` in Result-Ok position (the derived variant-decode `ok(Pair(..))`
    // shape) MATERIALIZES the tagged variant block (the SAME block `let p = Pair(..)` builds, with its
    // recursive `$__drop_<V>` drop) and wraps it — NOT a dangling `CallFn "Pair"`. Covers a tuple
    // variant (heap + scalar fields), a scalar variant, a unit variant, and the Err arm; the consumer
    // reads the Ok payload as a real variant. Byte-identical to v0 `--target wasm`.
    let src = "type Shape = | Pair(Int, String) | Solo(Int) | Plain\n\
        fn build(t: Int, n: Int, s: String) -> Result[Shape, String] =\n\
        if t == 0 then ok(Pair(n, s)) else if t == 1 then ok(Solo(n)) else if t == 2 then ok(Plain) else err(\"bad\")\n\
        effect fn main() -> Unit = {\n\
        match build(0, 7, \"x\") { ok(v) => match v { Pair(n, s) => println(int.to_string(n) + s), Solo(n) => println(\"solo\"), Plain => println(\"plain\") }, err(e) => println(e) }\n\
        match build(2, 0, \"\") { ok(v) => match v { Pair(n, s) => println(\"p\"), Solo(n) => println(\"solo\"), Plain => println(\"plain\") }, err(e) => println(e) }\n\
        match build(9, 0, \"\") { ok(v) => match v { Pair(n, s) => println(\"p\"), Solo(n) => println(\"solo\"), Plain => println(\"plain\") }, err(e) => println(e) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("variant_result_ctor", &render_wasm_program(&prog)) {
        assert_eq!(out, "7x\nplain\nbad");
    }
}

#[test]
fn derived_codec_option_fields_decode() {
    // B-2 completion: Codec `Option[T]` fields. The self-hosted `__decode_option_T` builds a
    // `Result[Option[T], String]` (ok(some(x)) / ok(none) / err(e)) — a STRING leaf freed by the
    // recursive `$__drop_opt_str` (`resrec:opt_str`), a SCALAR leaf flat — byte-identical to v0.
    // Encode → decode → re-encode roundtrip: present (Some) survives, absent + explicit-null → None.
    let src = "type Rec: Codec = { name: String, nick: Option[String], age: Option[Int] }\n\
        effect fn main() -> Unit = {\n\
        let r1 = Rec { name: \"A\", nick: some(\"nn\"), age: some(30) }\n\
        let v1 = r1.encode()\n\
        println(json.stringify(v1))\n\
        match Rec.decode(v1) { ok(r) => println(json.stringify(r.encode())), err(e) => println(\"err:\" + e) }\n\
        let pv = json.parse(\"{\\\"name\\\":\\\"B\\\",\\\"age\\\":null}\")\n\
        match pv { ok(pj) => match Rec.decode(pj) { ok(r) => println(json.stringify(r.encode())), err(e) => println(\"err:\" + e) }, err(pe) => println(\"parse:\" + pe) } }\n";
    let prog = lower_source(&format!("import json\n{src}"));
    assert!(prog.functions.iter().any(|f| f.name == "__decode_option_string"), "string option decode helper must link");
    assert!(prog.functions.iter().any(|f| f.name == "__decode_option_int"), "int option decode helper must link");
    if let Some(out) = build_and_run("codec_option_field", &render_wasm_program(&prog)) {
        assert_eq!(
            out,
            "{\"name\":\"A\",\"nick\":\"nn\",\"age\":30}\n\
             {\"name\":\"A\",\"nick\":\"nn\",\"age\":30}\n\
             {\"name\":\"B\",\"nick\":null,\"age\":null}"
        );
    }

}

#[test]
fn heap_and_fn_captures_execute_and_free_via_drop_closure() {
    // CLOSURE ENV FULL MODE: a String capture (co-owned, read back borrowed), a
    // List[Int] capture, and Fn captures (compose — the block captures two other
    // closure blocks, freed by $__drop_closure's SELF-RECURSION). Every closure
    // drop routes through the self-describing $__drop_closure — slot 0 (the
    // fnidx) is never treated as a pointer: a corrupted free would trap here,
    // so a clean byte-matched run IS the slot-0/mask/recursion pin.
    let src = "fn greeter(name: String) -> (String) -> String = (x) => name + \", \" + x\n\
        fn adder(n: Int) -> (Int) -> Int = (x) => x + n\n\
        fn compose(f: (Int) -> Int, g: (Int) -> Int) -> (Int) -> Int = (x) => g(f(x))\n\
        effect fn main() -> Unit = {\n\
        let hi = greeter(\"Hello\")\n\
        println(hi(\"world\"))\n\
        let ns = [10, 20, 30]\n\
        let picker = (i: Int) => list.get(ns, i) ?? 0\n\
        println(int.to_string(picker(1)))\n\
        let h = compose(adder(3), adder(100))\n\
        println(int.to_string(h(5))) }\n";
    let prog = lower_source(src);
    let wat = render_wasm_program(&prog);
    assert!(
        wat.contains("$__drop_closure"),
        "closure drops must route through the uniform recursive $__drop_closure"
    );
    if let Some(out) = build_and_run("heap_fn_captures", &wat) {
        assert_eq!(out, "Hello, world\n20\n108");
    }
}
