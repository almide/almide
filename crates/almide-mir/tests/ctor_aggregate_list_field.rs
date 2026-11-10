//! A variant constructor whose field is a list of tuples or of records
//! (`Record(String, List[(String, T)])`, `Match(T, List[Arm])`) renders on the
//! certified native render: the variant is a typed value there and releases
//! the list with itself. The rule is the element's shape — a tuple or a
//! record — so a field of another heap kind the variant's release does not
//! cover (a map, a list of lists) is still refused whole, never built wrong.

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| format!("{e:?}")))
        .expect("spawn")
        .join()
        .expect("join")
}

fn program(variant: &str, show_arm: &str, build: &str) -> String {
    format!(
        "type Arm = {{ ctor: String, body: Raw }}\n\n\
         type Raw =\n  | RName(String)\n  | {variant}\n\n\
         fn show(r: Raw) -> String =\n  match r {{\n    RName(s) => s,\n    {show_arm},\n  }}\n\n\
         fn mk(n: Int) -> Raw = if n > 0 then {build} else RName(\"none\")\n\n\
         effect fn main() -> Unit = {{\n  println(show(mk(1)))\n  println(show(mk(0)))\n}}\n"
    )
}

#[test]
fn a_list_of_tuples_or_records_is_a_constructor_field() {
    let pairs = program(
        "RRecord(String, List[(String, Raw)])",
        "RRecord(s, fs) => s + int.to_string(list.len(fs))",
        "RRecord(\"p\", [(\"a\", RName(\"x\"))])",
    );
    let records = program(
        "RMatch(Raw, List[Arm])",
        "RMatch(x, arms) => show(x) + int.to_string(list.len(arms))",
        "RMatch(RName(\"m\"), [{ ctor: \"c\", body: RName(\"z\") }])",
    );
    for (name, src) in [("pairs", pairs), ("records", records)] {
        let code = render(&src).unwrap_or_else(|e| panic!("{name} refused: {e}"));
        assert!(code.contains("fn almd_mk("), "{name}: no mk in:\n{code}");
    }
}

#[test]
fn a_field_of_another_heap_kind_is_still_refused() {
    let map = program(
        "RMap(Map[String, Raw])",
        "RMap(m) => int.to_string(map.len(m))",
        "RMap([\"a\": RName(\"x\")])",
    );
    let nested = program(
        "RNest(List[List[String]])",
        "RNest(xs) => int.to_string(list.len(xs))",
        "RNest([[\"a\"], [\"b\"]])",
    );
    for (name, src) in [("map", map), ("nested list", nested)] {
        assert!(render(&src).is_err(), "{name} field rendered");
    }
}
