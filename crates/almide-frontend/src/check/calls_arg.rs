// ── Per-argument unification and the argument-mismatch diagnostic ─
//
// One call argument at a time: unify it against its parameter, and when the
// resolved types differ, build the E005 with its fix-it hint. Split out of
// `calls.rs`, which resolves the call as a whole; both halves are `impl Checker`
// blocks `include!`d from there, so imports come from that file.

impl Checker {
    /// Unify a single call argument against its parameter type, updating bindings. Reports diagnostics for structural bound violations and type mismatches. Returns true if E005 was emitted (caller should skip redundant E001 constraint).
    fn unify_call_arg(
        &mut self, site: ArgSite<'_>,
        param_ty: &Ty, arg_ty: &Ty,
        structural_bounds: &HashMap<Sym, Ty>,
        apart: &[Sym],
        bindings: &mut HashMap<Sym, Ty>,
    ) -> bool {
        let ArgSite { fn_name: _, param_name: _ } = site;
        if let Ty::TypeVar(tv) = param_ty {
            self.unify_call_arg_typevar(site, *tv, arg_ty, structural_bounds, apart, bindings)
        } else {
            self.unify_call_arg_concrete(site, param_ty, arg_ty, apart, bindings)
        }
    }

    /// The caller's own type parameters that a call to `sig` must keep apart
    /// from the callee's same-named ones (#4218). Inside `gather[A]`, the `A`
    /// of an argument's type is the CALLER's `A`, and `option.is_none[A]`'s
    /// `A` is another variable that only shares its spelling: unifying the two
    /// by name made `A := A?` (and even `A := A`) fail the occurs check, so
    /// the callee's parameter stayed unbound — E025 on `option.is_none(o)` —
    /// where the same body over `T` checked. A call to the fn being checked
    /// itself is left as it was: there the letters ARE the same variables.
    pub(crate) fn caller_letters_apart_from(&self, sig: &crate::types::FnSig) -> Vec<Sym> {
        let Some((key, letters)) = &self.current_fn else { return Vec::new() };
        if letters.is_empty() || sig.generics.is_empty() {
            return Vec::new();
        }
        let is_self = self.env.functions.get(key).is_some_and(|cur| {
            cur.generics == sig.generics && cur.params == sig.params && cur.ret == sig.ret
        });
        if is_self {
            return Vec::new();
        }
        letters.iter().copied().filter(|l| sig.generics.contains(l)).collect()
    }

    /// [`crate::types::unify`] of a callee's signature type against a
    /// caller-side type with the caller's letters in `apart` renamed for the
    /// match, then renamed back in the bindings: the callee's `A` binds to
    /// the caller's `A?` as it binds to any other type.
    pub(crate) fn unify_apart(sig_ty: &Ty, actual: &Ty, apart: &[Sym], bindings: &mut HashMap<Sym, Ty>) -> bool {
        if apart.is_empty() {
            return crate::types::unify(sig_ty, actual, bindings);
        }
        let outer = |g: &Sym| sym(&format!("{}\u{0}outer", g.as_str()));
        let there: HashMap<Sym, Ty> = apart.iter().map(|g| (*g, Ty::TypeVar(outer(g)))).collect();
        let back: HashMap<Sym, Ty> = apart.iter().map(|g| (outer(g), Ty::TypeVar(*g))).collect();
        let renamed = crate::types::substitute(actual, &there);
        // Bound values are caller-side types too: rename them for the
        // consistency check, and back afterwards.
        let mut scratch: HashMap<Sym, Ty> =
            bindings.iter().map(|(k, v)| (*k, crate::types::substitute(v, &there))).collect();
        let ok = crate::types::unify(sig_ty, &renamed, &mut scratch);
        *bindings = scratch.into_iter().map(|(k, v)| (k, crate::types::substitute(&v, &back))).collect();
        ok
    }

