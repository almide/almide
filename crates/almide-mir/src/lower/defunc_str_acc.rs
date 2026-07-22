impl LowerCtx {
    /// C1 DEFUNCTIONALIZATION for `list.flat_map` / `list.filter_map` over a `List[String]` source
    /// producing a `List[String]` — the toml `emit_sections` / dojo `hints_block` shapes. Each
    /// element's closure body produces a SUBLIST (`flat_map` → `List[String]`; `filter_map` → the
    /// 0-or-1-element `Option[String]`, physically the SAME `DynListStr`-of-Strings), which is
    /// CONCATENATED onto a loop-carried `List[String]` ACCUMULATOR via the proven `__list_concat_rc`
    /// drop-old + SetLocal slot.
    ///
    /// SOUNDNESS by REUSE — the accumulator is the exact `i(id)m` loop-carried slot the heap `fold`
    /// arm proves (`OwnershipChecker.v check_line_unroll_sound`): seeded with a fresh empty `List`
    /// (`i`), each iteration `acc = __list_concat_rc(acc, sub)` is a drop-old (`d`) + SetLocal (folds
    /// the concat's `i` into the slot) — a refcount-preserving body, leak/double-free-free for any
    /// iteration count. The per-element SUBLIST is lowered as a SYNTHETIC `let` (an OWNED tracked temp
    /// — NOT a moved-out `Consume`), so my explicit `drop_arm_locals` frees it (`id`) within the
    /// iteration; `__list_concat_rc` BORROWS both args (rc-incs each element into the new list), so
    /// after the drops the elements are single-owned by `acc`. A body the let-bind subset cannot lower
    /// returns `None` → the whole HOF rolls back and the caller WALLs (caps honest).
    fn lower_defunc_str_acc_hof(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        // The closure body returns `Option[String]` (filter_map) or `List[String]` (flat_map) — both a
        // `DynListStr` of Strings. Bail on an obvious non-heap body (the walker re-checks per leaf).
        if !is_heap_ty(&body.ty) {
            return None;
        }

        // Borrow the source list (evaluated once); a non-handle iterable is out of subset.
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok()?.into_iter().next()? {
            CallArg::Handle(v) => v,
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

        // Seed the ACCUMULATOR: a fresh EMPTY `List[String]` (an i32 Alloc dst). This is the
        // loop-carried slot — reassigned in place via SetLocal each iteration (the proven i(id)m slot).
        // NOT registered for drop here: the slot owns it; the loop's drop-old or the caller's scope-end
        // drop frees it exactly once. Marked `heap_elem_lists` so EVERY drop of it (drop-old + the
        // caller's scope-end) is the recursive `DropListStr` (frees its owned element Strings).
        let zero_len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero_len, value: 0 });
        let acc = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: acc,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: zero_len },
        });
        // The acc's drop grain follows the ELEMENT class: a matrix-shaped sublist type
        // (`List[Matrix]` — repeat_kv) needs the nested DropListListStr sweep; a String
        // element the flat per-slot DropListStr.
        if crate::lower::is_list_list_str_ty(&body.ty) {
            self.list_list_str_lists.insert(acc);
        } else {
            self.heap_elem_lists.insert(acc);
        }

        // The loop index (stable mutable i64 local) + the +1 step constant.
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // Load element[i] from the SOURCE list: addr = src_h + 12 + i*8. The READ WIDTH depends on the
        // SOURCE element type, NOT the heap String OUTPUT this HOF accumulates: a HEAP source
        // (`List[String]`/`List[Value]`) element is the slot's HANDLE (`LoadHandle` = i32 Ptr, a
        // BORROWED heap value the body reads); a SCALAR source (`List[Int]` — e.g. `filter_map((x:Int)
        // => Some(int.to_string(x)))`) element is the i64 VALUE (`Load { width: 8 }`). Hardcoding
        // `LoadHandle` for a scalar source loaded each i64 Int element as an i32, corrupting every i64
        // op consuming it (`i64.rem_s`/`i64.gt_s`/`i64.eq`/`int.to_string`) → invalid wasm (the
        // filter_map heap-result-element-load-width holes). Mirrors `lower_defunc_list_hof_inner`'s
        // `src_heap` element read.
        let src_heap = matches!(&xs.ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0]));
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let src_base = self.load_addr(h, 12);
        let src_addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: src_addr, op: IntOp::Add, a: src_base, b: i8_v });
        let elem = self.fresh_value();
        let read_kind = if src_heap { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
        self.ops.push(Op::Prim { kind: read_kind, dst: Some(elem), args: vec![src_addr] });

        // Bind the lambda PARAM (the element) to the loaded value/handle. CAPTURES resolve through
        // `value_of`. Only a HEAP element is a BORROW (`param_values`) — the source list owns it, so a
        // body that MOVES it out (`some(elem)`) auto-acquires its own ref. A SCALAR element is a plain
        // i64 value (no ownership, no `param_values`) — registering it as a borrow would mis-route its
        // (non-existent) drop.
        self.value_of.insert(params[0].0, elem);
        if src_heap {
            self.param_values.insert(elem);
        }

        // Lower the closure BODY by PUSHING its control flow (if/match/block) DOWN and APPENDING each
        // terminal sublist to the loop-carried `acc` slot — NEVER binding a merged-if heap value (which
        // the flat certificate cannot drop soundly — `im·im·d`; see `lower_bind`). Each leaf is appended
        // via the proven `acc = acc + <leaf>` slot reassignment (drop-old + SetLocal = `i(id)`). A leaf
        // the subset cannot lower → `None` → the whole HOF rolls back and the caller WALLs.
        self.in_frame += 1;
        self.in_defunc_body += 1;
        let ok = self.append_body_to_str_acc(body, acc).is_some();
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        if !ok {
            return None;
        }

        // Advance the index and close the loop.
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);

        // The accumulator's final value is an OWNED `List[String]` returned to the caller, which
        // registers it for the outer scope-end drop (C1 does NOT push it itself — that would
        // double-drop). It is already in `heap_elem_lists` so that drop is the recursive `DropListStr`.
        Some(acc)
    }

    /// C1 DEFUNCTIONALIZATION for a `list.filter_map` building a HEAP-but-non-String element list
    /// (`List[record]`/`List[Value]`/`List[(String,Value)]`) — the dojo `backfill_dir` shape
    /// `task_files |> list.filter_map((f) => match fs.read_text(dir+"/"+f) { ok(c) =>
    /// some(parse_task_md(f, c)), err(_) => none })`. A write-cursor result list (like `filter`)
    /// combined with a keep/skip VARIANT match (like the str-acc path, but the kept element is an
    /// OWNED record/Value MOVED into the cursor slot instead of a String appended to an accumulator).
    ///
    /// The per-element body MUST be a 2-arm variant `match subj { … }` over a self-host Option/Result
    /// CALL subject (`append_variant_match_to_result_list`): the keep arm yields `some(<elem>)` (build
    /// the element, store at the cursor, bump), the skip arm yields `none` (no-op). A non-match body,
    /// a non-self-host subject, or an out-of-subset element returns `None` → the caller rolls back +
    /// WALLs.
    ///
    /// SOUNDNESS by REUSE: the result list is alloc'd once at `len(xs)` (the MAX) with its real length
    /// patched to the write-cursor after the loop (exactly `filter`, control_p5 `filter` arm); each
    /// KEPT element is a FRESH OWNED record/Value (`lower_heap_result_arm`, cert `i`) MOVED into the
    /// slot (`Consume` = `m`), the list's recursive `DropList*` freeing all `cursor` elements at scope
    /// end — the proven capturing-filter conditional-acquire (5a0a9efb). The per-iteration subject is a
    /// balanced `i…d` episode (dropped after the arms), its payload borrowed through the keep arm.
    fn lower_defunc_filter_map_hof(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        result_elem: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        // The body is a 2-arm variant match (the keep/skip decision), OR a BLOCK whose leading lets feed
        // the tail match (`(k) => { let val = json.get_string(obj, k); match val { … } }` — porta
        // load_porta_config's CAPTURING `secrets` filter_map). The leading lets are lowered per-iteration
        // AFTER the element param is bound (captures resolve via value_of), BEFORE the match arms. Defer
        // anything else.
        let (lead_stmts, subject, arms): (&[almide_ir::IrStmt], &IrExpr, &[IrMatchArm]) = match &body.kind
        {
            IrExprKind::Match { subject, arms } if is_variant_ty(&subject.ty) => {
                (&[], subject.as_ref(), arms.as_slice())
            }
            IrExprKind::Block { stmts, expr: Some(tail) } => match &tail.kind {
                IrExprKind::Match { subject, arms } if is_variant_ty(&subject.ty) => {
                    (stmts.as_slice(), subject.as_ref(), arms.as_slice())
                }
                _ => return None,
            },
            _ => return None,
        };

        // Borrow the source list (evaluated once); a non-handle iterable is out of subset.
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok()?.into_iter().next()? {
            CallArg::Handle(v) => v,
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

        // A fresh OWNED `DynList` of `len(xs)` slots (the MAX; the real length is patched to the
        // write-cursor after the loop). The recursive scope-end drop set follows the element type,
        // exactly like the `map` heap-element result: a `(String, Value)` tuple → DropListStrValue, a
        // dynamic Value → DropListValue, else (a String or a record handle) → the recursive DropListStr
        // (rc_dec each owned element handle — the convention the C2-lifted record `map` already uses).
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: len_v },
        });
        let rh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(rh), args: vec![dst] });
        let result_is_str_value_tuple = matches!(result_elem,
            Ty::Tuple(tys) if tys.len() == 2
                && matches!(tys[0], Ty::String) && crate::lower::is_value_ty(&tys[1]));
        if result_is_str_value_tuple {
            self.str_value_elem_lists.insert(dst);
        } else if crate::lower::is_value_ty(result_elem) {
            self.value_elem_lists.insert(dst);
        } else {
            self.heap_elem_lists.insert(dst);
        }
        // The write-cursor (count of kept elements) — a stable mutable local.
        let cursor = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: cursor, value: 0 });

        // The loop index (stable mutable i64 local) + the +1 step constant.
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // Load element[i] from the SOURCE list (a handle for a heap source, an i64 value for a scalar
        // source) — mirrors `lower_defunc_list_hof_inner`'s `src_heap` element read.
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        let i8_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let src_base = self.load_addr(h, 12);
        let src_addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: src_addr, op: IntOp::Add, a: src_base, b: i8_v });
        let src_heap = matches!(&xs.ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0]));
        let elem = self.fresh_value();
        let read_kind = if src_heap { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
        self.ops.push(Op::Prim { kind: read_kind, dst: Some(elem), args: vec![src_addr] });

        // Bind the lambda PARAM (the element). Captures resolve through `value_of`. A HEAP element is a
        // BORROW (`param_values`); a heap AGGREGATE is also a materialized aggregate (so a `let
        // (k,v)=pair` destructure borrows its slots).
        self.value_of.insert(params[0].0, elem);
        if src_heap {
            self.param_values.insert(elem);
            if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a) = &xs.ty {
                if a.len() == 1
                    && (matches!(&a[0], Ty::Tuple(_)) || self.aggregate_field_tys(&a[0]).is_some())
                {
                    self.materialized_aggregates.insert(elem);
                }
            }
        }

        // Lower the per-element keep/skip variant match into the write-cursor result list. A Block body's
        // leading lets (`let val = json.get_string(obj, k)`) are lowered FIRST in this per-iteration frame
        // (their captures resolve via value_of, the element param is bound above) so the tail match's
        // subject (`val`) is in scope; their own heap temps are freed within the iteration frame.
        self.in_frame += 1;
        self.in_defunc_body += 1;
        let mut lead_ok = true;
        for stmt in lead_stmts {
            if self.lower_stmt(stmt).is_err() {
                lead_ok = false;
                break;
            }
        }
        let ok = lead_ok
            && self
                .append_variant_match_to_result_list(subject, arms, rh, cursor, result_elem, eight)
                .is_some();
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        if !ok {
            return None;
        }

        // Advance the index and close the loop.
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);

        // Patch the result list's `len` field (offset 4) to the write-cursor (the count of kept
        // elements); the unused tail slots are harmless (a `${list}`/`xs[i]` reads `len`).
        let four = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: four, value: 4 });
        let lenaddr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: lenaddr, op: IntOp::Add, a: rh, b: four });
        self.ops.push(Op::Prim {
            kind: PrimKind::Store { width: 4 },
            dst: None,
            args: vec![lenaddr, cursor],
        });
        Some(dst)
    }

    /// Walk a `flat_map`/`filter_map` closure BODY, pushing its control flow (if / match / block)
    /// DOWN and appending each TERMINAL sublist to the loop-carried `acc` slot. The body returns
    /// `List[String]` (flat_map) or `Option[String]` (filter_map); each terminal is one of:
    ///   - `some(e)` / a singleton/multi `List` literal → append those elements
    ///   - `none` / `[]` → no-op
    ///   - a `List[String]` concat / call / `??` → append the whole (owned, droppable) sublist
    /// Returns `Some(())` on success, `None` (the caller rolls back + WALLs) for any out-of-subset
    /// leaf. CRUCIAL: a merged-if/match heap value is NEVER bound — the if/match is a UNIT control
    /// structure with appends in the arms, so the unsound `im·im·d` flat-cert shape never arises.
    fn append_body_to_str_acc(&mut self, body: &IrExpr, acc: ValueId) -> Option<()> {
        match &body.kind {
            // `e!` (effect-fn error propagation) is the identity on its inner sublist.
            IrExprKind::Unwrap { expr } => self.append_body_to_str_acc(expr, acc),
            // A block `{ stmts; tail }`: lower the statements as effects in a per-arm frame (their heap
            // locals ride to scope end / are dropped by `drop_arm_locals`), then recurse the tail.
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let arm_mark = self.live_heap_handles.len();
                for s in stmts {
                    self.lower_stmt(s).ok()?;
                }
                let r = self.append_body_to_str_acc(tail, acc);
                self.drop_arm_locals(arm_mark);
                r
            }
            // `if cond then T else E`: a UNIT control structure — lower cond to a scalar bool, then
            // recurse each arm as an append (only the taken arm runs). NO merged heap value. The cond's
            // own transient heap temps (`x == ""` → a `""` literal) are freed in a cond-LOCAL frame
            // BEFORE the `IfThen` (`lower_heap_result_cond`) — never deferred into an arm, where the
            // sibling branch would leave them uninitialized (the nested-if `rc_dec`-of-garbage trap).
            IrExprKind::If { cond, then, else_ } => {
                let cond_v = self.lower_heap_result_cond(cond)?;
                self.ops.push(Op::IfThen { cond: cond_v, dst: None });
                self.unit_arm_depth += 1;
                let then_ok = self.append_body_to_str_acc(then, acc);
                self.ops.push(Op::Else { val: None });
                let else_ok = then_ok.and_then(|_| self.append_body_to_str_acc(else_, acc));
                self.unit_arm_depth -= 1;
                self.ops.push(Op::EndIf { val: None });
                else_ok
            }
            // A `match` desugaring to a Bool/literal `if` chain (NOT a variant some/none match — that
            // is the variant-payload-bind frontier, deferred here). Recurse the desugared if.
            IrExprKind::Match { subject, arms } if !is_variant_ty(&subject.ty) => {
                let if_expr = self.desugar_match_to_if(subject, arms, &body.ty)?;
                self.append_body_to_str_acc(&if_expr, acc)
            }
            // A VARIANT `match subj { some(pl) => …, none => … }` over a per-element `Option[Value]`
            // (`match json.get(case, "payload") { some(pl) => if get_kind(pl)=="tuple" then <map>
            // else [], none => [] }` — the bindgen `gen_variant_type/struct/class` shape). A UNIT
            // control structure (NO merged value): tag-read @4, then APPEND each arm into `acc` by
            // recursing the walker — never bind a merged heap-if value. The Some payload `pl` is the
            // subject's BORROWED slot-0 handle (`param_values`), live through the some-arm; the
            // per-iteration subject is dropped AFTER both arms. See `append_variant_match_to_str_acc`.
            IrExprKind::Match { subject, arms } if is_variant_ty(&subject.ty) => {
                self.append_variant_match_to_str_acc(subject, arms, acc)
            }
            // `none` / `[]` (empty) — append nothing.
            IrExprKind::OptionNone => Some(()),
            IrExprKind::List { elements } if elements.is_empty() => Some(()),
            // `some(e)` — append the singleton `[e]`: lower the String payload to a FRESH OWNED value
            // (a LitStr / concat / `${interp}` / a Dup'd Var / a call result), store it into a
            // 1-element `List[String]` (an owned droppable sublist), then append + free it.
            IrExprKind::OptionSome { expr } => {
                let arm_mark = self.live_heap_handles.len();
                let piece = self.lower_owned_str_payload(expr)?;
                let sub = self.materialize_str_singleton(piece);
                self.append_owned_sub_to_acc(sub, acc, arm_mark)
            }
            // A `List[String]` literal / call / `??` leaf — an OWNED, DROPPABLE sublist (a single `i`,
            // NOT a merged-if). Lower it owned, then append + free it.
            IrExprKind::List { .. } | IrExprKind::Call { .. } | IrExprKind::UnwrapOr { .. } => {
                let arm_mark = self.live_heap_handles.len();
                let sub = self.lower_owned_str_sublist(body)?;
                self.append_owned_sub_to_acc(sub, acc, arm_mark)
            }
            // A `left + right` ConcatList leaf. FIRST try the whole concat as ONE owned sublist (the
            // common `acc_list + [x]` shape — unchanged). If that DECLINES (an operand is itself a
            // UNIT control structure that has no owned-value form — the julia `match {…} + [""]`
            // shape: the left is a variant match the str-acc walker appends, not materializes),
            // APPEND each operand IN ORDER instead: `acc += left; acc += right`. Order-preserving
            // (left's elements then right's, exactly `left + right`) and each operand is an
            // independently-balanced append (the per-leaf `i(id)` discipline), so the concat is the
            // same loop-carried `i(id)m` whether materialized whole or appended piecewise. Both
            // operands must be appendable; otherwise the whole HOF rolls back + WALLs.
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, left, right } => {
                let ops_mark = self.ops.len();
                let arm_mark = self.live_heap_handles.len();
                if let Some(sub) = self.lower_owned_str_sublist(body) {
                    return self.append_owned_sub_to_acc(sub, acc, arm_mark);
                }
                // Roll back EVERY op + handle the declined whole-concat attempt pushed (a partial
                // `try_lower_concat_list` may have emitted ops before failing), then append the two
                // operands separately (left then right — concat order).
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(arm_mark);
                self.append_body_to_str_acc(left, acc)?;
                self.append_body_to_str_acc(right, acc)
            }
            // A bare `Var` leaf (`{ let lines = …; lines }` — the flat_map body that binds its
            // per-element sublist to a `let` then returns it, the bindgen `gen_unpack_named` /
            // `gen_variant_type` shape). The Var is an OWNED, tracked `List[String]` local (its `let`
            // pushed it to `live_heap_handles` + `heap_elem_lists`, freed by the ENCLOSING block's
            // `drop_arm_locals`). APPEND it to `acc` by BORROW: `__list_concat_rc(acc, lines)` rc-incs
            // its elements into the new acc, then drop-old acc + SetLocal — exactly the owned-sublist
            // append, but WITHOUT taking ownership of `lines` (no `Consume`, no per-leaf
            // `drop_arm_locals` over it). `lines` is freed EXACTLY ONCE by its own `let` scope (the
            // block teardown), and the concat co-acquired its elements into `acc` — no double-free, no
            // leak. A BORROWED Var (a param/loop-element in `param_values`, owned elsewhere) is also
            // safe: the concat only borrows it, and we never free it here. An untracked/global Var (no
            // `value_for`) declines → the HOF rolls back + the caller WALLs (honest).
            IrExprKind::Var { id } => {
                let sub = self.value_for(*id).ok()?;
                let new = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(new),
                    name: "__list_concat_rc".to_string(),
                    args: vec![CallArg::Handle(acc), CallArg::Handle(sub)],
                    result: Some(crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
                });
                self.heap_elem_lists.insert(new);
                let drop_acc = self.drop_op_for(acc);
                self.ops.push(drop_acc);
                self.ops.push(Op::SetLocal { local: acc, src: new });
                Some(())
            }
            _ => None,
        }
    }
}
