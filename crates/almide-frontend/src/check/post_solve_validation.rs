
/// Fill each undecidable slot (an unbound `?` inference var or `Unknown`) in
/// `ty` with an example concrete type, so the E025 hint shows an annotation
/// whose SHAPE matches the reported binding. Result's err slot fills as
/// `String` (the stdlib's uniform error channel); every other hole fills as
/// `Int`. Both are examples, marked `e.g.` at the emit site.
fn fill_example_ty(ty: &Ty) -> Ty {
    fn go(ty: &Ty, in_result_err: bool) -> Ty {
        let hole = || if in_result_err { Ty::String } else { Ty::Int };
        match ty {
            Ty::Unknown => hole(),
            Ty::TypeVar(n) if n.as_str().starts_with('?') => hole(),
            Ty::Applied(ctor, args) => {
                let is_result = *ctor == almide_lang::types::TypeConstructorId::Result;
                Ty::Applied(ctor.clone(), args.iter().enumerate().map(|(i, a)|
                    go(a, is_result && i == 1)).collect())
            }
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|t| go(t, false)).collect()),
            Ty::Fn { is_effect: _, params, ret } => Ty::Fn { is_effect: false, 
                params: params.iter().map(|t| go(t, false)).collect(),
                ret: Box::new(go(ret, false)),
            },
            Ty::Named(n, args) => Ty::Named(*n, args.iter().map(|t| go(t, false)).collect()),
            Ty::Union(ts) => Ty::Union(ts.iter().map(|t| go(t, false)).collect()),
            Ty::Record { fields } =>
                Ty::Record { fields: fields.iter().map(|(n, t)| (*n, go(t, false))).collect() },
            _ => ty.clone(),
        }
    }
    go(ty, false)
}

/// Infer types for default value expressions in type declarations.
/// Prevents ICE "missing type for expr" during lowering.
fn infer_default_exprs(checker: &mut Checker, ty: &mut ast::TypeExpr) {
    match ty {
        ast::TypeExpr::Variant { cases, .. } => {
            for case in cases {
                if let ast::VariantCase::Record { fields, .. } = case {
                    infer_field_defaults(checker, fields);
                }
            }
        }
        // A plain record's defaults are checked too (#2518): a record-literal
        // default (`s: Sampling = Sampling {}`) left unchecked reached the
        // native build with a missing field (E0063) and no qualified type.
        ast::TypeExpr::Record { fields } => infer_field_defaults(checker, fields),
        _ => {}
    }
}

fn infer_field_defaults(checker: &mut Checker, fields: &mut [ast::FieldType]) {
    for field in fields {
        let declared = checker.resolve_type_expr(&field.ty);
        if let Some(ref mut default_expr) = field.default {
            let val_ty = checker.infer_expr(default_expr);
            // The field's declared type is the source of truth for
            // its default value — flow it in so an empty default
            // (`items: List[Shape] = []`) pins its element to `Shape`
            // instead of staying undecidable (E018).
            checker.constrain(declared, val_ty, format!("default for field {}", field.name));
        }
    }
}

impl Checker {

    pub(crate) fn check_match_exhaustiveness(&mut self, subject_ty: &Ty, arms: &[ast::MatchArm]) {
        let missing = exhaustiveness::check_exhaustiveness(subject_ty, arms, &self.env);
        if !missing.is_empty() {
            let list = missing
                .iter()
                .map(|m| m.pattern.clone())
                .collect::<Vec<_>>()
                .join(", ");
            let resolved = self.env.resolve_named(subject_ty);
            let has_guarded_arms = arms.iter().any(|a| a.guard.is_some());
            let hint_base = if missing.len() == 1 && missing[0].pattern == "_" {
                let ty_name = match &resolved {
                    Ty::Int => "Int",
                    Ty::Float => "Float",
                    Ty::String => "String",
                    _ => "this type",
                };
                format!("match on {} requires a catch-all '_' pattern", ty_name)
            } else {
                // Paste-ready arms: indent + join with newlines so the LLM
                // (or user) can copy the block straight into the source.
                // `_ => todo()` is appended as a fallback for incremental
                // compilation, mirroring Rust's `unimplemented!()` idiom.
                let arms_block = missing
                    .iter()
                    .map(|m| format!("  {}", m.arm_template))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "add arms for {}:\n{}\nOr use `_ => todo()` to compile incrementally.",
                    list, arms_block
                )
            };
            // §4: when guarded arms are present, exhaustiveness skips
            // them — the user may read "missing X" and assume their
            // `X if cond => ...` arm already covered X. Add a note
            // explaining the rule so the fix is to either drop the
            // guard or add `_ => ...`.
            let hint = if has_guarded_arms {
                format!(
                    "{}\n\
                     Note: guarded arms (`pat if cond =>`) do NOT count \
                     toward exhaustiveness — the guard can fail at \
                     runtime. Add an unguarded arm covering the pattern(s) \
                     above (often `_ => ...`).",
                    hint_base
                )
            } else {
                hint_base
            };
            self.emit(Diagnostic::error(
                format!("non-exhaustive match: missing {}", list),
                hint,
                "match",
            ).with_code("E010"));
        }

