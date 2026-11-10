//! The adapter of a non-scalar linked callback, over a whole render (#4640).
//!
//! A source cannot spell a linked name (a user fn named `almide_rt_…` is
//! escaped), so the program's fn is renamed in the IR to the name the native
//! text calls, the way a package's module fn arrives.
use super::{parse_or_wall, program_to_ir_mode, render_native_parts_with};
use crate::lower::LowerError;
use crate::native_callbacks::Scope;

const LINKED: &str = "almide_rt_cb_0v0_greet";
const PROGRAM: &str = "fn greet(name: String) -> Option[String] = if name == \"\" then none else some(\"hello ${name}\")\nfn main() -> Unit = println(\"x\")\n";
const NATIVE: &str = "pub fn f() { crate::almide_rt_cb_0v0_greet(\"x\"); }";
const HOST: &str = "pub fn almide_rt_cb_0v0_greet(name: &str) -> Option<AlmideString> {\n}\n";

fn render() -> Result<String, LowerError> {
    render_native_parts_with(|| {
        let mut ir = program_to_ir_mode(parse_or_wall(PROGRAM)?, &[], true)?;
        for f in ir.functions.iter_mut().filter(|f| f.name.as_str() == "greet") {
            f.name = almide_base::intern::sym(LINKED);
        }
        Ok(ir)
    }).map(|(rust, _, _)| rust)
}

#[test]
fn a_non_scalar_callback_is_adapted_to_the_signature_the_host_source_holds() {
    let _scope = Scope::enter(NATIVE.into()).with_host_source(|| HOST.to_string());
    let rust = render().expect("the program renders");
    assert!(rust.contains("pub fn almide_rt_cb_0v0_greet(name: &str) -> Option<AlmideString> {\n    let __almide_ret = almd_almide_rt_cb_0v0_greet(name);"), "{rust}");
}

/// The control: the same program with no host source has no adapter to give
/// the native code, and is refused rather than built without one.
#[test]
fn without_a_host_source_the_same_callback_is_refused() {
    let _scope = Scope::enter(NATIVE.into());
    let refused = render().expect_err("no adapter, no render");
    assert!(format!("{refused:?}").contains("is called from native code with a signature that has no single host spelling"), "{refused:?}");
}

/// A host signature outside the adapted spellings is refused by name.
#[test]
fn a_host_type_with_no_adapter_is_refused_by_name() {
    let host = HOST.replace("&str", "&mut AlmideString");
    let _scope = Scope::enter(NATIVE.into()).with_host_source(move || host);
    let refused = render().expect_err("no adapter, no render");
    assert!(format!("{refused:?}").contains("callback parameter `name: &mut AlmideString` is not a host spelling of String"), "{refused:?}");
}

const TREE_PROGRAM: &str = "type Tree = | Leaf | Node(Tree, Int, Tree)\nfn grow(n: Int) -> Tree = Node(Leaf, n, Node(Leaf, n + 1, Leaf))\nfn main() -> Unit = println(\"x\")\n";
const TREE_NATIVE: &str = "pub fn f() { crate::almide_rt_cb_0v0_grow(1); }";
const TREE_ITEM: &str = "#[derive(Clone, Debug, PartialEq)]\npub enum Tree {\n    Leaf,\n    Node(AlmideBox<Tree>, i64, AlmideBox<Tree>)\n}";
const TREE_SIG: &str = "pub fn almide_rt_cb_0v0_grow(n: i64) -> Tree {\n}\n";

fn render_tree() -> Result<String, LowerError> {
    render_native_parts_with(|| {
        let mut ir = program_to_ir_mode(parse_or_wall(TREE_PROGRAM)?, &[], true)?;
        for f in ir.functions.iter_mut().filter(|f| f.name.as_str() == "grow") {
            f.name = almide_base::intern::sym("almide_rt_cb_0v0_grow");
        }
        Ok(ir)
    }).map(|(rust, _, _)| rust)
}

/// A variant in a callback's signature: the host source's own definition is
/// carried, and the value is copied across case by case, a recursive field
/// through a call.
#[test]
fn a_variant_callback_carries_the_definition_the_host_source_holds() {
    let _scope = Scope::enter(TREE_NATIVE.into()).with_host_source(|| format!("{TREE_ITEM}\nimpl AlmideRepr for Tree {{}}\n\n{TREE_SIG}"));
    let rust = render_tree().expect("the program renders");
    assert!(rust.contains(&format!("#[allow(non_camel_case_types)]\n{TREE_ITEM}\n")), "{rust}");
    assert!(rust.contains("pub struct AlmideBox<T>(Box<T>);"), "{rust}");
    assert!(rust.contains("pub fn almide_rt_cb_0v0_grow(n: i64) -> Tree {\n    let __almide_ret = almd_almide_rt_cb_0v0_grow(n);\n    __almide_callback_value_0(&__almide_ret)\n}"), "{rust}");
    assert!(rust.contains(" => Tree::Node(AlmideBox::new(__almide_callback_value_0(&**__case_f0)), (*(__case_f1) as i64), AlmideBox::new(__almide_callback_value_0(&**__case_f2))),"), "{rust}");
    assert!(!rust.contains("impl AlmideRepr for Tree"), "only the definition is carried:\n{rust}");
}

/// The controls: with no definition in the host source, or with one whose
/// field is not the declared type, nothing is carried and the callback is
/// refused by name — the record and variant arm is what builds the fixture.
#[test]
fn a_variant_callback_with_no_agreeing_definition_is_refused_by_name() {
    for host in [TREE_SIG.to_string(), format!("{}\n\n{TREE_SIG}", TREE_ITEM.replace("i64", "AlmideString"))] {
        let _scope = Scope::enter(TREE_NATIVE.into()).with_host_source(move || host);
        let refused = render_tree().expect_err("no definition, no render");
        assert!(format!("{refused:?}").contains("callback return `Tree` (Tree) has no host adapter"), "{refused:?}");
    }
}
