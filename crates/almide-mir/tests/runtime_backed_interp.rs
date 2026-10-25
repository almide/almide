//! A runtime-backed value (`HttpCall`, `HttpRequest`, `JsonPath`, …) interpolates
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

const START: &str = "  let c = http.start(\"GET\", \"http://127.0.0.1:1/x\", \"\", map.from_list([(\"A\", \"1\")]), { total_ms: 1000, idle_ms: 1000 })!\n";

#[test]
fn an_unwrapped_http_call_interpolates_inside_a_list_and_a_tuple_literal() {
    let src = format!(
        "import http\n\neffect fn main() -> Unit = {{\n{START}  println(\"${{[c, c]}}\")\n  println(\"${{(c, 1)}}\")\n}}\n"
    );
    let ops = main_ops(&src);
    assert_eq!(calls(&ops, "http.__http_call_repr"), 3, "one repr per element:\n{ops:#?}");
    assert_eq!(calls(&ops, "compound.to_string"), 0, "walled through compound:\n{ops:#?}");
}

const REQ: &str = "  let req = http.new_request(\"POST\", \"/x\", \"b\", map.from_list([(\"Authorization\", \"Bearer T\")]))\n  let path = json.root() |> json.field(\"user\")\n";

#[test]
fn a_tuple_of_runtime_backed_values_interpolates_through_the_declared_reprs() {
    let src = format!(
        "import http\nimport json\n\neffect fn main() -> Unit = {{\n{REQ}  println(\"${{(req, path, 1)}}\")\n}}\n"
    );
    let ops = main_ops(&src);
    assert_eq!(calls(&ops, "http.__http_request_repr"), 1, "{ops:#?}");
    assert_eq!(calls(&ops, "json.__json_path_repr"), 1, "{ops:#?}");
    assert_eq!(calls(&ops, "list.to_string_s"), 0, "the carrier was displayed:\n{ops:#?}");
}

#[test]
fn a_list_literal_of_runtime_backed_interpolations_lowers() {
    let src = format!(
        "import http\nimport json\n\neffect fn main() -> Unit = {{\n{REQ}  let lines = [\"${{req}}\", \"${{(req, path, 1)}}\", \"${{some(path)}}\"]\n  println(lines |> list.join(\"\\n\"))\n}}\n"
    );
    let ops = main_ops(&src);
    assert_eq!(calls(&ops, "http.__http_request_repr"), 2, "{ops:#?}");
    assert_eq!(calls(&ops, "json.__json_path_repr"), 2, "{ops:#?}");
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