    // TypeVar-param path of `unify_call_arg`: bind to a structural bound
    // (checked compatible) when one is declared, else fall through to plain
    // unification.
    fn unify_call_arg_typevar(
        &mut self, site: ArgSite<'_>, tv: Sym, arg_ty: &Ty,
        structural_bounds: &HashMap<Sym, Ty>,
        apart: &[Sym],
        bindings: &mut HashMap<Sym, Ty>,
    ) -> bool {
        let ArgSite { fn_name, param_name } = site;
        if let Some(bound) = structural_bounds.get(&tv) {
            // #4107: a structural bound would read a foreign opaque
            // record's fields through the type variable.
            let arg_now = super::types::resolve_ty(arg_ty, &self.uf);
            if self.foreign_opaque_record(&arg_now).is_some() {
                self.reject_opaque_crossing(&arg_now, true);
                return true;
            }
            let resolved = self.env.resolve_named(arg_ty);
            if bound.compatible(&resolved) || bound.compatible(arg_ty) {
                bindings.insert(tv, arg_ty.clone());
                false
            } else {
                self.emit(super::err(
                    format!("argument '{}' does not satisfy bound {}: got {}", param_name, bound.display(), arg_ty.display()),
                    "The argument must have the required fields",
                    format!("call to {}()", fn_name)));
                true
            }
        } else {
            Self::unify_apart(&Ty::TypeVar(tv), arg_ty, apart, bindings);
            false
        }
    }

    // Concrete (non-TypeVar) param path of `unify_call_arg`: unify, then
    // report a type-mismatch diagnostic with a fix-it hint when the resolved
    // types differ.
    fn unify_call_arg_concrete(
        &mut self, site: ArgSite<'_>, param_ty: &Ty, arg_ty: &Ty,
        apart: &[Sym],
        bindings: &mut HashMap<Sym, Ty>,
    ) -> bool {
        Self::unify_apart(param_ty, arg_ty, apart, bindings);
        let expected = if bindings.is_empty() { param_ty.clone() } else { crate::types::substitute(param_ty, bindings) };
        // #4107: a foreign opaque record never fills another record shape
        // (a structural param would read its fields) and no other record
        // fills it (a forge) — at any depth of the slot.
        if let Some((opaque, reading)) = self.opaque_crossing(&expected, arg_ty) {
            self.reject_opaque_crossing(&opaque, reading);
            return true;
        }
        let expected_resolved = self.env.resolve_named(&expected);
        let arg_resolved = self.env.resolve_named(arg_ty);
        if !types_mismatch(&expected_resolved, &arg_resolved) {
            // #4173: structural compatibility ignores the effect bit, so an
            // `effect (A) -> B` value passed check into a plain `(A) -> B`
            // slot and failed later in rustc (E0271 / E0308) or walled the
            // wasm leg. The runtime shapes differ: the value returns the
            // effect carrier.
            if self.effect_value_in_pure_slot(&expected_resolved, &arg_resolved) {
                self.emit_effect_value_in_pure_slot(site, &expected, arg_ty);
                return true;
            }
            return false;
        }
        // #1055: an `effect (A) -> B` slot accepts BOTH a pure `(A) -> B`
        // value (running no effects satisfies the permission) and a FALLIBLE
        // `(A) -> Result[B, String]` one — the effect carrier IS
        // `Result[_, String]`, so a `!`-using handler already has the slot's
        // runtime shape. The effect-agnostic structural check above only
        // covers the pure spelling; this arm admits the carrier spelling.
        if self.effect_slot_accepts(&expected_resolved, &arg_resolved) {
            return false;
        }
        let arg_shown = self.with_slot_defaults(&super::types::resolve_ty(arg_ty, &self.uf));
        let arg_resolved = self.with_slot_defaults(&arg_resolved);
        self.emit_call_arg_mismatch(site, &expected, &arg_shown, &expected_resolved, &arg_resolved);
        true
    }

