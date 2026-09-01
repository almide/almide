impl<'a> Interpreter<'a> {

    // ── Match ───────────────────────────────────────────────────

    fn eval_match(&mut self, subject: &IrExpr, arms: &[IrMatchArm], scope: &Scope) -> Flow {
        let subj = val!(self.eval_expr(subject, scope));
        for arm in arms {
            let mut binds = Vec::new();
            if self.try_match(&arm.pattern, &subj, &mut binds) {
                let frame = scope.child();
                for (id, v) in &binds {
                    frame.bind(*id, v.clone());
                }
                // Evaluate the guard (if any) in the arm's frame.
                if let Some(guard) = &arm.guard {
                    match self.eval_expr(guard, &frame) {
                        Flow::Value(Value::Bool(true)) => {}
                        Flow::Value(Value::Bool(false)) => continue,
                        Flow::Value(other) => {
                            return Flow::Abort(format!(
                                "internal: match guard is {} not Bool",
                                other.type_name()
                            ))
                        }
                        other => return other,
                    }
                }
                return self.eval_expr(&arm.body, &frame);
            }
        }
        // Name the scrutinee: a checked program's match IS exhaustive, so
        // landing here means the interp built a value shape the arms never
        // anticipated — the value is the diagnosis.
        Flow::Abort(format!(
            "internal: non-exhaustive match (no arm matched scrutinee {subj:?})"
        ))
    }

    /// Attempt to match `value` against `pattern`, accumulating bindings.
    /// Returns `true` on success (bindings valid only then). Implements the
    /// IR-level pattern engine directly — including `List` patterns, since
    /// `ListPatternLoweringPass` runs post-cut so list patterns are still
    /// present.
    fn try_match(
        &mut self,
        pattern: &IrPattern,
        value: &Value,
        binds: &mut Vec<(VarId, Value)>,
    ) -> bool {
        if let Some(m) = self.try_match_leaf(pattern, value, binds) { return m; }
        if let Some(m) = self.try_match_container(pattern, value, binds) { return m; }
        if let Some(m) = self.try_match_variant(pattern, value, binds) { return m; }
        // Every `IrPattern` variant is claimed by one of the three groups, so this
        // is unreachable in practice; a NEW variant reads as "does not match"
        // rather than matching everything, which is the safe direction for an
        // oracle (a false negative shows up as an unmatched arm, not a wrong one).
        false
    }

    /// Wildcard, binding, and literal patterns.
    ///
    /// Extracted from `try_match` (name-router split, arms verbatim and in source
    /// order). `None` means "not my group". A pattern no group claims cannot
    /// match, which is what the router returns — the same answer the original
    /// exhaustive match gave, since every `IrPattern` variant is covered here.
    fn try_match_leaf(
        &mut self,
        pattern: &IrPattern,
        value: &Value,
        binds: &mut Vec<(VarId, Value)>,
    ) -> Option<bool> {
        Some(match pattern {
            IrPattern::Wildcard => true,
            IrPattern::Bind { var, .. } => {
                binds.push((*var, value.clone()));
                true
            }
            IrPattern::Literal { expr } => {
                // Evaluate the literal in an empty scope (literals are closed).
                let scope = Scope::root();
                match self.eval_expr(expr, &scope) {
                    Flow::Value(lit) => &lit == value,
                    _ => false,
                }
            }
            _ => return None,
        })
    }

    /// Positional container patterns (tuple, list) — arity-checked, then
    /// element-wise.
    ///
    /// Extracted from `try_match` (name-router split, arms verbatim and in source
    /// order). `None` means "not my group". A pattern no group claims cannot
    /// match, which is what the router returns — the same answer the original
    /// exhaustive match gave, since every `IrPattern` variant is covered here.
    fn try_match_container(
        &mut self,
        pattern: &IrPattern,
        value: &Value,
        binds: &mut Vec<(VarId, Value)>,
    ) -> Option<bool> {
        Some(match pattern {
            IrPattern::Tuple { elements } => match value {
                Value::Tuple(items) if items.len() == elements.len() => elements
                    .iter()
                    .zip(items.iter())
                    .all(|(p, v)| self.try_match(p, v, binds)),
                _ => false,
            },
            IrPattern::As { var, inner, .. } => {
                binds.push((*var, value.clone()));
                self.try_match(inner, value, binds)
            }
            IrPattern::List { elements, rest } => match value.as_iter_items() {
                // #1461 list-rest: rest relaxes the length test to >= and
                // binds the tail past the prefix as a fresh list value.
                Some(items) if rest.is_some() && items.len() >= elements.len() => {
                    let prefix_ok = elements
                        .iter()
                        .zip(items.iter())
                        .all(|(p, v)| self.try_match(p, v, binds));
                    if prefix_ok
                        && let Some(r) = rest
                        && let IrPattern::Bind { var, .. } = r.as_ref()
                    {
                        let tail: Vec<Value> = items[elements.len()..].to_vec();
                        binds.push((*var, Value::List(std::rc::Rc::new(tail))));
                    }
                    prefix_ok
                }
                Some(items) if rest.is_none() && items.len() == elements.len() => elements
                    .iter()
                    .zip(items.iter())
                    .all(|(p, v)| self.try_match(p, v, binds)),
                _ => false,
            },
            _ => return None,
        })
    }

    /// `Option`/`Result` patterns, user constructors, and records.
    ///
    /// Extracted from `try_match` (name-router split, arms verbatim and in source
    /// order). `None` means "not my group". A pattern no group claims cannot
    /// match, which is what the router returns — the same answer the original
    /// exhaustive match gave, since every `IrPattern` variant is covered here.
    fn try_match_variant(
        &mut self,
        pattern: &IrPattern,
        value: &Value,
        binds: &mut Vec<(VarId, Value)>,
    ) -> Option<bool> {
        Some(match pattern {
            // Carrier patterns: unwrap the matching carrier, then recurse.
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
                match carrier_payload(pattern, value) {
                    Some(v) => self.try_match(inner, v, binds),
                    None => false,
                }
            }
            IrPattern::None => matches!(value, Value::Option(None)),
            IrPattern::Constructor { name, args } => self.try_match_ctor(name, args, value, binds),
            IrPattern::RecordPattern { name, fields, rest } => {
                self.try_match_record(name, fields, *rest, value, binds)
            }
            _ => return None,
        })
    }

    /// `try_match`'s `Constructor` arm: the ctor name must agree, and the
    /// payload arity must match the sub-pattern count.
    fn try_match_ctor(
        &mut self,
        name: &str,
        args: &[IrPattern],
        value: &Value,
        binds: &mut Vec<(VarId, Value)>,
    ) -> bool {
        let Value::Variant { ctor, payload, .. } = value else { return false };
        if ctor.as_str() != name {
            return false;
        }
        match payload {
            VariantPayload::Unit => args.is_empty(),
            VariantPayload::Tuple(items) if items.len() == args.len() => args
                .iter()
                .zip(items.iter())
                .all(|(p, v)| self.try_match(p, v, binds)),
            _ => false,
        }
    }

    /// `try_match`'s `RecordPattern` arm, split out as its own method — see
    /// the caller for why the mid-body `return false`s are behavior-preserving
    /// here (each was already a same-call-boundary early exit of `try_match`).
    fn try_match_record(
        &mut self,
        name: &str,
        fields: &[IrFieldPattern],
        rest: bool,
        value: &Value,
        binds: &mut Vec<(VarId, Value)>,
    ) -> bool {
        let Some((obj_name, obj_fields)) = record_shape(value) else { return false };
        // Name must match when the pattern names a constructor.
        if !name.is_empty() && obj_name.is_none_or(|n| n.as_str() != name) {
            return false;
        }
        if !rest && fields.len() != obj_fields.len() {
            return false;
        }
        fields.iter().all(|fp| self.try_match_field(fp, obj_fields, binds))
    }

    /// One `{ field: pat }` entry of a record pattern. The field must exist;
    /// its sub-pattern must then match. Shorthand `{ x, y }` lowers to explicit
    /// `Bind` sub-patterns (verified via IR dump), so binding is handled here
    /// uniformly, and a field with NO sub-pattern is a structural-only match
    /// (the field must exist, but binds nothing).
    fn try_match_field(
        &mut self,
        fp: &IrFieldPattern,
        obj_fields: &[(Sym, Value)],
        binds: &mut Vec<(VarId, Value)>,
    ) -> bool {
        let fname = fp.name.as_str();
        let Some((_, fv)) = obj_fields.iter().find(|(k, _)| k.as_str() == fname) else {
            return false;
        };
        match &fp.pattern {
            Some(sub) => self.try_match(sub, fv, binds),
            None => true,
        }
    }

    // ── Operators ───────────────────────────────────────────────

    fn eval_binop(&mut self, op: BinOp, left: &IrExpr, right: &IrExpr, scope: &Scope) -> Flow {
        // Short-circuit logical operators evaluate the right side lazily.
        match op {
            BinOp::And => {
                let l = val!(self.eval_expr(left, scope));
                return match l {
                    Value::Bool(false) => Flow::val(Value::Bool(false)),
                    Value::Bool(true) => self.eval_expr(right, scope),
                    other => Flow::Abort(format!(
                        "internal: `and` on {}",
                        other.type_name()
                    )),
                };
            }
            BinOp::Or => {
                let l = val!(self.eval_expr(left, scope));
                return match l {
                    Value::Bool(true) => Flow::val(Value::Bool(true)),
                    Value::Bool(false) => self.eval_expr(right, scope),
                    other => Flow::Abort(format!("internal: `or` on {}", other.type_name())),
                };
            }
            _ => {}
        }

        let l = val!(self.eval_expr(left, scope));
        let r = val!(self.eval_expr(right, scope));
        // The UNSIGNED 64-bit lane (#872, C-179): a `UInt64` operand's `i64`
        // slot is a u64 BIT PATTERN, so division, remainder and ordering must
        // read it unsigned — the signed reading made `u64::MAX / 3` come out
        // 0 while both backends agreed on the right answer (the third oracle
        // dissenting from a correct consensus). Either side may be the one
        // carrying the declared type: a literal operand records `Int`.
        if matches!(left.ty, Ty::UInt64) || matches!(right.ty, Ty::UInt64) {
            if let Some(f) = Self::apply_binop_unsigned(op, &l, &r) {
                return f;
            }
        }
        // C-002, the width trap: a signed NARROW `MIN / -1` (Int8's `-128 / -1`)
        // has no representable result at its declared width, and BOTH backends
        // abort — native's `i8::checked_div` returns None, and the wasm guard
        // compares against the TRUE per-width MIN precisely because `i32.div_s`
        // itself would happily return 128. The interpreter's i64 `checked_div`
        // is that i32.div_s in disguise: it computed 128 and voted `exit=0
        // stdout=128` against a correct two-target abort consensus, the first
        // dissent the stdlib-pool coverage surfaced (int8_div_overflow began
        // voting the moment `int.from_int8` gained a body).
        if let Some(flow) = Self::narrow_div_overflow(op, &l, &r, &left.ty, &right.ty) {
            return flow;
        }
        let out = self.apply_binop(op, l, r);
        // Arithmetic on a NARROW sized integer wraps at its declared width on
        // both backends (C-180, #889) — the interpreter carries every integer
        // in one i64 like the IR does, so it has to re-wrap for the same
        // reason the renderers do, or the third oracle dissents from a correct
        // consensus (`Int8 127 + 1` = 128 here vs -128 there).
        Self::narrow_wrap_flow(out, op, &left.ty, &right.ty)
    }

    /// The one way `/` and `%` DO leave a narrow signed range: `MIN / -1`
    /// (see the C-002 caller comment). Unsigned widths cannot reach it (no
    /// negative operands) and the 64-bit case is already the i64
    /// `checked_div`/`checked_rem` None. `%` takes the same guard: native's
    /// `i8::checked_rem(-128, -1)` is None too, the same overflow question
    /// asked of the remainder machinery.
    fn narrow_div_overflow(op: BinOp, l: &Value, r: &Value, lt: &Ty, rt: &Ty) -> Option<Flow> {
        use BinOp::*;
        if !matches!(op, DivInt | ModInt) {
            return None;
        }
        let width = |t: &Ty| match t {
            Ty::Int8 => Some(8u32),
            Ty::Int16 => Some(16),
            Ty::Int32 => Some(32),
            _ => None,
        };
        let bits = width(lt).or_else(|| width(rt))?;
        let (Value::Int(a), Value::Int(b)) = (l, r) else { return None };
        let min = -(1i64 << (bits - 1));
        (*a == min && *b == -1).then(|| Flow::Abort("integer overflow".to_string()))
    }

    /// Re-wrap an arithmetic result to the DECLARED narrow width of whichever
    /// operand carries it (a literal operand records the canonical `Int`).
    /// Comparisons cannot leave the range, and `/`/`%` cannot either once the
    /// operands are in it — save for the signed `MIN / -1`, which
    /// [`Self::narrow_div_overflow`] aborts before the op runs — so `+`/`-`/`*`
    /// are wrapped — and `^`, which is
    /// repeated multiplication and wraps like it (congruent mod 2^bits, so
    /// wrapping the final value equals wrapping every step). `^` was missing
    /// from this gate exactly as it was missing from the renderers' (C-180's
    /// pow amendment): `Int32 999997 ^ 2` had the interpreter dissenting from
    /// a correct two-target consensus with the un-narrowed 999994000009.
    fn narrow_wrap_flow(flow: Flow, op: BinOp, lt: &Ty, rt: &Ty) -> Flow {
        use BinOp::*;
        if !matches!(op, AddInt | SubInt | MulInt | PowInt) {
            return flow;
        }
        let width = |t: &Ty| match t {
            Ty::Int8 => Some((8u32, true)),
            Ty::Int16 => Some((16, true)),
            Ty::Int32 => Some((32, true)),
            Ty::UInt8 => Some((8, false)),
            Ty::UInt16 => Some((16, false)),
            Ty::UInt32 => Some((32, false)),
            _ => None,
        };
        let Some((bits, signed)) = width(lt).or_else(|| width(rt)) else { return flow };
        let Flow::Value(Value::Int(v)) = flow else { return flow };
        let shift = 64 - bits;
        let wrapped = if signed { (v << shift) >> shift } else { (v as u64 & ((1u64 << bits) - 1)) as i64 };
        Flow::val(Value::Int(wrapped))
    }

    /// The `UInt64` reading of the integer operators whose signedness is
    /// observable. Add/sub/mul wrap identically under either reading and are
    /// deliberately absent; anything not listed falls through to the signed
    /// table.
    fn apply_binop_unsigned(op: BinOp, l: &Value, r: &Value) -> Option<Flow> {
        use BinOp::*;
        let (Value::Int(a), Value::Int(b)) = (l, r) else { return None };
        let (ua, ub) = (*a as u64, *b as u64);
        Some(match op {
            DivInt => match ua.checked_div(ub) {
                Some(v) => Flow::val(Value::Int(v as i64)),
                None => Flow::Abort(div_msg(*b)),
            },
            ModInt => match ua.checked_rem(ub) {
                Some(v) => Flow::val(Value::Int(v as i64)),
                None => Flow::Abort(div_msg(*b)),
            },
            Lt => Flow::val(Value::Bool(ua < ub)),
            Lte => Flow::val(Value::Bool(ua <= ub)),
            Gt => Flow::val(Value::Bool(ua > ub)),
            Gte => Flow::val(Value::Bool(ua >= ub)),
            _ => return None,
        })
    }

    /// Apply a resolved binary operator. The operator alone picks the family,
    /// so the values move straight into the arm that uses them — no operand is
    /// cloned to probe a table that will not claim it.
    pub(crate) fn apply_binop(&mut self, op: BinOp, l: Value, r: Value) -> Flow {
        use BinOp::*;
        match op {
            AddInt | SubInt | MulInt | DivInt | ModInt | PowInt => apply_binop_int(op, l, r),
            AddFloat | SubFloat | MulFloat | DivFloat | ModFloat | PowFloat => {
                apply_binop_float(op, l, r)
            }
            ConcatStr | ConcatList => self.apply_binop_concat(op, l, r),
            Eq | Neq | Lt | Gt | Lte | Gte => apply_binop_compare(op, l, r),
            // Matrix ops would dispatch to the runtime matrix bridge; not yet
            // implemented in this phase.
            MulMatrix | AddMatrix | SubMatrix | ScaleMatrix => {
                Flow::Unsupported("matrix arithmetic".into())
            }
            And | Or => unreachable!("short-circuited above"),
        }
    }

    /// `ConcatStr` / `ConcatList`. String concat is the one arithmetic form
    /// that charges determinism fuel by RESULT SIZE, so it needs `self`.
    ///
    /// An `Int` operand under these ops is a BLOCK ADDRESS by construction —
    /// the lowering already typed the concat, so `ConcatStr` on an Int means
    /// a pool body concatenated a value that flowed through the
    /// address-uniform tier (`__rx_sub(..) + rep` in the regex engine). It is
    /// coerced through the arena here; an Int that is NOT a live block keeps
    /// the loud abort, because that genuinely is an interp bug.
    fn apply_binop_concat(&mut self, op: BinOp, l: Value, r: Value) -> Flow {
        let (l, r) = match op {
            BinOp::ConcatStr => (self.coerce_block_str(l), self.coerce_block_str(r)),
            BinOp::ConcatList => (self.coerce_block_list(l), self.coerce_block_list(r)),
            _ => (l, r),
        };
        match (op, l, r) {
            (BinOp::ConcatStr, Value::Str(a), Value::Str(b)) => {
                let out = format!("{}{}", a, b);
                // T3-5 dynamic charge mirror: 1 + result_byte_len/16,
                // keyed on the same result both backends key on.
                if self.det_in_user.get() {
                    self.det_fuel
                        .set(self.det_fuel.get().wrapping_sub(1 + (out.len() as i64 >> 4)));
                    if self.det_cut() {
                        return Flow::Return(Value::Int(0));
                    }
                }
                Flow::val(Value::str(out))
            }
            (BinOp::ConcatList, a, b)
                if matches!(a, Value::List(_) | Value::Range { .. })
                    && matches!(b, Value::List(_) | Value::Range { .. }) =>
            {
                // A BOUND range concats as the list it denotes (fuzz seeds
                // 47/83/89: `[1, 2] + r` aborted here while native and the
                // wasm leg both answered the materialized list). Over-cap
                // spans abstain like index/len do — both backends take the
                // C-197 resource abort there, so a lazy answer would be a
                // wrong third vote.
                let (Some(mut v), Some(tail)) = (a.as_iter_items(), b.as_iter_items()) else {
                    return Flow::Unsupported(
                        "range concat beyond the interp materialization cap                          (both backends take the C-197 resource path)"
                            .into(),
                    );
                };
                v.extend(tail);
                Flow::val(Value::list(v))
            }
            (op, a, b) => {
                let what = if matches!(op, BinOp::ConcatStr) { "string" } else { "list" };
                Flow::Abort(format!(
                    "internal: {} concat on {} and {}",
                    what,
                    a.type_name(),
                    b.type_name()
                ))
            }
        }
    }

    fn eval_unop(&mut self, op: UnOp, v: Value) -> Flow {
        match (op, v) {
            (UnOp::NegInt, Value::Int(n)) => Flow::val(Value::Int(n.wrapping_neg())),
            (UnOp::NegFloat, Value::Float(f)) => Flow::val(Value::Float(-f)),
            (UnOp::Not, Value::Bool(b)) => Flow::val(Value::Bool(!b)),
            (op, v) => Flow::Abort(format!(
                "internal: unop {:?} on {}",
                op,
                v.type_name()
            )),
        }
    }

    // ── Helpers ─────────────────────────────────────────────────

    fn var_name(&self, id: VarId) -> String {
        if (id.0 as usize) < self.program.var_table.len() {
            self.program.var_table.get(id).name.to_string()
        } else {
            format!("v{}", id.0)
        }
    }
}

