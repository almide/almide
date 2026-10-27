// The `+` operator's typing: numeric add, String/List/Bytes concat, Matrix
// addition, sized-numeric mixing. Included by `infer_statements.rs`.

impl Checker {

    /// Infer the result type of the + operator (numeric add or string/list concat).
    fn infer_plus_op(&mut self, lc: &Ty, rc: &Ty, lt: Ty, left: &ast::Expr, right: &ast::Expr) -> Ty {
        if let Some(t) = self.infer_plus_op_concat(lc, rc, &lt) {
            return t;
        }
        // Bytes concatenation (R-bytes-16): lowered to `bytes.__concat`.
        if *lc == Ty::Bytes && *rc == Ty::Bytes {
            return Ty::Bytes;
        }
        // Matrix addition
        if *lc == Ty::Matrix || *rc == Ty::Matrix {
            return self.check_matrix_operands("+", lc, rc);
        }
        self.infer_plus_op_numeric_check(lc, rc);
        if let Some(t) = self.infer_plus_op_sized(lc, rc, left, right) {
            return t;
        }
        if *lc == Ty::Float || *rc == Ty::Float { Ty::Float } else { lt }
    }

    /// String/List concatenation guard of [`Self::infer_plus_op`]: unify a
    /// TypeVar/Unknown side with the other side's String/List type, and pin
    /// List element types when concatenating two Lists. `Some` means this
    /// rule applied and the caller should return that type immediately.
    /// Verbatim text move.
    fn infer_plus_op_concat(&mut self, lc: &Ty, rc: &Ty, lt: &Ty) -> Option<Ty> {
        let is_concat_ty = |t: &Ty| matches!(t, Ty::String | Ty::Applied(TypeConstructorId::List, _));
        let is_unknown_ty = |t: &Ty| matches!(t, Ty::Unknown | Ty::TypeVar(_));
        // When one side is List and the other is TypeVar, unify the TypeVar with the List type
        if is_unknown_ty(lc) && is_concat_ty(rc) {
            self.unify_infer(lt, rc);
            let resolved_lt = resolve_ty(lt, &self.uf);
            // Now unify element types if both resolved to List
            if let (Ty::Applied(TypeConstructorId::List, la), Ty::Applied(TypeConstructorId::List, ra)) = (&resolved_lt, rc) {
                if let (Some(le), Some(re)) = (la.first(), ra.first()) {
                    self.unify_infer(le, re);
                }
            }
            return Some(resolve_ty(lt, &self.uf));
        }
        if (is_concat_ty(lc) && (is_concat_ty(rc) || is_unknown_ty(rc)))
            || (is_concat_ty(rc) && is_unknown_ty(lc)) {
            // Unify element types for list concatenation: List[?0] + List[Int] → ?0 = Int.
            //
            // `constrain`, not `unify_infer`: the latter binds inference variables and stays
            // SILENT when both sides are concrete and different, so `[1] + ["a"]` type-checked
            // and then emitted invalid Rust natively / printed the String block's ADDRESS as an
            // Int element on wasm (#1030). The list LITERAL path already reports this exact
            // mismatch as E001 (`[1, "a"]`), so the same property of the same language was
            // checked on one path and not the other. `constrain` still unifies, so the
            // inference-variable cases this rule exists for are unaffected — it additionally
            // REPORTS when unification is impossible.
            if let (Ty::Applied(TypeConstructorId::List, la), Ty::Applied(TypeConstructorId::List, ra)) = (lc, rc) {
                if let (Some(le), Some(re)) = (la.first(), ra.first()) {
                    self.constrain(le.clone(), re.clone(), "list concatenation element");
                }
            }
            return Some(resolve_ty(lt, &self.uf));
        }
        None
    }

    /// Numeric-operand diagnostic guard of [`Self::infer_plus_op`] — emits
    /// E-diagnostic only; the caller falls through regardless. Verbatim
    /// text move.
    fn infer_plus_op_numeric_check(&mut self, lc: &Ty, rc: &Ty) {
        // Sized Numeric Types (Stage 1c): arithmetic accepts canonical
        // `Int` / `Float` plus every sized variant. Same-type pairing is
        // enforced below; mixing widths is an explicit conversion.
        let is_numeric = |t: &Ty| matches!(
            t,
            Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_)
                | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                | Ty::Float32 | Ty::Float64
                | Ty::Matrix | Ty::Named(..)
        );
        if !is_numeric(lc) || !is_numeric(rc) {
            // A Result operand has a specific way out — the generic "use
            // numeric types" hint never mentioned the unwrap operators, so
            // the one real fix was undiscoverable (#1050).
            let hint = if lc.is_result() || rc.is_result() {
                "Unwrap the Result operand first: `!` propagates the error (effect fn body), \
                 `?? fallback` supplies a default, or `match` handles ok/err"
            } else {
                "Use + with numeric types, String, List, or Bytes"
            };
            self.emit(super::err(
                format!("operator '+' requires numeric, String, or List types but got {} and {}", lc.display(), rc.display()),
                hint, format!("operator +")));
        }
    }

    /// Sized-numeric-type guard of [`Self::infer_plus_op`]: reject mixed
    /// widths (E-diagnostic + return the left type), or return the common
    /// sized type when both sides are sized and compatible. Verbatim text
    /// move.
    fn infer_plus_op_sized(&mut self, lc: &Ty, rc: &Ty, left: &ast::Expr, right: &ast::Expr) -> Option<Ty> {
        // Result type resolution:
        //   - Same sized type on both sides → that sized type.
        //   - Canonical Float promotes Int mixes to Float (legacy rule).
        //   - Mixed sized widths are rejected; the diagnostic is
        //     emitted by `compatible` / `unify_infer` callers, so here
        //     we just fall through with `lt` to avoid an extra error.
        let is_sized_scalar = |t: &Ty| matches!(
            t,
            Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                | Ty::Float32 | Ty::Float64
        );
        // Sized Numeric Types (Stage 1c): both sides sized AND widths
        // differ is a type error. The permissive `Ty::Int` / `Ty::Float`
        // canonical pair stays (it carries the literal-coercion slot for
        // `let x: Int32 = 42` style bindings). Mixing `Int32` and `Int16`
        // has no such cover — it's always wrong, always needs explicit
        // `.to_intN()`.
        // A sized operand meeting a canonical `Int`/`Float` VALUE is the same
        // mistake with the wide side spelled differently (#902).
        if let Some(t) = self.check_mixed_canonical_width("+", lc, rc, left, right) {
            return Some(t);
        }
        if is_sized_scalar(lc) && is_sized_scalar(rc) && lc != rc {
            self.emit(super::err(
                format!(
                    "operator '+' mixes sized numeric types {} and {} — \
                     explicit conversion required (e.g. `{}.from(x)`)",
                    lc.display(), rc.display(),
                    lc.display().to_lowercase()),
                "Convert one side with `<type>.from(x)` (`int32.from(x)`, `float64.from(x)`) before the op",
                format!("operator +")));
            return Some(lc.clone());
        }
        if lc.compatible(rc) && is_sized_scalar(lc) {
            return Some(lc.clone());
        }
        None
    }
}
