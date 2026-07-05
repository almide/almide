// `infer_expr_inner` group 2 — literals, identifiers, simple containers,
// and the operator / control-flow arms (Int … Match). Disjoint from every
// other group; see `infer_expr_inner` for the dispatch contract. Split out
// of `infer.rs` (via `include!`) to keep each file under the 1000-line
// ceiling; imports come from `infer.rs` (this file is textually inlined).

impl Checker {
    pub(super) fn infer_expr_inner_g2(&mut self, expr: &mut ast::Expr) -> Option<Ty> {
        Some(match &mut expr.kind {
            ExprKind::Int { .. } => Ty::Int,
            ExprKind::Float { .. } => Ty::Float,
            ExprKind::String { .. } => Ty::String,
            ExprKind::InterpolatedString { parts, .. } => {
                for part in parts.iter_mut() {
                    if let ast::StringPart::Expr { expr } = part {
                        self.infer_expr(expr);
                    }
                }
                Ty::String
            }
            ExprKind::Bool { .. } => Ty::Bool,
            ExprKind::Unit => Ty::Unit,

            ExprKind::None => Ty::option(self.fresh_var()),

            ExprKind::Ident { name, .. } => {
                self.env.used_vars.insert(sym(name));
                if let Some(ty) = self.env.lookup_var(name).cloned() { self.instantiate_ty(&ty) }
                else if let Some(ty) = self.env.top_lets.get(&sym(name)).cloned() { self.instantiate_ty(&ty) }
                // Const param: `N: Int` in generic params resolves to its underlying type
                else if let Some(Ty::ConstParam { ty, .. }) = self.env.types.get(&sym(name)).cloned() {
                    *ty
                }
                else if let Some(sig) = self.env.functions.get(&sym(name)).cloned() {
                    Ty::Fn {
                        params: sig.params.iter().map(|(_, t)| t.clone()).collect(),
                        ret: Box::new(sig.ret.clone()),
                    }
                }
                else {
                    // Only suggest `import` for modules that require explicit import
                    // and whose names won't be confused with common variable names.
                    // e.g. `value`, `error`, `string`, `list` are too common as
                    // variable names — suggesting `import value` is misleading.
                    let (hint, fix): (String, Option<String>) = if crate::stdlib::is_import_suggestable(name) {
                        let desc = crate::stdlib::module_description(name);
                        (format!("Add `import {}` (stdlib: {})\nOr run `almide fmt` to auto-add missing imports", name, desc),
                         Some(format!("import {}", name)))
                    } else {
                        let candidates = self.env.all_visible_names();
                        if let Some(suggestion) = almide_base::diagnostic::suggest(name, candidates.iter().map(|s| s.as_str())) {
                            (format!("Did you mean `{}`?", suggestion), Some(suggestion.to_string()))
                        } else {
                            ("Check the variable name".to_string(), None)
                        }
                    };
                    let mut diag = super::err(format!("undefined variable '{}'", name), hint, format!("variable {}", name)).with_code("E003");
                    if let Some(fix) = fix {
                        if let Some(stripped) = fix.strip_prefix("import ") {
                            // Zero-width insert at the top of file — the
                            // new `import <module>\n` line is prepended.
                            // `apply_try_to` handles `end_col == col` as
                            // an insertion point.
                            diag = diag.with_try_replace(
                                1, 1, 1,
                                format!("import {}\n", stripped),
                            );
                        } else if let Some(span) = self.current_span {
                            // Typo fuzzy suggestion: replace the
                            // offending identifier with the suggested name.
                            diag = diag.with_try_replace(
                                span.line, span.col, span.end_col,
                                fix,
                            );
                        } else {
                            diag = diag.with_try(format!("// {}  →  {}\n{}", name, fix, fix));
                        }
                    }
                    self.emit(diag);
                    Ty::Unknown
                }
            }
            ExprKind::List { elements, .. } => {
                if elements.is_empty() {
                    let ty = Ty::list(self.fresh_var());
                    self.register_empty_collection(ty.clone(), super::EmptyCollectionKind::ListLiteral);
                    ty
                }
                else {
                    let first = self.infer_expr(&mut elements[0]);
                    for elem in elements.iter_mut().skip(1) { let et = self.infer_expr(elem); self.constrain(first.clone(), et, "list element"); }
                    Ty::list(first)
                }
            }

            ExprKind::Tuple { elements, .. } => Ty::Tuple(elements.iter_mut().map(|e| self.infer_expr(e)).collect()),
            ExprKind::SpreadRecord { base, fields, .. } => {
                let base_ty = self.infer_expr(base);
                for f in fields.iter_mut() { self.infer_expr(&mut f.value); }
                base_ty
            }
            ExprKind::IndexAccess { object, index, .. } => {
                let obj_ty = self.infer_expr(object);
                self.infer_expr(index);
                let is_range = matches!(&index.kind, ExprKind::Range { .. });
                let concrete = resolve_ty(&obj_ty, &self.uf);
                if is_range {
                    concrete
                } else {
                    match &concrete {
                        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                        Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => Ty::option(args[1].clone()),
                        Ty::Bytes => Ty::Int,
                        Ty::String => {
                            self.emit(super::err(
                                "cannot index a String with `[]`",
                                "a String is a UTF-8 codepoint sequence, not an array — use `string.get(s, i)` (returns `Option[String]`) or `string.char_at(s, i)`",
                                "string index",
                            ).with_code("E026"));
                            Ty::Unknown
                        }
                        _ => Ty::Unknown,
                    }
                }
            }
            ExprKind::Binary { op, left, right, .. } => {
                let lt = self.infer_expr(left);
                let rt = self.infer_expr(right);
                match op.as_str() {
                    "+" => {
                        let lc = resolve_ty(&lt, &self.uf);
                        let rc = resolve_ty(&rt, &self.uf);
                        self.infer_plus_op(&lc, &rc, lt)
                    }
                    "-" | "*" | "/" | "%" | "^" => {
                        let lc = resolve_ty(&lt, &self.uf);
                        let rc = resolve_ty(&rt, &self.uf);
                        // Matrix operators: *, +, - on Matrix types
                        if lc == Ty::Matrix || rc == Ty::Matrix {
                            Ty::Matrix
                        } else {
                            // Sized Numeric Types (Stage 1c): same-width
                            // arithmetic accepts every sized numeric variant.
                            let is_numeric = |t: &Ty| matches!(
                                t,
                                Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_)
                                    | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                                    | Ty::Float32 | Ty::Float64
                                    | Ty::Matrix
                                    // GPU vector/matrix types (Vec2, Vec3, Vec4, Mat3, Mat4)
                                    // support arithmetic ops; emitted as WGSL builtins.
                                    | Ty::Named(..)
                            );
                            let is_sized_scalar = |t: &Ty| matches!(
                                t,
                                Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                                    | Ty::Float32 | Ty::Float64
                            );
                            if !is_numeric(&lc) || !is_numeric(&rc) {
                                self.emit(super::err(
                                    format!("operator '{}' requires numeric types but got {} and {}", op, lc.display(), rc.display()),
                                    "Use numeric types (Int or Float)", format!("operator {}", op)));
                            }
                            // Stage 1c: reject mixed-sized-width arithmetic.
                            // See `infer_plus_op` for rationale.
                            if is_sized_scalar(&lc) && is_sized_scalar(&rc) && lc != rc {
                                self.emit(super::err(
                                    format!(
                                        "operator '{}' mixes sized numeric types {} and {} — \
                                         explicit conversion required (e.g. `.to_{}()`)",
                                        op, lc.display(), rc.display(),
                                        lc.display().to_lowercase()),
                                    "Convert one side with `.to_intN()` / `.to_floatN()` before the op",
                                    format!("operator {}", op)));
                                lc
                            } else if lc.compatible(&rc) && is_sized_scalar(&lc) {
                                lc
                            } else if lc == Ty::Float || rc == Ty::Float { Ty::Float } else { lt }
                        }
                    }
                    "++" => {
                        self.emit(super::err(
                            format!("operator '++' has been removed. Use '+' for concatenation"),
                            "Replace ++ with +", "operator ++"));
                        lt
                    }
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        // Check none comparison: only valid with Option types
                        let left_is_none = matches!(left.kind, ExprKind::None);
                        let right_is_none = matches!(right.kind, ExprKind::None);
                        if right_is_none && !left_is_none {
                            let lc = resolve_ty(&lt, &self.uf);
                            if !lc.is_option() && !matches!(lc, Ty::Unknown | Ty::TypeVar(_)) {
                                self.emit(super::err(
                                    format!("cannot compare {} with none — only Option types support none comparison", lc.display()),
                                    "Use Option type or check with is_ok()/is_err() for Result", "comparison with none"));
                            }
                        }
                        if left_is_none && !right_is_none {
                            let rc = resolve_ty(&rt, &self.uf);
                            if !rc.is_option() && !matches!(rc, Ty::Unknown | Ty::TypeVar(_)) {
                                self.emit(super::err(
                                    format!("cannot compare none with {} — only Option types support none comparison", rc.display()),
                                    "Use Option type or check with is_ok()/is_err() for Result", "comparison with none"));
                            }
                        }
                        // Unify left/right types so TypeVars in none/err/constructors get resolved
                        self.unify_infer(&lt, &rt);
                        // Ordering (< <= > >=) is defined ONLY on scalar orderable
                        // types. On a compound operand (Tuple/Option/Result/List/
                        // Map/Set/Record/custom) the checker used to pass while
                        // codegen diverged: native silently relied on Rust's derive
                        // (and FAILED on records, E0369) and WASM ICEd
                        // (equality.rs no-comparison arm). Reject uniformly so check
                        // matches codegen on both targets; equality (== !=) still
                        // works (deep structural). #652
                        if matches!(op.as_str(), "<" | ">" | "<=" | ">=") {
                            let lc = resolve_ty(&lt, &self.uf);
                            let orderable = matches!(lc,
                                Ty::Int | Ty::Float | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                                | Ty::Float32 | Ty::Float64 | Ty::String | Ty::Bool
                                | Ty::Unknown | Ty::TypeVar(_) | Ty::Never);
                            if !orderable {
                                self.emit(super::err(
                                    format!("operator '{}' is not defined for {} — ordering applies to Int, Float, String, and Bool", op, lc.display()),
                                    "Compare scalar fields explicitly, or use list.sort / list.min / list.max for ordered collections",
                                    format!("operator {}", op)));
                            }
                        }
                        Ty::Bool
                    }
                    "and" | "or" => {
                        let lc = resolve_ty(&lt, &self.uf);
                        let rc = resolve_ty(&rt, &self.uf);
                        let is_bool = |t: &Ty| matches!(t, Ty::Bool | Ty::Unknown | Ty::TypeVar(_));
                        if !is_bool(&lc) {
                            self.emit(super::err(
                                format!("operator '{}' requires Bool but got {}", op, lc.display()),
                                "Use Bool values with logical operators", format!("operator {}", op)));
                        }
                        if !is_bool(&rc) {
                            self.emit(super::err(
                                format!("operator '{}' requires Bool but got {}", op, rc.display()),
                                "Use Bool values with logical operators", format!("operator {}", op)));
                        }
                        Ty::Bool
                    }
                    _ => lt,
                }
            }

            ExprKind::Unary { op, operand, .. } => {
                let is_neg_lit = op.as_str() == "-" && matches!(&operand.kind, ExprKind::Int { .. });
                let oid = operand.id;
                let t = self.infer_expr(operand);
                // #626: `-<int literal>` lets the negation reach i64::MIN, whose
                // magnitude (2^63) overflows a bare positive literal but is a
                // valid i64. Mark the candidate (registered while inferring the
                // operand) so its post-solve range check uses the signed MIN bound.
                if is_neg_lit {
                    if let Some(site) = self.deferred_int_overflow_checks.iter_mut().find(|s| s.expr_id == oid) {
                        site.negated = true;
                    }
                }
                match op.as_str() { "not" => Ty::Bool, _ => t }
            }

            ExprKind::If { cond, then, else_, .. } => {
                self.infer_expr(cond);
                let then_ty = self.infer_expr(then);
                let else_ty = self.infer_expr(else_);
                // In effect fn bodies, auto-unwrap Result[T, E] → T per
                // branch before unifying them, mirroring the match-arm rule
                // (see ExprKind::Match above). Without this, an `if` whose
                // one branch is a `match` on an effect-fn call (auto-unwrapped
                // to T) and whose other branch is an explicit `ok(...)`
                // (stays Result[T, E]) fails E001 — the asymmetry is a
                // checker artefact, not a real type error: codegen's
                // wrap_tail_in_ok normalizes both to Result form. Scoped to
                // `auto_unwrap`, so pure-fn / test if/else are untouched.
                // Auto-unwrap Result[T, E] → T on BOTH branches for the
                // cross-branch COMPARISON only, then return the THEN branch's
                // real (non-unwrapped) type as the if-expression's type.
                //
                // Two requirements pull in opposite directions and this split
                // satisfies both:
                //   • M1 (E001): an `if` whose one branch is a `match` on an
                //     effect-fn call (auto-unwrapped to `T` inside the match)
                //     and whose other branch is an explicit `ok(...)`
                //     (`Result[T, E]`) must not error. Comparing both at the
                //     unwrapped `T` level removes the spurious asymmetry.
                //   • No-regress (`validate_positive`: `if .. then ok(n) else
                //     err(..)`): the if's TYPE must stay `Result[T, E]` so the
                //     WASM emitter sees the real value shape (the branches are
                //     genuine Result constructors). Returning the un-unwrapped
                //     `then_ty` preserves this; codegen's wrap_tail_in_ok then
                //     normalizes every branch to Result form regardless.
                // Scoped to `auto_unwrap`, so pure-fn / test if/else are
                // untouched (they keep the strict same-type rule).
                let cmp_unwrap = |t: &Ty, uf: &_| -> Ty {
                    match resolve_ty(t, uf) {
                        Ty::Applied(TypeConstructorId::Result, ref args) if args.len() == 2 => args[0].clone(),
                        _ => t.clone(),
                    }
                };
                let (cmp_then, cmp_else) = if self.env.auto_unwrap {
                    (cmp_unwrap(&then_ty, &self.uf), cmp_unwrap(&else_ty, &self.uf))
                } else {
                    (then_ty.clone(), else_ty.clone())
                };
                // Specialize the Unit-leak `try:` snippet: if an arm is a
                // bare assignment `x = ...` (returns Unit), we want to cite
                // the actual variable name in the suggested rewrite.
                let hint = if_arm_fix_hint(then, else_);
                self.constrain_with_hint(cmp_then, cmp_else, "if branches", hint);
                then_ty
            }

            ExprKind::IfLet { name, scrutinee, then, else_ } => {
                // Swift-style implicit unwrap: `name` binds the value INSIDE the
                // scrutinee's Option[T] / Result[T, E] (the T). Lowering desugars this
                // to a `match` on Some/Ok once the scrutinee type is known; the checker
                // only INFERS (no rewrite — desugar belongs in lowering).
                let scrut_ty = self.infer_expr(scrutinee);
                let resolved = resolve_ty(&scrut_ty, &self.uf);
                let bound_ty = match &resolved {
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => {
                        args[0].clone()
                    }
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
                        args[0].clone()
                    }
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.emit(super::err(
                            format!("`if let` requires an Option or Result, found `{}`", other.display()),
                            "bind the inner value of an Option/Result: `if let v = some_option { … } else { … }`".to_string(),
                            "if let scrutinee".to_string(),
                        ).with_code("E001"));
                        Ty::Unknown
                    }
                };
                self.env.push_scope();
                self.env.define_var(name, bound_ty);
                let then_ty = self.infer_expr(then);
                self.env.pop_scope();
                let else_ty = self.infer_expr(else_);
                self.constrain_with_hint(then_ty.clone(), else_ty, "if let branches", None);
                then_ty
            }

            ExprKind::Match { subject, arms, .. } => {
                let subject_ty = self.infer_expr(subject);
                let sc = resolve_ty(&subject_ty, &self.uf);
                self.check_match_exhaustiveness(&sc, arms);
                let mut arm_types = Vec::new();
                // Real (un-substituted) arm types, used to pick the overall match
                // result type. An `err(..)` arm produces a genuine `Result[T, E]`
                // value — it is NOT divergent — so even when every arm is `err`,
                // the match still has a concrete Result type (not `Never`).
                let mut arm_real_types = Vec::new();
                // If ANY arm is an explicit `ok(..)`/`err(..)` ctor, this match PRODUCES a Result (it
                // re-wraps — base64 decode's `match bs { ok(b) => ok(string.from_bytes(b)), err(e) =>
                // err(e) }`), so NO arm is auto-unwrapped: every arm keeps its Result type and the
                // match types as Result, not its OK type. (Auto-unwrapping only the effect-call arms
                // while a ctor arm stayed Result mismatched — `Result[(String,Int),String]` vs
                // `(String,Int)` in toml parse_key_part; mistyping the whole match as the OK type
                // walled the v1 MIR / mis-rewrapped native — base64 decode.) The pure auto-unwrap case
                // (no ctor arm, just effect-call/value arms unifying to T) is unchanged.
                let arms_have_result_ctor = arms.iter().any(|a|
                    matches!(&a.body.kind, ExprKind::Ok { .. } | ExprKind::Err { .. }));
                for arm in arms.iter_mut() {
                    self.env.push_scope();
                    let sub_c = resolve_ty(&subject_ty, &self.uf);
                    self.bind_pattern(&arm.pattern, &sub_c);
                    if let Some(ref mut guard) = arm.guard { self.infer_expr(guard); }
                    let arm_ty = self.infer_expr(&mut arm.body);
                    arm_real_types.push(arm_ty.clone());
                    // err() in a match arm is an early return — unify as Never
                    // so it doesn't constrain sibling arm types.
                    let arm_ty = if matches!(&arm.body.kind, ExprKind::Err { .. }) {
                        Ty::Never
                    } else if self.env.auto_unwrap && !arms_have_result_ctor {
                        // In effect fn bodies, auto-unwrap Result[T, E] → T so match arms mixing
                        // effect fn calls (Result) with pure expressions (T) unify correctly. Skipped
                        // when an arm is an explicit ok/err ctor (see arms_have_result_ctor above):
                        // then the match re-wraps a Result and ALL arms keep it.
                        let resolved = resolve_ty(&arm_ty, &self.uf);
                        match resolved {
                            Ty::Applied(TypeConstructorId::Result, ref args) if args.len() == 2 => args[0].clone(),
                            _ => arm_ty,
                        }
                    } else {
                        arm_ty
                    };
                    arm_types.push(arm_ty);
                    self.env.pop_scope();
                }
                // Unify all arm types with each other (not with a shared result var
                // that can be contaminated by external constraints)
                if let Some(first) = arm_types.first().cloned() {
                    for aty in &arm_types[1..] {
                        self.constrain(first.clone(), aty.clone(), "match arm");
                    }
                    // The overall match type is the first non-`Never` arm type.
                    // `Never` arms (every `err(..)` arm) carry no useful result
                    // type but they DO produce a Result value, so when they are
                    // the only arms we recover the concrete type from the real
                    // (un-substituted) arm types — preferring an `err` arm's
                    // `Result[T, E]` so the match types as Result, never `Never`.
                    if matches!(first, Ty::Never) {
                        arm_types.iter()
                            .find(|t| !matches!(t, Ty::Never))
                            .cloned()
                            .or_else(|| arm_real_types.iter()
                                .find(|t| !matches!(resolve_ty(t, &self.uf), Ty::Never))
                                .cloned())
                            .unwrap_or(first)
                    } else {
                        first
                    }
                } else {
                    Ty::Unit
                }
            }
            _ => return None,
        })
    }
}
