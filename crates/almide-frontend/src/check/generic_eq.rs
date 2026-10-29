// Continuation of `impl Checker` (spliced into check/mod.rs by `include!`):
// `==` reached through a type variable, and a Secret (#4239).
//
// `==` on a Secret is decided where the operand type is concrete: a
// `Secret[String]` / `Secret[Bytes]` compares in constant time (the lowering
// routes it to `is_equal_constant_time`), any other Secret is E103. Inside
// `fn same[A](a: A, b: A) -> Bool = a == b` the operand type is the rigid
// `A`, so neither rule can fire there, and the instance compares
// structurally — `same(secret.new(1), secret.new(2))` compiled, and a
// `Secret[String]` reaching `same` compared byte by byte, early exit and all.
//
// The rule chosen: a generic body's `==` is ONE structural compare for every
// instantiation. That is how a bound on `A` works everywhere else — it is
// checked at the call site against the instantiated type and never changes
// what the body runs — so the compare is a REQUIREMENT on `A` ("compared
// structurally"), carried exactly like the #2496 string-form requirement:
// recorded on the fn, transferred to a generic caller, and judged at every
// concrete call. A type that holds a Secret anywhere does not meet it (E103),
// whatever the Secret holds: switching the instance to the constant-time
// compare would make the body mean different things per instantiation, and
// refusing keeps "a Secret never reaches a structural compare" true by
// construction. A stdlib fn whose runtime bounds a parameter by equality
// (`list.contains`, `list.index_of`, …) is the same generic compare, so a
// Secret-holding binding there is E103 too.

/// One `==` / `!=` / `assert_eq` / stdlib-equality site, pending the
/// post-solve equality checks.
#[derive(Debug, Clone)]
pub(crate) struct EqSite {
    pub ty: Ty,
    pub span: Option<crate::ast::Span>,
    pub what: String,
    /// The enclosing GENERIC fn's key, when the site is inside one.
    pub in_fn: Option<Sym>,
    /// The compare is a generic (structural) one: a stdlib fn's equality
    /// bound on its parameter.
    pub generic_compare: bool,
}

impl Checker {
    /// The key of the generic fn whose body is being checked, if any.
    pub(crate) fn generic_fn_key(&self) -> Option<Sym> {
        self.current_fn.as_ref().filter(|(_, gs)| !gs.is_empty()).map(|(k, _)| *k)
    }

    /// An `==` site whose operand type names the enclosing fn's generics
    /// becomes a requirement on that fn.
    fn record_eq_requirement(&mut self, site: &EqSite, resolved: &Ty) {
        if !mentions_rigid_generic(resolved) {
            return;
        }
        if let Some(fn_key) = site.in_fn {
            let origin = InterpLoc {
                file: self.source_file.clone(),
                span: site.span,
                label: format!("`{}`", fn_key.as_str()),
            };
            self.eq_reqs.entry(fn_key).or_default().push(InterpReq { ty: resolved.clone(), origin, via: Vec::new() });
        }
    }

    /// E103 for a concrete stdlib generic compare over a Secret-holding
    /// type (`list.contains(keys, k)`): the stdlib body compares
    /// structurally, never in constant time.
    fn generic_compare_secret_diag(&self, site: &EqSite, resolved: &Ty) -> Option<Diagnostic> {
        if !site.generic_compare || !crate::secret_type::holds_secret(&self.env, resolved) {
            return None;
        }
        let mut diag = err(
            format!("{} cannot compare '{}': its compare is generic and structural, and a Secret compares only in constant time at a concrete `==`", site.what, resolved.display()),
            "Compare what the secrets hold, explicitly: `secret.reveal(a) == secret.reveal(b)`, or write the \
             `==` on the concrete Secret[String] / Secret[Bytes], which is constant time.",
            site.what.clone(),
        ).with_code("E103");
        if let Some(s) = site.span {
            diag.line = Some(s.line);
            diag.col = Some(s.col);
        }
        Some(diag)
    }

