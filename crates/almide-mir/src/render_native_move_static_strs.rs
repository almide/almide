//! The string literals of the function being rendered (#4006, no-perf-loss):
//! immortal static blocks whose buffer is never unique, and the `&str`
//! operand a binding that only ever holds its literal is read as.
use super::REBOUND;
use crate::{MirFunction, Op, ValueId};

thread_local! {
    static STATIC_STRS: std::cell::RefCell<std::collections::BTreeMap<ValueId, String>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// The string literals of the function being rendered: values an
/// `Alloc { init: Str }` defines, rendered as immortal static blocks, whose
/// buffer is never unique (an edit copies it first).
pub(crate) fn set_static_strs(func: &MirFunction) {
    let lits = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Alloc { dst, init: crate::Init::Str(s), .. } => Some((*dst, s.clone())),
            _ => None,
        })
        .collect();
    STATIC_STRS.with(|m| *m.borrow_mut() = lits);
}

/// A slot that starts at a literal and is rebound holds owned strings
/// later: an edit of it copies only the literal, once.
pub(crate) fn is_static_str(v: ValueId) -> bool {
    !REBOUND.with(|m| m.borrow().contains(&v)) && STATIC_STRS.with(|m| m.borrow().contains_key(&v))
}

/// A string literal read as a `&str` operand (#4006, no-perf-loss): the
/// literal itself, as the standard codegen passes it, instead of the static
/// block's view (a load of its header, its length and its bytes' address).
///
/// Only for a binding the literal is all it ever holds: a slot that starts
/// at a literal and is rebound (`acc = acc + s`) holds other strings later.
pub(crate) fn static_str_operand(v: ValueId) -> Option<String> {
    if REBOUND.with(|m| m.borrow().contains(&v)) {
        return None;
    }
    STATIC_STRS.with(|m| m.borrow().get(&v).map(|s| format!("\"{}\"", almide_base::rust_lit::escape_rust_str(s))))
}
