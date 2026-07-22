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

    /// A flat_map closure body that is a VARIANT `match subj { some(pl) => …, none => … }` over a
    /// per-element `Option[Value]` subject (`match json.get(case, "payload") { … }` — the bindgen
    /// `gen_variant_type/struct/class` shape). Lowered as a UNIT control structure that APPENDS each
    /// arm into the loop-carried `acc` slot (NO merged heap-if value — the same discipline the `if`
    /// case proves). Returns `Some(())` on success, `None` (the caller rolls back + WALLs) outside
    /// the subset.
    ///
    /// SUBSET: a 2-arm `[some(scalar|heap bind?), none]` (no guards), over an Option whose materialize
    /// makes it a TRACKED nested-ownership block (a self-host Option call — `json.get`/`list.first`/…).
    /// A self-host Result subject (Ok/Err) self-gates out (only Option here); a custom variant / a
    /// non-self-host Option declines.
    ///
    /// SOUNDNESS — per-iteration subject + borrowed payload, exactly the `try_lower_variant_value_match`
    /// `str_heap_bind`/`opt_tuple_bind` path but for a UNIT append target:
    ///  - The subject `json.get(case, "payload")` is materialized into a FRESH OWNED `Option[Value]`
    ///    block (cert `i`) INSIDE this iteration's frame, tracked `materialized_options` +
    ///    `heap_elem_lists` so its drop is the recursive `DropListStr` (frees the owned Value payload).
    ///  - A `some(pl)` HEAP payload binds `pl` to the subject's slot-0 handle (`LoadHandle` @12, in
    ///    `param_values`) — a BORROW: the subject still owns it, so `pl` is NOT a second owner and the
    ///    some-arm's reads/appends never free it. A consuming append auto-acquires (the leaf builders
    ///    Dup/copy into the owned sublist), so no double-free.
    ///  - The subject must stay live THROUGH the some-arm (the borrow is read there), so it is dropped
    ///    AFTER both arms — within THIS iteration's frame (cert `d`). So the subject is a balanced
    ///    `i…d` episode per iteration (like the heap `if` cond's transient temps), and `acc` stays the
    ///    loop-carried `i(id)m` slot. No leak (the subject + its payload are freed each iteration), no
    ///    double-free (the payload is borrowed, freed once by the subject's `DropListStr`), no
    ///    sibling-arm trap (the tag picks exactly one arm; the appends are per-arm-balanced).
    ///  - BRANCH OWNERSHIP ISOLATION: the two arms are ALTERNATE — snapshot/restore the
    ///    owned/borrowed sets around the some-arm so a borrow it consumes does not leak into the
    ///    none-arm's lowering view (mirrors `try_lower_variant_value_match`). The `acc` SetLocal is a
    ///    real op (survives the snapshot restore — only lowering-time tracking is reset).
    fn append_variant_match_to_str_acc(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        acc: ValueId,
    ) -> Option<()> {
        use crate::PrimKind;
        // ONLY an INLINE self-host Option CALL subject (`match json.get(case, "payload") { … }`). A
        // let-bound Var subject (`let pv = json.get(…); match pv { … }`) is NOT admitted: borrowing
        // that Option block in the unit-append context produced an EMPTY some-arm (WRONG bytes — a value
        // miscompile the leak oracle does not catch), so it WALLs honestly until that read is
        // understood. A Result / custom variant / a non-self-host Option also declines.
        if arms.len() != 2
            || arms.iter().any(|a| a.guard.is_some())
            || !is_self_host_option_call(subject)
            || !is_heap_ty(&subject.ty)
        {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        // Materialize the per-element subject into a FRESH OWNED Option block (cert `i`), dropped AFTER
        // the arms (cert `d`) within THIS iteration — the per-iteration `i…d` balance. Track it like the
        // statement/value match entry (so the heap-payload bind gate opens AND the post-arm drop is the
        // recursive DropListStr that frees the owned Value payload).
        let subj = match self
            .lower_call_args(std::slice::from_ref(subject))
            .ok()
            .and_then(|a| a.into_iter().next())
        {
            Some(CallArg::Handle(v)) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        self.materialized_options.insert(subj);
        if crate::lower::is_heap_elem_list_ty(&subject.ty) {
            self.heap_elem_lists.insert(subj);
        }
        // Parse the arms into (some_body, some_bind, none_body). A SCALAR payload binds a value COPY;
        // a HEAP payload (`pl: Value`) binds the slot-0 @12 handle as a BORROW, gated on the subject
        // being a tracked nested-ownership list (`heap_elem_lists`). A nested ctor / heap bind over a
        // non-nested-ownership subject declines.
        let mut some: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        let mut none: Option<&IrExpr> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Some { inner } => {
                    let bind = match inner.as_ref() {
                        IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Some((*var, false)),
                        IrPattern::Bind { var, ty }
                            if is_heap_ty(ty) && self.heap_elem_lists.contains(&subj) =>
                        {
                            Some((*var, true))
                        }
                        IrPattern::Wildcard => None,
                        _ => {
                            self.ops.truncate(ops_mark);
                            self.live_heap_handles.truncate(lhh_mark);
                            return None;
                        }
                    };
                    if some.is_some() {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
                    some = Some((&arm.body, bind));
                }
                IrPattern::None | IrPattern::Wildcard => {
                    if none.is_some() {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
                    none = Some(&arm.body);
                }
                _ => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            }
        }
        let ((some_body, some_bind), none_body) = match (some, none) {
            (Some(s), Some(n)) => (s, n),
            _ => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        // tag = load32(handle(subj) + 4); if tag != 0 then Some-arm else None-arm (UNIT — dst None).
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        // Bind the Some payload BEFORE the IfThen so it is in scope for the some-arm. A SCALAR is a
        // value COPY (load64); a HEAP element is `LoadHandle` (@12, an i32 Ptr) recorded in
        // `param_values` (a BORROW — the subject owns it, freed by its post-arm DropListStr).
        if let Some((bind_var, is_heap)) = some_bind {
            let payload = if is_heap {
                self.load_at_offset(h, 12, PrimKind::LoadHandle)
            } else {
                self.load_at_offset(h, 12, PrimKind::Load { width: 8 })
            };
            self.value_of.insert(bind_var, payload);
            if is_heap {
                self.param_values.insert(payload);
            }
        }
        self.ops.push(Op::IfThen { cond: tag, dst: None });
        // BRANCH OWNERSHIP ISOLATION: the arms are alternate — snapshot the owned/borrowed sets
        // before the some-arm, restore before the none-arm (the emitted ops are per-branch; only the
        // lowering-time tracking is reset). The shared payload binds survive (inserted before IfThen).
        let pv_snapshot = self.param_values.clone();
        let lhh_snapshot = self.live_heap_handles.clone();
        let ma_snapshot = self.materialized_aggregates.clone();
        self.unit_arm_depth += 1;
        let some_ok = self.append_body_to_str_acc(some_body, acc);
        self.ops.push(Op::Else { val: None });
        self.param_values = pv_snapshot;
        self.live_heap_handles = lhh_snapshot;
        self.materialized_aggregates = ma_snapshot;
        let none_ok = some_ok.and_then(|_| self.append_body_to_str_acc(none_body, acc));
        self.unit_arm_depth -= 1;
        self.ops.push(Op::EndIf { val: None });
        if none_ok.is_none() {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        // SUBJECT-DROP-AFTER-ARMS: the Some payload borrowed slot-0, so the fresh per-iteration Option
        // stayed live through both arms — drop it ONCE here (the recursive DropListStr frees it + its
        // owned Value payload), closing the per-iteration `i…d` balance. The subject is ALWAYS a fresh
        // owned inline-call result (the Var-subject borrow form is gated out above), so this drop is
        // unconditional.
        if let Some(pos) = self.live_heap_handles.iter().rposition(|&v| v == subj) {
            self.live_heap_handles.remove(pos);
            let op = self.drop_op_for(subj);
            self.ops.push(op);
        }
        Some(())
    }

    /// A `filter_map` closure body that is a 2-arm VARIANT `match subj { … }` deciding keep/skip,
    /// lowered into a WRITE-CURSOR result list (`lower_defunc_filter_map_hof`). The subject is a
    /// self-host Option CALL (`some(pl)`/`none`) OR a self-host Result(-str) CALL (`ok(pl)`/`err(_)`)
    /// — the dojo `match fs.read_text(dir+"/"+f) { ok(content) => some(parse_task_md(f, content)),
    /// err(_) => none }`. Mirrors `append_variant_match_to_str_acc` (UNIT control, per-arm action,
    /// branch isolation, drop-subject-after) BUT (a) ADMITS Result `ok`/`err` arms with the INVERSE
    /// tag (Result Ok = tag==0 vs Option Some = tag!=0) exactly as `try_lower_variant_value_match`
    /// already does (control_p2), and (b) the keep arm stores an OWNED record/Value at the cursor
    /// instead of appending a String. Returns `Some(())` on success, `None` (the caller rolls back +
    /// WALLs) outside the subset.
    ///
    /// SOUNDNESS — per-iteration subject + borrowed payload, exactly the Option path:
    ///  - The subject (`fs.read_text(…)`) is materialized into a FRESH OWNED block (cert `i`) INSIDE
    ///    this iteration's frame, tracked so its post-arm drop frees the owned payload recursively. A
    ///    str-Result (cap-as-tag @16) is `materialized_results_str` + `heap_elem_lists` (DropListStr
    ///    frees slot-0's String); a scalar Result (len-as-tag @4) is `materialized_results`; an Option
    ///    (len-as-tag @4) is `materialized_options` (+ `heap_elem_lists` for a heap payload).
    ///  - A `some(pl)`/`ok(pl)` HEAP payload binds `pl` to the subject's slot-0 @12 handle as a BORROW
    ///    (`param_values`) — the subject still owns it, freed once by its post-arm drop. The keep arm
    ///    builds a FRESH OWNED element (`lower_heap_result_arm`), so no double-free.
    ///  - The subject must stay live THROUGH the keep arm (the borrow is read there) → dropped AFTER
    ///    both arms (cert `d`), closing the per-iteration `i…d` balance.
    ///  - BRANCH OWNERSHIP ISOLATION around the then-arm (snapshot/restore param_values +
    ///    live_heap_handles + materialized_aggregates), so a consume in one alternate arm does not
    ///    leak into the other's lowering view.
    /// The HEAP-Ok Result SUBJECT drop-route classification for
    /// [`Self::append_variant_match_to_result_list`] — routes `subj`'s scope-end drop by the
    /// Ok payload's exact shape. NOT [`Self::track_heap_ok_result_subject_drop`] (control_p2.rs)
    /// — that sibling ALSO checks `result_ok_record_drop_fn` first (a RECORD-Ok `resrec:`
    /// route), which this call site's original inline chain never did; reusing it here would
    /// add new behavior for a record-Ok subject, not just flatten nesting. Verbatim extraction
    /// (guard-clause flattening) of the former inline if-else-if chain, no behavior change —
    /// see docs/roadmap/active/code-health-codopsy.md.
    fn track_heap_ok_result_subj_drop_no_record(&mut self, subj: ValueId, ty: &Ty) {
        if crate::lower::is_result_listval_ty(ty) {
            self.value_result_lists.insert(subj);
            return;
        }
        if crate::lower::is_value_result_ty(ty) {
            self.value_result_results.insert(subj);
            return;
        }
        if crate::lower::is_str_int_result_ty(ty) {
            self.str_int_result_results.insert(subj);
            return;
        }
        if crate::lower::is_value_int_result_ty(ty) {
            self.value_int_result_results.insert(subj);
            return;
        }
        if crate::lower::is_list_str_int_result_ty(ty) {
            self.list_str_int_result_results.insert(subj);
            return;
        }
        if crate::lower::is_list_value_int_result_ty(ty) {
            self.list_value_int_result_results.insert(subj);
            return;
        }
        self.heap_elem_lists.insert(subj);
    }

    fn append_variant_match_to_result_list(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        rh: ValueId,
        cursor: ValueId,
        result_elem: &Ty,
        eight: ValueId,
    ) -> Option<()> {
        use crate::PrimKind;
        // Gate: a 2-arm guard-free match over a heap (variant) subject. The subject must materialize to
        // a self-host Option/Result(-str) CALL or a USER `Named` call returning Option/Result (NOT a
        // let-bound Var — only a Call subject passes the tracking below, mirroring
        // `try_lower_variant_value_match`). A custom variant / non-variant subject rolls back at the
        // `is_option/is_result` check.
        if arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) || !is_heap_ty(&subject.ty) {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let rollback = |s: &mut Self| {
            s.ops.truncate(ops_mark);
            s.live_heap_handles.truncate(lhh_mark);
            None
        };
        // Materialize the per-element subject into a FRESH OWNED block (cert `i`), dropped AFTER the
        // arms (cert `d`) within THIS iteration.
        let subj = match self
            .lower_call_args(std::slice::from_ref(subject))
            .ok()
            .and_then(|a| a.into_iter().next())
        {
            Some(CallArg::Handle(v)) => v,
            _ => return rollback(self),
        };
        // Track the subject EXACTLY as `try_lower_variant_value_match` (control_p2): a self-host or
        // user `Named` Option/Result, with the type-driven drop set so the per-iteration subject drop
        // frees its owned payload correctly. The arm tag arrangement is the uniform skeleton then=tag≠0
        // / else=tag==0 (Option → then=Some/else=None; Result → then=Err/else=Ok).
        let is_named_call =
            matches!(&subject.kind, IrExprKind::Call { target: CallTarget::Named { .. }, .. });
        if is_self_host_option_call(subject)
            || (is_named_call
                && is_variant_ty(&subject.ty)
                && !crate::lower::is_result_ty(&subject.ty))
        {
            self.materialized_options.insert(subj);
            if crate::lower::is_heap_elem_list_ty(&subject.ty) {
                self.heap_elem_lists.insert(subj);
            }
        }
        if is_self_host_result_call(subject)
            || (is_named_call
                && crate::lower::is_result_ty(&subject.ty)
                && !Self::is_heap_ok_result(&subject.ty))
        {
            self.materialized_results.insert(subj);
            // Scalar-Ok / heap-Err `Result[Int, String]` (the byte-match fixture's `mkResult`): the
            // len-as-tag read stays @4, but track heap_elem_lists so the Err arm's String payload drops
            // via DropListStr (Ok=len0 frees nothing, Err=len1 frees slot-0's String).
            if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a) =
                &subject.ty
            {
                if a.len() == 2 && !is_heap_ty(&a[0]) && is_heap_ty(&a[1]) {
                    self.heap_elem_lists.insert(subj);
                }
            }
        }
        if is_self_host_result_str_call(subject)
            || (is_named_call && Self::is_heap_ok_result(&subject.ty))
        {
            self.materialized_results_str.insert(subj);
            self.track_heap_ok_result_subj_drop_no_record(subj, &subject.ty);
        }
        let is_option = self.materialized_options.contains(&subj);
        let is_result_str = self.materialized_results_str.contains(&subj);
        let is_result = self.materialized_results.contains(&subj) || is_result_str;
        if !is_option && !is_result {
            return rollback(self);
        }
        let tag_off = if is_result_str { 16 } else { 4 };
        // Parse the arms into (then_body, then_bind) [tag != 0] and (else_body, else_bind) [tag == 0],
        // the uniform skeleton: Option → then=Some / else=None; Result → then=Err / else=Ok. A heap
        // payload binds the @12 handle as a BORROW (gated on the subject being a nested-ownership list);
        // a scalar payload a value copy; a wildcard nothing.
        let heap_or_scalar_bind = |s: &Self, inner: &IrPattern| -> Result<Option<(VarId, bool)>, ()> {
            match inner {
                IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Ok(Some((*var, false))),
                IrPattern::Bind { var, ty }
                    if is_heap_ty(ty)
                        && (s.heap_elem_lists.contains(&subj)
                            || s.value_result_lists.contains(&subj)
                            || s.value_result_results.contains(&subj)) =>
                {
                    Ok(Some((*var, true)))
                }
                IrPattern::Wildcard => Ok(None),
                _ => Err(()),
            }
        };
        let mut then_slot: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        let mut else_slot: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        for arm in arms {
            let parsed: Result<(bool, Option<(VarId, bool)>), ()> = match &arm.pattern {
                IrPattern::Some { inner } if is_option => {
                    heap_or_scalar_bind(self, inner).map(|b| (true, b))
                }
                IrPattern::None | IrPattern::Wildcard if is_option => Ok((false, None)),
                IrPattern::Err { inner } if !is_option => {
                    heap_or_scalar_bind(self, inner).map(|b| (true, b))
                }
                IrPattern::Ok { inner } if !is_option => {
                    heap_or_scalar_bind(self, inner).map(|b| (false, b))
                }
                _ => Err(()),
            };
            match parsed {
                Ok((true, bind)) if then_slot.is_none() => then_slot = Some((&arm.body, bind)),
                Ok((false, bind)) if else_slot.is_none() => else_slot = Some((&arm.body, bind)),
                _ => return rollback(self),
            }
        }
        let ((then_body, then_bind), (else_body, else_bind)) = match (then_slot, else_slot) {
            (Some(t), Some(e)) => (t, e),
            _ => return rollback(self),
        };
        // tag = load32(handle(subj) + tag_off); bind payload(s) BEFORE the IfThen (in scope for the
        // arm that reads them); then a UNIT IfThen (dst None) with per-arm keep/skip.
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        let bind_payload = |s: &mut Self, bind: Option<(VarId, bool)>| {
            if let Some((bind_var, is_heap)) = bind {
                let payload = if is_heap {
                    s.load_at_offset(h, 12, PrimKind::LoadHandle)
                } else {
                    s.load_at_offset(h, 12, PrimKind::Load { width: 8 })
                };
                s.value_of.insert(bind_var, payload);
                if is_heap {
                    s.param_values.insert(payload);
                }
            }
        };
        bind_payload(self, then_bind);
        bind_payload(self, else_bind);
        self.ops.push(Op::IfThen { cond: tag, dst: None });
        let pv_snapshot = self.param_values.clone();
        let lhh_snapshot = self.live_heap_handles.clone();
        let ma_snapshot = self.materialized_aggregates.clone();
        self.unit_arm_depth += 1;
        let then_ok = self.emit_filter_map_arm(then_body, rh, cursor, result_elem, eight);
        self.ops.push(Op::Else { val: None });
        self.param_values = pv_snapshot;
        self.live_heap_handles = lhh_snapshot;
        self.materialized_aggregates = ma_snapshot;
        let else_ok =
            then_ok.and_then(|_| self.emit_filter_map_arm(else_body, rh, cursor, result_elem, eight));
        self.unit_arm_depth -= 1;
        self.ops.push(Op::EndIf { val: None });
        if else_ok.is_none() {
            return rollback(self);
        }
        // SUBJECT-DROP-AFTER-ARMS: the keep arm borrowed slot-0, so the fresh per-iteration subject
        // stayed live through both arms — drop it ONCE here, closing the per-iteration `i…d` balance.
        if let Some(pos) = self.live_heap_handles.iter().rposition(|&v| v == subj) {
            self.live_heap_handles.remove(pos);
            let op = self.drop_op_for(subj);
            self.ops.push(op);
        }
        Some(())
    }

    /// One arm of a `filter_map` keep/skip variant match (`append_variant_match_to_result_list`):
    ///   - `none` / `[]` (empty) → SKIP (no store).
    ///   - `some(<elem>)` → KEEP: build `<elem>` as a FRESH OWNED record/Value (`lower_heap_result_arm`,
    ///     which Consumes it = moved out of the iteration scope), store its handle at `result[cursor*8]`,
    ///     then `cursor += 1`. The element is already owned (rc 1) → just store, NO `Dup` (unlike
    ///     `filter`, which keeps a BORROWED source element).
    /// A `e!` wrapper is stripped (effect-fn error propagation is identity on its inner value here).
    /// Any other body shape returns `None` → the caller rolls back + WALLs.
    fn emit_filter_map_arm(
        &mut self,
        body: &IrExpr,
        rh: ValueId,
        cursor: ValueId,
        result_elem: &Ty,
        eight: ValueId,
    ) -> Option<()> {
        use crate::PrimKind;
        let body = match &body.kind {
            IrExprKind::Unwrap { expr } => expr.as_ref(),
            _ => body,
        };
        match &body.kind {
            IrExprKind::OptionNone => Some(()),
            IrExprKind::List { elements } if elements.is_empty() => Some(()),
            // A BLOCK arm body (`none => { let obj = …; let b = …; if b then some(e) else none }` —
            // porta load_porta_config's secrets `none`-arm): lower the leading lets as per-arm effects
            // (their captures resolve via value_of; their heap temps freed at the arm frame end), then
            // recurse on the tail. Mirrors `append_body_to_str_acc`'s Block case for the str-acc path.
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let arm_mark = self.live_heap_handles.len();
                for s in stmts {
                    self.lower_stmt(s).ok()?;
                }
                let r = self.emit_filter_map_arm(tail, rh, cursor, result_elem, eight);
                self.drop_arm_locals(arm_mark);
                r
            }
            // A CONDITIONAL arm body (`if from_env then some(e2) else none` — the secrets none-arm's
            // keep/skip decision): a UNIT control structure, only the taken arm runs. Lower the cond to a
            // scalar bool, then recurse each arm as an append/skip into the SAME result-list cursor (the
            // cursor's `SetLocal` increment is in-place under `unit_arm_depth`). No merged heap value —
            // the record is built+stored INSIDE the taken arm. Mirrors `append_body_to_str_acc`'s If case.
            IrExprKind::If { cond, then, else_ } => {
                let cond_v = self.lower_heap_result_cond(cond)?;
                self.ops.push(Op::IfThen { cond: cond_v, dst: None });
                self.unit_arm_depth += 1;
                let then_ok = self.emit_filter_map_arm(then, rh, cursor, result_elem, eight);
                self.ops.push(Op::Else { val: None });
                let else_ok =
                    then_ok.and_then(|_| self.emit_filter_map_arm(else_, rh, cursor, result_elem, eight));
                self.unit_arm_depth -= 1;
                self.ops.push(Op::EndIf { val: None });
                else_ok
            }
            IrExprKind::OptionSome { expr } => {
                let arm_mark = self.live_heap_handles.len();
                let elem_v = self.lower_heap_result_arm(expr, result_elem)?;
                // store the OWNED element handle at result[cursor*8].
                let c8 = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: c8, op: IntOp::Mul, a: cursor, b: eight });
                let rbase = self.load_addr(rh, 12);
                let raddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: raddr, op: IntOp::Add, a: rbase, b: c8 });
                let eh = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(eh), args: vec![elem_v] });
                self.ops.push(Op::Prim {
                    kind: PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![raddr, eh],
                });
                // cursor += 1.
                let one = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: one, value: 1 });
                let cnext = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: cnext, op: IntOp::Add, a: cursor, b: one });
                self.ops.push(Op::SetLocal { local: cursor, src: cnext });
                self.drop_arm_locals(arm_mark);
                Some(())
            }
            _ => None,
        }
    }
}
