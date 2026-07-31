//! In-place container mutation, modeled by binding write-back.
//!
//! The stdlib's `mut`-receiver mutators (`list.push`, `map.insert`, …) mutate
//! the CALLER's binding and mostly return `Unit`: the program reads the effect
//! on the variable, never a returned container. The generic dispatch path
//! evaluates every argument by value before the callee sees it, so by then the
//! receiver's binding identity is gone and the effect has nowhere to land —
//! which is why this whole family used to abstain.
//!
//! Intercepting one step earlier fixes that. At the call site the receiver is
//! still an *expression*; when it is a plain `Var`, its `VarId` is the binding,
//! and the mutation becomes read → transform → `scope.assign`. That is exactly
//! the shape `IrStmtKind::IndexAssign` already uses for `xs[i] = v`, and it is
//! exactly Almide's COW value semantics: `list.push(xs, v)` observes as
//! `xs = xs + [v]`, so an alias bound before the push keeps its own elements
//! (the C-033 `alias_cow` matrix pins that promise on both backends).
//!
//! Two receiver shapes stay out of scope, and both abstain by NAME so the
//! ledger records what is missing rather than a generic capability gap:
//!
//! 1. **A `mut` parameter.** `call_function` binds parameters in the callee's
//!    own frame, so a write-back there stops at that frame and the caller never
//!    sees it. Native/wasm return the buffer to the caller's slot
//!    (`MutParamLoweringPass`, C-132); until the interp models that copy-out,
//!    writing back locally would silently drop the effect — a wrong third vote,
//!    which is worse than an honest skip (issue #1022).
//! 2. **A non-`Var` receiver** — a record field (`push9(b.items, 7)`), an
//!    index, a temporary. There is no single `VarId` to assign to.

use std::rc::Rc;

use almide_ir::VarId;

use crate::value::Value;

/// The ops this tier writes back. The rule is a WHOLE-CONTAINER mutation over
/// a container the interp models natively: append one element, remove one
/// element, or empty the container. Every one has exact semantics with no
/// bounds or endianness surface, and each mirrors a named runtime fn:
/// `almide_rt_list_push` → `Vec::push`, `almide_rt_map_insert` →
/// `AlmideMap::insert`, `almide_rt_string_push` → `String::push_str`, …
///
/// Deliberately NOT here: the `bytes` byte-level writer family
/// (`set_*` / `append_*` / `write_*`, plus `fill` / `copy_from` /
/// `copy_within` / `heap_restore`). Those write scalars INTO a buffer at an
/// offset, so each carries its own bounds rule and little-endian byte order to
/// reproduce; getting one wrong emits a wrong third vote. They stay an honest,
/// separately-named abstain (issue #1021).
///
/// `every_written_back_op_is_an_intercepted_op` (below) is the gate that keeps
/// this table and `is_inplace_mutating_op` from drifting apart.
pub(crate) fn writes_back(module: &str, func: &str) -> bool {
    matches!(
        (module, func),
        ("list", "push")
            | ("list", "pop")
            | ("list", "clear")
            | ("map", "insert")
            | ("map", "delete")
            | ("map", "clear")
            | ("string", "push")
            | ("string", "clear")
            | ("bytes", "push")
            | ("bytes", "clear")
    )
}

/// The receiver's `VarId`, when the receiver is a plain variable.
pub(crate) fn receiver_var(recv: &almide_ir::IrExpr) -> Option<VarId> {
    match &recv.kind {
        almide_ir::IrExprKind::Var { id } => Some(*id),
        _ => None,
    }
}

