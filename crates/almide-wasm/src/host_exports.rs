//! The JS-host export switch for the STRUCTURAL leg (#2265, the twin of
//! `almide_mir::host_exports`): `almide build --target wasm --host js`
//! needs two things a stock module does not export — the allocator, so the
//! host can build a `String` block the guest will own, and the flat release,
//! so the host can drop a `String` the guest handed back — and the callee's
//! ownership of each exported function's params, so the host knows whether
//! the credit it passed comes back. Off (the default) the module's bytes are
//! byte-identical to a build without the switch, so no size ledger moves.
//!
//! THREAD-LOCAL with a scoped guard, for the same cross-test hygiene the
//! heap-cap knob documents. The ownership notes are a side channel of the
//! same scope: the emitter records them while the guard is set and the CLI
//! takes them after the build.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

thread_local! {
    static JS_HOST: Cell<bool> = const { Cell::new(false) };
    static EXPORT_OWNED: RefCell<BTreeMap<String, Vec<bool>>> = const { RefCell::new(BTreeMap::new()) };
}

/// Is a JS host being built for the module under emission?
pub fn js_host() -> bool {
    JS_HOST.with(|c| c.get())
}

/// The export name of the allocator (`(len: i32) -> block: i32`, header set).
pub const ALLOC_EXPORT: &str = "__alloc";
/// The export name of the flat release (`(block: i32)`).
pub const RELEASE_EXPORT: &str = "__release";

/// Record which params of an exported function the CALLEE owns (releases
/// at its exit plan) — `false` means borrowed, the caller keeps its credit.
pub(crate) fn note_export(name: &str, param_owned: Vec<bool>) {
    if js_host() {
        EXPORT_OWNED.with(|m| { m.borrow_mut().insert(name.to_string(), param_owned); });
    }
}

/// The ownership notes recorded since the guard was set, keyed by export name.
pub fn export_param_owned() -> BTreeMap<String, Vec<bool>> {
    EXPORT_OWNED.with(|m| m.borrow().clone())
}

/// Turn the switch on for a scope and restore the previous state on drop.
#[must_use = "the guard restores the previous state when dropped; binding it to `_` restores immediately"]
pub struct JsHostGuard(bool);

impl JsHostGuard {
    pub fn set() -> Self {
        let prev = js_host();
        JS_HOST.with(|c| c.set(true));
        EXPORT_OWNED.with(|m| m.borrow_mut().clear());
        Self(prev)
    }
}

impl Drop for JsHostGuard {
    fn drop(&mut self) {
        JS_HOST.with(|c| c.set(self.0));
    }
}