        // §2: unreachable arms are a hard error. A pattern already
        // covered by earlier arms is almost always a generation mistake
        // — the LLM mis-encoded an earlier condition. Reporting at
        // error level (not warning) surfaces the problem on the first
        // CI run rather than being lost in stdout noise.
        // Code: E014 (E011 is the pre-existing "mutable var mutated
        // inside closure" diagnostic in `infer.rs`).
        let dead = exhaustiveness::find_unreachable_arms(subject_ty, arms, &self.env);
        for idx in dead {
            let arm = &arms[idx];
            let mut diag = Diagnostic::error(
                "unreachable match arm",
                "This arm's pattern is already covered by an earlier arm. \
                 Either delete it, or tighten the earlier arm so this one is reachable.",
                "match",
            ).with_code("E014");
            // Patterns don't carry spans in the AST. The arm body's
            // span is adjacent to the pattern (`pattern => body`), so
            // the diagnostic lands on the right line — close enough
            // for LLM / human navigation.
            if let Some(span) = arm.body.span {
                diag.file = self.source_file.clone();
                diag.line = Some(span.line);
                diag.col = Some(span.col);
                if span.end_col > span.col {
                    diag.end_col = Some(span.end_col);
                }
            }
            self.emit(diag);
        }
    }

    // ── Type resolution ──

    pub fn resolve_type_expr(&self, te: &ast::TypeExpr) -> Ty {
        crate::canonicalize::resolve::resolve_type_expr_in(te, Some(&self.env.types), self.current_module_prefix.as_deref())
    }

    pub(crate) fn resolve_field_type(&mut self, ty: &Ty, field: &str) -> Ty {
        let resolved = self.env.resolve_named(ty);
        match &resolved {
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().find(|(n, _)| n == field).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown),
            Ty::TypeVar(tv) => self.resolve_field_on_typevar(ty, tv, field),
            _ => {
                // The type might be an inference variable that hasn't been
                // resolved yet (e.g. lambda param `t` whose type `?x` will
                // be unified with a record type after constraint solving).
                // Defer the field access: park a fresh var and unify it
                // once the object type is concrete.
                if matches!(&resolved, Ty::TypeVar(n) if n.as_str().starts_with('?')) {
                    let result = self.fresh_var();
                    self.deferred_field_accesses.push((
                        ty.clone(),
                        almide_base::intern::sym(field),
                        result.clone(),
                        self.current_span,
                    ));
                    return result;
                }
                Ty::Unknown
            },
        }
    }

    /// Resolve `field` on a type that is still a type VARIABLE.
    ///
    /// Four outcomes, in order: an existing structural bound answers directly;
    /// exactly one registered record has the field, so the variable can be
    /// unified with it; several records share the field AND its type, so the type
    /// is known but the object is not, and the access is deferred; or the
    /// candidates genuinely disagree (`Cubic.a: Vec2` vs `Color.a: Float`), where
    /// an inference var defers and anything else recovers as `Unknown`.
    fn resolve_field_on_typevar(
        &mut self,
        ty: &Ty,
        tv: &almide_base::intern::Sym,
        field: &str,
    ) -> Ty {
        // First check existing structural bounds
        if let Some(bound) = self.env.structural_bounds.get(tv).cloned() {
            let result = self.resolve_field_type(&bound, field);
            if !matches!(result, Ty::Unknown) {
                return result;
            }
        }
        // Search env.types for record types with this field.
        // Only unify if exactly one candidate exists (unambiguous).
        let field_sym = almide_base::intern::sym(field);
        let mut candidates: Vec<(almide_base::intern::Sym, Ty)> = Vec::new();
        for (_name, reg_ty) in &self.env.types {
            match reg_ty {
                Ty::Record { fields } | Ty::OpenRecord { fields } => {
                    if let Some((_, fty)) = fields.iter().find(|(n, _)| *n == field_sym) {
                        candidates.push((*_name, fty.clone()));
                    }
                }
                _ => {}
            }
        }
        // Deduplicate: prefixed (`mod.Todo`) and unprefixed (`Todo`)
        // aliases resolve to the same record definition; keep one.
        // Sort first so dedup_by can catch non-adjacent duplicates.
        candidates.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        candidates.dedup_by(|a, b| {
            self.env.types.get(&a.0) == self.env.types.get(&b.0)
        });
        // A slice pattern carries the "exactly one candidate" check and
        // the binding together, so the two cannot drift apart.
        if let [(type_name, field_ty)] = candidates.as_slice() {
            let (type_name, field_ty) = (*type_name, field_ty.clone());
            let named = Ty::Named(type_name, vec![]);
            self.unify_infer(ty, &named);
            field_ty
        } else if !candidates.is_empty() && candidates.iter().all(|(_, t)| *t == candidates[0].1) {
            // Multiple types share the same field name+type: safe to return
            // the type but don't unify the object (ambiguous which type it is).
            // Deferred field access will resolve once the parent chain is concrete.
            let field_ty = candidates[0].1.clone();
            let result = self.fresh_var();
            self.deferred_field_accesses.push((
                ty.clone(),
                almide_base::intern::sym(field),
                result.clone(),
                self.current_span,
            ));
            self.unify_infer(&result, &field_ty);
            field_ty
        } else {
            // Ambiguous candidates (e.g. Cubic.a: Vec2, Color.a: Float).
            // If this is an inference var, defer — once the type resolves
            // the field lookup will succeed unambiguously.
            if tv.as_str().starts_with('?') {
                let result = self.fresh_var();
                self.deferred_field_accesses.push((
                    ty.clone(),
                    almide_base::intern::sym(field),
                    result.clone(),
                    self.current_span,
                ));
                return result;
            }
            Ty::Unknown
        }
    }
}

