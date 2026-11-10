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
