// TRACKED-CALL classification — is a `match`/`??` subject a call to a
// self-host fn whose result is a real MATERIALIZED Option/Result block (and
// may therefore EXECUTE instead of linearize)? include!-spliced from
// control_b.rs, sharing the lower module's imports.

/// Is `subject` a call to a SELF-HOST Option-returning stdlib fn? Such a call returns a
/// real MATERIALIZED 0-or-1-element-list Option (its impl returns through `Some(scalar)`/
/// `None` helpers, tail-materialized), so a `match` over its result may EXECUTE — the call
/// dst is tracked in `materialized_options`. NARROW to the fns ACTUALLY self-hosted today
/// (`list.get`): a fn merely declared Option-returning but NOT self-hosted would return a
/// deferred `Opaque` (len0) that must NOT be tracked, else the match would misread it as
/// `None`. Add a name here only when its self-host impl + registry entry land together.

fn is_self_host_result_call(subject: &IrExpr) -> bool {
    match &subject.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
            is_self_host_result_module_fn(module.as_str(), func.as_str())
        }
        _ => false,
    }
}

/// Is the match subject a self-host call returning a HEAP-Ok Result (`result.zip` /
/// `value.as_string` — the cap-as-tag 1-slot DynListStr)? Drives the `materialized_results_str` +
/// `heap_elem_lists` tracking so a direct `match` over it executes (binds the @12 payload handle).
fn is_self_host_result_str_call(subject: &IrExpr) -> bool {
    match &subject.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
            crate::lower::is_self_host_result_str_module_fn(module.as_str(), func.as_str())
        }
        _ => false,
    }
}
