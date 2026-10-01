// Operator inference: unary operators, the binary dispatch, time arithmetic,
// and the arithmetic / comparison / logical binops. `include!`d through
// infer_control_ops.rs (the 800-line file budget); it shares that module's
// scope and imports.

impl Checker {
    fn infer_expr_g2_unary(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Unary { op, operand, .. } = &mut expr.kind else { unreachable!("infer_expr_g2_unary called on the wrong ExprKind") };
                // #626: `-<int literal>` lets the negation reach i64::MIN, whose
                // magnitude (2^63) overflows a bare positive literal but is a
                // valid i64. Mark the candidate (registered while inferring the
                // operand) so its post-solve range check uses the signed MIN bound.
                //
                // The sign recorded is the NET sign of this node's WHOLE operand
                // chain, not "this node is a minus": `--300` is +300 and
                // `--9223372036854775808` is +2^63, which no signed type holds.
                // Each `Unary` on the way up writes the parity of its own subtree
                // and the operand is inferred first, so the OUTERMOST minus writes
                // last and its answer — the one the source actually states — wins.
                let chain = (op.as_str() == "-")
                    .then(|| super::int_literal_chain(operand))
                    .flatten()
                    .map(|(lit_id, _, inner_negated)| (lit_id, !inner_negated));
                let t = self.infer_expr(operand);
                if let Some((lit_id, negated)) = chain {
                    if let Some(site) = self.deferred_int_overflow_checks.iter_mut().find(|s| s.expr_id == lit_id) {
                        site.negated = negated;
                    }
                }
                // #2196: `-f()` / `not g()` on an effect call — the same
                // operand rule as a binary operand, reported at the call.
                let t = self.operand_effect_unwrap(operand, t);
                let resolved = resolve_ty(&t, &self.uf);
                match op.as_str() {
                    "not" => {
                        self.check_unary_not_operand(&resolved);
                        Ty::Bool
                    }
                    _ => {
                        self.check_unary_neg_operand(&resolved);
                        t
                    }
                }
    }

    /// The operand rule of prefix `not`: a Bool. Before this the checker
    /// accepted any operand and `not 5` reached the IR verifier as an
    /// internal compiler error on both targets.
    fn check_unary_not_operand(&mut self, t: &Ty) {
        if matches!(t, Ty::Bool | Ty::Unknown | Ty::TypeVar(_)) {
            return;
        }
        self.emit(super::err(
            format!("operator 'not' requires Bool but got {}", t.display()),
            "Use `not` on a Bool; compare first (e.g. `not (x == 0)`)",
            "operator not").with_code("E001"));
    }

