//! The runtime-backed equality family (#4749): every `RUNTIME_BACKED_TYPES`
//! row has a declared equality, `<module>.__<type>_eq`, beside its declared
//! repr, and the certified native render's `==` over such a value calls it.
//!
//! The family's completeness rule: each row declares BOTH surfaces in its owner
//! module, as a native `@intrinsic`. No row is exempt — a runtime-backed type
//! the source can name can be compared, so a missing equality is a wall the
//! user meets at `==`.

use almide_lang::stdlib_info::{
    bundled_source, runtime_backed_eq_surface, runtime_backed_repr_surface, RUNTIME_BACKED_TYPES,
};

/// The `@intrinsic(...)` line directly above `fn <func>(` in `src`, if any.
fn declared_intrinsic(src: &str, func: &str) -> Option<String> {
    let lines: Vec<&str> = src.lines().collect();
    let at = lines.iter().position(|l| l.starts_with(&format!("fn {func}(")))?;
    lines[..at].iter().rev().take_while(|l| l.starts_with('@')).find_map(|l| {
        l.strip_prefix("@intrinsic(\"").and_then(|r| r.split('"').next()).map(str::to_string)
    })
}

#[test]
fn every_runtime_backed_type_declares_its_repr_and_its_equality() {
    assert!(!RUNTIME_BACKED_TYPES.is_empty());
    for (module, ty) in RUNTIME_BACKED_TYPES {
        let src = bundled_source(module).unwrap_or_else(|| panic!("{module} is bundled"));
        for surface in [runtime_backed_repr_surface(ty), runtime_backed_eq_surface(ty)] {
            let surface = surface.unwrap_or_else(|| panic!("{ty} is runtime-backed"));
            let (owner, func) = surface.split_once('.').expect("module-qualified");
            assert_eq!(owner, *module, "{surface} is owned by {module}");
            assert!(declared_intrinsic(src, func).is_some(), "{module} declares `{func}` as a native @intrinsic");
        }
    }
}

#[test]
fn a_runtime_backed_eq_lowers_to_its_declared_equality_on_the_native_render() {
    let src = "import http\n\nfn same(a: HttpRequest, b: HttpRequest) -> Bool = a == b\n\neffect fn main() -> Unit = {\n  let r = http.new_request(\"GET\", \"/\", \"\", [])\n  println(\"${same(r, r)}\")\n}\n";
    almide_mir::pipeline::try_render_rust_source(src).expect("`==` over an HttpRequest renders on the certified native render");
}