    /// Post-solve, per program: propagate the `==` requirements through
    /// calls made inside generic bodies and report every concrete
    /// instantiation that holds a Secret (#4239). Runs after
    /// `validate_interp_instantiations` (which resolves this program's
    /// generic calls) and `validate_eq_operand_types` (which records the
    /// requirements).
    fn validate_eq_instantiations(&mut self) {
        propagate_reqs(&self.generic_calls, &mut self.eq_reqs);
        let mut found: Vec<(GenericCall, InterpReq, Ty)> = Vec::new();
        for call in &self.generic_calls {
            let Some(reqs) = self.eq_reqs.get(&call.callee) else { continue };
            for req in reqs {
                let t = crate::types::substitute(&req.ty, &call.bindings);
                if mentions_rigid_generic(&t) || mentions_unresolved(&t) {
                    continue;
                }
                if crate::secret_type::holds_secret(&self.env, &t) {
                    found.push((call.clone(), req.clone(), t));
                }
            }
        }
        for (call, req, concrete) in found {
            self.report_eq_instantiation(&call, &req, &concrete);
        }
    }

    /// One E103 for a (call, requirement) pair, anchored like E089's
    /// instantiation report: on the first site of the chain in this file.
    fn report_eq_instantiation(&mut self, call: &GenericCall, req: &InterpReq, concrete: &Ty) {
        let key = (
            call.site.file.clone().unwrap_or_default(),
            call.site.span.map_or(0, |s| s.line),
            call.site.span.map_or(0, |s| s.col),
            req.origin.file.clone().unwrap_or_default(),
            req.origin.span.map_or(0, |s| s.line),
            req.origin.span.map_or(0, |s| s.col),
        );
        if !self.eq_reported.insert(key) {
            return;
        }
        let here = |loc: &InterpLoc| loc.file == self.source_file;
        let anchor = std::iter::once(&call.site)
            .chain(req.via.iter())
            .chain(std::iter::once(&req.origin))
            .find(|l| here(l))
            .unwrap_or(&call.site)
            .clone();
        let loc_text = |loc: &InterpLoc| -> String {
            let pos = loc.span.map(|s| format!("{}:{}", s.line, s.col)).unwrap_or_default();
            if here(loc) { format!("line {pos}") } else { format!("{}:{pos}", loc.file.clone().unwrap_or_default()) }
        };
        let via_text = if req.via.is_empty() {
            String::new()
        } else {
            let hops: Vec<String> = req.via.iter().map(|v| format!("{} at {}", v.label, loc_text(v))).collect();
            format!(", through {}", hops.join(", "))
        };
        let mut diag = err(
            format!(
                "{} compares a value of type `{}` with `==` at {}; the call to {} at {}{} makes it `{}` — a generic `==` is structural, and a Secret compares only in constant time at a concrete `==`",
                req.origin.label,
                req.ty.display(),
                loc_text(&req.origin),
                call.site.label,
                loc_text(&call.site),
                via_text,
                concrete.display(),
            ),
            "Compare what the secrets hold, explicitly: `secret.reveal(a) == secret.reveal(b)`, or write the \
             `==` on the concrete Secret[String] / Secret[Bytes] (constant time) instead of through a generic fn.",
            format!("call to {}", call.site.label),
        ).with_code("E103");
        diag.file = anchor.file.clone();
        if let Some(s) = anchor.span {
            diag.line = Some(s.line);
            diag.col = Some(s.col);
        }
        for other in std::iter::once(&req.origin).chain(std::iter::once(&call.site)) {
            if here(other)
                && other.span != anchor.span
                && let Some(s) = other.span
            {
                diag = diag.with_secondary(s.line, Some(s.col), if other.span == req.origin.span { "the compare" } else { "the call" });
            }
        }
        self.diagnostics.push(diag);
    }

    /// E033 for a record update whose base resolved, after the solve, to a
    /// foreign opaque record — the base was still open where the spread
    /// was inferred (`{ ...x }` on a lambda parameter).
    fn validate_opaque_spreads(&mut self) {
        for (ty, span) in std::mem::take(&mut self.deferred_opaque_spreads) {
            let resolved = resolve_ty(&ty, &self.uf);
            let saved = self.current_span;
            self.current_span = span;
            self.reject_foreign_opaque_record(&resolved, "build a record update of");
            self.current_span = saved;
        }
    }
}

