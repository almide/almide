// Continuation of `impl Checker` (spliced into check/mod.rs by `include!`):
// the post-solve string-form checks over `${…}` segments.
//
// E089 — a value with no defined string form cannot be interpolated. The
// native leg renders a segment through `Display` (scalars, String) or
// `AlmideRepr` (containers, records, variants); a type with neither passed
// `check` and died at rustc with E0277 behind the codegen wall, while both
// wasm legs walled the shape. So the set below has never printed anywhere,
// and rejecting it at check time changes no program that built:
//
//   Bytes            (the rule since b8e69c77c; the hint names the spellings)
//   Unit             `"${u}"`, `"${noop()}"`
//   Matrix           `Display` is not implemented for AlmideMatrix
//   a raw pointer    `*mut u8`
//   a function value `Rc<dyn Fn>` — bare, or held by a container, tuple,
//                    anonymous record, or a named record / variant field
//                    (transitively, including through the type's arguments)
//
// Inside a container Unit, Bytes and Matrix DO render (`[(), ()]`,
// `[[1, 2]]`, `[matrix.from_lists([[0]])]`) — the `AlmideRepr` impls exist
// — so only the function-value / raw-pointer gap propagates through a
// compound; the three scalars are gaps at the top level only.
//
// GENERIC BODIES (#2496): `fn show[T](v: T) = "v=${v}"` is fine for
// `show(1)` and E0277 for `show(bytes)`; the segment's type is a rigid
// `T` when the body is checked, so the rule cannot fire there. Instead the
// segment becomes a REQUIREMENT on `show` — "`T` must have a string form"
// — and every call to a generic user fn is judged against its callee's
// requirements once the call's bindings resolve. A requirement whose
// substituted type still names the CALLER's generics transfers to the
// caller (`fn wrap[U](u: U) = show(u)` inherits show's), to a fixpoint.
//
// The entry program is inferred before the modules it imports, so a call
// and the requirement it violates may sit in files inferred either way
// round; requirements and calls are checker-wide and the check re-runs at
// every program's post-solve, anchoring the diagnostic on the site in the
// file being inferred and naming the other in the message. A pair is
// reported once.

/// One `${…}` segment, pending the post-solve string-form checks.
#[derive(Debug, Clone)]
pub(crate) struct InterpSite {
    pub ty: Ty,
    pub span: Option<crate::ast::Span>,
    /// The segment is a CALL in an effect-fn body: the #1123 implicit-
    /// propagation check owns its Result, so the debug-form warning stays
    /// quiet — the string-form rejection still applies to its value.
    pub auto_unwrap_call: bool,
    /// The enclosing fn (its resolution key) and that fn's generics, when
    /// the segment sits inside a generic fn body.
    pub in_fn: Option<(Sym, Vec<Sym>)>,
}

/// A call to a generic user fn, pending the resolution of its bindings.
#[derive(Debug, Clone)]
pub(crate) struct DeferredGenericCall {
    pub callee: Sym,
    pub bindings: std::collections::HashMap<Sym, Ty>,
    pub span: Option<crate::ast::Span>,
    /// The generic fn the call sits inside, when it does — its bindings
    /// may then name that fn's rigid generics, and the callee's
    /// requirements transfer to it.
    pub caller: Option<Sym>,
}

/// A call to a generic user fn with its bindings resolved (#2496).
#[derive(Debug, Clone)]
pub(crate) struct GenericCall {
    pub callee: Sym,
    pub bindings: std::collections::HashMap<Sym, Ty>,
    pub site: InterpLoc,
    pub caller: Option<Sym>,
}

/// A requirement a generic fn's body places on its instantiations: the
/// segment type (in the fn's rigid generics) must have a string form.
#[derive(Debug, Clone)]
pub(crate) struct InterpReq {
    pub ty: Ty,
    /// The `${…}` segment the requirement comes from.
    pub origin: InterpLoc,
    /// The calls the requirement travelled through to reach this fn,
    /// innermost first: `wrap` calling `show(u)` inherits show's
    /// requirement via that call.
    pub via: Vec<InterpLoc>,
}

/// A source location a post-solve check may anchor a diagnostic on.
#[derive(Debug, Clone)]
pub(crate) struct InterpLoc {
    pub file: Option<String>,
    pub span: Option<crate::ast::Span>,
    /// What the location is, for the message (`` `show` ``'s segment, a
    /// call to `wrap`).
    pub label: String,
}

