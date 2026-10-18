//! A fn tail `ok(<heap expr>)` / `err(<heap expr>)` whose payload is neither a
//! tuple nor a record (a `??` fallback, a call) lowers on v1 by binding the
//! payload first, as the same body written with a `let` already did.
//! `spec/wasm_cross/fan_ordered_arms.almd :: reader` walled on this shape.

fn lowered(src: &str) -> Vec<almide_mir::MirFunction> {
    almide_mir::pipeline::lowered_functions(src).expect("reaches the MIR lowering")
}

fn assert_lowers(src: &str, name: &str) {
    let funcs = lowered(src);
    let f = funcs
        .iter()
        .find(|f| f.name.as_str() == name)
        .unwrap_or_else(|| panic!("`{name}` walled on v1:\n{src}"));
    assert!(almide_mir::verify_ownership(f).is_ok(), "`{name}` fails ownership verification");
}

const MAIN: &str = "effect fn main() -> Unit = {\n  println(reader(\"K\")!)\n}\n";

#[test]
fn ok_of_a_coalesced_string_lowers_in_tail_position() {
    let src = format!(
        "import env\n\neffect fn reader(key: String) -> Result[String, String] = {{\n  ok(env.get(key) ?? \"unset\")\n}}\n\n{MAIN}"
    );
    assert_lowers(&src, "reader");
}

#[test]
fn err_of_a_concatenated_string_lowers_in_tail_position() {
    let src = format!(
        "effect fn reader(key: String) -> Result[String, String] = {{\n  err(\"missing \" + key)\n}}\n\n{MAIN}"
    );
    assert_lowers(&src, "reader");
}

#[test]
fn ok_of_a_list_from_a_call_lowers_in_tail_position() {
    let src = "effect fn reader(key: String) -> Result[List[String], String] = {\n  ok(string.split(key, \",\"))\n}\n\n\
               effect fn main() -> Unit = {\n  println(\"${list.len(reader(\"a,b\")!)}\")\n}\n";
    assert_lowers(src, "reader");
}
