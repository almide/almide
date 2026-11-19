// E011 — a write to an enclosing `var` from inside a closure of a PURE fn.
// Included by `infer_statements.rs`.

impl Checker {
    /// E011: a pure fn's closure must not mutate a `var` the fn declared
    /// outside it — mutation through a closure is an effect the pure fn
    /// cannot host (fix: mark the enclosing fn `effect fn`, or move the
    /// mutation out of the closure). The rule judges the MUTATED BINDING,
    /// so every write form reaches it with its root: a whole assign
    /// (`seen = v`), a place write (`seen[k] = v`, `s.f = v`,
    /// `o.inner.xs = v`) and an in-place mutator taking it as a `mut`
    /// argument (`list.push(seen, v)`). Only the whole assign used to be
    /// judged, so `seen = v` was E011 while `seen[k] = v` passed (#3344).
    pub(crate) fn check_closure_escape(&mut self, name: &str, shape: String) {
        let name = sym(name);
        if !self.env.mutable_vars.contains(&name) || self.env.can_call_effect {
            return;
        }
        use crate::type_env::MutableDecl;
        // Decided from how the binding was declared (#4859 ruling): E011 is
        // about a LOCAL mutable binding a closure captures. A module-level
        // `var` may be written by a pure fn directly or through an inlined
        // callback (#3317, C-187 family), and a `mut` parameter writes through
        // to the caller's binding, which the fn may write directly. A binding
        // whose declaration was not recorded is refused inside a closure,
        // never admitted.
        let refused = match self.env.mutable_decl.get(&name).copied() {
            Some(MutableDecl::Local { lambda_depth }) => self.env.lambda_depth > lambda_depth,
            Some(MutableDecl::TopLevel) | Some(MutableDecl::Param) => false,
            None => self.env.lambda_depth > 0,
        };
        if refused {
            self.emit(super::err(
                format!("mutable variable '{}' is mutated inside a closure in a pure function — use effect fn instead", name),
                "Move the mutation out of the closure, or mark the enclosing function as `effect fn`",
                shape).with_code("E011"));
        }
    }
}