include!("post_solve_checks.rs");

/// Resolve inferred TypeVars in the type map after constraint solving.
fn resolve_type_map(type_map: &mut crate::types::TypeMap, uf: &UnionFind) {
    for ty in type_map.values_mut() {
        *ty = resolve_ty(ty, uf);
    }
}

/// If `expr` is a block whose value comes from a trailing `let` binding
/// (i.e. no tail expression, last statement is `Stmt::Let { name, .. }`),
/// return that binding name. This is the top dojo E001 anti-pattern:
/// `fn f() -> Int = { let x = ...  }` — the fn returns Unit because a
/// bare `let` evaluates to Unit, not to the bound value.
fn trailing_let_name(expr: &ast::Expr) -> Option<String> {
    let ast::ExprKind::Block { stmts, expr: tail } = &expr.kind else { return None };
    if tail.is_some() { return None; }
    match stmts.last()? {
        ast::Stmt::Let { name, .. } | ast::Stmt::Var { name, .. } => Some(name.to_string()),
        _ => None,
    }
}

/// Structural signature compare for `reimpl-lint`. Param names are
/// ignored (stdlib uses `xs` / `n` etc., user may use anything);
/// types are compared element-wise with TypeVar treated as a
/// wildcard (the stdlib side may be generic, the user side usually
/// monomorphic — still counts as a reimplementation).
fn sigs_match_structurally(
    stdlib_params: &[(almide_base::intern::Sym, Ty)],
    stdlib_ret: &Ty,
    user_params: &[Ty],
    user_ret: &Ty,
) -> bool {
    if stdlib_params.len() != user_params.len() { return false; }
    for ((_, sty), uty) in stdlib_params.iter().zip(user_params.iter()) {
        if !ty_reimpl_eq(sty, uty) { return false; }
    }
    ty_reimpl_eq(stdlib_ret, user_ret)
}

