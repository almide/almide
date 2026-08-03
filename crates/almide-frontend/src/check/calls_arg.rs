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
        bindings: &mut HashMap<Sym, Ty>,
    ) -> bool {
        let ArgSite { fn_name, param_name } = site;
        if let Ty::TypeVar(tv) = param_ty {
            self.unify_call_arg_typevar(site, *tv, arg_ty, structural_bounds, bindings)
        } else {
            self.unify_call_arg_concrete(site, param_ty, arg_ty, bindings)
        }
    }

    // TypeVar-param path of `unify_call_arg`: bind to a structural bound
    // (checked compatible) when one is declared, else fall through to plain
    // unification.
    fn unify_call_arg_typevar(
        &mut self, site: ArgSite<'_>, tv: Sym, arg_ty: &Ty,
        structural_bounds: &HashMap<Sym, Ty>,
        bindings: &mut HashMap<Sym, Ty>,
    ) -> bool {
        let ArgSite { fn_name, param_name } = site;
        if let Some(bound) = structural_bounds.get(&tv) {
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
            crate::types::unify(&Ty::TypeVar(tv), arg_ty, bindings);
            false
        }
    }

    // Concrete (non-TypeVar) param path of `unify_call_arg`: unify, then
    // report a type-mismatch diagnostic with a fix-it hint when the resolved
    // types differ.
    fn unify_call_arg_concrete(
        &mut self, site: ArgSite<'_>, param_ty: &Ty, arg_ty: &Ty,
        bindings: &mut HashMap<Sym, Ty>,
    ) -> bool {
        crate::types::unify(param_ty, arg_ty, bindings);
        let expected = if bindings.is_empty() { param_ty.clone() } else { crate::types::substitute(param_ty, bindings) };
        let expected_resolved = self.env.resolve_named(&expected);
        let arg_resolved = self.env.resolve_named(arg_ty);
        if !types_mismatch(&expected_resolved, &arg_resolved) {
            return false;
        }
        self.emit_call_arg_mismatch(site, &expected, arg_ty, &expected_resolved, &arg_resolved);
        true
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
        self.narrowed_try_hint(tys)
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
        let hint = self.call_arg_mismatch_hint(fn_name, tys, float_sibling);
        let mut diag = super::err(
            format!("argument '{}' expects {} but got {}", param_name, expected.display(), arg_ty.display()),
            hint,
            format!("call to {}()", fn_name)).with_code("E005");
        if let Some(&(line, col)) = self.env.fn_decl_spans.get(&sym(fn_name)) {
            diag = diag.with_secondary(line, Some(col), format!("fn {}() defined here", fn_name));
        }
        // Show fix code: replace argument with conversion expression. Suppressed for the math-Float-sibling case (#740): the fix is to change the function, not to wrap the arg in a truncating cast.
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
    fn substitute_self_in_ty(&self, ty: &Ty, replacement: &Ty) -> Ty {
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
