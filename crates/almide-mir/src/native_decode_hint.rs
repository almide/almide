//! Slot-hinted field reads in a derived `T.decode` on the native render.
//!
//! An object is an insertion-ordered list of pairs, so a field read scans it
//! and decoding an n-field record costs O(n²) key compares. The derive knows
//! each field's place in the declaration and stamps the key order on the fn
//! (`@codec_slots`); a document `T.encode` wrote carries its keys in that
//! order. The standard render hands each borrowed read that place
//! (`pass_decode_slot_hint.rs`) and the runtime's `_at` form tries it first,
//! then scans as before — the same value and the same two error texts on
//! every document. Here the borrowing twin of such a read renders through
//! that `_at` form too.
//!
//! A read is hinted when it sits in a fn carrying `@codec_slots`, reads that
//! fn's own document parameter, its key is a string literal naming a declared
//! slot, and its result is held through the borrowing twin
//! (`render_native_borrow_read::borrowed_results`). Any other read keeps the
//! scan.
use crate::{CallArg, Init, MirFunction, Op, ValueId};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

thread_local! {
    static SLOTS: RefCell<BTreeMap<String, Vec<String>>> = const { RefCell::new(BTreeMap::new()) };
    static HINTS: RefCell<BTreeMap<ValueId, usize>> = const { RefCell::new(BTreeMap::new()) };
}

/// The declared key order of every derived decode of `ir`, for this render.
pub(crate) struct Scope(BTreeMap<String, Vec<String>>);

impl Scope {
    pub(crate) fn enter(ir: &almide_ir::IrProgram) -> Self {
        let slots = ir.functions.iter().filter_map(|f| Some((f.name.to_string(), codec_slots(f)?))).collect();
        Self(SLOTS.with(|s| s.replace(slots)))
    }
}

impl Drop for Scope {
    fn drop(&mut self) { SLOTS.with(|s| s.replace(std::mem::take(&mut self.0))); }
}

fn codec_slots(f: &almide_ir::IrFunction) -> Option<Vec<String>> {
    let attr = f.attrs.iter().find(|a| a.name.as_str() == "codec_slots")?;
    attr.args.iter().map(|a| match &a.value {
        almide_lang::ast::AttrValue::String { value } => Some(value.clone()),
        _ => None,
    }).collect()
}

/// Record the hinted reads of the function about to render: each borrowed
/// floor result (`borrowed`) read from the document parameter under a
/// literal key the fn's declared slots name, with that slot.
pub(crate) fn set(func: &MirFunction, borrowed: &BTreeSet<ValueId>) {
    let hints = SLOTS.with(|s| s.borrow().get(&func.name).map(|slots| hints(func, borrowed, slots))).unwrap_or_default();
    HINTS.with(|h| *h.borrow_mut() = hints);
}

fn hints(func: &MirFunction, borrowed: &BTreeSet<ValueId>, slots: &[String]) -> BTreeMap<ValueId, usize> {
    let Some(doc) = func.params.first().map(|p| p.value) else { return BTreeMap::new() };
    let literals: BTreeMap<ValueId, &str> = func.ops.iter().filter_map(|op| match op {
        Op::Alloc { dst, init: Init::Str(s), .. } => Some((*dst, s.as_str())),
        _ => None,
    }).collect();
    func.ops.iter().filter_map(|op| {
        let Op::CallFn { dst: Some(d), args, .. } = op else { return None };
        let [CallArg::Handle(v), CallArg::Handle(k)] = args.as_slice() else { return None };
        let key = literals.get(k)?;
        (*v == doc && borrowed.contains(d)).then_some(())?;
        Some((*d, slots.iter().position(|s| s == key)?))
    }).collect()
}

/// The declared slot of the hinted read `d` of the function being rendered.
pub(crate) fn slot(d: ValueId) -> Option<usize> {
    HINTS.with(|h| h.borrow().get(&d).copied())
}

/// `expr` with `, <slot>` appended to the arguments of its call `symbol(…)`.
pub(crate) fn with_slot(expr: &str, symbol: &str, slot: usize) -> Option<String> {
    let open = expr.find(&format!("{symbol}("))? + symbol.len() + 1;
    let (mut depth, mut quoted, mut escaped) = (1usize, false, false);
    for (i, c) in expr[open..].char_indices() {
        match (quoted, c) {
            (true, _) if escaped => escaped = false,
            (true, '\\') => escaped = true,
            (_, '"') => quoted = !quoted,
            (false, '(') => depth += 1,
            (false, ')') => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            let at = open + i;
            return Some(format!("{}, {slot}usize{}", &expr[..at], &expr[at..]));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slot_lands_inside_the_call_it_names() {
        let e = r#"(f_ref(v0, "a)\"(b")).map_err(AlmideString::from)"#;
        assert_eq!(with_slot(e, "f_ref", 3).unwrap(), r#"(f_ref(v0, "a)\"(b", 3usize)).map_err(AlmideString::from)"#);
        assert_eq!(with_slot(e, "g_ref", 3), None);
    }

    #[test]
    fn only_a_literal_slot_key_read_of_the_document_is_hinted() {
        let v = ValueId;
        let ptr = || crate::Repr::Ptr { layout: crate::LayoutId::new(0) };
        let lit = |d: u32, s: &str| Op::Alloc { dst: v(d), repr: ptr(), init: Init::Str(s.into()) };
        let read = |d: u32, doc: u32, k: u32| Op::CallFn { dst: Some(v(d)), name: "value.__field".into(), args: vec![CallArg::Handle(v(doc)), CallArg::Handle(v(k))], result: None };
        let func = MirFunction {
            params: vec![crate::MirParam { value: v(0), repr: ptr() }],
            ops: vec![lit(1, "b"), read(2, 0, 1), lit(3, "zz"), read(4, 0, 3), read(5, 2, 1), read(6, 0, 1)],
            ..Default::default()
        };
        let borrowed = BTreeSet::from([v(2), v(4), v(5)]);
        let got = hints(&func, &borrowed, &["a".to_string(), "b".to_string()]);
        // v4: not a slot; v5: not the document; v6: not held through the twin.
        assert_eq!(got, BTreeMap::from([(v(2), 1)]));
    }
}
