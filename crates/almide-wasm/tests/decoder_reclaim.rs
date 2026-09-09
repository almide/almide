//! Decoder scratch storage must be reused across calls (#2046).
mod harness;

#[test]
fn extraction_preserves_constant_stack_tail_calls() {
    let source = r#"effect fn bounce(n: Int) -> Int = if n == 0 then 42 else bounce(n - 1)!
effect fn main() -> Unit = println(int.to_string(bounce(200000)!))
"#;
    let ir = almide_spine::s5::lower_to_ir("tail_extraction.almd", source).unwrap();
    let bytes = almide_wasm::emit_program(&ir).unwrap();
    let out = harness::run_wasm(&bytes).unwrap();
    assert_eq!(out.exit, 0, "{}", out.stderr);
    assert_eq!(out.stdout.trim(), "42");
}

#[test]
fn extraction_preserves_mutable_call_writeback() {
    for (name, expected) in [
        ("effect_mut_generic_port", "1 [\"z\", \"a\", \"a2\"]"),
        ("mut_param_call_chain", "1\nsum=2080\np0=15\ngrown=99 last=99"),
        ("mut_param_effect_never_err", "6\n6\n6\n6"),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../spec/wasm_cross/{name}.almd"));
        let source = std::fs::read_to_string(&path).unwrap();
        let ir = almide_spine::s5::lower_to_ir(&path.to_string_lossy(), &source).unwrap();
        let bytes = almide_wasm::emit_program(&ir).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let out = harness::run_wasm(&bytes).unwrap();
        assert_eq!(out.exit, 0, "{name}: {}", out.stderr);
        assert_eq!(out.stdout.trim(), expected, "{name}: writeback output");
    }
}

fn heap_after(n: u32, declarations: &str, input: &str, decode: &str, expected: u32) -> u64 {
    let source = format!(
        r#"import json
{declarations}
effect fn main() -> Unit = {{
  let v = json.parse("{input}")!
  var total = 0
  for i in 0..<{n} {{
    total = total + ({decode})
  }}
  println(int.to_string(total))
}}
"#
    );
    let ir = almide_spine::s5::lower_to_ir("decoder_reclaim.almd", &source).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("structural wasm");
    let out = harness::run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    assert_eq!(out.stdout.trim(), (expected * n).to_string());
    out.heap_end.expect("heap watermark")
}

#[test]
fn repeated_derived_decode_reclaims_scratch_storage() {
    flat(
        "type Flat: Codec = { id: Int, name: String }",
        r#"{\"id\":42,\"name\":\"Ada Lovelace\"}"#,
        "match Flat.decode(v) { ok(u) => u.id, err(_) => 0 }",
        42,
    );
}

fn flat(declarations: &str, input: &str, decode: &str, expected: u32) {
    let h1 = heap_after(1000, declarations, input, decode, expected);
    let h8 = heap_after(8000, declarations, input, decode, expected);
    assert_eq!(h1, h8, "decoder heap grows with call count");
}

#[test]
fn nested_derived_decode_reclaims_children() {
    flat(
        "type Child: Codec = { name: String }\ntype Parent: Codec = { kids: List[Child] }",
        r#"{\"kids\":[{\"name\":\"Ada\"},{\"name\":\"Grace\"}]}"#,
        "match Parent.decode(v) { ok(p) => list.len(p.kids), err(_) => 0 }",
        2,
    );
}

#[test]
fn failed_derived_decode_reclaims_partial_result() {
    flat(
        "type Flat: Codec = { name: String, id: Int }",
        r#"{\"name\":\"Ada\",\"id\":false}"#,
        "match Flat.decode(v) { ok(_) => 0, err(e) => if string.len(e) > 0 then 1 else 0 }",
        1,
    );
}

#[test]
fn missing_field_decode_reclaims_error_storage() {
    flat(
        "type Flat: Codec = { name: String, id: Int }",
        r#"{\"name\":\"Ada\"}"#,
        "match Flat.decode(v) { ok(_) => 0, err(e) => if string.len(e) > 0 then 1 else 0 }",
        1,
    );
}