/// Apply one in-place op to the receiver's storage slot. Returns the value the
/// call itself evaluates to (`list.pop -> Option[A]`, everything else `Unit`),
/// or `None` for a shape the typed IR should never produce.
///
/// Every arm reaches its container through `Rc::make_mut`, which is the COW
/// rule itself: sole owner → mutate in place (so a push loop stays linear);
/// an alias exists → clone once, and the alias keeps the elements it had at
/// its bind point (C-033 `alias_cow`).
///
/// Each arm mirrors its native runtime fn exactly, including the truncating
/// `val as u8` of `almide_rt_bytes_push` — the interp models `Bytes` as a
/// `List` of `Int`, so without the mask a `bytes.push(b, 256)` would read back
/// as 256 here and 0 on both backends.
pub(crate) fn apply(module: &str, func: &str, slot: &mut Value, rest: Vec<Value>) -> Option<Value> {
    match module {
        "list" => apply_list(func, slot, rest),
        "map" => apply_map(func, slot, rest),
        "string" => apply_string(func, slot, rest),
        "bytes" => apply_bytes(func, slot, rest),
        _ => None,
    }
}

fn apply_list(func: &str, slot: &mut Value, rest: Vec<Value>) -> Option<Value> {
    let Value::List(rc) = slot else { return None };
    let xs = Rc::make_mut(rc);
    match func {
        // `almide_rt_list_push`: Vec::push.
        "push" => {
            xs.push(rest.into_iter().next()?);
            Some(Value::Unit)
        }
        // `almide_rt_list_pop`: Vec::pop — returns the removed element as an
        // Option, and `None` on an empty list (no abort).
        "pop" => Some(Value::Option(xs.pop().map(Box::new))),
        "clear" => {
            xs.clear();
            Some(Value::Unit)
        }
        _ => None,
    }
}

fn apply_map(func: &str, slot: &mut Value, rest: Vec<Value>) -> Option<Value> {
    let Value::Map(rc) = slot else { return None };
    let entries = Rc::make_mut(rc);
    match func {
        // `almide_rt_map_insert` -> `AlmideMap::insert`: replace the value in
        // place when the key is present (keeping its slot), else append. The
        // shared helper is the same one `map.set` and `IrStmtKind::MapInsert`
        // use, so the mutating and functional forms cannot diverge.
        "insert" => {
            let mut it = rest.into_iter();
            crate::eval::map_insert(entries, it.next()?, it.next()?);
            Some(Value::Unit)
        }
        // `AlmideMap::remove`: "Remove, keeping the order of the remaining
        // entries" — so a positional remove, never a swap-remove.
        "delete" => {
            let key = rest.into_iter().next()?;
            if let Some(i) = entries.iter().position(|(k, _)| k == &key) {
                entries.remove(i);
            }
            Some(Value::Unit)
        }
        "clear" => {
            entries.clear();
            Some(Value::Unit)
        }
        _ => None,
    }
}

fn apply_string(func: &str, slot: &mut Value, rest: Vec<Value>) -> Option<Value> {
    let Value::Str(rc) = slot else { return None };
    let s = Rc::make_mut(rc);
    match func {
        // `almide_rt_string_push`: push_str — the argument is a String suffix,
        // not a char.
        "push" => {
            let Value::Str(suffix) = rest.into_iter().next()? else {
                return None;
            };
            s.push_str(&suffix);
            Some(Value::Unit)
        }
        "clear" => {
            s.clear();
            Some(Value::Unit)
        }
        _ => None,
    }
}