    /// #4173: does an `effect (A) -> B` value fill a plain `(A) -> B` slot,
    /// directly or as the element of the same container (`List`, `Option`, a
    /// tuple)? Only the plain-spelled effect type is meant: a value typed with
    /// the carrier (`(A) -> Result[B, String]`, a named effect fn) already
    /// mismatches on its return, and the fallible-HOF rewrites own that shape.
    fn effect_value_in_pure_slot(&self, expected: &Ty, actual: &Ty) -> bool {
        let inner = |e: &Ty, a: &Ty| self.effect_value_in_pure_slot(&self.env.resolve_named(e), &self.env.resolve_named(a));
        match (expected, actual) {
            (Ty::Fn { is_effect: false, .. }, Ty::Fn { is_effect: true, ret, .. }) => {
                !matches!(self.env.resolve_named(ret), Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _))
            }
            (Ty::Applied(ei, ea), Ty::Applied(ai, aa)) if ei == ai && ea.len() == aa.len() => {
                ea.iter().zip(aa.iter()).any(|(e, a)| inner(e, a))
            }
            (Ty::Tuple(ea), Ty::Tuple(aa)) if ea.len() == aa.len() => ea.iter().zip(aa.iter()).any(|(e, a)| inner(e, a)),
            _ => false,
        }
    }

    /// The #4173 diagnostic: E005, naming both ways out.
    fn emit_effect_value_in_pure_slot(&mut self, site: ArgSite<'_>, expected: &Ty, arg_ty: &Ty) {
        let ArgSite { fn_name, param_name } = site;
        let mut diag = super::err(
            format!(
                "argument '{}' expects {} but got {}: an effect fn value cannot fill a plain fn slot",
                param_name,
                expected.display(),
                arg_ty.display()
            ),
            format!(
                "The value runs effects and returns the effect carrier, so a plain `{}` slot cannot call it. \
                 Either make the slot an effect slot — `{}: effect (A) -> B` — in an `effect fn`, or pass a \
                 lambda that handles the error: `(x) => f(x) ?? fallback`.",
                expected.display(),
                param_name
            ),
            format!("call to {}()", fn_name),
        )
        .with_code("E005");
        if let Some(&(line, col)) = self.env.fn_decl_spans.get(&sym(fn_name)) {
            diag = diag.with_secondary(line, Some(col), format!("fn {}() defined here", fn_name));
        }
        self.emit(diag);
    }

    /// The #1055 effect-slot acceptance: expected is `Fn { is_effect: true }`
    /// and the argument is a fn whose params match and whose return is the
    /// slot's return WRAPPED in the effect carrier `Result[B, String]`.
    /// (The unwrapped pure spelling already passed the structural check.)
    ///
    /// The acceptance holds INSIDE a container too (#2588): `[get_user, me]`
    /// is a `List` of named effect fn values — each typed as its carrier — and
    /// it fills a `List[effect (A) -> B]` slot, exactly as one of them fills an
    /// `effect (A) -> B` slot. Only the carrier spelling is admitted there: a
    /// PURE fn nested in a container has a different runtime shape, and no
    /// adapter reaches inside a list to wrap it.
    fn effect_slot_accepts(&self, expected: &Ty, actual: &Ty) -> bool {
        let fits = |e: &Ty, a: &Ty| {
            let (e, a) = (self.env.resolve_named(e), self.env.resolve_named(a));
            !types_mismatch(&e, &a) || self.effect_slot_accepts(&e, &a)
        };
        match (expected, actual) {
            (Ty::Applied(ei, ea), Ty::Applied(ai, aa)) if ei == ai && ea.len() == aa.len() => {
                return ea.iter().zip(aa.iter()).all(|(e, a)| fits(e, a));
            }
            (Ty::Tuple(ea), Ty::Tuple(aa)) if ea.len() == aa.len() => {
                return ea.iter().zip(aa.iter()).all(|(e, a)| fits(e, a));
            }
            // A PURE fn whose params/return are effect fn types — a middleware,
            // `(HttpHandler) -> HttpHandler` — accepts the lambda spelled with
            // the carrier in those positions.
            (Ty::Fn { params: ep, ret: er, is_effect: false }, Ty::Fn { params: ap, ret: ar, is_effect: false })
                if ep.len() == ap.len() =>
            {
                return ep.iter().zip(ap.iter()).all(|(e, a)| fits(e, a)) && fits(er, ar);
            }
            _ => {}
        }
        let Ty::Fn { params: ep, ret: er, is_effect: true } = expected else {
            return false;
        };
        let Ty::Fn { params: ap, ret: ar, .. } = actual else { return false };
        if ep.len() != ap.len() {
            return false;
        }
        let params_ok = ep.iter().zip(ap.iter()).all(|(e, a)| {
            !types_mismatch(&self.env.resolve_named(e), &self.env.resolve_named(a))
        });
        if !params_ok {
            return false;
        }
        match self.env.resolve_named(ar) {
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, args)
                if args.len() == 2 =>
            {
                !types_mismatch(&self.env.resolve_named(er), &self.env.resolve_named(&args[0]))
                    && matches!(self.env.resolve_named(&args[1]), Ty::String)
            }
            _ => false,
        }
    }

    // Fix-it hint for `emit_call_arg_mismatch`: the first Some in a chain of
    // focused hint derivations wins — Float-sibling (#740), `?`-narrowing
    // (#1071), likely-typevar, unwrap-family confusion, wrapped-into-plain
    // (#1050 + its Option sibling) — else the generic conversion hint.
    fn call_arg_mismatch_hint(&self, fn_name: &str, tys: MismatchTys<'_>, float_sibling: Option<&'static str>) -> String {
        if let Some(sib) = float_sibling {
            return format!(
                "`{}` is Int-only. For Floats use `{}(x)`, which preserves the Float — \
                 not `float.to_int`, which truncates",
                fn_name, sib
            );
        }
        regex_pattern_hint(fn_name, tys)
            .or_else(|| self.narrowed_try_hint(tys))
            .or_else(|| self.likely_typevar_hint(fn_name, tys))
            .or_else(|| unwrap_family_hint(fn_name, tys))
            .or_else(|| wrapped_into_plain_hint(tys))
            .unwrap_or_else(|| Self::hint_with_conversion("Fix the argument type", tys.expected, tys.arg))
    }

    /// #1071: an Option[T] handed to a T param where the argument expression
    /// is a `?` postfix. The writer wanted error propagation; `?` IS the
    /// Result→Option conversion (one meaning everywhere), so name that rule —
    /// the templates after this would guess from the types alone and teach a
    /// false move. `!` is the propagating unwrap, and since #1067 it works in
    /// a pure fn too when the fn's declared return is Result/Option (C-211) —
    /// the hint names the condition instead of branching on it.
    fn narrowed_try_hint(&self, tys: MismatchTys<'_>) -> Option<String> {
        let inner = tys.arg_resolved.option_inner()?;
        if types_mismatch(tys.expected_resolved, &inner) || !self.arg_is_try_postfix() {
            return None;
        }
        Some("`?` converts a Result to an Option — it is not error propagation. \
              `!` is the propagating unwrap (valid in an effect fn body, a test \
              block, or a fn returning Result/Option): replace the `?` with `!`; \
              or `?? fallback` supplies a default, `match` handles ok/err"
            .to_string())
    }

    /// An undeclared capitalized bare name in the expected type — likely a
    /// missing type parameter on the writer's OWN fn. #1071: never taught for
    /// a callee the writer does not own (a stdlib signature cannot be
    /// re-declared), nor for a name that IS a known builtin type (`Value`,
    /// the runtime-backed stdlib nominals) — the premise "'X' is not a known
    /// type" would be false.
    fn likely_typevar_hint(&self, fn_name: &str, tys: MismatchTys<'_>) -> Option<String> {
        let Ty::Named(name, args) = tys.expected else { return None };
        let n = name.as_str();
        let callee_is_stdlib = fn_name
            .split_once('.')
            .map_or(false, |(m, f)| crate::stdlib::lookup_sig(m, f).is_some());
        let is_known_builtin = n == "Value"
            || almide_lang::stdlib_info::runtime_backed_type_owner(n).is_some();
        let is_likely_typevar = args.is_empty()
            && !n.is_empty()
            && n.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
            && !self.env.types.contains_key(name)
            && !self.env.constructors.contains_key(name)
            && !callee_is_stdlib
            && !is_known_builtin;
        is_likely_typevar.then(|| {
            format!("'{}' is not a known type. To use it as a type parameter, declare it: fn {}[{}](...)", n, fn_name, n)
        })
    }

    // Emits the E005 argument-type-mismatch diagnostic for
    // `unify_call_arg_concrete`: derives a fix-it hint (Float-sibling /
    // likely-typevar / generic conversion hint) and, where possible, a
    // `// Try:` fix code snippet.
    fn emit_call_arg_mismatch(
        &mut self, site: ArgSite<'_>, expected: &Ty, arg_ty: &Ty,
        expected_resolved: &Ty, arg_resolved: &Ty,
    ) {
        let ArgSite { fn_name, param_name } = site;
        // #740: an Int-only math builtin given a Float — point at the Float-preserving sibling, not the truncating `float.to_int`.
        let float_sibling = if matches!(arg_resolved, Ty::Float)
            && matches!(expected_resolved, Ty::Int)
        {
            Self::math_float_sibling(fn_name)
        } else {
            None
        };
        let tys = MismatchTys { expected, arg: arg_ty, expected_resolved, arg_resolved };
        // #2097: the value came straight from a stdlib call whose module holds
        // a same-stem sibling returning exactly the expected type — name it.
        let stdlib_sibling = if float_sibling.is_none() { self.stdlib_sibling_rewrite(tys) } else { None };
        let hint = match &stdlib_sibling {
            Some((hint, _)) => hint.clone(),
            None => self.call_arg_mismatch_hint(fn_name, tys, float_sibling),
        };
        // #1108 Phase 2b: a FALLIBLE callback into a plain fn-typed slot —
        // the one shape the bit does not yet flow through (2b-iii). Name the
        // two working spellings instead of the generic "fix the type".
        let hint = match (expected_resolved, arg_resolved) {
            (Ty::Fn { ret: er, .. }, Ty::Fn { ret: ar, .. })
                if !er.is_result() && ar.is_result() =>
            {
                // #3163: the accepted cells are NAMED from the matrix the
                // rewrite reads, so the hint cannot claim a cell it refuses.
                format!(
                    "The callback is FALLIBLE (`(A) -> Result[..]`) but the slot is total. \
                     Either declare the slot fallible — `{}: (A) -> B!` — and consume the \
                     Result in the HOF body, or handle the error inside the lambda \
                     (`?? fallback` / match). These stdlib HOFs accept a fallible callback \
                     natively (first-err short-circuit): {}; plain-slot transparency for user \
                     HOFs is #1108 Phase 2b-iii.",
                    param_name,
                    almide_lang::fallible_hofs::summary()
                )
            }
            _ => hint,
        };
        let mut diag = super::err(
            format!("argument '{}' expects {} but got {}", param_name, expected.display(), arg_ty.display()),
            hint,
            format!("call to {}()", fn_name)).with_code("E005");
        if let Some(&(line, col)) = self.env.fn_decl_spans.get(&sym(fn_name)) {
            diag = diag.with_secondary(line, Some(col), format!("fn {}() defined here", fn_name));
        }
        if let Some((_, fixed)) = &stdlib_sibling {
            diag = diag.with_try(format!("// Try:\n{}", fixed));
        } else // Show fix code: replace argument with conversion expression. Suppressed for the math-Float-sibling case (#740): the fix is to change the function, not to wrap the arg in a truncating cast.
        if float_sibling.is_none() {
            if let Some(span) = self.current_span {
                if let Some((_, template)) = Self::conversion_template(expected, arg_ty) {
                    if let Some(src) = self.source_slice(span) {
                        let fixed = template.replace("{}", &src);
                        diag = diag.with_try(format!("// Try:\n{}", fixed));
                    }
                }
            }
        }
        self.emit(diag);
    }
    /// #2097: the mismatched argument is a DIRECT stdlib call (optionally
    /// `!`-unwrapped), and that call's module has a same-stem sibling whose
    /// return type — seen through the same unwrap — is the expected type.
    /// `fs.read_bytes` returns `List[Int]`, `fs.read_bytes_raw` returns
    /// `Bytes`, four lines apart; nothing in the first name suggests the
    /// second exists, and "Fix the argument type" was the only hint.
    ///
    /// Returns the hint and the argument text with the callee renamed. A
    /// lookup, not a guess: the sibling must share the stem (`read_bytes` /
    /// `read_bytes_raw`), take the same number of arguments, and return the
    /// expected type exactly. Anything else keeps today's wording.
    fn stdlib_sibling_rewrite(&self, tys: MismatchTys<'_>) -> Option<(String, String)> {
        let is_ident = |s: &str| {
            !s.is_empty()
                && s.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        };
        // A postfix node's span is the operator alone; `!` means the payload
        // was handed on. `?` is `narrowed_try_hint`'s case — leave it.
        let through_postfix = |span: crate::ast::Span| -> Option<(String, bool)> {
            let text = self.source_slice(span)?;
            match text.trim() {
                "!" => {
                    let inner = *self.postfix_inner_spans.get(&(span.line, span.col, span.end_col))?;
                    Some((self.source_slice(inner)?, true))
                }
                "?" => None,
                t => Some((t.to_string(), false)),
            }
        };
        let arg_span = self.current_span?;
        let (arg_text, arg_unwrapped) = through_postfix(arg_span)?;
        // A bare name: follow it to the `let` that bound it, and rewrite
        // there. The origin is scoped with the binding (`TypeEnv::let_origin`):
        // a parameter, a pattern, or another function's `let` of the same
        // name answers `None`, never a stale span.
        let (call_text, unwrapped, bound_name) = if is_ident(arg_text.trim()) {
            let name = arg_text.trim().to_string();
            let span = self.env.let_origin(&name)?;
            let (text, unwrapped) = through_postfix(span)?;
            (text, unwrapped, Some(name))
        } else {
            (arg_text, arg_unwrapped, None)
        };
        let call = call_text.trim();
        let (head, rest) = call.split_once('(')?;
        if !rest.ends_with(')') {
            return None;
        }
        let (module, func) = head.split_once('.')?;
        if !is_ident(module) || !is_ident(func) {
            return None;
        }
        let origin = crate::stdlib::lookup_sig(module, func)?;
        // Seen through the argument's own unwrap: `call()!` hands the payload on.
        let through = |ret: &Ty| -> Option<Ty> {
            if unwrapped { ret.result_ok_ty().or_else(|| ret.option_inner()) } else { Some(ret.clone()) }
        };
        // The origin must actually produce what the checker saw — otherwise
        // the text under the caret is not the call we think it is.
        let origin_view = self.env.resolve_named(&through(&origin.ret)?);
        if types_mismatch(&origin_view, tys.arg_resolved) {
            return None;
        }
        let same_stem = |a: &str, b: &str| {
            a.starts_with(b) && a[b.len()..].starts_with('_')
                || b.starts_with(a) && b[a.len()..].starts_with('_')
        };
        let mut siblings = crate::stdlib::module_functions_current(module);
        siblings.sort_unstable();
        for name in siblings {
            if name == func || !same_stem(name, func) {
                continue;
            }
            let Some(sig) = crate::stdlib::lookup_sig(module, name) else { continue };
            if sig.params.len() != origin.params.len() {
                continue;
            }
            let Some(view) = through(&sig.ret) else { continue };
            if types_mismatch(tys.expected_resolved, &self.env.resolve_named(&view)) {
                continue;
            }
            let renamed = format!("{module}.{name}({rest}{}", if unwrapped { "!" } else { "" });
            let (hint, fixed) = match &bound_name {
                Some(b) => (
                    format!(
                        "`{b}` came from `{module}.{func}`, which returns {} — `{module}.{name}` returns {}, which is what this argument expects",
                        origin.ret.display(), sig.ret.display()
                    ),
                    format!("let {b} = {renamed}"),
                ),
                None => (
                    format!(
                        "`{module}.{func}` returns {} — `{module}.{name}` returns {}, which is what this argument expects",
                        origin.ret.display(), sig.ret.display()
                    ),
                    renamed,
                ),
            };
            return Some((hint, fixed));
        }
        None
    }

    // Whether the argument expression currently under the E005 caret is a
    // `?` postfix. The checker unifies on types, not AST nodes, so the arg
    // expression itself is out of reach here — but `current_span` covers
    // exactly the argument text, and a trailing `?` in that slice can only
    // be the Try postfix (`??` ends in its right operand, string literals in
    // a quote).
    fn arg_is_try_postfix(&self) -> bool {
        self.current_span
            .and_then(|sp| self.source_slice(sp))
            .map_or(false, |src| {
                let t = src.trim_end();
                t.ends_with('?') && !t.ends_with("??")
            })
    }
    /// Substitute Ty::TypeVar("Self") with a concrete type in a protocol method return type.
    pub(super) fn substitute_self_in_ty(&self, ty: &Ty, replacement: &Ty) -> Ty {
        match ty {
            Ty::TypeVar(name) if name == "Self" => replacement.clone(),
            _ => ty.map_children(&|child| self.substitute_self_in_ty(child, replacement)),
        }
    }
}

