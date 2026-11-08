//! The declared signature of every lifted lambda (#4006 family G): the
//! lambda's own parameter types and its body type, recorded where the
//! lowering lifts it (a `MirParam` carries only a repr, which cannot tell a
//! String from a list). The native render reads them to type a lambda's
//! params and result and to group its dispatch tables by signature.

use almide_lang::types::Ty;
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Clone)]
pub(crate) struct Capture {
    pub ty: Ty,
    pub rich: bool,
    pub cell: bool,
}

impl Capture {
    pub(crate) fn slot_ty(&self) -> Option<Ty> {
        if self.cell { return None; }
        Some(if self.rich { Ty::Tuple(vec![Ty::Int, self.ty.clone()].into()) } else { self.ty.clone() })
    }
}

thread_local! {
    static SIGS: RefCell<BTreeMap<String, (Vec<Ty>, Ty)>> = const { RefCell::new(BTreeMap::new()) };
    static CELLS: RefCell<Vec<Ty>> = const { RefCell::new(Vec::new()) };
    static CAPTURES: RefCell<BTreeMap<String, Vec<Capture>>> = const { RefCell::new(BTreeMap::new()) };
}

/// Physical capture slots, in the same order as the closure block. Rich
/// captures name their checked wrapper type, rather than erasing its payload.
pub(crate) fn record_captures(name: &str, slots: Vec<Capture>) {
    CAPTURES.with(|s| s.borrow_mut().insert(name.to_string(), slots));
}

pub(crate) fn captures(name: &str) -> Option<Vec<Capture>> {
    CAPTURES.with(|s| s.borrow().get(name).cloned())
}

pub(crate) fn declared_types() -> Vec<Ty> {
    let mut ts: Vec<Ty> = SIGS.with(|s| s.borrow().values().map(|(params, ret)| Ty::Fn {
        params: params.clone(), ret: Box::new(ret.clone()), is_effect: false,
    }).collect());
    CAPTURES.with(|s| ts.extend(s.borrow().values().flatten().filter_map(Capture::slot_ty)));
    ts.extend(cell_types());
    ts
}

pub(crate) fn record(name: &str, params: Vec<Ty>, ret: Ty) {
    SIGS.with(|s| s.borrow_mut().insert(name.to_string(), (params, ret)));
}

pub(crate) fn get(name: &str) -> Option<(Vec<Ty>, Ty)> {
    SIGS.with(|s| s.borrow().get(name).cloned())
}

pub(crate) fn clear() {
    SIGS.with(|s| s.borrow_mut().clear());
    CAPTURES.with(|s| s.borrow_mut().clear());
    CELLS.with(|s| s.borrow_mut().clear());
}

pub(crate) fn snapshot() -> (BTreeMap<String, (Vec<Ty>, Ty)>, BTreeMap<String, Vec<Capture>>, Vec<Ty>) {
    (SIGS.with(|s| s.borrow().clone()), CAPTURES.with(|s| s.borrow().clone()), cell_types())
}

pub(crate) fn restore(saved: (BTreeMap<String, (Vec<Ty>, Ty)>, BTreeMap<String, Vec<Capture>>, Vec<Ty>)) {
    SIGS.with(|s| *s.borrow_mut() = saved.0);
    CAPTURES.with(|s| *s.borrow_mut() = saved.1);
    CELLS.with(|s| *s.borrow_mut() = saved.2);
}

/// A storage identity, distinct from a source tuple/list of the same fields.
pub(crate) fn cell_key(t: &Ty) -> String { format!("shared-cell({t:?})") }
pub(crate) fn cell_wrapper_key(t: &Ty) -> String { format!("shared-cell-wrapper({t:?})") }
pub(crate) fn record_cell(t: &Ty) {
    CELLS.with(|s| { let mut s = s.borrow_mut(); if !s.contains(t) { s.push(t.clone()); } });
}
pub(crate) fn cell_types() -> Vec<Ty> { CELLS.with(|s| s.borrow().clone()) }
