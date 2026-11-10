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

pub(crate) fn plan(used: &[&str], bodies: &str) -> Option<almide_codegen::fs_scope::FsScope> {
    let roots = ROOTS.with(|r| r.borrow().clone())?;
    if !crate::native_floor::carries_fs_host(used) { return None; }
    let mut calls = bodies.to_owned();
    for shim in used.iter().filter(|s| !crate::native_floor::is_runtime_text(s)) {
        calls.push_str(shim);
    }
    Some(almide_codegen::fs_scope::FsScope::new(&roots, "", "", &calls))
}