/// The two views of an E005 mismatch — the declared types and their
/// env-resolved forms, for both sides. The four travel together through the
/// hint chain (a hint that mixes a resolved expected with an unresolved arg
/// compares the wrong pair).
#[derive(Copy, Clone)]
struct MismatchTys<'a> {
    expected: &'a Ty,
    arg: &'a Ty,
    expected_resolved: &'a Ty,
    arg_resolved: &'a Ty,
}

/// A plain value handed to the unwrap family means the caller believes it is
/// still wrapped — the classic effect-fn confusion: there, Result-returning
/// calls are auto-propagated, so values arrive already unwrapped and the
/// fallback belongs on the producing call.
fn unwrap_family_hint(fn_name: &str, tys: MismatchTys<'_>) -> Option<String> {
    let unwrap_family = matches!(fn_name,
        "option.unwrap_or" | "option.unwrap_or_else" | "option.unwrap"
        | "result.unwrap_or" | "result.unwrap_or_else" | "result.unwrap");
    if unwrap_family
        && (tys.expected.is_option() || tys.expected.is_result())
        && !tys.arg.is_option() && !tys.arg.is_result() && !tys.arg.is_unresolved()
    {
        return Some(format!(
            "nothing to unwrap — the value is already {}. In an effect fn, a \
             Result-returning call is auto-propagated (`?`), so its value arrives \
             unwrapped; for a fallback, apply `?? <default>` to the producing call instead",
            tys.arg.display()));
    }
    None
}

