//! E096: a fn with a `mut` parameter cannot become a function value (#3456).
//!
//! A `mut` parameter writes back to the caller's `var` (C-226). That promise
//! lives in the fn's SIGNATURE (`FnSig::mut_params`), and a function TYPE has
//! no place for it: `(St) -> Unit` is all a value of `bump` carries. Once the
//! fn is a value, neither target passes the argument by mutable reference —
//! wasm silently dropped the write and native failed rustc E0596. So the name
//! is refused where it turns into a value: a bare identifier, a stdlib or user
//! module member. Every flow of a value (a `let`, an argument, a record field,
//! a list element, a return) starts at one of those references, so judging
//! the reference covers them all. A call never reaches here: the call path
//! resolves its callee's signature directly and keeps the write-back.
//!
//! The hint fits the place (#3469): filling a fn-typed call slot, it suggests
//! a lambda of the slot's arity that copies each `mut` argument into a local
//! `var` — names the lambda itself binds, never the fn's own parameter names.
use super::{Checker, err};
use crate::types::{FnSig, Ty};

impl Checker {
    /// `user_fn` is false for a stdlib fn, whose `mut` the writer cannot drop.
    pub(super) fn reject_mut_param_fn_value(&mut self, shown: &str, sig: &FnSig, user_fn: bool) {
        let Some(&first) = sig.mut_params.first() else { return };
        let slot = self.fn_value_slot.take();
        let param = sig.params.get(first).map(|(n, _)| n.to_string()).unwrap_or_default();
        let (fix, own_fix) = match slot {
            Some(Ty::Fn { params, ret, .. }) if params.len() == sig.params.len() => (
                format!("Pass a lambda that copies into a local `var`: `{}`.", slot_lambda(shown, sig, !matches!(*ret, Ty::Unit))),
                "return the new value",
            ),
            _ => (direct_call_fix(shown, sig), "rebind it with `var` inside the body"),
        };
        let own_copy = if user_fn {
            format!(" If '{shown}' only changes its own copy, drop `mut` from '{param}' and {own_fix}")
        } else {
            String::new()
        };
        self.emit(err(
            format!("fn '{shown}' has a `mut` parameter '{param}' and cannot be used as a function value"),
            format!("A `mut` parameter writes back to the caller's `var`, and a function value cannot carry that. {fix}{own_copy}"),
            format!("{shown} used as a value"),
        ).with_code("E096"));
    }
}

/// The `let f = bump` shape: call the fn on the caller's `var` directly.
fn direct_call_fix(shown: &str, sig: &FnSig) -> String {
    let name = |i: usize| sig.params.get(i).map(|(n, _)| n.to_string()).unwrap_or_default();
    let is_mut = |i: &usize| sig.mut_params.contains(i);
    let all: Vec<usize> = (0..sig.params.len()).collect();
    let args = all.iter().map(|&i| name(i)).collect::<Vec<_>>().join(", ");
    let vars = all.iter().filter(|i| is_mut(i)).map(|&i| format!("`var {}`", name(i))).collect::<Vec<_>>().join(", ");
    let lambda = all.iter().filter(|i| !is_mut(i)).map(|&i| name(i)).collect::<Vec<_>>().join(", ");
    format!("Call it directly (`{shown}({args})` with {vars}), or pass a lambda that names the `var` (`({lambda}) => {shown}({args})`).")
}

/// `(item) => { var copy = item; grow(copy); copy }` for a slot of the fn's
/// arity. The tail is the call's own result, or — for a `Unit` fn in a slot
/// that wants a value — the single `mut` copy, which holds the new value.
fn slot_lambda(shown: &str, sig: &FnSig, slot_wants_value: bool) -> String {
    let arity = sig.params.len();
    let lp = |i: usize| if arity == 1 { "item".to_string() } else { format!("a{}", i + 1) };
    let copy = |i: usize| if sig.mut_params.len() == 1 { "copy".to_string() } else { format!("{}_copy", lp(i)) };
    let is_mut = |i: usize| sig.mut_params.contains(&i);
    let params = (0..arity).map(lp).collect::<Vec<_>>().join(", ");
    let vars = sig.mut_params.iter().map(|&i| format!("var {} = {}; ", copy(i), lp(i))).collect::<String>();
    let args = (0..arity).map(|i| if is_mut(i) { copy(i) } else { lp(i) }).collect::<Vec<_>>().join(", ");
    let call = format!("{shown}({args})");
    let body = match (sig.ret == Ty::Unit && slot_wants_value, sig.mut_params.as_slice()) {
        (true, [only]) => format!("{call}; {}", copy(*only)),
        _ => call,
    };
    format!("({params}) => {{ {vars}{body} }}")
}
