// ── eval.rs, part 4: carriers, records and access ──
//
// include!-spliced into `eval.rs` at module level next to `eval_loops_stmts.rs`
// and `eval_match.rs` (the 800-line file discipline, #1856; the `val!` macro
// and the imports are eval.rs's own). The `?` / `!` / `?.field` carrier
// operators, record literals and spread, member / index access, and string
// interpolation.

impl<'a> Interpreter<'a> {
    // ── `?` / `!` / `?.field` ───────────────────────────────────

    /// `Try`/`Unwrap` — short-circuit the enclosing fn on Err/None.
    /// `node_ty` is the marker NODE's own type: when it is `Option[...]`, the
    /// checker resolved this `!` as the effect-RESULT-layer strip on a
    /// declared-Option effect call (`f(..)! : Option[T]` — #1125, C-216). The
    /// interp's effect convention returns the raw Option, so the marker is
    /// the identity there — pass the Option through, do NOT unwrap some/none.
    fn eval_try_unwrap(&mut self, expr: &IrExpr, node_ty: &Ty, scope: &Scope) -> Flow {
        let v = val!(self.eval_expr(expr, scope));
        self.try_unwrap_value(v, node_ty)
    }

    /// The value half of [`Self::eval_try_unwrap`] — the `!`/`?` marker's
    /// normalization on an ALREADY-evaluated operand. Split out so the
    /// tail-call trampoline can fold the same normalization over a chain's
    /// final value (`run_callable`'s pending list) instead of re-implementing
    /// it: one instrument, two call sites.
    pub(crate) fn try_unwrap_value(&mut self, v: Value, node_ty: &Ty) -> Flow {
        self.try_unwrap_value_flag(v, marker_is_option_identity(node_ty))
    }

    /// The flag form: the only fact `!` normalization reads off the marker
    /// node's type is "is it the C-216 Option identity" — one bit, so the
    /// trampoline carries the bit instead of cloning a `Ty` per hop
    /// (#1232, the last quick-win row).
    pub(crate) fn try_unwrap_value_flag(&mut self, v: Value, opt_identity: bool) -> Flow {
        if opt_identity {
            if let Value::Option(_) = v {
                return Flow::val(v);
            }
        }
        match v {
            Value::Result(Ok(inner)) => Flow::val(*inner),
            Value::Result(Err(e)) => Flow::Return(Value::Result(Err(e))),
            Value::Option(Some(inner)) => Flow::val(*inner),
            // #556: `expr!` on a None propagates an Err whose message is
            // "none" on BOTH backends (the codegen lowers Option `!` to
            // `ok_or("none")?`). Returning a bare Option(None) made the
            // main-error path print the Rust-internal "called
            // Option::unwrap() on a None value" — a wrong third vote
            // against the native==wasm "Error: none".
            Value::Option(None) => {
                Flow::Return(Value::Result(Err(Box::new(Value::str("none".to_string())))))
            }
            other => Flow::val(other),
        }
    }

    fn eval_optional_chain(&mut self, expr: &IrExpr, field: Sym, scope: &Scope) -> Flow {
        let v = val!(self.eval_expr(expr, scope));
        match v {
            Value::Option(None) => Flow::val(Value::Option(None)),
            Value::Option(Some(inner)) => match self.eval_member(*inner, field) {
                Flow::Value(m) => Flow::val(Value::Option(Some(Box::new(m)))),
                other => other,
            },
            other => match self.eval_member(other, field) {
                Flow::Value(m) => Flow::val(Value::Option(Some(Box::new(m)))),
                other => other,
            },
        }
    }

    // ── Record literal / spread ────────────────────────────────