/// A requirement whose type, under a call's bindings, still names the
/// CALLER's generics is the caller's requirement too. To a fixpoint, since a
/// chain may be any length. Shared by the string-form (#2496) and the
/// equality (#4239) requirements.
fn propagate_reqs(calls: &[GenericCall], reqs_by_fn: &mut std::collections::HashMap<Sym, Vec<InterpReq>>) {
    loop {
        let mut added: Vec<(Sym, InterpReq)> = Vec::new();
        for call in calls {
            let Some(caller) = call.caller else { continue };
            let Some(reqs) = reqs_by_fn.get(&call.callee) else { continue };
            for req in reqs {
                let t = crate::types::substitute(&req.ty, &call.bindings);
                if !mentions_rigid_generic(&t) {
                    continue;
                }
                let same = |r: &InterpReq| r.ty == t && r.origin.file == req.origin.file && r.origin.span == req.origin.span;
                let known = reqs_by_fn.get(&caller).is_some_and(|rs| rs.iter().any(same))
                    || added.iter().any(|(c, r)| *c == caller && same(r));
                if known {
                    continue;
                }
                let mut via = vec![call.site.clone()];
                via.extend(req.via.iter().cloned());
                added.push((caller, InterpReq { ty: t, origin: req.origin.clone(), via }));
            }
        }
        if added.is_empty() {
            break;
        }
        for (caller, req) in added {
            reqs_by_fn.entry(caller).or_default().push(req);
        }
    }
}

// The post-solve equality pass (moved from post_solve_checks.rs for its
// 800-line budget): it feeds the #4239 requirements above.
impl Checker {
    /// E016 (#2606): `==` / `!=` / `assert_eq` / `assert_ne` need an operand
    /// type with EQUALITY, and "Function types are never Eq"
    /// (docs/specs/type-system.md) — through any Result / Option / List /
    /// tuple / record field / variant payload. The checker used to accept the
    /// comparison and leave it to rustc (E0369 on `Rc<dyn Fn>`, plus E0277 for
    /// the missing `Debug` in `assert_eq`); E016 is the class that already
    /// rejects a function in a Set element or Map key for the same reason.
    fn validate_eq_operand_types(&mut self) {
        use std::collections::HashSet;
        let mut reported: HashSet<(usize, usize)> = HashSet::new();
        let checks = std::mem::take(&mut self.deferred_eq_checks);
        for site in checks {
            let resolved = resolve_ty(&site.ty, &self.uf);
            // #4239: an operand typed by the enclosing fn's generics is also
            // a requirement on every instantiation, judged at its calls.
            self.record_eq_requirement(&site, &resolved);
            let (span, what) = (site.span, site.what.clone());
            if self.env.is_eq(&resolved) {
                // #4239: a stdlib generic compare is structural.
                if let Some(diag) = self.generic_compare_secret_diag(&site, &resolved) {
                    if span.is_some_and(|s| !reported.insert((s.line, s.col))) {
                        continue;
                    }
                    self.diagnostics.push(diag);
                    continue;
                }
                // #4107: a Secret compares only in constant time (R-secret-03).
                if let Some(mut diag) = crate::secret_type::secret_eq_diagnostic(&self.env, &resolved, &what) {
                    if span.is_some_and(|s| !reported.insert((s.line, s.col))) {
                        continue;
                    }
                    if let Some(s) = span {
                        diag.line = Some(s.line);
                        diag.col = Some(s.col);
                    }
                    self.diagnostics.push(diag);
                }
                continue;
            }
            if span.is_some_and(|s| !reported.insert((s.line, s.col))) {
                continue;
            }
            let ty_name = resolved.display();
            let mut diag = err(
                format!("type '{}' has no equality — {} cannot compare it (it contains a function)", ty_name, what),
                concat!(
                    "Function values have no equality, so neither does a Result, Option, list, tuple, ",
                    "record or variant that holds one. Compare what you can observe instead: the tag ",
                    "(`result.is_err(r)`, `option.is_some(o)`), the error (`result.to_err_option(r) == some(e)`), ",
                    "or the function's output on a sample input.",
                )
                .to_string(),
                what.clone(),
            ).with_code("E016");
            if let Some(s) = span {
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            self.diagnostics.push(diag);
        }
    }
}