/// Insert / overwrite a key in an insertion-ordered map entry vec, matching the
/// compact-ordered-dict: existing key updates in place; new key appends.
pub(crate) fn map_insert(entries: &mut Vec<(Value, Value)>, key: Value, value: Value) {
    if let Some(slot) = entries.iter_mut().find(|(k, _)| k == &key) {
        slot.1 = value;
    } else {
        entries.push((key, value));
    }
}

fn int2(l: Value, r: Value, f: impl FnOnce(i64, i64) -> Flow) -> Flow {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => f(a, b),
        (a, b) => Flow::Abort(format!(
            "internal: int op on {} and {}",
            a.type_name(),
            b.type_name()
        )),
    }
}

fn float2(l: Value, r: Value, f: impl FnOnce(f64, f64) -> f64) -> Flow {
    match (l, r) {
        (Value::Float(a), Value::Float(b)) => Flow::val(Value::Float(f(a, b))),
        (a, b) => Flow::Abort(format!(
            "internal: float op on {} and {}",
            a.type_name(),
            b.type_name()
        )),
    }
}

/// Integer `^`, step for step with `almide_pow!` (native) and the wasm
/// `math.pow` loop: exponentiation by squaring with wrapping multiply over the
/// full i64 exponent. `None` means a NEGATIVE exponent, which has no integer
/// result and aborts on every target (#895).
fn int_pow(base: i64, exp: i64) -> Option<i64> {
    if exp < 0 {
        return None;
    }
    let (mut result, mut b, mut e) = (1i64, base, exp as u64);
    while e > 0 {
        if e & 1 == 1 {
            result = result.wrapping_mul(b);
        }
        e >>= 1;
        if e > 0 {
            b = b.wrapping_mul(b);
        }
    }
    Some(result)
}

