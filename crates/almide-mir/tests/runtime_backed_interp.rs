//! A runtime-backed value (`HttpCall`, `HttpRequest`, …) interpolates
//! through its DECLARED repr on v1, bare or inside a literal tuple / list /
//! `some(..)` — never through the self-hosted carrier the newtype erasure leaves
//! (`list.to_string_s` over an `HttpRequest` would print every header value).
//! The literal containers are flattened into text parts, so nothing has to be
//! materialized just to be printed:
//!   - `spec/embedded_cross/http_call_repr.almd :: main` (`${[c, c]}` / `${(c, 1)}`
//!     over `let c = http.start(..)!`) walled on the untracked-subject match;
//!   - `spec/wasm_cross/runtime_backed_value_repr.almd :: main` (`${(req, path, 1)}`,
//!     inside a list literal) walled on the nested-ownership list literal.

fn lowered(src: &str) -> Vec<almide_mir::MirFunction> {
    almide_mir::pipeline::lowered_functions(src).expect("reaches the MIR lowering")
}

fn main_ops(src: &str) -> Vec<String> {
    let funcs = lowered(src);
    let f = funcs.iter().find(|f| f.name.as_str() == "main").unwrap_or_else(|| panic!("`main` walled on v1"));
    assert!(almide_mir::verify_ownership(f).is_ok(), "`main` fails ownership verification");
    f.ops.iter().map(|op| format!("{op:?}")).collect()
}

fn calls(ops: &[String], name: &str) -> usize {
    let needle = format!("name: \"{name}\"");
    ops.iter().filter(|o| o.contains("CallFn") && o.contains(&needle)).count()
}

const START: &str = "  let c = http.start(http.new_request(\"GET\", \"http://127.0.0.1:1/x\", \"\", [(\"A\", \"1\")]), { total_ms: 1000, idle_ms: 1000 })!\n";

#[test]
fn an_unwrapped_http_call_interpolates_inside_a_list_and_a_tuple_literal() {
    let src = format!(
        "import http\n\neffect fn main() -> Unit = {{\n{START}  println(\"${{[c, c]}}\")\n  println(\"${{(c, 1)}}\")\n}}\n"
    );
    let ops = main_ops(&src);
    assert_eq!(calls(&ops, "http.__http_call_repr"), 3, "one repr per element:\n{ops:#?}");
    assert_eq!(calls(&ops, "compound.to_string"), 0, "walled through compound:\n{ops:#?}");
}

const REQ: &str = "  let req = http.new_request(\"POST\", \"/x\", \"b\", [(\"Authorization\", \"Bearer T\")])\n  let path = value.root() |> value.key(\"user\")\n";

#[test]
fn a_tuple_of_runtime_backed_values_interpolates_through_the_declared_reprs() {
    let src = format!(
        "import http\nimport json\n\neffect fn main() -> Unit = {{\n{REQ}  println(\"${{(req, path, 1)}}\")\n}}\n"
    );
    let ops = main_ops(&src);
    assert_eq!(calls(&ops, "http.__http_request_repr"), 1, "{ops:#?}");
    assert_eq!(calls(&ops, "list.to_string_s"), 0, "the carrier was displayed:\n{ops:#?}");
}

#[test]
fn a_list_literal_of_runtime_backed_interpolations_lowers() {
    let src = format!(
        "import http\nimport json\n\neffect fn main() -> Unit = {{\n{REQ}  let lines = [\"${{req}}\", \"${{(req, path, 1)}}\", \"${{some(path)}}\"]\n  println(lines |> list.join(\"\\n\"))\n}}\n"
    );
    let ops = main_ops(&src);
    assert_eq!(calls(&ops, "http.__http_request_repr"), 2, "{ops:#?}");
    for carrier in ["list.to_string_s", "list.to_string_x", "option.to_string_ls"] {
        assert_eq!(calls(&ops, carrier), 0, "the carrier was displayed via {carrier}:\n{ops:#?}");
    }
}

/// A RECURSIVE record that mentions no runtime-backed type keeps its own
/// display route: the pass must not read the recursion as "mentions one"
/// (a depth cap did, and walled `spec/wasm_cross/compound_repr_recursive_interp.almd`).
#[test]
fn a_recursive_record_without_a_runtime_backed_field_is_left_alone() {
    let src = "type RNode = { val: Int, kids: List[RNode] }\n\neffect fn main() -> Unit = {\n  let n: RNode = { val: 1, kids: [] }\n  println(\"${n}\")\n}\n";
    let ops = main_ops(src);
    assert_eq!(calls(&ops, "compound.to_string"), 0, "routed to the runtime-backed wall:\n{ops:#?}");
}


/// `main`'s ops, or `None` when `main` walled on v1 (the honest refusal: the
/// native leg then renders the program with its standard codegen).
fn main_ops_if_lowered(src: &str) -> Option<Vec<String>> {
    let funcs = lowered(src);
    let f = funcs.iter().find(|f| f.name.as_str() == "main")?;
    assert!(almide_mir::verify_ownership(f).is_ok(), "`main` fails ownership verification");
    Some(f.ops.iter().map(|op| format!("{op:?}")).collect())
}

/// Calls to the structural repr v1 synthesizes for a user type (`__repr_Ev`,
/// `__repr_Tag_Int`): after newtype erasure it walks the payload's CARRIER.
fn synthesized_reprs(ops: &[String], ty: &str) -> usize {
    let needle = format!("name: \"__repr_{ty}");
    ops.iter().filter(|o| o.contains("CallFn") && o.contains(&needle)).count()
}

/// #4230: a user VARIANT whose payload is runtime-backed (a tuple case, a
/// record case, and a generic variant whose fixed field is one) reaches the
/// declared-repr pass the way a record does. Before, `main` lowered and
/// displayed the value through the structural `__repr_Ev`, which after newtype
/// erasure walks the carrier (`List[String]`, every header value), the form
/// C-432 forbids. Now the part routes to the runtime-backed wall, so v1 refuses
/// `main` rather than certifying the carrier's text.
#[test]
fn a_user_variant_with_a_runtime_backed_payload_is_not_displayed_through_its_carrier() {
    let cases = [
        ("type Ev = | Got(HttpRequest) | Nothing", "let v = Got(req)", "Ev"),
        ("type Ev = | Hit { at: value.Path } | Nothing", "let v = Hit { at: path }", "Ev"),
        ("type Tag[T] = | Tagged(T, HttpRequest)", "let v: Tag[Int] = Tagged(1, req)", "Tag"),
    ];
    for (decl, bind, ty) in cases {
        let src = format!("import http\nimport json\n\n{decl}\n\neffect fn main() -> Unit = {{\n{REQ}  {bind}\n  println(\"${{v}}\")\n}}\n");
        let shown = main_ops_if_lowered(&src).map_or(0, |ops| synthesized_reprs(&ops, ty));
        assert_eq!(shown, 0, "`{decl}` was displayed through its structural repr (the carrier):\n{src}");
    }
}

/// The control for the test above: a variant whose payloads mention no
/// runtime-backed type keeps its own structural display, so the probe above
/// does see that route when it is taken.
#[test]
fn a_variant_without_a_runtime_backed_payload_is_left_alone() {
    let src = "type P = | A(Int) | B(String) | C\n\neffect fn main() -> Unit = {\n  let p = A(1)\n  println(\"${p}\")\n}\n";
    let ops = main_ops(src);
    assert_eq!(synthesized_reprs(&ops, "P"), 1, "{ops:#?}");
    assert_eq!(calls(&ops, "compound.to_string"), 0, "routed to the runtime-backed wall:\n{ops:#?}");
}
