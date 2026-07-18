
/// Infer types for default value expressions in type declarations.
/// Prevents ICE "missing type for expr" during lowering.
fn infer_default_exprs(checker: &mut Checker, ty: &mut ast::TypeExpr) {
    if let ast::TypeExpr::Variant { cases } = ty {
        for case in cases {
            if let ast::VariantCase::Record { fields, .. } = case {
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
            Ty::TypeVar(tv) => {
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
                if candidates.len() == 1 {
                    let (type_name, field_ty) = candidates.pop().unwrap();
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
                        ));
                        return result;
                    }
                    Ty::Unknown
                }
            }
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
                    ));
                    return result;
                }
                Ty::Unknown
            },
        }
    }
}

impl Checker {
    /// Validate that all Map literal key types are hashable (post-solve).
    /// ALS-C9 / E026: an order-sensitive combinator (`list.sort`/`min`/`max`,
    /// `sort_by`'s key) needs an ORDERABLE element — the native runtime's
    /// `T: Ord` bound. A bare Float element is fine (it routes to the `_float`
    /// twins); a Map/Set/Fn element — or Float NESTED inside a compound — has
    /// no order, and previously check accepted it while native rustc rejected
    /// the monomorph ("AlmideMap: Ord is not satisfied" — the check-vs-build
    /// gap, fuzz seed-20260718 index 629).
    fn validate_ord_elem_types(&mut self) {
        use std::collections::HashSet;
        let mut reported: HashSet<String> = HashSet::new();
        let checks = std::mem::take(&mut self.deferred_ord_elem_checks);
        for (subject_ty, span, fn_name) in checks {
            let resolved = resolve_ty(&subject_ty, &self.uf);
            // list.sort/min/max enqueue the LIST subject; sort_by enqueues the
            // key type directly. Extract the element when it is a List.
            let elem = match &resolved {
                Ty::Applied(almide_lang::types::TypeConstructorId::List, a) if a.len() == 1 => {
                    a[0].clone()
                }
                _ => resolved.clone(),
            };
            // An unresolved slot is E025's business; a BARE Float rides the
            // `_float` twins.
            if matches!(elem, Ty::Unknown | Ty::TypeVar(_) | Ty::Float) {
                continue;
            }
            if self.env.is_ord(&elem) {
                continue;
            }
            let ty_name = Self::type_display_name(&elem);
            if !reported.insert(format!("{fn_name}:{ty_name}")) {
                continue;
            }
            let mut diag = err(
                format!("type '{}' has no ordering — cannot be used with {}", ty_name, fn_name),
                "Ordering needs Int, Bool, String, Float, or lists/tuples/records of those.                  Map, Set, and function values have no order; Float inside a compound                  element has none either (compare via an explicit key instead)."
                    .to_string(),
                format!("call to {}", fn_name),
            ).with_code("E026");
            if let Some(s) = span {
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            self.diagnostics.push(diag);
        }
    }

    /// E027: every `Ty::Named` mentioned in an ANNOTATION must be a declared
    /// type. An undeclared name flowed through unification unconstrained
    /// (`let xs: List[Inner] = []`) and compiled to a nonexistent Rust type
    /// (E0412/E0425) — check accepted, build failed (the acceptance-parity
    /// gap, differential-fuzz seed 20260718 index 940's mutated-away `type`
    /// declaration). Generic params are immune: resolve_type_expr turns an
    /// in-scope generic into `Ty::TypeVar` at annotation time, never `Named`.
    fn validate_unknown_named_types(&mut self) {
        use std::collections::HashSet;
        fn collect_named(ty: &Ty, out: &mut Vec<Sym>) {
            match ty {
                Ty::Named(s, args) => {
                    out.push(*s);
                    for a in args { collect_named(a, out); }
                }
                Ty::Applied(_, args) | Ty::Tuple(args) | Ty::Union(args) => {
                    for a in args { collect_named(a, out); }
                }
                Ty::Fn { params, ret } => {
                    for p in params { collect_named(p, out); }
                    collect_named(ret, out);
                }
                Ty::Record { fields } | Ty::OpenRecord { fields } => {
                    for (_, f) in fields { collect_named(f, out); }
                }
                _ => {}
            }
        }
        let mut reported: HashSet<Sym> = HashSet::new();
        let checks = std::mem::take(&mut self.deferred_unknown_type_checks);
        for (ty, span, ctx) in checks {
            let resolved = resolve_ty(&ty, &self.uf);
            let mut names = Vec::new();
            collect_named(&resolved, &mut names);
            for s in names {
                // `Value` is the BUILT-IN dynamic type (json/codec surface) —
                // nominal by name but never declared in env.types.
                if s.as_str() == "Value" || self.env.types.contains_key(&s) || !reported.insert(s) {
                    continue;
                }
                let mut diag = err(
                    format!("unknown type '{}'", s),
                    format!("no `type {}` is declared (or imported) in this program — declare it, or check the spelling", s),
                    ctx.clone(),
                ).with_code("E027");
                if let Some(sp) = span {
                    diag.line = Some(sp.line);
                    diag.col = Some(sp.col);
                }
                self.diagnostics.push(diag);
            }
        }
    }

    fn validate_map_key_types(&mut self) {
        use std::collections::HashSet;
        let mut reported: HashSet<String> = HashSet::new();
        let checks = std::mem::take(&mut self.deferred_map_key_checks);
        for (key_ty, span) in checks {
            let resolved = resolve_ty(&key_ty, &self.uf);
            if !self.env.is_hash(&resolved) {
                let ty_name = Self::type_display_name(&resolved);
                reported.insert(ty_name.clone());
                let mut diag = err(
                    format!("type '{}' is not hashable — cannot be used as a Map key", ty_name),
                    "Map keys must be hashable. Use String, Int, Bool, or a record/variant with only hashable fields.".to_string(),
                    "map literal".to_string(),
                );
                if let Some(s) = span {
                    diag.line = Some(s.line);
                    diag.col = Some(s.col);
                }
                self.diagnostics.push(diag);
            }
        }
        // #598: the deferred queue only catches map LITERALS. Sweep the SOLVED
        // type map for EVERY Map[K, V] — so the stdlib builder API
        // (map.new/map.set/from_list/insert) gets the same authoritative
        // unhashable-key rejection the literal already gets, in the frontend
        // (identical on both targets). Previously an unhashable Float key built
        // via map.from_list passed `check`, then on wasm SILENTLY collapsed
        // distinct keys (len 2 → 1) or produced a [COMPILER BUG] invalid module.
        let map_keys: Vec<crate::types::Ty> = self.type_map.values()
            .filter_map(|t| match t {
                crate::types::Ty::Applied(almide_lang::types::TypeConstructorId::Map, args) if args.len() == 2 =>
                    Some(args[0].clone()),
                _ => None,
            })
            .collect();
        for key_ty in map_keys {
            let resolved = resolve_ty(&key_ty, &self.uf);
            // Skip inference vars that never pinned down — that is the empty
            // collection class, reported elsewhere; only fire on a CONCRETE
            // unhashable key.
            if matches!(resolved, crate::types::Ty::Unknown | crate::types::Ty::TypeVar(_)) {
                continue;
            }
            if !self.env.is_hash(&resolved) {
                let ty_name = Self::type_display_name(&resolved);
                if reported.insert(ty_name.clone()) {
                    self.diagnostics.push(err(
                        format!("type '{}' is not hashable — cannot be used as a Map key", ty_name),
                        "Map keys must be hashable. Use String, Int, Bool, or a record/variant with only hashable fields.".to_string(),
                        "map key".to_string(),
                    ));
                }
            }
        }
    }

    /// Reject empty-collection producers whose element type the program never
    /// pins down (post-solve). The Rust/Swift rule: an empty `[]`/`[:]`/set whose
    /// element type cannot be inferred from any surrounding context is a COMPILE
    /// ERROR — not a slot codegen may silently default. Firing here, in the
    /// frontend, makes the error identical on BOTH targets (Rust rustc would
    /// reject `Vec::<_>::new()` with E0282; wasm carries no element type and used
    /// to run — that cross-target asymmetry is what this closes). Observability is
    /// irrelevant, exactly as in Rust/Swift: `for _ in []` is an error even though
    /// the element is never read. Each `?A` that survived the whole-program solve
    /// (against `self.uf`) had no inference source; a populated/annotated form
    /// would have unified it the normal way and resolved clean here.
    /// Post-solve range check for int literals that overflow `i64` (#626). The
    /// effective type is the binding's declared type if one was recorded, else
    /// the literal's resolved type, else a default `Int` (i64). A literal that
    /// does not fit that type's range — and is not the negated `i64::MIN`
    /// magnitude — would silently fold to 0 in codegen, so it is rejected (E024).
    fn validate_int_overflow_literals(&mut self) {
        let checks = std::mem::take(&mut self.deferred_int_overflow_checks);
        for site in checks {
            // Effective type: explicit binding annotation, else the literal's
            // own resolved type, else default Int.
            let eff = site.context_ty.clone()
                .map(|t| resolve_ty(&t, &self.uf))
                .or_else(|| self.type_map.get(&site.expr_id).map(|t| resolve_ty(t, &self.uf)))
                .unwrap_or(Ty::Int);
            // Only a concrete integer context decides this; an unresolved/var or
            // non-integer effective type is left to the normal checker.
            let eff = match eff {
                Ty::Int | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 => eff,
                _ => Ty::Int, // fall back to the default Int context
            };
            if int_literal_fits_type(&site.raw, &eff, site.negated) { continue; }
            let mut diag = err(
                format!("integer literal '{}' is out of range for {}", site.raw, eff.display()),
                format!(
                    "{} would silently fold to 0 here; use a literal within the type's range, \
                     or model larger magnitudes as Float (lossy) or a parsed string",
                    eff.display(),
                ),
                format!("integer literal {}", site.raw),
            ).with_code("E024");
            if let Some(s) = site.span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
                if s.end_col > s.col { diag.end_col = Some(s.end_col); }
            }
            self.diagnostics.push(diag);
        }
    }

