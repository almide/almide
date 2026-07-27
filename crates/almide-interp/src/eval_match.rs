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
        Flow::Abort("internal: non-exhaustive match (no arm matched)".into())
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
            IrPattern::List { elements } => match value.as_iter_items() {
                Some(items) if items.len() == elements.len() => elements
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
            IrPattern::Some { inner } => match value {
                Value::Option(Some(v)) => self.try_match(inner, v, binds),
                _ => false,
            },
            IrPattern::None => matches!(value, Value::Option(None)),
            IrPattern::Ok { inner } => match value {
                Value::Result(Ok(v)) => self.try_match(inner, v, binds),
                _ => false,
            },
            IrPattern::Err { inner } => match value {
                Value::Result(Err(v)) => self.try_match(inner, v, binds),
                _ => false,
            },
            IrPattern::Constructor { name, args } => match value {
                Value::Variant { ctor, payload, .. } if ctor.as_str() == name => match payload {
                    VariantPayload::Unit => args.is_empty(),
                    VariantPayload::Tuple(items) if items.len() == args.len() => args
                        .iter()
                        .zip(items.iter())
                        .all(|(p, v)| self.try_match(p, v, binds)),
                    _ => false,
                },
                _ => false,
            },
            IrPattern::RecordPattern { name, fields, rest } => {
                self.try_match_record(name, fields, *rest, value, binds)
            }
            _ => return None,
        })
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
        let (obj_name, obj_fields): (Option<Sym>, &Vec<(Sym, Value)>) = match value {
            Value::Record { name, fields } => (*name, fields),
            Value::Variant { ctor, payload: VariantPayload::Record(fields), .. } => {
                (Some(*ctor), fields)
            }
            _ => return false,
        };
        // Name must match when the pattern names a constructor.
        if !name.is_empty() {
            match obj_name {
                Some(n) if n.as_str() == name => {}
                _ => return false,
            }
        }
        if !rest && fields.len() != obj_fields.len() {
            return false;
        }
        for fp in fields {
            let fname = fp.name.as_str();
            let fv = match obj_fields.iter().find(|(k, _)| k.as_str() == fname) {
                Some((_, v)) => v,
                None => return false,
            };
            match &fp.pattern {
                // Shorthand `{ x, y }` lowers to explicit `Bind`
                // sub-patterns (verified via IR dump), so binding is
                // handled here uniformly.
                Some(sub) => {
                    if !self.try_match(sub, fv, binds) {
                        return false;
                    }
                }
                // A field with no sub-pattern is a structural-only
                // match (the field must exist, but binds nothing).
                None => {}
            }
        }
        true
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
        let out = self.apply_binop(op, l, r);
        // Arithmetic on a NARROW sized integer wraps at its declared width on
        // both backends (C-180, #889) — the interpreter carries every integer
        // in one i64 like the IR does, so it has to re-wrap for the same
        // reason the renderers do, or the third oracle dissents from a correct
        // consensus (`Int8 127 + 1` = 128 here vs -128 there).
        Self::narrow_wrap_flow(out, op, &left.ty, &right.ty)
    }

    /// Re-wrap an arithmetic result to the DECLARED narrow width of whichever
    /// operand carries it (a literal operand records the canonical `Int`).
    /// Comparisons cannot leave the range, and `/`/`%` cannot either once the
    /// operands are in it, so only `+`/`-`/`*` are wrapped.
    fn narrow_wrap_flow(flow: Flow, op: BinOp, lt: &Ty, rt: &Ty) -> Flow {
        use BinOp::*;
        if !matches!(op, AddInt | SubInt | MulInt) {
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

    pub(crate) fn apply_binop(&mut self, op: BinOp, l: Value, r: Value) -> Flow {
        // `l`/`r` are moved into the half that claims the operator, so the first
        // attempt gets clones and the second gets the originals.
        if let Some(f) = self.apply_binop_a(op, l.clone(), r.clone()) {
            return f;
        }
        self.apply_binop_b(op, l, r)
            .unwrap_or_else(|| Flow::Abort(format!("internal: no rule for binop {:?}", op)))
    }

    /// The first half of `apply_binop`'s arm table.
    ///
    /// Extracted from `apply_binop` (arm-table halving): arms verbatim and in
    /// source order, so the router's order is the only ordering that matters.
    pub(crate) fn apply_binop_a(&mut self, op: BinOp, l: Value, r: Value) -> Option<Flow> {
        use BinOp::*;
        Some(match op {
            // Integer arithmetic. Native release emits bare `+`/`-`/`*` which
            // WRAP (no panic) — replicate with wrapping ops.
            AddInt => int2(l, r, |a, b| Flow::val(Value::Int(a.wrapping_add(b)))),
            SubInt => int2(l, r, |a, b| Flow::val(Value::Int(a.wrapping_sub(b)))),
            MulInt => int2(l, r, |a, b| Flow::val(Value::Int(a.wrapping_mul(b)))),
            // Total div / mod: `almide_div!` / `almide_mod!` semantics —
            // checked_div/checked_rem, None → abort with the exact native msg.
            DivInt => int2(l, r, |a, b| match a.checked_div(b) {
                Some(v) => Flow::val(Value::Int(v)),
                None => Flow::Abort(div_msg(b)),
            }),
            ModInt => int2(l, r, |a, b| match a.checked_rem(b) {
                Some(v) => Flow::val(Value::Int(v)),
                None => Flow::Abort(div_msg(b)),
            }),
            // Total pow: `almide_pow!` / `almide_rt_math_pow` semantics —
            // exponentiation by squaring over the FULL i64 exponent, wrapping
            // multiply, and a negative exponent aborts with the same message as
            // both compiled targets (#895). `wrapping_pow(b as u32)` diverged
            // twice: it wrapped a negative exponent into a huge u32 and it
            // truncated exponents past 2^32.
            PowInt => int2(l, r, |a, b| match int_pow(a, b) {
                Some(v) => Flow::val(Value::Int(v)),
                None => Flow::Abort("negative exponent".to_string()),
            }),

            AddFloat => float2(l, r, |a, b| a + b),
            SubFloat => float2(l, r, |a, b| a - b),
            MulFloat => float2(l, r, |a, b| a * b),
            _ => return None,
        })
    }

    /// The second half of `apply_binop`'s arm table.
    ///
    /// Extracted from `apply_binop` (arm-table halving): arms verbatim and in
    /// source order, so the router's order is the only ordering that matters.
    pub(crate) fn apply_binop_b(&mut self, op: BinOp, l: Value, r: Value) -> Option<Flow> {
        use BinOp::*;
        Some(match op {
            DivFloat => float2(l, r, |a, b| a / b),
            ModFloat => float2(l, r, |a, b| a % b),
            PowFloat => float2(l, r, |a, b| a.powf(b)),

            ConcatStr => match (l, r) {
                (Value::Str(a), Value::Str(b)) => {
                    Flow::val(Value::str(format!("{}{}", a, b)))
                }
                (a, b) => Flow::Abort(format!(
                    "internal: string concat on {} and {}",
                    a.type_name(),
                    b.type_name()
                )),
            },
            ConcatList => match (l, r) {
                (Value::List(a), Value::List(b)) => {
                    let mut v = (*a).clone();
                    v.extend((*b).clone());
                    Flow::val(Value::list(v))
                }
                (a, b) => Flow::Abort(format!(
                    "internal: list concat on {} and {}",
                    a.type_name(),
                    b.type_name()
                )),
            },

            Eq => Flow::val(Value::Bool(l == r)),
            Neq => Flow::val(Value::Bool(l != r)),
            Lt | Gt | Lte | Gte => match l.partial_cmp_val(&r) {
                Some(ord) => {
                    let res = match op {
                        Lt => ord == std::cmp::Ordering::Less,
                        Gt => ord == std::cmp::Ordering::Greater,
                        Lte => ord != std::cmp::Ordering::Greater,
                        Gte => ord != std::cmp::Ordering::Less,
                        _ => unreachable!(),
                    };
                    Flow::val(Value::Bool(res))
                }
                // #556 F2: a None here is the NaN case (Float partial_cmp) —
                // both backends return IEEE false for every NaN comparison
                // (`<`/`>`/`<=`/`>=`), so the interp must too, NOT abort. A
                // genuine type-mismatch ordering can't reach here: the checker
                // rejects it, and codegen never emits cross-type compares.
                None => Flow::val(Value::Bool(false)),
            },

            And | Or => unreachable!("short-circuited above"),

            // Matrix ops would dispatch to the runtime matrix bridge; not yet
            // implemented in this phase.
            MulMatrix | AddMatrix | SubMatrix | ScaleMatrix => {
                Flow::Unsupported("matrix arithmetic".into())
            }
            _ => return None,
        })
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
