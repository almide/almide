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
use super::{Checker, err};
use crate::types::FnSig;

impl Checker {
    /// `user_fn` is false for a stdlib fn, whose `mut` the writer cannot drop.
    pub(super) fn reject_mut_param_fn_value(&mut self, shown: &str, sig: &FnSig, user_fn: bool) {
        let Some(&first) = sig.mut_params.first() else { return };
        let name = |i: usize| sig.params.get(i).map(|(n, _)| n.to_string()).unwrap_or_default();
        let is_mut = |i: &usize| sig.mut_params.contains(i);
        let all: Vec<usize> = (0..sig.params.len()).collect();
        let args = all.iter().map(|&i| name(i)).collect::<Vec<_>>().join(", ");
        let vars = all.iter().filter(|i| is_mut(i)).map(|&i| format!("`var {}`", name(i))).collect::<Vec<_>>().join(", ");
        let lambda = all.iter().filter(|i| !is_mut(i)).map(|&i| name(i)).collect::<Vec<_>>().join(", ");
        let param = name(first);
        let own_copy = if user_fn {
            format!(" If '{shown}' only changes its own copy, drop `mut` from '{param}' and rebind it with `var` inside the body")
        } else {
            String::new()
        };
        self.emit(err(
            format!("fn '{shown}' has a `mut` parameter '{param}' and cannot be used as a function value"),
            format!(
                "A `mut` parameter writes back to the caller's `var`, and a function value cannot carry that. \
                 Call it directly (`{shown}({args})` with {vars}), or pass a lambda that names the `var` \
                 (`({lambda}) => {shown}({args})`).{own_copy}"
            ),
            format!("{shown} used as a value"),
        ).with_code("E096"));
    }
}