/// Reimpl-lint type equality: structural on `Applied`, exact on
/// primitives, `TypeVar` on the stdlib side matches any Ty on the
/// user side. Asymmetric — user's `TypeVar` doesn't match stdlib
/// concrete (we don't want a generic user fn to claim reimpl of a
/// concrete stdlib one).
fn ty_reimpl_eq(stdlib_ty: &Ty, user_ty: &Ty) -> bool {
    match (stdlib_ty, user_ty) {
        (Ty::TypeVar(_), _) => true,
        (Ty::Applied(sid, sargs), Ty::Applied(uid, uargs)) => {
            if sid != uid || sargs.len() != uargs.len() { return false; }
            sargs.iter().zip(uargs.iter()).all(|(s, u)| ty_reimpl_eq(s, u))
        }
        (Ty::Tuple(stys), Ty::Tuple(utys)) => {
            if stys.len() != utys.len() { return false; }
            stys.iter().zip(utys.iter()).all(|(s, u)| ty_reimpl_eq(s, u))
        }
        (Ty::Fn { is_effect: _, params: sp, ret: sr }, Ty::Fn { is_effect: _, params: up, ret: ur }) => {
            if sp.len() != up.len() { return false; }
            sp.iter().zip(up.iter()).all(|(s, u)| ty_reimpl_eq(s, u))
                && ty_reimpl_eq(sr, ur)
        }
        (Ty::Named(sn, sa), Ty::Named(un, ua)) => {
            sn == un
                && sa.len() == ua.len()
                && sa.iter().zip(ua.iter()).all(|(s, u)| ty_reimpl_eq(s, u))
        }
        _ => stdlib_ty == user_ty,
    }
}

/// What the E018 diagnostic says about one kind of empty collection.
///
/// `what` names the construct, `fix` is prose describing the annotation that
/// resolves it, and `try_fix` is a concrete parseable line the user can paste.
/// The three used to be two separate `match`es over the same enum, so a new kind
/// could get a `fix` and no `try_fix`; one table makes that impossible.
struct EmptyCollectionAdvice {
    what: &'static str,
    fix: &'static str,
    try_fix: &'static str,
}

/// The let-binding form is the primary fix because it always works. The inline
/// `[]: List[Int]` call-argument form is offered only for the bare list literal,
/// where it is verified to parse and infer.
fn empty_collection_advice(kind: EmptyCollectionKind) -> EmptyCollectionAdvice {
    let (what, fix, try_fix) = match kind {
        EmptyCollectionKind::ListLiteral => (
            "empty list `[]`",
            "bind it with an explicit element type, e.g. `let xs: List[Int] = []`, \
             or annotate the literal inline: `list.len([]: List[Int])`",
            "let xs: List[Int] = []",
        ),
        EmptyCollectionKind::MapLiteral => (
            "empty map `[:]`",
            "bind it with explicit key/value types, e.g. `let m: Map[String, Int] = [:]`",
            "let m: Map[String, Int] = [:]",
        ),
        EmptyCollectionKind::SetNew => (
            "`set.new()`",
            "bind it with an explicit element type, e.g. `let s: Set[Int] = set.new()`",
            "let s: Set[Int] = set.new()",
        ),
        EmptyCollectionKind::ListWithCapacity => (
            "`list.with_capacity(n)`",
            "bind it with an explicit element type, e.g. `let xs: List[Int] = list.with_capacity(n)`",
            "let xs: List[Int] = list.with_capacity(n)",
        ),
        EmptyCollectionKind::ForInEmpty => (
            "the empty list iterated by `for`",
            "bind the list to an explicitly-typed variable first, e.g. \
             `let xs: List[Int] = []` then `for _ in xs { ... }`",
            "let xs: List[Int] = []\nfor _ in xs { ... }",
        ),
    };
    EmptyCollectionAdvice { what, fix, try_fix }
}