/// The exact native abort message for a failing checked int div/mod.
fn div_msg(divisor: i64) -> String {
    if divisor == 0 {
        "division by zero".to_string()
    } else {
        "integer overflow".to_string()
    }
}

/// The signed integer operators. Native release emits bare `+`/`-`/`*` which
/// WRAP (no panic) — replicate with wrapping ops. Div/mod are total
/// (`almide_div!` / `almide_mod!`): checked_div/checked_rem, `None` aborts with
/// the exact native message.
///
/// Total pow mirrors `almide_pow!` / `almide_rt_math_pow`: exponentiation by
/// squaring over the FULL i64 exponent, wrapping multiply, and a negative
/// exponent aborts with the same message as both compiled targets (#895).
/// `wrapping_pow(b as u32)` diverged twice: it wrapped a negative exponent into
/// a huge u32 and it truncated exponents past 2^32.
fn apply_binop_int(op: BinOp, l: Value, r: Value) -> Flow {
    use BinOp::*;
    match op {
        AddInt => int2(l, r, |a, b| Flow::val(Value::Int(a.wrapping_add(b)))),
        SubInt => int2(l, r, |a, b| Flow::val(Value::Int(a.wrapping_sub(b)))),
        MulInt => int2(l, r, |a, b| Flow::val(Value::Int(a.wrapping_mul(b)))),
        DivInt => int2(l, r, |a, b| match a.checked_div(b) {
            Some(v) => Flow::val(Value::Int(v)),
            None => Flow::Abort(div_msg(b)),
        }),
        ModInt => int2(l, r, |a, b| match a.checked_rem(b) {
            Some(v) => Flow::val(Value::Int(v)),
            None => Flow::Abort(div_msg(b)),
        }),
        PowInt => int2(l, r, |a, b| match int_pow(a, b) {
            Some(v) => Flow::val(Value::Int(v)),
            None => Flow::Abort("negative exponent".to_string()),
        }),
        op => Flow::Abort(format!("internal: no rule for binop {:?}", op)),
    }
}