    /// The operand rule of prefix `-`: a SIGNED numeric type. Negation has
    /// no value in an unsigned domain, and `-128.to_uint16()` (which parses
    /// as `-(128.to_uint16())`) passed check and then failed the native
    /// build (`cannot apply unary operator '-' to type u16`) while the wasm
    /// leg printed a UInt16 of -128 (fuzz seed 585689703896 index 7506).
    fn check_unary_neg_operand(&mut self, t: &Ty) {
        match t {
            Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_)
            | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
            | Ty::Float32 | Ty::Float64
            | Ty::Matrix | Ty::Named(..) => {}
            Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 => {
                let name = t.display();
                self.emit(super::err(
                    format!("operator '-' cannot negate the unsigned type {}", name),
                    format!(
                        "Negate the signed value before converting — `(-128).to_{}()` — \
                         or subtract from zero in the unsigned type (`0.to_{}() - x`)",
                        name.to_lowercase(), name.to_lowercase()),
                    "operator -").with_code("E001"));
            }
            _ => {
                self.emit(super::err(
                    format!("operator '-' requires a signed numeric type but got {}", t.display()),
                    "Use numeric types (Int or Float)",
                    "operator -").with_code("E001"));
            }
        }
    }

    fn infer_expr_g2_binary(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Binary { op, left, right, .. } = &mut expr.kind else { unreachable!("infer_expr_g2_binary called on the wrong ExprKind") };
        let lt = self.infer_expr(left);
        let rt = self.infer_expr(right);
        // #1050: operand-position strip. In an effect-fn body the checker
        // strips Result from a CALL operand so the operator check reads the
        // OK type — and, since #2196, reports the call (E041, or E042 when
        // the operator's value is discarded), so `helper() + 1` is the same
        // explicit-`!` rule as `let x = helper()` (ADR-0008). VARs are
        // untouched: a var's unwrap is decided at its binding, and a
        // Result-typed var operand stays a type error (whose hint names the
        // unwrap operators). This also closes an acceptance-parity hole:
        // `helper() == ok(0)` used to pass check and explode in the generated
        // Rust (auto-? unwrapped the left side under a Result comparand);
        // it is now an honest check-time mismatch.
        let lt = self.operand_effect_unwrap(left, lt);
        let rt = self.operand_effect_unwrap(right, rt);
        self.pin_binop_literal_context(op, left, right, &lt, &rt);
        // ADR-0001 S3: the time-type operator matrix intercepts BEFORE the
        // generic paths — `Named` types pass the generic numeric check (the
        // GPU-vector allowance), which would silently admit `T * T`.
        {
            let lc0 = resolve_ty(&lt, &self.uf);
            let rc0 = resolve_ty(&rt, &self.uf);
            if let Some(t) = self.infer_time_binop(op.as_str(), &lc0, &rc0, &lt, &rt) {
                return t;
            }
        }
        match op.as_str() {
            "+" => {
                let lc = resolve_ty(&lt, &self.uf);
                let rc = resolve_ty(&rt, &self.uf);
                self.infer_plus_op(&lc, &rc, lt, left, right)
            }
            "-" | "*" | "/" | "%" | "^" => self.infer_binop_arith(op, &lt, &rt, left, right),
            "++" => {
                self.emit(super::err(
                    format!("operator '++' has been removed. Use '+' for concatenation"),
                    "Replace ++ with +", "operator ++"));
                lt
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => self.infer_binop_compare(op, left, right, &lt, &rt),
            "and" | "or" => self.infer_binop_logical(op, &lt, &rt),
            _ => lt,
        }
    }

    /// The #1050 operand strip: `expr` is a binary or unary operand; when it
    /// is a CALL whose type resolved to `Result[T, E]` inside an auto-unwrap
    /// context (an effect-fn body outside lambdas), give the operator `T` so
    /// the operator check reads the value the writer meant. Anything else
    /// (vars, ctors, non-effect contexts) passes through.
    ///
    /// The strip is RECOVERY, not acceptance (ADR-0008, #2196): the call is
    /// queued for the post-solve E041 report at its own span — where the `!`
    /// goes — and a position that discards the operator's value (a statement)
    /// upgrades it to E042 through `queue_implicit_prop_leaves`. Before this,
    /// `let x = f() + 1` passed check and the lowering's `insert_auto_try`
    /// propagated the error with no `!` in the source.
    fn operand_effect_unwrap(&mut self, operand: &ast::Expr, t: Ty) -> Ty {
        if !self.env.auto_unwrap || !matches!(operand.kind, ExprKind::Call { .. }) {
            return t;
        }
        let resolved = resolve_ty(&t, &self.uf);
        match resolved.result_ok_ty() {
            Some(ok) => {
                self.deferred_implicit_prop_checks.push((t, operand.span, "of this operand", true, false));
                ok
            }
            None => t,
        }
    }

    /// ADR-0001 S3: the time-type operator matrix. `None` = no time operand
    /// (or an op outside the matrix) — fall through to the generic paths.
    /// Same-clock algebra: `T + T`, `T - T` (0-saturating), `T * Int` /
    /// `Int * T`, and comparisons. Everything else on a time type is a NAMED
    /// error: `T * T` (the Go #64420 silent-10⁹ class), clock mixing, bare
    /// Int join, and the intentionally omitted `/` (S7).
    fn infer_time_binop(&mut self, op: &str, lc: &Ty, rc: &Ty, lt: &Ty, rt: &Ty) -> Option<Ty> {
        let sides = TimeOperands {
            lc,
            rc,
            lt,
            rt,
            l: time_clock_of(lc),
            r: time_clock_of(rc),
        };
        if sides.l.is_none() && sides.r.is_none() {
            return None;
        }
        match op {
            "/" | "%" | "^" => Some(self.time_binop_undefined(op, &sides)),
            "*" => Some(self.time_binop_scale(&sides)),
            "+" | "-" => Some(self.time_binop_additive(op, &sides)),
            "==" | "!=" | "<" | ">" | "<=" | ">=" => Some(self.time_binop_compare(op, &sides)),
            _ => None,
        }
    }

    /// `/`, `%`, `^` on a time type: never defined. `/` is intentionally
    /// omitted (ADR-0001 S7); the others were never in the matrix.
    fn time_binop_undefined(&mut self, op: &str, sides: &TimeOperands) -> Ty {
        let hint = if op == "/" {
            "Division is intentionally omitted (ADR-0001 S7) — divide the Int \
             before constructing, or scale with `*`"
        } else {
            "The time algebra is `T + T`, `T - T` (0-saturating), `T * Int`, \
             and comparisons — nothing else"
        };
        self.emit(super::err(
            format!("operator '{op}' is not defined on time types"),
            hint,
            format!("operator {op}")));
        sides.time_side().clone()
    }

    /// `T * Int` / `Int * T` scales; `T * T` would be time², which has no
    /// meaning.
    fn time_binop_scale(&mut self, sides: &TimeOperands) -> Ty {
        match (sides.l, sides.r) {
            (Some(_), Some(_)) => {
                self.emit(super::err(
                    "cannot multiply two time quantities".to_string(),
                    "time × time has no meaning (the result would be time²) — scale \
                     with an Int: `t * 3`",
                    "operator *".to_string()));
                sides.lc.clone()
            }
            (Some(_), None) => {
                self.constrain(sides.rt.clone(), Ty::Int, "time scale factor");
                sides.lc.clone()
            }
            _ => {
                self.constrain(sides.lt.clone(), Ty::Int, "time scale factor");
                sides.rc.clone()
            }
        }
    }

    /// `T + T` / `T - T` — same clock only. A bare Int operand is a NAMED
    /// error, never an implicit join.
    fn time_binop_additive(&mut self, op: &str, sides: &TimeOperands) -> Ty {
        match (sides.l, sides.r) {
            (Some(a), Some(b)) => {
                if a != b {
                    self.emit_clock_mix(op, if op == "+" { "add" } else { "subtract" });
                }
                sides.lc.clone()
            }
            (Some(a), None) | (None, Some(a)) => {
                self.emit(super::err(
                    format!(
                        "operator '{op}' needs two {a} values, found {} and {}",
                        sides.lc.display(),
                        sides.rc.display()
                    ),
                    format!(
                        "A bare number is never a time — wrap it: {}.ms(n)",
                        time_module_of(a)
                    ),
                    format!("operator {op}")));
                sides.time_side().clone()
            }
            (None, None) => sides.lc.clone(),
        }
    }

    /// Comparisons — same clock only, and never against a bare Int.
    fn time_binop_compare(&mut self, op: &str, sides: &TimeOperands) -> Ty {
        match (sides.l, sides.r) {
            (Some(a), Some(b)) => {
                if a != b {
                    self.emit_clock_mix(op, "compare");
                }
            }
            (Some(a), None) | (None, Some(a)) => {
                self.emit(super::err(
                    format!(
                        "cannot compare {} with {} — both sides must be {a}",
                        sides.lc.display(),
                        sides.rc.display()
                    ),
                    format!(
                        "A bare number is never a time — wrap it: {}.ms(n)",
                        time_module_of(a)
                    ),
                    format!("operator {op}")));
            }
            (None, None) => {}
        }
        Ty::Bool
    }

    /// The two clocks have no bridge (ADR-0001): a deterministic budget is
    /// Compute, a wall-clock deadline is Duration — there is no conversion.
    fn emit_clock_mix(&mut self, op: &str, verb: &str) {
        self.emit(super::err(
            format!("cannot {verb} Compute and Duration"),
            "The two clocks have no bridge (ADR-0001): a deterministic budget is \
             Compute, a wall-clock deadline is Duration — there is no conversion",
            format!("operator {op}")));
    }

    /// E024, binop-operand edition (fuzz seed-20260718 index 114): a bare
    /// int literal meeting a SIZED operand adopts its width at lowering —
    /// pin that width as the literal's range context so the post-solve
    /// check fires (`(x - x) - 256` with x: Int8 — native rustc rejected
    /// `256i8` while check passed). Every literal has a site now (the
    /// liberal enqueue), so this only sets context_ty. Verbatim text move
    /// out of [`Self::infer_expr_g2_binary`].
    fn pin_binop_literal_context(&mut self, op: &Sym, left: &ast::Expr, right: &ast::Expr, lt: &Ty, rt: &Ty) {
        if matches!(op.as_str(), "+" | "-" | "*" | "/" | "%" | "^") {
            let lit_id = |e: &ast::Expr| match &e.kind {
                ExprKind::Int { .. } => Some(e.id),
                ExprKind::Unary { op, operand, .. }
                    if op.as_str() == "-"
                        && matches!(&operand.kind, ExprKind::Int { .. }) =>
                {
                    Some(operand.id)
                }
                ExprKind::Paren { expr }
                    if matches!(&expr.kind, ExprKind::Int { .. }) =>
                {
                    Some(expr.id)
                }
                _ => None,
            };
            let is_sized_int = |t: &Ty| matches!(
                t,
                Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
            );
            let lc0 = resolve_ty(lt, &self.uf);
            let rc0 = resolve_ty(rt, &self.uf);
            let l_lit = lit_id(left);
            let r_lit = lit_id(right);
            if is_sized_int(&lc0) {
                if let Some(id) = r_lit {
                    self.pin_int_literal_context(id, &lc0);
                }
            }
            if is_sized_int(&rc0) {
                if let Some(id) = l_lit {
                    self.pin_int_literal_context(id, &rc0);
                }
            }
        }
    }

    /// `-`/`*`/`/`/`%`/`^` arm of [`Self::infer_expr_g2_binary`]: Matrix
    /// arithmetic, numeric-operand and mixed-sized-width diagnostics, and
    /// same-width/Float-promotion result-type resolution. Verbatim text move.
    fn infer_binop_arith(&mut self, op: &Sym, lt: &Ty, rt: &Ty, left: &ast::Expr, right: &ast::Expr) -> Ty {
        let lc = resolve_ty(lt, &self.uf);
        let rc = resolve_ty(rt, &self.uf);
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
                // Same Result-specific hint as `+` (#1050): the generic text
                // never named the unwrap operators.
                let hint = if lc.is_result() || rc.is_result() {
                    "Unwrap the Result operand first: `!` propagates the error (effect fn body), \
                     `?? fallback` supplies a default, or `match` handles ok/err"
                } else {
                    "Use numeric types (Int or Float)"
                };
                self.emit(super::err(
                    format!("operator '{}' requires numeric types but got {} and {}", op, lc.display(), rc.display()),
                    hint, format!("operator {}", op)));
            }
            // A sized operand meeting a canonical `Int`/`Float` VALUE is the
            // same mistake with the wide side spelled differently (#902).
            if let Some(t) = self.check_mixed_canonical_width(op.as_str(), &lc, &rc, left, right) {
                return t;
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
            } else if lc == Ty::Float || rc == Ty::Float { Ty::Float } else { lt.clone() }
        }
    }

    /// `==`/`!=`/`<`/`>`/`<=`/`>=` arm of [`Self::infer_expr_g2_binary`]:
    /// none-comparison validity, TypeVar unification, and the ordering
    /// (`<`/`>`/`<=`/`>=`) scalar-orderable-types restriction (#652).
    /// Verbatim text move.
    fn infer_binop_compare(&mut self, op: &Sym, left: &ast::Expr, right: &ast::Expr, lt: &Ty, rt: &Ty) -> Ty {
        // Check none comparison: only valid with Option types
        let left_is_none = matches!(left.kind, ExprKind::None);
        let right_is_none = matches!(right.kind, ExprKind::None);
        if right_is_none && !left_is_none {
            let lc = resolve_ty(lt, &self.uf);
            if !lc.is_option() && !matches!(lc, Ty::Unknown | Ty::TypeVar(_)) {
                self.emit(super::err(
                    format!("cannot compare {} with none — only Option types support none comparison", lc.display()),
                    "Use Option type or check with is_ok()/is_err() for Result", "comparison with none"));
            }
        }
        if left_is_none && !right_is_none {
            let rc = resolve_ty(rt, &self.uf);
            if !rc.is_option() && !matches!(rc, Ty::Unknown | Ty::TypeVar(_)) {
                self.emit(super::err(
                    format!("cannot compare none with {} — only Option types support none comparison", rc.display()),
                    "Use Option type or check with is_ok()/is_err() for Result", "comparison with none"));
            }
        }
        // Unify left/right types so TypeVars in none/err/constructors get resolved
        self.unify_infer(lt, rt);
        // #1050: a Result on exactly ONE side of ==/!= can never compare —
        // `unify_infer` stays silent on a concrete mismatch, so this shape
        // used to pass check and explode as an E0308 in the generated Rust
        // (`helper() == ok(0)`: the operand strip / auto-? gives the call
        // side `T` while the ctor side keeps `Result`). Report it here with
        // the unwrap operators named.
        if matches!(op.as_str(), "==" | "!=") {
            let lc = resolve_ty(lt, &self.uf);
            let rc = resolve_ty(rt, &self.uf);
            let opaque = |t: &Ty| matches!(t, Ty::Unknown | Ty::TypeVar(_) | Ty::Never);
            if lc.is_result() != rc.is_result() && !opaque(&lc) && !opaque(&rc) {
                self.emit(super::err(
                    format!("operator '{}' compares {} with {}", op, lc.display(), rc.display()),
                    "Unwrap the Result operand first (`!` in an effect fn body, `?? fallback`, \
                     or `match` on ok/err) — or compare two Results",
                    format!("operator {}", op)).with_code("E037"));
            } else if !(left_is_none || right_is_none)
                && lc.is_option() != rc.is_option() && !opaque(&lc) && !opaque(&rc) {
                // #1518: the same one-sided rule for Option. `opt() == 1`
                // (and the deeper `Option[Int]? == Int`) passed check and died
                // as rustc E0308 natively / an unresolvable-Eq wall on wasm.
                // A literal `none` operand is excluded: the dedicated
                // none-comparison rule above already reported it.
                self.emit(super::err(
                    format!("operator '{}' compares {} with {}", op, lc.display(), rc.display()),
                    "Unwrap the Option operand first (`?? fallback`, or `match` on some/none) \
                     — or compare two Options",
                    format!("operator {}", op)).with_code("E037"));
            } else if same_head_applied_mismatch(&lc, &rc) {
                // #1116: `unify_infer` stays silent on a concrete mismatch, so
                // same-head shapes with different params (`Result[Int, String]
                // == Result[Int, Int]`, `Option[Int] == Option[String]`)
                // passed check and died as rustc E0308 behind the
                // codegen-produced-invalid-Rust wall. Both sides fully
                // concrete + structurally different = can never compare.
                self.emit(super::err(
                    format!("operator '{}' compares {} with {}", op, lc.display(), rc.display()),
                    "== requires both operands to have the same type — convert one side \
                     (or compare the payloads after unwrapping)",
                    format!("operator {}", op)).with_code("E037"));
            }
            // #1116: `none == none` (both sides `Option[?0]`, never pinned)
            // passed check and died as rustc E0282. Queue the unified operand
            // type for the post-solve E025 sweep — same rule as bindings and
            // interpolation segments.
            self.deferred_unresolved_binding_checks.push(super::UnresolvedBindingSite {
                ty: lt.clone(), name: None, span: left.span,
            });
            // #2606: and for the post-solve Eq check — a function anywhere in
            // the operand type has no equality.
            self.deferred_eq_checks.push((lt.clone(), left.span, format!("operator '{}'", op)));
        }
        // Ordering (< <= > >=) is defined ONLY on scalar orderable
        // types. On a compound operand (Tuple/Option/Result/List/
        // Map/Set/Record/custom) the checker used to pass while
        // codegen diverged: native silently relied on Rust's derive
        // (and FAILED on records, E0369) and WASM ICEd
        // (equality.rs no-comparison arm). Reject uniformly so check
        // matches codegen on both targets; equality (== !=) still
        // works (deep structural). #652
        if matches!(op.as_str(), "<" | ">" | "<=" | ">=") {
            let lc = resolve_ty(lt, &self.uf);
            let orderable = matches!(lc,
                Ty::Int | Ty::Float | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                | Ty::Float32 | Ty::Float64 | Ty::String | Ty::Bool
                | Ty::Unknown | Ty::TypeVar(_) | Ty::Never);
            if !orderable {
                self.emit(super::err(
                    format!("operator '{}' is not defined for {} — ordering applies to Int, Float, String, and Bool", op, lc.display()),
                    "Compare scalar fields explicitly, or use list.sort / list.min / list.max for ordered collections",
                    format!("operator {}", op)).with_code("E030"));
            }
        }
        Ty::Bool
    }

    /// `and`/`or` arm of [`Self::infer_expr_g2_binary`]. Verbatim text move.
    fn infer_binop_logical(&mut self, op: &Sym, lt: &Ty, rt: &Ty) -> Ty {
        let lc = resolve_ty(lt, &self.uf);
        let rc = resolve_ty(rt, &self.uf);
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
}
