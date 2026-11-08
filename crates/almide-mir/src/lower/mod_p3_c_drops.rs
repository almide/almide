// the scope-end drop ops of LowerCtx, include!-spliced back from mod_p3_c.rs at
// module level (a code move under the 800-line ceiling the codopsy gate holds this
// crate to).

impl LowerCtx {
    /// Guard-clause flattening of the former (nested, then outer) else-if chains — every branch
    /// still returns the SAME `Op` value it evaluated to before, just via an early `return`
    /// instead of an `if`/`else if` tail expression. No behavior change — see
    /// docs/roadmap/active/code-health-codopsy.md.
    pub(crate) fn drop_op_for(&self, v: ValueId) -> Op {
        if let Some(op) = self.variant_drop_route(v) {
            return op;
        }
        if let Some(op) = self.set_drop_route(v) {
            return op;
        }
        if self.closure_values.contains(&v) {
            // A CLOSURE BLOCK frees through the uniform, SELF-DESCRIBING
            // `$__drop_closure` (fixed runtime): at the last ref it reads the drop
            // header (slot 1), recursively drops the captured-closure slots, rc_decs
            // the captured-heap slots, and NEVER touches slot 0 (the fnidx — a table
            // index, not a pointer). Works for any closure value regardless of where
            // it was created (a call-result's captures are unknowable here).
            return Op::DropVariant { v, ty: "closure".to_string() };
        }
        Op::Drop { v }
    }

    /// The drop route recorded on a tracked VARIANT handle. Its `ty` string is a
    /// tagged spelling: a bare type name is a real user ADT, while the
    /// `optrec:` / `resrec:` / `reserr:` prefixes and the two `list_*_*` names
    /// select a dedicated inline wrapper drop instead.
    fn variant_drop_route(&self, v: ValueId) -> Option<Op> {
        let ty = self.value_drops.get(&v).and_then(|d| d.named_route.as_ref())?;
            // `List[(Int, String)]` was routed here as a pseudo-"variant" but has no generated
            // `$__drop_list_int_str` ADT helper (the `DropVariant` render emitted a dangling call →
            // invalid wat). Route it to the dedicated INLINE `DropListIntStr` (frees each tuple's
            // String slot + block, then the list). Every real user-ADT variant keeps `DropVariant`.
            if ty == "list_int_str" {
                return Some(Op::DropListIntStr { v });
            }
            if ty == "list_str_int" {
                return Some(Op::DropListStrInt { v });
            }
            if let Some(drop_fn) = ty.strip_prefix("optrec:") {
                // An Option WRAPPER holding a heap RECORD payload (`some({key, val})`): recurse into
                // the @12 record via `$__drop_<drop_fn>` at the wrapper's last ref, then free the
                // wrapper block. The `optrec:` prefix is injected by `materialize_opt_aggregate_some`.
                return Some(Op::DropWrapperRec {
                    v,
                    drop_fn: drop_fn.to_string(),
                    is_result: false,
                    err_rec: false,
                });
            }
            if let Some(drop_fn) = ty.strip_prefix("resrec:") {
                // A Result WRAPPER holding a heap RECORD Ok payload (`ok({val, next})`): recurse into
                // the @12 record (tag@16==0) via `$__drop_<drop_fn>`, else `rc_dec` the @12 Err
                // String, then free the wrapper. Injected by `materialize_result_aggregate`.
                return Some(Op::DropWrapperRec {
                    v,
                    drop_fn: drop_fn.to_string(),
                    is_result: true,
                    err_rec: false,
                });
            }
            if let Some(drop_fn) = ty.strip_prefix("reserr:") {
                // The heap-Ok × variant-ERR wrapper (`Result[String, MathError]` — the
                // `err(NegativeInput(x))` class): recurse into the @12 VARIANT (tag@16==1)
                // via `$__drop_<drop_fn>`, else `rc_dec` the @12 Ok payload, then free the
                // wrapper. Injected by `try_lower_result_err_variant_ctor_heap_ok` and the
                // both-heap `seed_variant_param` branch (rich-variant Err types).
                return Some(Op::DropWrapperRec {
                    v,
                    drop_fn: drop_fn.to_string(),
                    is_result: true,
                    err_rec: true,
                });
            }
        Some(Op::DropVariant { v, ty: ty.clone() })
    }

    /// The set-membership drop routes, MOST SPECIFIC FIRST — the order is
    /// load-bearing: `List[List[String]]` must be tried before the generic
    /// heap-element list (which also matches its shape and would leak the inner
    /// rows' cell Strings), and each Result/Option wrapper before the flat list.
    fn set_drop_route(&self, v: ValueId) -> Option<Op> {
        let routes: [(bool, fn(ValueId) -> Op); 12] = [
            (self.value_drops.get(&v).is_some_and(|d| d.value_result_list), |v| Op::DropResultListValue { v }),
            (self.value_drops.get(&v).is_some_and(|d| d.value_result), |v| Op::DropResultValue { v }),
            (self.value_drops.get(&v).is_some_and(|d| d.str_int_result), |v| Op::DropResultStrInt { v }),
            (self.value_drops.get(&v).is_some_and(|d| d.value_int_result), |v| Op::DropResultValueInt { v }),
            (self.value_drops.get(&v).is_some_and(|d| d.list_value_int_result), |v| Op::DropResultListValueInt { v }),
            (self.value_drops.get(&v).is_some_and(|d| d.list_str_int_result), |v| Op::DropResultListStrInt { v }),
            (self.value_drops.get(&v).is_some_and(|d| d.list_str_result), |v| Op::DropResultListStr { v }),
            (self.value_elem_lists.contains(&v), |v| Op::DropListValue { v }),
            (self.str_value_elem_lists.contains(&v), |v| Op::DropListStrValue { v }),
            (self.value_drops.get(&v).is_some_and(|d| d.str_str_elems), |v| Op::DropListStrStr { v }),
            (self.value_drops.get(&v).is_some_and(|d| d.list_list_str), |v| Op::DropListListStr { v }),
            (
                self.value_drops.get(&v).is_some_and(|d| d.flat_elems) || self.record_masks.contains_key(&v),
                |v| Op::DropListStr { v },
            ),
        ];
        let (_, make) = routes.into_iter().find(|(hit, _)| *hit)?;
        Some(make(v))
    }

    pub(crate) fn emit_scope_end_drops(&mut self) {
        // Reverse binding order (LIFO scope teardown). A `List[String]` value is released by a
        // RECURSIVE `DropListStr` (frees its owned element Strings); every other heap value by
        // a flat `Drop`.
        let drops: Vec<Op> =
            self.live_heap_handles.iter().rev().map(|v| self.drop_op_for(*v)).collect();
        self.ops.extend(drops);
    }
}