    fn eval_record_literal(
        &mut self,
        name: &Option<Sym>,
        fields: &[(Sym, IrExpr)],
        ty: &Ty,
        scope: &Scope,
    ) -> Flow {
        let mut out = Vec::with_capacity(fields.len());
        for (k, v) in fields {
            out.push((*k, val!(self.eval_expr(v, scope))));
        }
        // A record-shaped node whose `name` is a registered
        // record-variant constructor builds a `Variant` (so it
        // equality- / pattern-matches as a variant). A plain record
        // type stays a `Record`. Empirically (probe /tmp/repr_probe),
        // both display identically as `Name { f: v }`.
        if let Some(n) = name {
            if let Some((ty_name, crate::dispatch::CtorKind::Record)) = self.variant_ctor(*n) {
                out = match self.fill_record_defaults(*n, out, scope) {
                    Ok(filled) => filled,
                    Err(flow) => return flow,
                };
                return Flow::val(Value::Variant {
                    ty: Some(ty_name),
                    ctor: *n,
                    payload: VariantPayload::Record(out),
                });
            }
        }
        // Recover the displayed shape exactly as the codegen walker does
        // (walker/expressions.rs:511-530, walker/types.rs:111). A record
        // LITERAL carries no inline `name` when its nominal type comes
        // from an annotation/inference — the name must be recovered from
        // the expression's type. Three cases, in the walker's order:
        //   1. `expr.ty == Ty::Named(n, _)`  → the nominal name `n`,
        //      fields in literal (declaration) order.
        //   2. `expr.ty == Ty::Record/OpenRecord` whose field-name set
        //      matches a registered NAMED record type (e.g. a nested
        //      list element `[{ val: 2, kids: [] }]` whose element type
        //      was inferred structurally) → that type's name, fields
        //      reordered to the type's DECLARATION order.
        //   3. A genuinely ANONYMOUS record → no name; the native
        //      synthesized struct stores fields in SORTED name order, so
        //      sort here to match the backends' repr.
        let resolved_name;
        if let Some(n) = name {
            resolved_name = Some(*n);
        } else {
            match ty {
                Ty::Named(n, _) => resolved_name = Some(*n),
                Ty::Record { .. } | Ty::OpenRecord { .. } => {
                    let mut key: Vec<Sym> = out.iter().map(|(k, _)| *k).collect();
                    key.sort();
                    if let Some((ty_name, decl_order)) = self.named_records.get(&key).cloned() {
                        // Case 2: reorder fields to declaration order.
                        let mut reordered = Vec::with_capacity(out.len());
                        for field in &decl_order {
                            if let Some(pos) = out.iter().position(|(k, _)| k == field) {
                                reordered.push(out.swap_remove(pos));
                            }
                        }
                        reordered.extend(out.drain(..));
                        out = reordered;
                        resolved_name = Some(ty_name);
                    } else {
                        // Case 3: true anonymous record → sorted fields.
                        out.sort_by(|a, b| a.0.cmp(&b.0));
                        resolved_name = None;
                    }
                }
                _ => resolved_name = None,
            }
        }
        if let Some(n) = resolved_name {
            out = match self.fill_record_defaults(n, out, scope) {
                Ok(filled) => filled,
                Err(flow) => return flow,
            };
        }
        Flow::val(Value::Record { name: resolved_name, fields: Rc::new(out) })
    }

    /// The interp-side twin of codegen's `default_fields` pass: a literal for
    /// a declared record type (or record-variant ctor) that omits defaulted
    /// fields still constructs them, and the whole record comes out in
    /// DECLARATION order — exactly the struct both backends build. A name
    /// with no known declaration (an anonymous record) passes through
    /// untouched.
    fn fill_record_defaults(
        &mut self,
        key: Sym,
        mut out: Vec<(Sym, Value)>,
        scope: &Scope,
    ) -> Result<Vec<(Sym, Value)>, Flow> {
        let Some(decl) = self.record_decls.get(&key).copied() else {
            return Ok(out);
        };
        // ALWAYS rebuild in DECLARATION order, defaults or not: a permuted
        // literal (`ERec { y: 2, x: 1 }`) must equal the declared-order value
        // — both backends normalize at lowering, and the payload Vec's
        // PartialEq is positional, so the old defaults-only early return made
        // the interp dissent `ne` against a native==wasm `eq` (caught by the
        // 3-way oracle while graduating variant_record_literal_equality).
        let mut filled = Vec::with_capacity(decl.len());
        for f in decl {
            if let Some(pos) = out.iter().position(|(k, _)| *k == f.name) {
                filled.push(out.swap_remove(pos));
            } else if let Some(def) = &f.default {
                match self.eval_expr(def, scope) {
                    Flow::Value(v) => filled.push((f.name, v)),
                    other => return Err(other),
                }
            }
        }
        // Anything the literal wrote beyond the declaration (open-record
        // extras) keeps its literal position after the declared fields.
        filled.extend(out);
        Ok(filled)
    }

    fn eval_spread_record(&mut self, base: &IrExpr, fields: &[(Sym, IrExpr)], scope: &Scope) -> Flow {
        let base_v = val!(self.eval_expr(base, scope));
        let (name, mut merged) = match base_v {
            Value::Record { name, fields } => (name, (*fields).clone()),
            other => {
                return Flow::Abort(format!(
                    "internal: spread base is {} not Record",
                    other.type_name()
                ))
            }
        };
        for (k, v) in fields {
            let vv = val!(self.eval_expr(v, scope));
            if let Some(slot) = merged.iter_mut().find(|(fk, _)| fk == k) {
                slot.1 = vv;
            } else {
                merged.push((*k, vv));
            }
        }
        Flow::val(Value::Record { name, fields: Rc::new(merged) })
    }

