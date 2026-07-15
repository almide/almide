impl LowerCtx {
    /// Is `container` a direct reference to a BORROWED heap PARAM (a record/tuple param the caller
    /// owns — its handle is in `param_values`)? Gates the heap-FIELD-projection return-materializer
    /// arm to exactly the build_config `opts.preopen_dirs` shape, excluding a fold/loop-derived LOCAL
    /// container (the `(B)` loop-accumulator frontier) whose lowering breaches the elided-call count
    /// gate. A non-Var / unbound / non-param container is NOT borrowed-param → `false` (defer).
    pub(crate) fn is_borrowed_param_container(&self, container: &IrExpr) -> bool {
        match &container.kind {
            IrExprKind::Var { id } => self
                .value_for(*id)
                .map(|v| self.param_values.contains(&v))
                .unwrap_or(false),
            _ => false,
        }
    }

    /// A LOCAL aggregate container whose block is genuinely MATERIALIZED (a record/tuple
    /// literal bind, a self-host call result tracked as an aggregate) — the (B)/loop-slot
    /// widening of [`Self::is_borrowed_param_container`]: projecting a field out of it via
    /// `dup_borrowed_slot` reads a REAL slot (never a deferred Opaque's garbage), and the
    /// moved-out ref is an independent `Dup` (the local drops its own ref once at scope
    /// end — the same no-double-free argument as the borrowed-param case, with the owner
    /// being this frame instead of the caller).
    pub(crate) fn is_materialized_local_container(&self, container: &IrExpr) -> bool {
        match &container.kind {
            IrExprKind::Var { id } => self
                .value_for(*id)
                .map(|v| self.materialized_aggregates.contains(&v))
                .unwrap_or(false),
            _ => false,
        }
    }

    /// `Some(piece)` for `Option[String]` = a 1-element `DynListStr`: store `piece`'s handle into
    /// slot 0 + CONSUME it (moves in), track as nested-ownership list + materialized Option.
    /// Reuses the proven Machinery-2 `store_str` op sequence — no new cert.
    pub(crate) fn materialize_opt_str_some(&mut self, piece: ValueId, repr: crate::Repr) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        let twelve = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: twelve, value: 12 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: oh, b: twelve });
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        self.heap_elem_lists.insert(obj);
        self.materialized_options.insert(obj);
        obj
    }

    /// `Some(<record>)` — an `Option[heap-record]` as a 1-element list holding the record handle @12
    /// (`some({key, val})` — porta find_eq_pos). SAME block as `materialize_opt_str_some` (the record
    /// is MOVED in at slot 0), but the Option's drop must RECURSIVELY free the record's nested heap
    /// fields — route it to `$__drop_<drop_fn>` via `variant_drop_handles="optrec:<drop_fn>"` (→
    /// [`Op::DropWrapperRec`] `is_result=false`), NOT the flat `heap_elem_lists`/`DropListStr` (which
    /// would `rc_dec` the record HANDLE only, leaking its String/List/Value fields). Cert is identical
    /// (Alloc `i` + the record `m` + the scope-end recursive `d`); only the drop ROUTE differs.
    pub(crate) fn materialize_opt_aggregate_some(
        &mut self,
        piece: ValueId,
        repr: crate::Repr,
        drop_fn: String,
    ) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        let twelve = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: twelve, value: 12 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: oh, b: twelve });
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        self.variant_drop_handles.insert(obj, format!("optrec:{drop_fn}"));
        self.materialized_options.insert(obj);
        obj
    }

    /// `Some((Int, String))` — an `Option[(Int, String)]` as a 1-element list holding the tuple handle
    /// (the `list.find` over a `List[(Int,String)]` result). SAME as `materialize_opt_str_some` but the
    /// payload is a TUPLE, so the Option's drop must RECURSIVELY free it (`$__drop_list_int_str`, the
    /// per-tuple rc==1 guard makes co-ownership with the source list safe) — routed via
    /// `variant_drop_handles="list_int_str"`, NOT the flat `heap_elem_lists` (which would leak the
    /// tuple's String).
    pub(crate) fn materialize_opt_int_str_some(&mut self, piece: ValueId, repr: crate::Repr) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        let twelve = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: twelve, value: 12 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: oh, b: twelve });
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        self.variant_drop_handles.insert(obj, "list_int_str".to_string());
        self.materialized_options.insert(obj);
        obj
    }

    /// Materialize `None` for an `Option[String]` as a 0-element `DynListStr` (tracked like
    /// `materialize_opt_str_some`). `DropListStr` over len 0 frees only the block.
    pub(crate) fn materialize_opt_str_none(&mut self, repr: crate::Repr) -> ValueId {
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: zero } });
        self.heap_elem_lists.insert(obj);
        self.materialized_options.insert(obj);
        obj
    }

    /// `Ok(string)` / `Err(string)` for a HEAP-Ok `Result[String, String]` = a len-1 `DynListStr`
    /// owning the one String at slot 0 (Ok's value OR Err's message), with the Ok/Err TAG written to
    /// the `cap` field (@8): 0=Ok, 1=Err. `len` stays 1 so `DropListStr` frees the String regardless
    /// of which arm. Cert = `materialize_opt_str_some` (Alloc `i` + the String `m` + scope-end `d`);
    /// the cap-tag store is an opaque prim op. Tracked in `materialized_results_str` for the match.
    /// Is `ty` a `Result[<record needing recursive drop>, String]` (porta read_valtype's
    /// `Result[{val, next}, String]`)? Gates the record-Ok Result ctor (arm + tail) — distinct from
    /// `is_heap_ok_result` (which would route a record Ok through the leaky flat `DropListStr`).
    pub(crate) fn is_record_result_ty(&self, ty: &Ty) -> bool {
        self.result_ok_record_drop_fn(ty).is_some()
    }

    /// A `Result[Option[<record needing recursive drop>], String]` — read_message's
    /// `Result[Option[JsonRpcRequest], String]`. Its `ok(none)` / `ok(some(rec))` / `ok(<Option Var>)`
    /// arms route to `try_lower_result_option_ctor` (the recursive `$__drop_opt_<R>`). `None` for a
    /// `Result[Option[String], String]` (flat) or a non-Option Ok.
    pub(crate) fn is_option_record_result_ty(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::Result, a) = ty else { return false };
        if a.len() != 2 || !matches!(&a[1], Ty::String) {
            return false;
        }
        let Ty::Applied(TypeConstructorId::Option, oa) = &a[0] else { return false };
        oa.len() == 1 && self.record_or_anon_drop_type_name(&oa[0]).is_some()
    }

    /// A `Result[Option[T], String]` whose Option leaf is a STRING or a SCALAR (Int/Float/Bool) — the
    /// derived-Codec `__decode_option_T` shape. Gates the `try_lower_result_option_scalar_str_ctor` arm
    /// (if/match) and is DISJOINT from `is_option_record_result_ty` (a record leaf) — the two together
    /// cover every `Result[Option[<leaf>], String]` the executable subset admits.
    pub(crate) fn is_option_scalar_str_result_ty(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::Result, a) = ty else { return false };
        if a.len() != 2 || !matches!(&a[1], Ty::String) {
            return false;
        }
        let Ty::Applied(TypeConstructorId::Option, oa) = &a[0] else { return false };
        oa.len() == 1 && matches!(&oa[0], Ty::String | Ty::Int | Ty::Float | Ty::Bool)
    }

    /// For a `Result[<record needing recursive drop>, String]` (`Result[Manifest, String]` —
    /// porta load_manifest / resolve_run_caps), the Ok record's generated recursive-drop name
    /// `<R>` (→ `$__drop_<R>`, registered by `build_record_layouts` / synthesized for an anon
    /// record). This is the SINGLE shape gate shared by BOTH the record-Result CONSTRUCTION
    /// (`try_lower_result_record_ctor` → `materialize_result_aggregate`, `resrec:<R>`) and the
    /// record-Result MATCH-SUBJECT path (`try_lower_variant_value_match`): the subject's
    /// scope-end drop routes through `Op::DropWrapperRec { is_result: true }` (recurse into the
    /// @12 record via `$__drop_<R>` at the Ok tag, else `rc_dec` the @12 Err String, then free
    /// the wrapper) — NEVER the flat `DropListStr` that frees only the @12 handle and LEAKS the
    /// record's nested heap fields (HOLE-1). `None` for a `Result[String, String]` / a
    /// scalar-only record Ok (the flat path is sound there).
    pub(crate) fn result_ok_record_drop_fn(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        match ty {
            Ty::Applied(TypeConstructorId::Result, a)
                if a.len() == 2 && matches!(a[1], Ty::String) =>
            {
                self.record_or_anon_drop_type_name(&a[0])
            }
            _ => None,
        }
    }

    /// Is `ty` a `Result[heap, heap]` (e.g. `Result[String, String]`)? Both Ok and Err own a heap
    /// payload, so it uses the cap-as-tag heap-Ok materialization, NOT the scalar len-as-tag one.
    pub(crate) fn is_heap_ok_result(ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
            if a.len() == 2 && is_heap_ty(&a[0]) && is_heap_ty(&a[1]))
    }

    /// Lower a heap-String piece (an `Ok`/`Err` payload) to its owned handle: a tracked Var, a
    /// String literal (fresh Alloc), or a Named-call result. Returns `None` for any other shape.
    pub(crate) fn lower_result_str_piece(&mut self, expr: &IrExpr) -> Option<ValueId> {
        match &expr.kind {
            // `ok(f)` / `err(msg)` over a Var that is STILL OWNED elsewhere — a borrowed param
            // (`fn validate(f) = .. ok(f)`) or a let-local with its own scope-end drop. The piece
            // is MOVED INTO the Result block (`materialize_result_str` `Consume`s it), so it must be
            // a FRESH owned reference: `Op::Dup` (acquire, cert `a`) the var, leaving the original
            // untouched (it drops exactly once at its own scope — no double-free, no bare move-out
            // `m` underflow the proven checker rejects). Same `a…m` balance as the bare-Var if-arm.
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                Some(dst)
            }
            IrExprKind::LitStr { value } => {
                let pr = repr_of(&expr.ty).ok()?;
                let p = self.fresh_value();
                self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                Some(p)
            }
            // `err("missing field '" + key + "'")` — the __str_concat chain's fresh owned String is
            // moved into the Result block (no Dup: it is already a fresh rc=1 reference).
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => self.try_lower_concat_str(expr),
            // `ok(file_env + ["tail"])` — a LIST concat piece (porta resolve_env's
            // tail-duplicated arm). `try_lower_concat_list` yields a fresh owned list
            // (`__list_concat_rc` co-owns heap elements), movable into the Result
            // block exactly like the String-concat piece above.
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                self.try_lower_concat_list(expr)
            }
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let lowered = self.lower_call_args(args).ok()?;
                let pr = repr_of(&expr.ty).ok()?;
                let p = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(p),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(pr),
                });
                Some(p)
            }
            // `ok(string.from_bytes(bytes))` / `ok(int.to_string(n))` — the Ok payload is a stdlib
            // MODULE call (or any other fresh-owned heap producer): lower it as a fresh owned heap
            // value (rc=1) that materialize_result_str then MOVES into the Result block, no Dup. This
            // is base64 decode's `match bs { ok(bytes) => ok(string.from_bytes(bytes)), … }`.
            _ => self.lower_owned_heap_field(expr),
        }
    }

    pub(crate) fn materialize_result_str(
        &mut self,
        piece: ValueId,
        repr: crate::Repr,
        is_err: bool,
        value_ok: bool,
    ) -> ValueId {
        use crate::PrimKind;
        // A 1-SLOT DynListStr (cap 1, len 1 — IDENTICAL block size to every other String/Value block,
        // so the single-head free-list reuses it; a wider block would be a distinct size that the
        // size-exact reuse leaks). Slot 0's LOW 32 bits (@12) own the String handle, its HIGH 32 bits
        // (@16) carry the Ok/Err tag — `DropListStr` does `i32.wrap` of the i64 slot, taking ONLY the
        // low-32 handle to free, so the high-32 tag is inert (never mistaken for a handle).
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        // slot 0 LOW (@12) := the String handle (zero-extended i64 → high 32 bits cleared), CONSUME
        // the piece (move-in). This 8-byte store MUST precede the tag store (it zeroes @16).
        let off12 = self.const_add(oh, 12);
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![off12, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        // slot 0 HIGH (@16) := the Ok/Err tag (0 = Ok, 1 = Err) — overwrites the cleared high half.
        let off16 = self.const_add(oh, 16);
        let tag = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tag, value: if is_err { 1 } else { 0 } });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![off16, tag] });
        // A Value-Ok Result (`Result[Value, String]`) drops via the recursive `Op::DropResultValue`
        // (Ok → `$__drop_value`); a String-Ok Result via the flat `DropListStr` (rc_dec the String).
        if value_ok {
            self.value_result_results.insert(obj);
        } else {
            self.heap_elem_lists.insert(obj);
        }
        self.materialized_results_str.insert(obj);
        obj
    }

    /// `ok(<record>)` / `err(<String>)` for a `Result[heap-record, String]` (porta read_valtype's
    /// `ok({val, next})`). SAME cap-as-tag block as `materialize_result_str` (`is_err` selects the
    /// @16 tag; the payload — record handle for Ok, String for Err — is MOVED into @12), but the
    /// wrapper's drop must, at the Ok arm, RECURSIVELY free the record's nested heap fields. Route it
    /// to `$__drop_<drop_fn>` via `variant_drop_handles="resrec:<drop_fn>"` (→ [`Op::DropWrapperRec`]
    /// `is_result=true`, which tag-dispatches: Ok → `$__drop_<drop_fn>`, Err → flat `rc_dec` of the
    /// @12 String), NOT the flat `heap_elem_lists`/`DropListStr` (which would leak the Ok record's
    /// fields). Same cert as `materialize_result_str` (Alloc `i` + payload `m` + recursive `d`).
    pub(crate) fn materialize_result_aggregate(
        &mut self,
        piece: ValueId,
        repr: crate::Repr,
        is_err: bool,
        drop_fn: String,
    ) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        let off12 = self.const_add(oh, 12);
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![off12, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        let off16 = self.const_add(oh, 16);
        let tag = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tag, value: if is_err { 1 } else { 0 } });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![off16, tag] });
        self.variant_drop_handles.insert(obj, format!("resrec:{drop_fn}"));
        self.materialized_results_str.insert(obj);
        obj
    }
}