/// Why a type has no defined string form (E089).
pub(crate) struct StringFormGap {
    /// "a Bytes value", "a `List[(Int) -> Int]` value — it holds a function value".
    pub what: String,
    pub hint: &'static str,
}

const BYTES_HINT: &str = "Interpolate what you mean: `${bytes.to_list(b)}` for the octets, \
     `${bytes.to_string_lossy(b)}` for UTF-8 text, or \
     `${int.to_string(bytes.len(b))}` for the length.";
const UNIT_HINT: &str = "A Unit carries nothing to print. Drop the segment, or interpolate the \
     value you meant — a `println` or other Unit-returning call has no text.";
const MATRIX_HINT: &str = "Interpolate what you mean: `${matrix.to_lists(m)}` for the rows, or \
     `${matrix.rows(m)}x${matrix.cols(m)}` for the shape.";
const RAWPTR_HINT: &str = "A raw pointer has no stable text. Interpolate the Bytes it was taken \
     from (`bytes.from_raw_ptr(p, n)`), or the length.";
const FN_HINT: &str = "A function value has no text. Call it and interpolate the result \
     (`${f(x)}`), or interpolate a name for it.";
const HOLDS_HINT: &str = "Interpolate the printable parts instead — map the function values \
     away first (`list.map(fs, (f) => f(0))`), or pick the fields that are data.";

/// A rigid generic: a `TypeVar` that is a declared generic name, not a
/// `?N` inference slot.
fn is_rigid_generic(t: &Ty) -> bool {
    matches!(t, Ty::TypeVar(n) if !n.as_str().starts_with('?'))
}

fn mentions_rigid_generic(ty: &Ty) -> bool {
    ty.any_child_recursive(&is_rigid_generic)
}

/// Unresolved after the whole program solved (E025 / E018 territory): the
/// instantiation cannot be judged, and is not.
fn mentions_unresolved(ty: &Ty) -> bool {
    ty.any_child_recursive(&|t| matches!(t, Ty::Unknown) || matches!(t, Ty::TypeVar(n) if n.as_str().starts_with('?')))
}

impl Checker {
    /// The key a fn DECLARED in the program being inferred resolves under
    /// at its call sites: `{module}.{name}` inside a module (the canonical
    /// registration; `declares_it_itself` in calls.rs asks the same
    /// question), bare in the entry program.
    pub(crate) fn fn_decl_key(&self, name: &str) -> Sym {
        match &self.current_module_prefix {
            Some(p) => sym(&format!("{p}.{name}")),
            None => sym(name),
        }
    }

    /// The key a CALL's generic callee is recorded under, or `None` for a
    /// stdlib fn (the trusted runtime below the check; none interpolates a
    /// generic). Mirrors `resolve_call_sig_and_import`'s resolution order:
    /// a selective import's canonical name, an alias-resolved qualified
    /// name, a module's own bare name under its prefix, else the bare name.
    pub(crate) fn generic_call_key(&self, name: &str, qualified_via_direct: Option<&str>) -> Option<Sym> {
        let key = if let Some(q) = qualified_via_direct {
            q.to_string()
        } else if let Some((module, func)) = name.split_once('.') {
            match self.env.import_table.resolve(module) {
                Some(canonical) => format!("{}.{}", canonical.as_str(), func),
                None => name.to_string(),
            }
        } else if let Some(p) = &self.current_module_prefix {
            let own = format!("{p}.{name}");
            if self.env.functions.contains_key(&sym(&own)) { own } else { name.to_string() }
        } else {
            name.to_string()
        };
        if let Some((module, _)) = key.rsplit_once('.') {
            // A user module may share a stdlib module's name (#2223): the
            // file's import table knows which one it brought in.
            let table = &self.env.import_table;
            let user_import = table.aliases.contains_key(&sym(module)) && !table.stdlib.contains(&sym(module));
            if crate::stdlib::is_stdlib_module(module) && !user_import {
                return None;
            }
        }
        Some(sym(&key))
    }

    /// `Some` when `ty` has no defined string form — see the file header
    /// for the set and its reasons.
    pub(crate) fn string_form_gap(&self, ty: &Ty) -> Option<StringFormGap> {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let gap = |what: String, hint: &'static str| Some(StringFormGap { what, hint });
        match ty {
            Ty::Bytes => gap("a Bytes value".into(), BYTES_HINT),
            Ty::Unit => gap("a Unit value".into(), UNIT_HINT),
            Ty::Matrix | Ty::Applied(TC::Matrix, _) => gap("a Matrix value".into(), MATRIX_HINT),
            Ty::RawPtr => gap("a raw pointer".into(), RAWPTR_HINT),
            Ty::Fn { .. } => gap("a function value".into(), FN_HINT),
            _ => {
                let leaf = self.unprintable_leaf_in(ty, &mut Vec::new())?;
                gap(format!("a `{}` value — it holds {leaf}", ty.display()), HOLDS_HINT)
            }
        }
    }

