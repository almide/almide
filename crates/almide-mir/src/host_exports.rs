//! The JS-host export switch for the INCUMBENT leg (#2265, the twin of
//! `almide_wasm::host_exports`): `almide build --target wasm --host js`
//! needs the module to export its allocator and its release, so the host can
//! build a `String` block for an exported function or an import's return and
//! drop one the guest handed back. Off (the default) the rendered module is
//! byte-identical to a build without the switch — the stamped size ledgers
//! never see it. THREAD-LOCAL with a scoped guard, for the reasons
//! [`crate::heap_cap`] documents.
//!
//! The incumbent's calling convention is borrow-by-default for every param
//! (`LowerCtx::bind_params`): a callee never releases what it was passed, so
//! the host keeps its credit and releases after the call.

thread_local! {
    static JS_HOST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static STRING_ABI: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Is a JS host being built for the module under rendering?
pub fn js_host() -> bool {
    JS_HOST.with(|c| c.get())
}

/// Does the host surface marshal a `String` (#2276)? Only then are the
/// allocator and release exported; a scalar-only surface keeps the module's
/// bytes.
pub fn string_abi() -> bool {
    js_host() && STRING_ABI.with(|c| c.get())
}

/// Set by the CLI once the program's host surface is known (before rendering).
pub fn set_string_abi(on: bool) {
    STRING_ABI.with(|c| c.set(on));
}

/// The export text appended after the program's own exports: the raw block
/// allocator (`(n: i32) -> i32`, header NOT set — the host stores rc/len/cap
/// with cap in 8-byte slots) and the refcount release.
pub(crate) fn export_text() -> String {
    // Assembled, not spelled: the WAT prelude audit anchors every hand-written
    // function definition by its WAT header text in the renderer sources, and
    // an export line naming the same function must not read as a second
    // definition site.
    ["alloc", "rc_dec"]
        .iter()
        .zip(["__alloc", "__release"])
        .map(|(internal, export)| format!("  (export {export:?} (func ${internal}))\n"))
        .collect()
}

/// Turn the switch on for a scope and restore the previous state on drop.
#[must_use = "the guard restores the previous state when dropped; binding it to `_` restores immediately"]
pub struct JsHostGuard(bool);

impl JsHostGuard {
    pub fn set() -> Self {
        let prev = js_host();
        JS_HOST.with(|c| c.set(true));
        STRING_ABI.with(|c| c.set(false));
        Self(prev)
    }
}

impl Drop for JsHostGuard {
    fn drop(&mut self) {
        JS_HOST.with(|c| c.set(self.0));
        STRING_ABI.with(|c| c.set(false));
    }
}