    // ── Member / index access ──────────────────────────────────

    fn eval_member(&mut self, object: Value, field: Sym) -> Flow {
        match object {
            Value::Record { fields, .. } => {
                match fields.iter().find(|(k, _)| *k == field) {
                    Some((_, v)) => Flow::val(v.clone()),
                    None => Flow::Abort(format!("internal: no field `{}` on record", field)),
                }
            }
            Value::Variant { payload: VariantPayload::Record(fields), .. } => {
                match fields.iter().find(|(k, _)| *k == field) {
                    Some((_, v)) => Flow::val(v.clone()),
                    None => Flow::Abort(format!("internal: no field `{}` on variant", field)),
                }
            }
            other => Flow::Abort(format!(
                "internal: member access `.{}` on {}",
                field,
                other.type_name()
            )),
        }
    }

    fn eval_index(&mut self, object: Value, index: Value) -> Flow {
        let i = match index {
            Value::Int(i) => i,
            other => {
                return Flow::Abort(format!(
                    "internal: list index is {} not Int",
                    other.type_name()
                ))
            }
        };
        match object {
            Value::List(xs) => {
                if i < 0 || (i as usize) >= xs.len() {
                    // Matches the codegen OOB contract: abort + exit 1.
                    Flow::Abort("index out of bounds".into())
                } else {
                    Flow::val(xs[i as usize].clone())
                }
            }
            // A Range indexes like the list it stands for: `(0..<5)[2] == 2`. Both
            // backends materialize a `let`-bound range that is indexed (only the
            // head-ONLY case skips materialization, #1400), so the interp must
            // agree rather than dissent — it just computes the element instead of
            // building the block. Bounds match `Value::List` above: the codegen OOB
            // contract is abort + exit 1.
            Value::Range { start, end, inclusive } => {
                let len = if inclusive {
                    (end.saturating_sub(start)).saturating_add(1)
                } else {
                    end.saturating_sub(start)
                };
                // An over-cap span is the C-197 resource edge: both backends
                // MATERIALIZE an indexed bound range and take the defined OOM
                // abort there, so a lazily computed element would be a wrong
                // third vote — abstain (same cap as `as_iter_items`).
                if len > 16 * 1024 * 1024 {
                    return Flow::Unsupported(
                        "indexed range beyond the interp materialization cap                          (both backends take the C-197 resource path)"
                            .into(),
                    );
                }
                if i < 0 || len <= 0 || i >= len {
                    Flow::Abort("index out of bounds".into())
                } else {
                    Flow::val(Value::Int(start + i))
                }
            }
            Value::Str(s) => {
                // String indexing returns the byte? Almide indexes strings via
                // string.* fns; a bare index on a String is unusual. Treat as
                // unsupported to avoid a wrong third vote.
                let _ = s;
                Flow::Unsupported("string index access".into())
            }
            // Address-model list (#1700): inside the pool tier the binding
            // holds a slot-block ADDRESS (see exec_stmt_index_assign's twin
            // arm). Element count in the len header, 8-byte Int slots.
            Value::Int(addr) => {
                let Ok(base) = u32::try_from(addr) else {
                    return Flow::Abort("index out of bounds".into());
                };
                let Some(count) = self.heap.load(base + almide_layout::LEN.offset, 4) else {
                    return Flow::Abort(format!("internal: index access on Int (v-addr {addr})"));
                };
                if i < 0 || i >= count {
                    return Flow::Abort("index out of bounds".into());
                }
                match self.heap.load(base + almide_layout::PAYLOAD + (i as u32) * 8, 8) {
                    Some(v) => Flow::val(Value::Int(v)),
                    None => Flow::Abort("index out of bounds".into()),
                }
            }
            other => Flow::Abort(format!(
                "internal: index access on {}",
                other.type_name()
            )),
        }
    }

    // ── String interpolation ───────────────────────────────────

    fn eval_string_interp(&mut self, parts: &[IrStringPart], scope: &Scope) -> Flow {
        let mut out = String::new();
        for part in parts {
            match part {
                IrStringPart::Lit { value } => out.push_str(value),
                IrStringPart::Expr { expr } => {
                    let v = val!(self.eval_expr(expr, scope));
                    // A bare top-level String stays raw; everything else routes
                    // through the bare-display path (which for compounds is
                    // `almide_repr`, for scalars is plain Display).
                    out.push_str(&v.display_bare());
                }
            }
        }
        Flow::val(Value::str(out))
    }
}