/// The f64 operators.
///
/// The float `**` / `^` OPERATOR is the same vendored-musl-libm transcendental
/// the MODULE path (`math.pow`, bridge.rs) already abstains on: both backends
/// route it through the vendored pow, Rust's `f64::powf` calls the PLATFORM
/// libm, and the two differ in the last ULP. The module path returned
/// `Unsupported` (an honest skip) while this operator arm silently voted with
/// the platform result — so the 3-way oracle cast a WRONG third vote and the
/// nightly fuzzer reported the disagreement as a finding (seed
/// 1785995202102876112 index 388: interp "5.340981952686458" vs both targets
/// "5.340981952686457", #924). Abstain here too — the interp does not vendor
/// the libm.
fn apply_binop_float(op: BinOp, l: Value, r: Value) -> Flow {
    use BinOp::*;
    match op {
        AddFloat => float2(l, r, |a, b| a + b),
        SubFloat => float2(l, r, |a, b| a - b),
        MulFloat => float2(l, r, |a, b| a * b),
        DivFloat => float2(l, r, |a, b| a / b),
        ModFloat => float2(l, r, |a, b| a % b),
        // The float `**` operator runs the SAME vendored musl-libm `pow` both
        // backends do (`crate::vendored_libm`) — #924's rule: a transcendental
        // reachable through an OPERATOR must agree with its module-fn spelling,
        // and both now compute the consensus algorithm instead of abstaining on
        // the platform libm's last ULP.
        PowFloat => float2(l, r, crate::vendored_libm::almide_rt_libm_pow),
        op => Flow::Abort(format!("internal: no rule for binop {:?}", op)),
    }
}

