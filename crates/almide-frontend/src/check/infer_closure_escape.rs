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
        let Some(&decl_depth) = self.env.var_lambda_depth.get(&name) else { return };
        if self.env.lambda_depth > decl_depth {
            self.emit(super::err(
                format!("mutable variable '{}' is mutated inside a closure in a pure function — use effect fn instead", name),
                "Move the mutation out of the closure, or mark the enclosing function as `effect fn`",
                shape).with_code("E011"));
        }
    }
}
