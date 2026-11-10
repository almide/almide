//! The 0-or-1 Option blocks a defunctionalized `list.find` fills in place
//! (#4006 C1, `lower/defunc_find.rs`), recorded where the lowering builds one
//! over a scalar payload: an `Alloc { DynList { len: 1 } }` whose length word
//! is stored 0 (none) and, on the hit, its slot 0 then its length word 1
//! (some). The block is an `Option[T]` by the call's declared result; its
//! MIR carries no type, so the native scalar-Option rewrite reads it here
//! (`native_option_rewrite`), as it reads a declared param's or callee's.
//! The wasm leg never reads this.

use crate::ValueId;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

thread_local! {
    static BLOCKS: RefCell<BTreeMap<String, BTreeSet<ValueId>>> = const { RefCell::new(BTreeMap::new()) };
}

pub(crate) fn record(fn_name: &str, block: ValueId) {
    BLOCKS.with(|b| b.borrow_mut().entry(fn_name.to_string()).or_default().insert(block));
}

/// The scalar Option blocks `fn_name`'s lowering filled in place.
pub(crate) fn of(fn_name: &str) -> BTreeSet<ValueId> {
    BLOCKS.with(|b| b.borrow().get(fn_name).cloned().unwrap_or_default())
}

pub(crate) fn clear() {
    BLOCKS.with(|b| b.borrow_mut().clear());
}

pub(crate) fn snapshot() -> BTreeMap<String, BTreeSet<ValueId>> {
    BLOCKS.with(|b| b.borrow().clone())
}

pub(crate) fn restore(saved: BTreeMap<String, BTreeSet<ValueId>>) {
    BLOCKS.with(|b| *b.borrow_mut() = saved);
}