/// The reverse of the unwrap-family case: a Result (or Option, its #1071
/// sibling) handed to a plain-T param means the caller meant the payload.
/// "Fix the argument type" names no path; the unwrap operators are the path
/// (#1050).
/// R-regex-02 (#3942): a regex operation takes a `Regex`, made once by
/// `regex.new`; a String in that place is a pattern written the old way, or
/// the subject piped into the pattern's place.
fn regex_pattern_hint(fn_name: &str, tys: MismatchTys<'_>) -> Option<String> {
    let expected = tys.expected.display();
    (fn_name.starts_with("regex.") && matches!(tys.arg_resolved, Ty::String) && expected.contains("Regex"))
        .then(|| "a regex operation takes a Regex, made once from the pattern: `regex.find(regex.new(\"[0-9]+\"), s)`; a pattern built at run time goes through `regex.compile`, and the subject goes second, never piped into the pattern's place".to_string())
}

fn wrapped_into_plain_hint(tys: MismatchTys<'_>) -> Option<String> {
    if tys.arg.is_result() && !tys.expected.is_result() && !tys.expected.is_unresolved() {
        return Some(format!(
            "the argument is a {} — unwrap it first: `!` propagates the error \
             (effect fn body), `?? fallback` supplies a default, or `match` \
             handles ok/err",
            tys.arg.display()));
    }
    if tys.arg_resolved.is_option() && !tys.expected_resolved.is_option() && !tys.expected_resolved.is_unresolved() {
        return Some(format!(
            "the argument is an {} — unwrap it first: `?? fallback` supplies \
             a default, or `match` handles the none case",
            tys.arg.display()));
    }
    None
}