/// Equality and the four orderings.
///
/// #556 F2: a `None` from `partial_cmp_val` is the NaN case (Float
/// partial_cmp) — both backends return IEEE false for every NaN comparison
/// (`<`/`>`/`<=`/`>=`), so the interp must too, NOT abort. A genuine
/// type-mismatch ordering can't reach here: the checker rejects it, and codegen
/// never emits cross-type compares.
fn apply_binop_compare(op: BinOp, l: Value, r: Value) -> Flow {
    use BinOp::*;
    use std::cmp::Ordering;
    match op {
        Eq => return Flow::val(Value::Bool(l == r)),
        Neq => return Flow::val(Value::Bool(l != r)),
        _ => {}
    }
    let Some(ord) = l.partial_cmp_val(&r) else {
        return Flow::val(Value::Bool(false));
    };
    let res = match op {
        Lt => ord == Ordering::Less,
        Gt => ord == Ordering::Greater,
        Lte => ord != Ordering::Greater,
        Gte => ord != Ordering::Less,
        op => return Flow::Abort(format!("internal: no rule for binop {:?}", op)),
    };
    Flow::val(Value::Bool(res))
}

/// The payload a `Some` / `Ok` / `Err` pattern matches against, or `None` when
/// the value carries the wrong shape (a `Some` pattern against an `Err`, …).
fn carrier_payload<'v>(pattern: &IrPattern, value: &'v Value) -> Option<&'v Value> {
    match (pattern, value) {
        (IrPattern::Some { .. }, Value::Option(Some(v)))
        | (IrPattern::Ok { .. }, Value::Result(Ok(v)))
        | (IrPattern::Err { .. }, Value::Result(Err(v))) => Some(v.as_ref()),
        _ => None,
    }
}

/// A record-shaped value as `(constructor name, fields)`. A record literal has
/// no ctor unless it is nominal; a record-payload variant reports its ctor.
fn record_shape(value: &Value) -> Option<(Option<Sym>, &Vec<(Sym, Value)>)> {
    match value {
        Value::Record { name, fields } => Some((*name, fields)),
        Value::Variant { ctor, payload: VariantPayload::Record(fields), .. } => {
            Some((Some(*ctor), fields))
        }
        _ => None,
    }
}
