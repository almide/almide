//! The declared signature of every lifted lambda (#4006 family G): the
//! lambda's own parameter types and its body type, recorded where the
//! lowering lifts it (a `MirParam` carries only a repr, which cannot tell a
//! String from a list). The native render reads them to type a lambda's
//! params and result and to group its dispatch tables by signature.

use almide_lang::types::Ty;
use std::cell::RefCell;
use std::collections::BTreeMap;

thread_local! {
    static SIGS: RefCell<BTreeMap<String, (Vec<Ty>, Ty)>> = const { RefCell::new(BTreeMap::new()) };
}

pub(crate) fn record(name: &str, params: Vec<Ty>, ret: Ty) {
    SIGS.with(|s| s.borrow_mut().insert(name.to_string(), (params, ret)));
}

pub(crate) fn get(name: &str) -> Option<(Vec<Ty>, Ty)> {
    SIGS.with(|s| s.borrow().get(name).cloned())
}

pub(crate) fn clear() {
    SIGS.with(|s| s.borrow_mut().clear());
}
