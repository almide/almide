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

    // Fix-it hint for `emit_call_arg_mismatch`: Float-sibling hint when
    // `float_sibling` is set (an Int-only math builtin given a Float, #740),
    // else a likely-typevar hint (an undeclared capitalized bare name), else
    // a generic conversion hint.
    fn call_arg_mismatch_hint(&self, fn_name: &str, expected: &Ty, arg_ty: &Ty, float_sibling: Option<&'static str>) -> String {
        if let Some(sib) = float_sibling {
            return format!(
                "`{}` is Int-only. For Floats use `{}(x)`, which preserves the Float — \
                 not `float.to_int`, which truncates",
                fn_name, sib
            );
        }
        if let Ty::Named(name, args) = expected {
            let n = name.as_str();
            let is_likely_typevar = args.is_empty()
                && !n.is_empty()
                && n.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
                && !self.env.types.contains_key(name)
                && !self.env.constructors.contains_key(name);
            if is_likely_typevar {
                return format!("'{}' is not a known type. To use it as a type parameter, declare it: fn {}[{}](...)", n, fn_name, n);
            }
        }
        // A plain value handed to the unwrap family means the caller believes
        // it is still wrapped — the classic effect-fn confusion: there,
        // Result-returning calls are auto-propagated, so values arrive
        // already unwrapped and the fallback belongs on the producing call.
        let unwrap_family = matches!(fn_name,
            "option.unwrap_or" | "option.unwrap_or_else" | "option.unwrap"
            | "result.unwrap_or" | "result.unwrap_or_else" | "result.unwrap");
        if unwrap_family
            && (expected.is_option() || expected.is_result())
            && !arg_ty.is_option() && !arg_ty.is_result() && !arg_ty.is_unresolved()
        {
            return format!(
                "nothing to unwrap — the value is already {}. In an effect fn, a \
                 Result-returning call is auto-propagated (`?`), so its value arrives \
                 unwrapped; for a fallback, apply `?? <default>` to the producing call instead",
                arg_ty.display());
        }
        // The reverse of the unwrap-family case: a Result handed to a plain-T
        // param means the caller meant the payload. "Fix the argument type"
        // names no path; the unwrap operators are the path (#1050).
        if arg_ty.is_result() && !expected.is_result() && !expected.is_unresolved() {
            return format!(
                "the argument is a {} — unwrap it first: `!` propagates the error \
                 (effect fn body), `?? fallback` supplies a default, or `match` \
                 handles ok/err",
                arg_ty.display());
        }
        Self::hint_with_conversion("Fix the argument type", expected, arg_ty)
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
        let hint = self.call_arg_mismatch_hint(fn_name, expected, arg_ty, float_sibling);
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
    /// Substitute Ty::TypeVar("Self") with a concrete type in a protocol method return type.
    fn substitute_self_in_ty(&self, ty: &Ty, replacement: &Ty) -> Ty {
        match ty {
            Ty::TypeVar(name) if name == "Self" => replacement.clone(),
            _ => ty.map_children(&|child| self.substitute_self_in_ty(child, replacement)),
        }
    }
}
