//! Explicit project filesystem scope carried through the certified render.
//! A nested render restores the preceding scope, including on early refusal.
use std::cell::RefCell;
thread_local! {
    static ROOTS: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

pub(crate) struct ScopeGuard(Option<Vec<String>>);
impl ScopeGuard {
    pub(crate) fn enter(roots: Option<Vec<String>>) -> Self {
        Self(ROOTS.with(|r| r.replace(roots)))
    }
}
impl Drop for ScopeGuard {
    fn drop(&mut self) {
        ROOTS.with(|r| r.replace(self.0.take()));
    }
}

/// The scope for a render whose `bodies` and glue `used` reach a call the
/// scope bounds — read off the declarations' path marks, not a module's
/// symbol prefix (#4825) — `Ok(None)` when none is reached or no scope is
/// set; `Err` when one reached cannot be bounded.
pub(crate) fn plan(used: &[&str], bodies: &str) -> Result<Option<almide_codegen::fs_scope::FsScope>, String> {
    let Some(roots) = ROOTS.with(|r| r.borrow().clone()) else { return Ok(None) };
    let mut calls = bodies.to_owned();
    for shim in used.iter().filter(|s| !crate::native_floor::is_runtime_text(s)) {
        calls.push_str(shim);
    }
    if !almide_codegen::fs_scope::FsScope::applies_to(&calls) {
        return Ok(None);
    }
    almide_codegen::fs_scope::FsScope::new(&roots, "", "", &calls).map(Some)
}
