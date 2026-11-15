// The `==` of a runtime-backed value on the native render (#4749); included by
// control_p3_c.rs beside the other typed-eq helpers.
impl LowerCtx {
    /// A runtime-backed value (`HttpRequest`) on the native render: its owner module's
    /// declared equality (`stdlib_info::runtime_backed_eq_surface`, #4749), the runtime
    /// struct's own `PartialEq`. The wasm legs erase such a value to its self-hosted
    /// carrier before this point, so the surface is asked for on the native render only.
    fn runtime_eq_call_name(ty: &Ty) -> Option<String> {
        let Ty::Named(n, args) = ty else { return None };
        if !crate::lower::owned_tco_slots() || !args.is_empty() {
            return None;
        }
        crate::native_runtime_types::RuntimeType::of(n.as_str())?;
        almide_lang::stdlib_info::runtime_backed_eq_surface(n.as_str())
    }
}