fn apply_bytes(func: &str, slot: &mut Value, rest: Vec<Value>) -> Option<Value> {
    // `Bytes` is a `List` of `Int` here (see `bridge.rs::bytes_fn`).
    let Value::List(rc) = slot else { return None };
    let xs = Rc::make_mut(rc);
    match func {
        // `almide_rt_bytes_push`: `b.push(val as u8)` — the cast truncates, so
        // the stored element is always 0..=255.
        "push" => {
            let Value::Int(v) = rest.into_iter().next()? else {
                return None;
            };
            xs.push(Value::Int((v as u8) as i64));
            Some(Value::Unit)
        }
        "clear" => {
            xs.clear();
            Some(Value::Unit)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ints(xs: &[i64]) -> Value {
        Value::list(xs.iter().map(|n| Value::Int(*n)).collect())
    }

    fn as_ints(v: &Value) -> Vec<i64> {
        match v {
            Value::List(xs) => xs
                .iter()
                .map(|e| match e {
                    Value::Int(n) => *n,
                    _ => panic!("non-int element"),
                })
                .collect(),
            _ => panic!("not a list"),
        }
    }

    fn map_of(pairs: &[(i64, i64)]) -> Value {
        Value::Map(Rc::new(
            pairs.iter().map(|(k, v)| (Value::Int(*k), Value::Int(*v))).collect(),
        ))
    }

    fn pairs(v: &Value) -> Vec<(i64, i64)> {
        match v {
            Value::Map(e) => e
                .iter()
                .map(|(k, v)| match (k, v) {
                    (Value::Int(k), Value::Int(v)) => (*k, *v),
                    _ => panic!("non-int entry"),
                })
                .collect(),
            _ => panic!("not a map"),
        }
    }

    #[test]
    fn list_push_appends_and_returns_unit() {
        let mut slot = ints(&[1, 2]);
        let out = apply("list", "push", &mut slot, vec![Value::Int(3)]).unwrap();
        assert_eq!(as_ints(&slot), vec![1, 2, 3]);
        assert_eq!(out, Value::Unit);
    }

    #[test]
    fn list_pop_returns_the_removed_element() {
        let mut slot = ints(&[1, 2, 3]);
        let out = apply("list", "pop", &mut slot, vec![]).unwrap();
        assert_eq!(as_ints(&slot), vec![1, 2]);
        assert_eq!(out, Value::Option(Some(Box::new(Value::Int(3)))));
    }

    #[test]
    fn list_pop_on_empty_is_none_not_an_error() {
        let mut slot = ints(&[]);
        let out = apply("list", "pop", &mut slot, vec![]).unwrap();
        assert_eq!(as_ints(&slot), Vec::<i64>::new());
        assert_eq!(out, Value::Option(None));
    }

    /// The COW promise of C-033, at the level this tier implements it: a value
    /// cloned out BEFORE the mutation keeps the elements it had at that point,
    /// and the mutated slot moves on alone.
    #[test]
    fn a_clone_taken_before_the_push_keeps_its_own_elements() {
        let mut slot = ints(&[1, 2, 3]);
        let alias = slot.clone();
        apply("list", "push", &mut slot, vec![Value::Int(99)]).unwrap();
        assert_eq!(as_ints(&slot), vec![1, 2, 3, 99]);
        assert_eq!(as_ints(&alias), vec![1, 2, 3]);
    }

    /// The same promise for the shrink direction, which is the shape
    /// `alias_cow` block J exercises (`list.clear` with a live alias).
    #[test]
    fn a_clone_taken_before_the_clear_keeps_its_own_elements() {
        let mut slot = ints(&[1, 2, 3]);
        let alias = slot.clone();
        apply("list", "clear", &mut slot, vec![]).unwrap();
        assert_eq!(as_ints(&slot), Vec::<i64>::new());
        assert_eq!(as_ints(&alias), vec![1, 2, 3]);
    }

    /// Sole ownership must mutate the SAME allocation — that is what keeps a
    /// push loop linear instead of copying the whole container per call.
    #[test]
    fn a_sole_owner_push_does_not_reallocate_the_container() {
        let mut slot = Value::list(Vec::with_capacity(8));
        let Value::List(rc) = &slot else { panic!() };
        let before = Rc::as_ptr(rc);
        for i in 0..4 {
            apply("list", "push", &mut slot, vec![Value::Int(i)]).unwrap();
        }
        let Value::List(rc) = &slot else { panic!() };
        assert_eq!(before, Rc::as_ptr(rc), "push cloned a uniquely-owned list");
        assert_eq!(as_ints(&slot), vec![0, 1, 2, 3]);
    }

    #[test]
    fn map_insert_replaces_in_place_and_append_keeps_order() {
        let mut slot = map_of(&[(1, 10), (2, 20)]);
        apply("map", "insert", &mut slot, vec![Value::Int(1), Value::Int(99)]).unwrap();
        apply("map", "insert", &mut slot, vec![Value::Int(3), Value::Int(30)]).unwrap();
        assert_eq!(pairs(&slot), vec![(1, 99), (2, 20), (3, 30)]);
    }

    #[test]
    fn map_delete_keeps_the_order_of_the_rest() {
        let mut slot = map_of(&[(1, 10), (2, 20), (3, 30)]);
        apply("map", "delete", &mut slot, vec![Value::Int(2)]).unwrap();
        assert_eq!(pairs(&slot), vec![(1, 10), (3, 30)]);
    }

    #[test]
    fn map_delete_of_a_missing_key_is_a_no_op() {
        let mut slot = map_of(&[(1, 10)]);
        apply("map", "delete", &mut slot, vec![Value::Int(9)]).unwrap();
        assert_eq!(pairs(&slot), vec![(1, 10)]);
    }

    #[test]
    fn string_push_appends_a_suffix_not_a_char() {
        let mut slot = Value::str("ab");
        apply("string", "push", &mut slot, vec![Value::str("cd")]).unwrap();
        assert_eq!(slot, Value::str("abcd"));
    }

    #[test]
    fn bytes_push_truncates_to_a_byte_like_the_native_cast() {
        // `almide_rt_bytes_push` does `b.push(val as u8)`.
        let mut slot = ints(&[]);
        apply("bytes", "push", &mut slot, vec![Value::Int(256)]).unwrap();
        apply("bytes", "push", &mut slot, vec![Value::Int(-1)]).unwrap();
        assert_eq!(as_ints(&slot), vec![0, 255]);
    }

    #[test]
    fn clear_empties_every_container_kind() {
        let mut l = ints(&[1, 2]);
        apply("list", "clear", &mut l, vec![]).unwrap();
        assert_eq!(as_ints(&l), Vec::<i64>::new());
        let mut b = ints(&[1]);
        apply("bytes", "clear", &mut b, vec![]).unwrap();
        assert_eq!(as_ints(&b), Vec::<i64>::new());
        let mut s = Value::str("x");
        apply("string", "clear", &mut s, vec![]).unwrap();
        assert_eq!(s, Value::str(String::new()));
        let mut m = map_of(&[(1, 1)]);
        apply("map", "clear", &mut m, vec![]).unwrap();
        assert!(pairs(&m).is_empty());
    }

    /// A shape the typed IR should never produce must be reported, not guessed
    /// at — `None` becomes an ICE-style abort rather than a silent wrong vote.
    #[test]
    fn a_receiver_of_the_wrong_shape_is_rejected() {
        let mut slot = Value::Int(1);
        assert!(apply("list", "push", &mut slot, vec![Value::Int(2)]).is_none());
        let mut slot = ints(&[1]);
        assert!(apply("list", "sort", &mut slot, vec![]).is_none());
    }

    /// The tier table and the interception predicate are two hand-written
    /// lists; this is the seam where they drift. Every op that `writes_back`
    /// claims MUST be one the guard actually routes here, or the write-back is
    /// dead code that never runs.
    #[test]
    fn every_written_back_op_is_an_intercepted_op() {
        for (m, f) in [
            ("list", "push"),
            ("list", "pop"),
            ("list", "clear"),
            ("map", "insert"),
            ("map", "delete"),
            ("map", "clear"),
            ("string", "push"),
            ("string", "clear"),
            ("bytes", "push"),
            ("bytes", "clear"),
        ] {
            assert!(writes_back(m, f), "{m}.{f} missing from the tier table");
            assert!(
                crate::hofs::is_inplace_mutating_op(m, f),
                "{m}.{f} writes back but is never intercepted — dead code"
            );
        }
    }
}