    fn validate_empty_collection_elements(&mut self) {
        let checks = std::mem::take(&mut self.deferred_empty_collection_checks);
        for site in checks {
            let resolved = resolve_ty(&site.ty, &self.uf);
            if !Self::has_unconstrained_element(&resolved) { continue; }
            // `what` names the construct; `fix` is a CONCRETE, parseable
            // annotation that resolves it. The let-binding form is the primary
            // fix (it always works); the inline `[]: List[Int]` call-arg form is
            // offered only for the bare list literal, where it is verified to
            // parse and infer.
            let (what, fix) = match site.kind {
                EmptyCollectionKind::ListLiteral => (
                    "empty list `[]`",
                    "bind it with an explicit element type, e.g. `let xs: List[Int] = []`, \
                     or annotate the literal inline: `list.len([]: List[Int])`",
                ),
                EmptyCollectionKind::MapLiteral => (
                    "empty map `[:]`",
                    "bind it with explicit key/value types, e.g. `let m: Map[String, Int] = [:]`",
                ),
                EmptyCollectionKind::SetNew => (
                    "`set.new()`",
                    "bind it with an explicit element type, e.g. `let s: Set[Int] = set.new()`",
                ),
                EmptyCollectionKind::ListWithCapacity => (
                    "`list.with_capacity(n)`",
                    "bind it with an explicit element type, e.g. `let xs: List[Int] = list.with_capacity(n)`",
                ),
                EmptyCollectionKind::ForInEmpty => (
                    "the empty list iterated by `for`",
                    "bind the list to an explicitly-typed variable first, e.g. \
                     `let xs: List[Int] = []` then `for _ in xs { ... }`",
                ),
            };
            let hint = format!(
                "{}'s element type cannot be inferred here. An empty collection \
                 carries no element to infer from — {}. (Almide follows Rust/Swift: \
                 an undecidable empty collection is an error even if its elements are \
                 never read; it is never silently defaulted.)",
                what, fix,
            );
            let try_fix = match site.kind {
                EmptyCollectionKind::ListLiteral => "let xs: List[Int] = []",
                EmptyCollectionKind::MapLiteral => "let m: Map[String, Int] = [:]",
                EmptyCollectionKind::SetNew => "let s: Set[Int] = set.new()",
                EmptyCollectionKind::ListWithCapacity => "let xs: List[Int] = list.with_capacity(n)",
                EmptyCollectionKind::ForInEmpty => "let xs: List[Int] = []\nfor _ in xs { ... }",
            };
            let mut diag = err(
                format!("cannot infer the element type of {}", what),
                hint,
                format!("{} with no element-type context", what),
            ).with_code("E018").with_try(try_fix);
            if let Some(s) = site.span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
                if s.end_col > s.col { diag.end_col = Some(s.end_col); }
            }
            self.diagnostics.push(diag);
        }
    }

    /// Post-solve: reject an un-annotated binding / discarded expression whose
    /// type still carries an unbound `?`-prefixed inference var anywhere in its
    /// tree. Such a var had NO inference source for the whole program (a phantom
    /// slot only reachable through an un-exercised branch — e.g. the error type
    /// of `result.or_else(r0, (e) => ok(0))`). Firing here, in the frontend,
    /// makes the error identical on BOTH targets and gives a `type annotation
    /// needed` diagnostic (cf. Rust E0282 / the sibling empty-collection E018)
    /// instead of the cryptic ConcretizeTypes COMPILER-BUG gate ICE (#662). A
    /// fully-decidable program leaves every binding type concrete, so this only
    /// fires on the genuinely-undecidable case; sites are deduped so a binding
    /// reused N times yields one error.
    fn validate_unresolved_binding_types(&mut self) {
        let checks = std::mem::take(&mut self.deferred_unresolved_binding_checks);
        let mut reported: std::collections::HashSet<(Option<u32>, Option<u32>)> = std::collections::HashSet::new();
        for site in checks {
            let resolved = resolve_ty(&site.ty, &self.uf);
            // A WHOLLY-Unknown binding type is error-recovery (a prior error was
            // already reported) or a structure the checker leaves Unknown but
            // codegen resolves (e.g. a cross-module variant `match` whose arms are
            // concrete). Neither is a genuinely-undecidable SLOT, so skip it — only
            // fire when a CONCRETE outer type carries an undecidable inner slot
            // (`Result[Int, ?]`, never pinned). Without this guard E025 false-fired
            // on valid cross-module variant matches.
            if matches!(resolved, Ty::Unknown) { continue; }
            // An unbound `?`-prefixed inference var (`fresh_var` the solver never
            // bound) anywhere in the tree is undecidable. A BARE `TypeVar` (`T`,
            // no `?`) is a rigid generic param — concrete in its scope — so it
            // must NOT trigger this (mirrors `has_unconstrained_element`).
            let undecidable = resolved.any_child_recursive(&|t: &Ty| match t {
                Ty::Unknown => true,
                Ty::TypeVar(n) => n.as_str().starts_with('?'),
                _ => false,
            });
            if !undecidable { continue; }
            let key = (site.span.map(|s| s.line as u32), site.span.map(|s| s.col as u32));
            if !reported.insert(key) { continue; }
            let what = match &site.name {
                Some(n) => format!("binding '{}'", n),
                None => "this expression".to_string(),
            };
            let fix = match &site.name {
                Some(n) => format!(
                    "Annotate the binding with the full type, e.g. `let {}: Result[Int, String] = ...`. \
                     An unconstrained slot (such as the error type of a value that is always `ok(...)`, \
                     reachable only through an un-exercised branch) cannot be inferred and is never \
                     silently defaulted (Almide follows Rust/Swift; cf. Rust E0282).",
                    n,
                ),
                None => "Bind the expression to an explicitly-typed `let`, e.g. \
                     `let r: Result[Int, String] = ...`, so the unconstrained slot is pinned. \
                     An unconstrained type slot cannot be inferred and is never silently defaulted \
                     (Almide follows Rust/Swift; cf. Rust E0282).".to_string(),
            };
            let mut diag = err(
                format!("cannot infer a concrete type for {} (type {})", what, resolved.display()),
                fix,
                format!("{} with an unconstrained type", what),
            ).with_code("E025");
            if let Some(n) = &site.name {
                diag = diag.with_try(format!("let {}: Result[Int, String] = ...", n));
            }
            if let Some(s) = site.span {
                diag.file = self.source_file.clone();
                diag.line = Some(s.line);
                diag.col = Some(s.col);
                if s.end_col > s.col { diag.end_col = Some(s.end_col); }
            }
            self.diagnostics.push(diag);
        }
    }

    /// True when `ty` (already resolved against the union-find) is a collection
    /// whose element/key/value slot is still an unresolved INFERENCE var — i.e.
    /// the element type was never pinned by context. `List[?A]`, `Set[?A]`,
    /// `Map[?K, _]`, `Map[_, ?V]` all qualify; a fully-concrete collection does
    /// not. We look one constructor deep (the producer's own container); a
    /// concrete element that itself nests an unresolved deeper payload is some
    /// OTHER expression's empty collection and is reported at its own site.
    ///
    /// A `?`-prefixed `TypeVar` is a fresh inference var (`fresh_var`) that the
    /// solver left unbound — undecidable. A BARE `TypeVar` (`T`, no `?`) is a
    /// rigid GENERIC PARAMETER, a perfectly good concrete element type in its
    /// scope: `fn make[T]() -> List[T] = []` is fine (Rust accepts
    /// `Vec::<T>::new()`), so it must NOT trigger the error.
    fn has_unconstrained_element(ty: &Ty) -> bool {
        use crate::types::TypeConstructorId as TCI;
        // Only an `Unknown` or a fresh inference var (`?`-prefixed) is
        // undecidable; a rigid generic param (`T`) is concrete.
        let is_unresolved = |t: &Ty| match t {
            Ty::Unknown => true,
            Ty::TypeVar(n) => n.as_str().starts_with('?'),
            _ => false,
        };
        match ty {
            Ty::Applied(TCI::List, args) | Ty::Applied(TCI::Set, args) if args.len() == 1 =>
                is_unresolved(&args[0]),
            Ty::Applied(TCI::Map, args) if args.len() == 2 =>
                is_unresolved(&args[0]) || is_unresolved(&args[1]),
            _ => false,
        }
    }

    /// Human-readable type name for diagnostics.
    fn type_display_name(ty: &Ty) -> String {
        match ty {
            Ty::Int => "Int".into(),
            Ty::Float => "Float".into(),
            Ty::String => "String".into(),
            Ty::Bool => "Bool".into(),
            Ty::Unit => "Unit".into(),
            Ty::Bytes => "Bytes".into(),
            Ty::Named(name, _) => name.as_str().to_string(),
            Ty::Fn { .. } => "Fn".into(),
            Ty::Applied(crate::types::TypeConstructorId::Map, _) => "Map".into(),
            Ty::Applied(crate::types::TypeConstructorId::List, args) => {
                if let Some(inner) = args.first() {
                    format!("List[{}]", Self::type_display_name(inner))
                } else {
                    "List".into()
                }
            }
            _ => format!("{:?}", ty),
        }
    }
}

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
        (Ty::Fn { params: sp, ret: sr }, Ty::Fn { params: up, ret: ur }) => {
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
