//! A list literal of two or more pairs where one pair's component is a `??`
//! fallback (`[("name", a), ("parameters", value.get(f, k) ?? d)]`, the
//! argument of an object constructor). The desugar binds that component in a
//! block where the element stands, `{ let $t = .. ?? ..; ("parameters", $t) }`,
//! and the literal's element gate refused a block element ("List argument
//! cannot be faithfully materialized"). The literal now lowers the element's
//! binds in place, then its tail as the element.
use std::process::Command;

/// Pairs with a fallback component after a plain pair, inside a mapped
/// closure and in a function body, with the key present and absent, and in a loop.
const SHAPE: &str = r##"import json

fn str_of(v: Value, key: String) -> String = value.get_string(v, key) ?? ""

fn declarations_of(tools: List[Value]) -> List[Value] = list.map(tools, (t) => {
  let f = value.get(t, "function") ?? value.object([])
  value.object([("name", value.string(str_of(f, "name"))),
    ("parameters", value.get(f, "parameters") ?? value.object([("type", value.string("object"))]))])
})

fn one(t: Value) -> Value =
  value.object([("name", value.string("x")), ("p", value.get(t, "parameters") ?? value.int(1)), ("q", value.get(t, "q") ?? value.string("none"))])

fn main() -> Unit = {
  let t1 = value.object([("function", value.object([("name", value.string("go")), ("parameters", value.int(3))]))])
  let t2 = value.object([("function", value.object([("name", value.string("stop"))]))])
  let t3 = value.object([])
  for d in declarations_of([t1, t2, t3]) {
    println(json.stringify(d))
  }
  println(json.stringify(one(value.object([("parameters", value.int(7))]))))
  println(json.stringify(one(value.object([("q", value.bool(true))]))))
  var n = 0
  for i in 0..<20000 {
    n = n + string.len(json.stringify(one(value.object([("parameters", value.int(i))]))))
  }
  println(int.to_string(n))
}
"##;

const SHAPE_STDOUT: &str = "{\"name\":\"go\",\"parameters\":3}\n{\"name\":\"stop\",\"parameters\":{\"type\":\"object\"}}\n{\"name\":\"\",\"parameters\":{\"type\":\"object\"}}\n{\"name\":\"x\",\"p\":7,\"q\":\"none\"}\n{\"name\":\"x\",\"p\":1,\"q\":true}\n648890\n";

/// Literals that never produced a block element: plain pairs, a single
/// pair with a fallback, and a fallback bound by the program first.
const CONTROL: &str = r##"import json

fn str_of(v: Value, key: String) -> String = value.get_string(v, key) ?? ""

fn plain(t: Value) -> Value =
  value.object([("name", value.string(str_of(t, "name"))), ("p", value.int(1))])

fn single(t: Value) -> Value =
  value.object([("p", value.get(t, "parameters") ?? value.int(1))])

fn bound(t: Value) -> Value = {
  let p = value.get(t, "parameters") ?? value.int(1)
  value.object([("name", value.string("x")), ("p", p)])
}

fn main() -> Unit = {
  let t = value.object([("name", value.string("go")), ("parameters", value.int(3))])
  println(json.stringify(plain(t)))
  println(json.stringify(single(t)))
  println(json.stringify(single(value.object([]))))
  println(json.stringify(bound(t)))
  println(json.stringify(bound(value.object([]))))
}
"##;

const CONTROL_STDOUT: &str = "{\"name\":\"go\",\"p\":1}\n{\"p\":3}\n{\"p\":1}\n{\"name\":\"x\",\"p\":3}\n{\"name\":\"x\",\"p\":1}\n";

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| format!("{e:?}")))
        .expect("spawn")
        .join()
        .expect("render thread")
}

fn runs_unwalled(name: &str, src: &str, stdout: &str) {
    render(src).unwrap_or_else(|e| panic!("{name} walls: {e}"));
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{name} {extra:?}");
    }
}

#[test]
fn a_fallback_component_in_a_multi_element_pair_list_is_built_in_place() {
    runs_unwalled("list_literal_bound_element", SHAPE, SHAPE_STDOUT);
}

#[test]
fn plain_single_and_prebound_pair_lists_are_built_as_before() {
    runs_unwalled("list_literal_bound_element_control", CONTROL, CONTROL_STDOUT);
}
