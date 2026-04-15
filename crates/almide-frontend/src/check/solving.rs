/// Constraint solving: unification via Union-Find.

use crate::types::Ty;
use super::Checker;
use super::err;
use super::types::{is_inference_var, resolve_ty};

impl Checker {
    pub(super) fn solve_constraints(&mut self) {
        let constraints = std::mem::take(&mut self.constraints);
        // Union-Find makes constraint solving order-independent.
        // A single pass suffices; re-processing is a harmless no-op.
        for c in &constraints {
            self.unify_infer(&c.expected, &c.actual);
        }
        // Emit diagnostics for unresolvable mismatches
        for c in &constraints {
            if !self.unify_infer(&c.expected, &c.actual) {
                let exp = resolve_ty(&c.expected, &self.uf);
                let act = resolve_ty(&c.actual, &self.uf);
                if exp != Ty::Unknown && act != Ty::Unknown {
                    let base = match c.context.as_str() {
                        "match arm" => "All match arms must share the same type. Change the mismatched arm to return the same type as the others, or change the first arm",
                        "if branches" | "if arm" => "Both branches of `if/then/else` must have the same type",
                        _ => "Fix the expression type or change the expected type",
                    };
                    let hint = Self::hint_with_conversion(base, &exp, &act);
                    // Context-specific try: snippet for the "Unit leak" failure
                    // mode — a statement (assignment / lone `let`) slips into a
                    // position expected to produce a value. dojo data shows this
                    // is the top E001 pattern for both 70b and 8b.
                    let try_snippet = unit_leak_snippet(&c.context, &exp, &act);
                    // Temporarily swap in the constraint's own span so the
                    // error is reported at the call site where the constraint
                    // was introduced, not at wherever checking happened to
                    // end up.
                    let saved_span = self.current_span;
                    if c.span.is_some() {
                        self.current_span = c.span;
                    }
                    let mut diag = err(
                        format!("type mismatch in {}: expected {} but got {}", c.context, exp.display(), act.display()),
                        hint, c.context.clone()).with_code("E001");
                    if let Some(snippet) = try_snippet {
                        diag = diag.with_try(snippet);
                    }
                    self.emit(diag);
                    self.current_span = saved_span;
                }
            }
        }
    }

    pub(crate) fn unify_infer(&mut self, a: &Ty, b: &Ty) -> bool {
        // Inference vars: union/bind immediately, always succeeds.
        // Conflicting concrete bindings are unified structurally when possible,
        // but the inference var case never returns false — matching HashMap semantics.
        match (is_inference_var(a), is_inference_var(b)) {
            (Some(ia), Some(ib)) => {
                self.uf.union(ia.0, ib.0);
                true
            }
            (Some(ia), None) => {
                let b_resolved = resolve_ty(b, &self.uf);
                if !self.uf.occurs(ia.0, &b_resolved) {
                    if let Some(existing) = self.uf.bind(ia.0, b_resolved.clone()) {
                        // Existing binding — try structural unify but don't fail
                        self.unify_infer(&existing, &b_resolved);
                    }
                }
                true
            }
            (None, Some(ib)) => {
                let a_resolved = resolve_ty(a, &self.uf);
                if !self.uf.occurs(ib.0, &a_resolved) {
                    if let Some(existing) = self.uf.bind(ib.0, a_resolved.clone()) {
                        self.unify_infer(&a_resolved, &existing);
                    }
                }
                true
            }
            (None, None) => {
                let a_resolved = resolve_ty(a, &self.uf);
                let b_resolved = resolve_ty(b, &self.uf);
                self.unify_structural(&a_resolved, &b_resolved)
            }
        }
    }

    fn unify_structural(&mut self, a: &Ty, b: &Ty) -> bool {
        if *a == Ty::Unknown || *b == Ty::Unknown { return true; }
        match (a, b) {
            (Ty::Applied(id1, args1), Ty::Applied(id2, args2)) if id1 == id2 && args1.len() == args2.len() => {
                args1.iter().zip(args2.iter()).all(|(x, y)| self.unify_infer(x, y))
            }
            (Ty::Tuple(a), Ty::Tuple(b)) if a.len() == b.len() =>
                a.iter().zip(b.iter()).all(|(x, y)| self.unify_infer(x, y)),
            (Ty::Fn { params: ap, ret: ar }, Ty::Fn { params: bp, ret: br }) if ap.len() == bp.len() =>
                ap.iter().zip(bp.iter()).all(|(x, y)| self.unify_infer(x, y)) && self.unify_infer(ar, br),
            (Ty::Record { fields: fa }, Ty::Record { fields: fb }) => {
                fa.len() == fb.len() && fa.iter().all(|(n, t)| fb.iter().any(|(n2, t2)| n == n2 && self.unify_infer(t, t2)))
            }
            (Ty::OpenRecord { fields: req, .. }, Ty::Record { fields: actual })
            | (Ty::OpenRecord { fields: req, .. }, Ty::OpenRecord { fields: actual, .. }) => {
                req.iter().all(|(n, t)| actual.iter().any(|(n2, t2)| n == n2 && self.unify_infer(t, t2)))
            }
            (Ty::Named(na, args_a), Ty::Named(nb, args_b)) if na == nb => {
                args_a.len() == args_b.len()
                    && args_a.iter().zip(args_b.iter()).all(|(ta, tb)| self.unify_infer(ta, tb))
                    || (args_a.is_empty() || args_b.is_empty())
            }
            (Ty::Named(_, _), _) => {
                let resolved = self.env.resolve_named(a);
                if resolved != *a { self.unify_infer(&resolved, b) } else { a.compatible(b) }
            }
            (_, Ty::Named(_, _)) => {
                let resolved = self.env.resolve_named(b);
                if resolved != *b { self.unify_infer(a, &resolved) } else { a.compatible(b) }
            }
            _ => a.compatible(b),
        }
    }
}

/// Produce a `try:` snippet for the "Unit leak" E001 pattern — the top
/// failure mode in dojo data: a statement (`let` binding without a tail,
/// or an assignment inside an if/match arm) ends up where a value was
/// expected. Only fires when `act == Unit` and `exp != Unit`, and the
/// context pins the leak to a specific syntactic hole.
fn unit_leak_snippet(context: &str, exp: &Ty, act: &Ty) -> Option<String> {
    if *act != Ty::Unit || *exp == Ty::Unit {
        return None;
    }
    let exp_str = exp.display();
    if context.starts_with("fn '") {
        Some(format!(
            "// fn body ends with a statement (returns Unit); \
            add a final expression that evaluates to {t}:\n\
            //   let tmp = <computation>\n\
            //   tmp                            // <-- the returned value\n\
            // Or inline:\n\
            //   <expression>                   // must have type {t}",
            t = exp_str
        ))
    } else if context == "if branches" || context == "if arm" {
        Some(format!(
            "// an if-arm is a statement (e.g. `x = y` or a bare `let`) — returns Unit.\n\
            // if/else is an *expression*: both arms must produce {t}. Rebind via let instead:\n\
            //   let new_x = if cond then <then-value> else <else-value>\n\
            // Or for loop-like state, use recursion:\n\
            //   fn step(x: {t}) -> {t} = if cond then step(<update>) else x",
            t = exp_str
        ))
    } else if context == "match arm" {
        Some(format!(
            "// a match arm is a statement (returns Unit). \
            Each arm must produce {t}.\n\
            //   match expr {{\n\
            //     PatA => value_a,   // <-- must be {t}\n\
            //     PatB => value_b,\n\
            //   }}",
            t = exp_str
        ))
    } else {
        None
    }
}