    /// The first function value / raw pointer reachable inside `ty` —
    /// through containers, tuples, unions, anonymous records, and named
    /// records / variants (their definition and their type arguments) —
    /// described for the message. `visiting` guards recursive types.
    fn unprintable_leaf_in(&self, ty: &Ty, visiting: &mut Vec<Sym>) -> Option<String> {
        match ty {
            Ty::Fn { .. } => Some("a function value".to_string()),
            Ty::RawPtr => Some("a raw pointer".to_string()),
            Ty::Applied(_, args) | Ty::Tuple(args) | Ty::Union(args) => {
                args.iter().find_map(|a| self.unprintable_leaf_in(a, visiting))
            }
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().find_map(|(n, t)| {
                self.unprintable_leaf_in(t, visiting).map(|l| format!("{l} (field `{n}`)"))
            }),
            Ty::Variant { cases, .. } => cases.iter().find_map(|c| {
                let payload: Vec<&Ty> = match &c.payload {
                    crate::types::VariantPayload::Unit => Vec::new(),
                    crate::types::VariantPayload::Tuple(ts) => ts.iter().collect(),
                    crate::types::VariantPayload::Record(fs) => fs.iter().map(|(_, t)| t).collect(),
                };
                payload.iter().find_map(|t| self.unprintable_leaf_in(t, visiting))
                    .map(|l| format!("{l} (case `{}`)", c.name))
            }),
            Ty::Named(name, args) => {
                if let Some(l) = args.iter().find_map(|a| self.unprintable_leaf_in(a, visiting)) {
                    return Some(l);
                }
                if visiting.contains(name) {
                    return None;
                }
                let def = self.env.types.get(name).cloned()?;
                visiting.push(*name);
                let found = self.unprintable_leaf_in(&def, visiting).map(|l| format!("{l} of `{name}`"));
                visiting.pop();
                found
            }
            _ => None,
        }
    }

    /// Post-solve, per program: E089 on a concrete segment with no string
    /// form; a segment in a generic body whose type names the fn's generics
    /// becomes a requirement on that fn; and the #1051 warning for a
    /// Result the lowering will not auto-`?`.
    fn validate_result_interpolations(&mut self) {
        let sites = std::mem::take(&mut self.deferred_result_interp_checks);
        for site in sites {
            let resolved = resolve_ty(&site.ty, &self.uf);
            if mentions_rigid_generic(&resolved) {
                if let Some((fn_key, _)) = site.in_fn.as_ref().filter(|(_, gs)| !gs.is_empty()) {
                    let origin = InterpLoc {
                        file: self.source_file.clone(),
                        span: site.span,
                        label: format!("`{}`", fn_key.as_str()),
                    };
                    self.interp_reqs.entry(*fn_key).or_default().push(InterpReq { ty: resolved, origin, via: Vec::new() });
                }
                continue;
            }
            // An undecidable slot is E025's business (the same carve-out the
            // orderable-element check makes): a recovery type like the
            // reserved-ctor `err` binding's `fn(?1) -> Result[?2, ?1]` would
            // otherwise stack a second error under the real one.
            if mentions_unresolved(&resolved) {
                continue;
            }
            if let Some(gap) = self.string_form_gap(&resolved) {
                let mut diag = err(
                    format!("{} has no defined string form — it cannot be interpolated", gap.what),
                    gap.hint.to_string(),
                    "string interpolation".to_string(),
                ).with_code("E089");
                if let Some(s) = site.span {
                    diag.file = self.source_file.clone();
                    diag.line = Some(s.line);
                    diag.col = Some(s.col);
                }
                self.diagnostics.push(diag);
                continue;
            }
            // #1123: a CALL segment's Result belongs to the implicit-
            // propagation check (E041); warning on it too would be noise.
            if site.auto_unwrap_call || !resolved.is_result() {
                continue;
            }
            let mut diag = Diagnostic::warning(
                format!("interpolating a {} prints its debug form (ok(…)/err(…))", resolved.display()),
                "If you meant the payload, unwrap first: `?? fallback` supplies a default, \
                 `match` handles ok/err, `!` propagates in an effect fn body. Interpolate \
                 the Result itself only for debug output",
                "string interpolation",
            );
            if let Some(s) = site.span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            self.diagnostics.push(diag);
        }
    }

    /// Post-solve, per program: resolve this program's generic calls into
    /// the checker-wide list, propagate requirements through calls made
    /// inside generic bodies, and report every concrete instantiation that
    /// has no string form (#2496).
    fn validate_interp_instantiations(&mut self) {
        let file = self.source_file.clone();
        for call in std::mem::take(&mut self.deferred_generic_calls) {
            let bindings = call.bindings.iter()
                .map(|(g, t)| (*g, resolve_ty(t, &self.uf)))
                .collect();
            self.generic_calls.push(GenericCall {
                callee: call.callee,
                bindings,
                site: InterpLoc { file: file.clone(), span: call.span, label: format!("`{}`", call.callee.as_str()) },
                caller: call.caller,
            });
        }
        self.propagate_interp_reqs();

        let mut found: Vec<(GenericCall, InterpReq, Ty, StringFormGap)> = Vec::new();
        for call in &self.generic_calls {
            let Some(reqs) = self.interp_reqs.get(&call.callee) else { continue };
            for req in reqs {
                let t = crate::types::substitute(&req.ty, &call.bindings);
                if mentions_rigid_generic(&t) || mentions_unresolved(&t) {
                    continue;
                }
                if let Some(gap) = self.string_form_gap(&t) {
                    found.push((call.clone(), req.clone(), t, gap));
                }
            }
        }
        for (call, req, concrete, gap) in found {
            self.report_interp_instantiation(&call, &req, &concrete, &gap);
        }
    }

    /// A requirement whose type, under a call's bindings, still names the
    /// CALLER's generics is the caller's requirement too. To a fixpoint,
    /// since a chain may be any length.
    fn propagate_interp_reqs(&mut self) {
        loop {
            let mut added: Vec<(Sym, InterpReq)> = Vec::new();
            for call in &self.generic_calls {
                let Some(caller) = call.caller else { continue };
                let Some(reqs) = self.interp_reqs.get(&call.callee) else { continue };
                for req in reqs {
                    let t = crate::types::substitute(&req.ty, &call.bindings);
                    if !mentions_rigid_generic(&t) {
                        continue;
                    }
                    let known = self.interp_reqs.get(&caller).is_some_and(|rs| rs.iter().any(|r| {
                        r.ty == t && r.origin.file == req.origin.file && r.origin.span == req.origin.span
                    })) || added.iter().any(|(c, r)| {
                        *c == caller && r.ty == t && r.origin.file == req.origin.file && r.origin.span == req.origin.span
                    });
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
                self.interp_reqs.entry(caller).or_default().push(req);
            }
        }
    }

    /// One E089 for a (call, requirement) pair, anchored on the first site
    /// of the chain — the call, the calls it travelled through, the
    /// segment — that lies in the file being inferred, so the snippet is
    /// real; the other end is named in the message. Reported once.
    fn report_interp_instantiation(&mut self, call: &GenericCall, req: &InterpReq, concrete: &Ty, gap: &StringFormGap) {
        let key = (
            call.site.file.clone().unwrap_or_default(),
            call.site.span.map_or(0, |s| s.line),
            call.site.span.map_or(0, |s| s.col),
            req.origin.file.clone().unwrap_or_default(),
            req.origin.span.map_or(0, |s| s.line),
            req.origin.span.map_or(0, |s| s.col),
        );
        if !self.interp_reported.insert(key) {
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
                "{} interpolates a value of type `{}` at {}; the call to {} at {}{} makes it `{}` — {} has no defined string form — it cannot be interpolated",
                req.origin.label,
                req.ty.display(),
                loc_text(&req.origin),
                call.site.label,
                loc_text(&call.site),
                via_text,
                concrete.display(),
                gap.what,
            ),
            gap.hint.to_string(),
            format!("call to {}", call.site.label),
        ).with_code("E089");
        diag.file = anchor.file.clone();
        if let Some(s) = anchor.span {
            diag.line = Some(s.line);
            diag.col = Some(s.col);
        }
        // The other end, when it is in this file too, underlined as well.
        for other in std::iter::once(&req.origin).chain(std::iter::once(&call.site)) {
            if here(other) && other.span != anchor.span {
                if let Some(s) = other.span {
                    diag = diag.with_secondary(s.line, Some(s.col), if other.span == req.origin.span { "the segment" } else { "the call" });
                }
            }
        }
        self.diagnostics.push(diag);
    }
}
