impl LowerCtx {

    pub(crate) fn lower_place_mutation(&mut self, target: VarId) -> Result<(), LowerError> {
        let v = self.value_for(target)?;
        if self.param_values.contains(&v) {
            return Err(LowerError::Unsupported(
                "in-place mutation of a borrowed param not in this brick".into(),
            ));
        }
        self.ops.push(Op::MakeUnique { v });
        Ok(())
    }

    pub(crate) fn value_for(&self, var: VarId) -> Result<ValueId, LowerError> {
        self.value_of
            .get(&var)
            .copied()
            .ok_or_else(|| LowerError::Unsupported(format!("use of unbound var {var:?}")))
    }

    /// Resolve a value-position variable reference, admitting a reference to a
    /// module-level `let` GLOBAL. A function-local var is in `value_of`. A miss is a
    /// global IFF it is in the DECLARED global set (`self.globals`) — the frontend
    /// guarantees every non-global reference is bound by a preceding local form, so a
    /// miss that is NOT a declared global is a genuine lowering gap and stays WALLED.
    ///
    /// A confirmed global is bound ONCE (cached in `value_of`, so repeated references
    /// reuse the one handle) as a fresh EXTERNAL value: a scalar global is a Copy
    /// `Const`; a heap global is a fresh owned `Alloc{Opaque}` dropped at scope end —
    /// we model an owned COPY rather than an alias of the module's object, which is
    /// memory-safe by construction (alloc once / drop once, the real global untouched)
    /// and its content deferred like every `Opaque`. Referencing a global does NOT
    /// re-run its initializer, so this adds no call/cap obligation.
    /// ASSIGN through a mutable module-level `var`'s storage slot. A scalar stores the
    /// value directly; a heap global builds the NEW value FIRST (so a self-referencing
    /// RHS — `items = items + [n]`, the #501 alias pin — reads the old block via its own
    /// owned `$__mg_get` Dup while the slot still holds it), then `$__mg_take`s the OLD
    /// block (the slot's owned reference transfers to us — a fresh-owned CallFn result,
    /// exactly the certificate's model), drops it by its type route, and stores+Consumes
    /// the new block (the record-slot move-in pattern). Inside the synthesized
    /// `__mg_init` the slot is still zero, so take+drop are SKIPPED (dropping handle 0
    /// would trap). A MODELED frame (the model-one-iteration `while` fallback / a
    /// non-executable branch arm) must WALL: both eliding and emitting a modeled global
    /// write diverge from v0 (the write is an EFFECT).
    fn lower_mutable_global_assign(
        &mut self,
        var: VarId,
        index: u32,
        gty: &Ty,
        value: &IrExpr,
    ) -> Result<(), LowerError> {
        use crate::PrimKind;
        if self.in_frame > 0 && self.unit_arm_depth == 0 && self.scalar_loop_depth == 0 {
            return Err(LowerError::Unsupported(format!(
                "assignment to mutable module-level var {var:?} inside a modeled (non-\
                 executable) frame — the global write is an effect the model would elide"
            )));
        }
        let in_mg_init = self.fn_name == "__mg_init";
        if !is_heap_ty(gty) {
            let src = self
                .lower_scalar_value(value)
                .or_else(|| self.try_lower_scalar_call(value, &value.ty))
                .ok_or_else(|| {
                    LowerError::Unsupported(format!(
                        "non-scalar value assigned to mutable module-level var {var:?} \
                         outside the executable subset"
                    ))
                })?;
            let addr = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: addr, value: crate::mg_slot_addr(index) as i64 });
            self.ops.push(Op::Prim {
                kind: PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, src],
            });
            return Ok(());
        }
        // Heap global: NEW value first (RHS may read the global), as a fresh OWNED handle.
        let new = self.lower_owned_heap_field(value).ok_or_else(|| {
            LowerError::Unsupported(format!(
                "heap value assigned to mutable module-level var {var:?} outside the \
                 executable subset"
            ))
        })?;
        let repr = repr_of(gty)?;
        let addr = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: addr, value: crate::mg_slot_addr(index) as i64 });
        if !in_mg_init {
            let old = self.fresh_value();
            self.ops.push(Op::CallFn {
                dst: Some(old),
                name: "__mg_take".to_string(),
                args: vec![crate::CallArg::Scalar(addr)],
                result: Some(repr),
            });
            // Route the old block's drop by the global's TYPE (the same classification a
            // call-arg temp gets), then release it — its holders elsewhere (an in-flight
            // RHS `__mg_get` Dup) keep their own references, so this frees at last-ref only.
            self.materialized_call_arg(old, repr, gty);
            let drop_old = self.drop_op_for(old);
            self.ops.push(drop_old);
            self.live_heap_handles.retain(|v| *v != old);
        }
        let handle = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![new] });
        self.ops.push(Op::Prim {
            kind: PrimKind::Store { width: 8 },
            dst: None,
            args: vec![addr, handle],
        });
        self.ops.push(Op::Consume { v: new });
        self.live_heap_handles.retain(|v| *v != new);
        Ok(())
    }

    /// Memoize a materialized GLOBAL in `value_of` — but only in straight-line
    /// context. Inside a REAL branch arm, a loop body, or any frame, the memo is
    /// unsound: the defining op lands in that region's block, and a later reader
    /// in a SIBLING arm (or after a zero-iteration loop) would reference a value
    /// whose definition never executed on its path — on wasm that local is 0, and
    /// `let CAP = 2097152` compared in both arms of an `if` made the else-arm
    /// compare against 0 and take the wrong side (#945: aivarium's glTF loader
    /// rejected every primitive; a 52849-vertex model loaded as zero geometry
    /// with no error). The memo WAS sound when both arms linearized — every op
    /// executed regardless of the branch — and became unsound exactly as real
    /// branching spread; nothing in the types said the cache was
    /// linearization-shaped, so nothing objected. One chokepoint, so the next
    /// materialization site cannot re-decide this per shape.
    fn memo_global(&mut self, var: VarId, dst: ValueId) {
        if self.in_frame == 0 && self.unit_arm_depth == 0 && self.scalar_loop_depth == 0 {
            self.value_of.insert(var, dst);
        }
    }

    pub(crate) fn value_or_global(&mut self, var: VarId) -> Result<ValueId, LowerError> {
        // A SHARED-CELL var (cells.rs) reads its cell slot FRESH on every reference —
        // checked BEFORE `value_of` (a cell var must never be cached: an intervening
        // closure call may have written the cell, exactly like a mutable global).
        if let Some(&cell) = self.cell_of.get(&var) {
            return self.lower_cell_read(var, cell);
        }
        if let Some(&v) = self.value_of.get(&var) {
            return Ok(v);
        }
        // A MUTABLE module-level `var`: read its STORAGE SLOT fresh on every reference
        // (never cached in `value_of` — an intervening write, ours or a callee's, must be
        // seen; materializing the const INITIALIZER instead was a probe-confirmed silent
        // miscompile: `5 3 0` vs native `5 8 8`). A scalar is a plain slot `Load`; a heap
        // global goes through `$__mg_get` (slot load + `rc_inc` — the returned handle is a
        // REAL owned reference, matching the certificate's fresh-owned CallFn result), is
        // routed for its type-correct scope-end drop, and its block is materialized-real
        // by construction (only real constructions are ever stored through `__mg_take`/
        // `Store`), so member reads and spreads work on it.
        if let Some((index, ty)) = crate::lower::mutable_global_info(var) {
            // The in-place mutator's receiver (#946): the COW that just ran staged
            // its post-MakeUnique handle — hand it over as a BORROW (the slot owns
            // the block and nothing between the COW and the call can release it)
            // instead of re-loading and `Dup`ing. Consumed once; a SECOND read of
            // the same var in the same statement (`bytes.copy_from(dst, dst, …)`'s
            // source position) takes the normal owned-Dup path below.
            if let Some((pv, pval)) = self.pending_inplace_receiver {
                if pv == var {
                    self.pending_inplace_receiver = None;
                    return Ok(pval);
                }
            }
            return self.read_mutable_global_slot(index, &ty);
        }
        let ty = self
            .globals
            .get(&var)
            .cloned()
            .ok_or_else(|| LowerError::Unsupported(format!("use of unbound var {var:?}")))?;
        if is_heap_ty(&ty) {
            return self.materialize_heap_global(var, &ty);
        }
        // A SCALAR module-level global: materialize its CONST (call-free)
        // initializer's REAL value — a literal, const arithmetic, or a
        // reference to another const global (`let SOLAR_MASS = 4.0 * PI * PI`,
        // which recurses back through value_or_global). This used to fall to
        // the deferred `Const` = 0 — every USE of a scalar top-level `let` read
        // zero (top_let_test printed `PI = 0`, a silent miscompile). A
        // call-bearing init would inject CallFn ops the gate's IR-side count
        // never sees, so it WALLS instead (honest, never wrong).
        if let Some(init) = self.global_inits.get(&var).cloned() {
            return self.materialize_scalar_global(var, &init);
        }
        if crate::lower::strict_values() {
            return Err(crate::lower::strict_const_wall("module-level global"));
        }
        let dst = self.fresh_value();
        self.ops.push(Op::Const { dst });
        self.memo_global(var, dst);
        Ok(dst)
    }

    /// Extracted from `value_or_global` (codopsy8 complexity sweep, arm 1 of 3): the
    /// MUTABLE module-level `var` arm verbatim — the fresh STORAGE-SLOT read whose "never
    /// cached in `value_of`" rationale the caller documents at the dispatch site. A scalar
    /// is a plain slot `Load`; a heap global borrows the slot's handle and `Dup`s it. Pure
    /// text move.
    fn read_mutable_global_slot(&mut self, index: u32, ty: &Ty) -> Result<ValueId, LowerError> {
        let addr = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: addr, value: crate::mg_slot_addr(index) as i64 });
        if is_heap_ty(ty) {
            let repr = repr_of(ty)?;
            // BORROW the slot's handle then `Dup` it — the same borrow-then-Dup the
            // spread-record copy uses (cert `a`; the render's Dup IS the `rc_inc`),
            // so the function owns a real reference the slot's later reassignment
            // cannot invalidate. No call op is injected (the caps count stays exact).
            let borrowed = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: crate::PrimKind::LoadHandle,
                dst: Some(borrowed),
                args: vec![addr],
            });
            let dst = self.fresh_value();
            self.ops.push(Op::Dup { dst, src: borrowed });
            self.materialized_call_arg(dst, repr, ty);
            self.materialized_aggregates.insert(dst);
            self.materialized_lists.insert(dst);
            return Ok(dst);
        }
        let dst = self.fresh_value();
        self.ops.push(Op::Prim {
            kind: crate::PrimKind::Load { width: 8 },
            dst: Some(dst),
            args: vec![addr],
        });
        Ok(dst)
    }

    /// Extracted from `value_or_global` (codopsy8 complexity sweep, arm 2 of 3): the
    /// `is_heap_ty` arm verbatim — a fresh owned copy of the global's CONST initializer,
    /// or the wall when no admissible const form exists. The per-initializer-SHAPE arms
    /// moved on to `materialize_heap_global_from_init`; nothing else changed.
    fn materialize_heap_global(&mut self, var: VarId, ty: &Ty) -> Result<ValueId, LowerError> {
        // A HEAP module-level global (the base64 alphabet, the aes S-box): MATERIALIZE a FRESH
        // OWNED copy of its CONST initializer as a DIRECT `Alloc` — a string literal (`Init::Str`),
        // an int-list literal (`Init::IntList`), or `bytes.from_list([int literals])` (`Init::Bytes`).
        // CRITICAL: only a CONST-foldable init (NO runtime call) is admitted, so the materialization
        // injects ZERO `CallFn` ops — the gate's IR-side `count_ir_calls` stays exact (`mir == ir`).
        // A COMPUTED init (`string.from_codepoint(10)`, a user call) would inject a call the IR-body
        // count never sees (mir>ir = a false caps de-taint), so it keeps WALLING (no regression).
        // The fresh owned copy is dropped at scope end like any literal (cert: one `i` + one `d`);
        // `value_of[var]` caches it so repeated references in the SAME function reuse the one copy.
        if let Some(init) = self.global_inits.get(&var).cloned() {
            if let Some(v) = self.materialize_heap_global_from_init(var, ty, &init)? {
                return Ok(v);
            }
        }
        crate::trace::trace("ALMIDE_DBG_ELEM", || {
            match self.global_inits.get(&var) {
                Some(i) => format!(
                    "[global-init] {var:?} init present, kind {}, contains_call={}\n{i:#?}",
                    crate::lower::kind_name(&i.kind),
                    crate::lower::expr_contains_call(i)
                ),
                None => format!("[global-init] {var:?} has NO global_inits entry"),
            }
        });
        Err(LowerError::Unsupported(format!(
            "reference to a heap module-level global {var:?} cannot be faithfully \
             materialized in this brick (no CONST initializer — a computed init would \
             inject an uncounted call)"
        )))
    }

    /// Extracted from `value_or_global` (codopsy8 complexity sweep, arm 2 of 3): the
    /// initializer-SHAPE router for a heap module-level global. Each shape is tried in the
    /// ORIGINAL order (alias-let → flat const data → pure literal list → record literal →
    /// Option ctor → list-of-records) and every arm body is a verbatim move; `Ok(None)`
    /// means no shape matched and the caller walls exactly as the fallthrough did.
    fn materialize_heap_global_from_init(
        &mut self,
        var: VarId,
        ty: &Ty,
        init: &IrExpr,
    ) -> Result<Option<ValueId>, LowerError> {
        if let Some(v) = self.materialize_global_alias_let(var, init)? {
            return Ok(Some(v));
        }
        if let Some(v) = self.materialize_const_data_global(var, ty, init)? {
            return Ok(Some(v));
        }
        if let Some(v) = self.materialize_literal_list_global(var, init) {
            return Ok(Some(v));
        }
        if let Some(v) = self.materialize_record_literal_global(var, ty, init) {
            return Ok(Some(v));
        }
        if let Some(v) = self.materialize_option_ctor_global(var, ty, init) {
            return Ok(Some(v));
        }
        Ok(self.materialize_record_list_global(var, init))
    }

    /// Extracted from `value_or_global` (codopsy8 complexity sweep, arm 2 of 3): the
    /// alias-let shape, verbatim. `Ok(None)` = the init is not a bare `Var`, or its source
    /// is not a declared global (the caller tries the next shape); `Err` = the MUTABLE
    /// source wall.
    fn materialize_global_alias_let(
        &mut self,
        var: VarId,
        init: &IrExpr,
    ) -> Result<Option<ValueId>, LowerError> {
        // A global whose init is ANOTHER global (`let DIRECT = letlib.GREETING` —
        // the #632 alias-let): recurse through value_or_global on the SOURCE id (a
        // fresh owned copy of ITS const init, cached + dropped at scope end); this
        // reference aliases the same local copy (reads only — no second owner).
        // Zero calls injected (the source resolves through the same const-only
        // machinery), so the count gate stays exact.
        let IrExprKind::Var { id: src } = &init.kind else {
            return Ok(None);
        };
        let src = *src;
        // `let snapshot = counter` over a MUTABLE source: v0 evaluates the
        // alias ONCE at startup; recursing here would read the slot's CURRENT
        // value at each use — a divergence, so WALL the alias instead.
        if crate::lower::is_mutable_global(src) {
            return Err(LowerError::Unsupported(format!(
                "global alias-let of a MUTABLE module-level var {src:?} (a \
                 startup snapshot) is not in this brick"
            )));
        }
        if self.globals.contains_key(&src) {
            let v = self.value_or_global(src)?;
            self.memo_global(var, v);
            return Ok(Some(v));
        }
        Ok(None)
    }

    /// Extracted from `value_or_global` (codopsy8 complexity sweep, arm 2 of 3): the flat
    /// CONST-data shape, verbatim — a `const_global_init`-foldable initializer (string
    /// literal, int-list literal, `bytes.from_list`) becomes a direct owned `Alloc`.
    fn materialize_const_data_global(
        &mut self,
        var: VarId,
        ty: &Ty,
        init: &IrExpr,
    ) -> Result<Option<ValueId>, LowerError> {
        let Some(const_init) = const_global_init(init) else {
            return Ok(None);
        };
        let repr = repr_of(ty)?;
        // An `Init::IntList` copy is a REAL populated scalar-list block, so register it
        // in `materialized_lists` — the gate `lower_scalar_index_access` consults. Without
        // this, `WEIGHTS[j]` over a module-global int list declined the scalar read and the
        // enclosing cond/bind walled (#1150); a local `let ws = [10, 20, 30]` already
        // registers through `lower_bind`, so the global copy was the one unregistered
        // producer of this block shape. `Init::Str`/`Init::Bytes` are NOT lists — excluded.
        let is_int_list = matches!(const_init, crate::Init::IntList(_));
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc { dst, repr, init: const_init });
        self.live_heap_handles.push(dst);
        if is_int_list {
            self.materialized_lists.insert(dst);
        }
        self.memo_global(var, dst);
        Ok(Some(dst))
    }

    /// Extracted from `value_or_global` (codopsy8 complexity sweep, arm 2 of 3): the
    /// pure-literal-list shape, verbatim (the gate and the builder call are unchanged, the
    /// nesting is now a guard clause).
    fn materialize_literal_list_global(
        &mut self,
        var: VarId,
        init: &IrExpr,
    ) -> Option<ValueId> {
        // A NESTED-OWNERSHIP heap global with no flat CONST-data form but a PURE
        // (call-free) LITERAL initializer — the `let DIFFICULTIES = ["basic", …]`
        // shape: materialize a FRESH OWNED copy via the SAME `DynListStr` builder a
        // local `let xs = [..]` uses (`try_lower_str_list_literal`). GATED to a
        // call-free literal list (`is_pure_literal_list`) so the materialization
        // injects ZERO `CallFn` — the IR reference is a single `Var` (0 calls), so the
        // gate's `mir == ir` count stays exact. A COMPUTED element (`[f(x)]`,
        // `string.repeat(..)`) is NOT pure → keeps walling (no mir>ir de-taint). The
        // builder registers the right recursive drop set (`heap_elem_lists` →
        // `DropListStr`); we add it to `live_heap_handles` so the fresh owned copy is
        // freed at scope end (cert one `i` + one `d`), the real module global untouched.
        if !is_pure_literal_list(init) {
            return None;
        }
        let dst = self.try_lower_str_list_literal(init)?;
        self.live_heap_handles.push(dst);
        self.memo_global(var, dst);
        Some(dst)
    }

    /// Extracted from `value_or_global` (codopsy8 complexity sweep, arm 2 of 3): the
    /// record-literal shape, verbatim. The two-part admission gate keeps its original
    /// evaluation order (`Record` shape first, `expr_contains_call` only after it matches).
    fn materialize_record_literal_global(
        &mut self,
        var: VarId,
        ty: &Ty,
        init: &IrExpr,
    ) -> Option<ValueId> {
        // A RECORD-literal heap global (`let CFG = Cfg { name: "c" }` — the #502
        // spread/member base): call-free fields construct through the SAME builder
        // a local record `let` uses (`try_lower_record_construct` — allocs + stores
        // ONLY, zero `CallFn`, so the gate's `mir == ir` count stays exact), which
        // registers the record's own drop route. The fresh owned copy frees at
        // scope end; the real module global is untouched.
        if !matches!(init.kind, IrExprKind::Record { .. }) {
            return None;
        }
        if crate::lower::expr_contains_call(init) {
            return None;
        }
        // A SCALAR-ONLY record global (`let _transparent = { r: 0.0, a: 0.0 }`)
        // constructs through the scalar builder (the mixed builder defers it).
        let dst = self
            .try_lower_record_construct(init)
            .or_else(|| self.try_lower_scalar_record_construct(init))?;
        if !self.live_heap_handles.contains(&dst) {
            self.live_heap_handles.push(dst);
        }
        // The copy's slots are REAL — register it so member reads and
        // `{ ...global, override }` spreads take the materialized path.
        self.materialized_aggregates.insert(dst);
        // A heap-nested record copy (`_default`'s `bg: Color` slot) frees
        // via its recursive `$__drop_<R>` at scope end — the flat mask
        // alone would leak a heap-IN-nested field on deeper shapes.
        if let Some(name) = self.record_or_anon_drop_type_name(ty) {
            self.record_masks.remove(&dst);
            self.variant_drop_handles.insert(dst, name);
        }
        self.memo_global(var, dst);
        Some(dst)
    }

    /// Extracted from `value_or_global` (codopsy8 complexity sweep, arm 2 of 3): the
    /// Option-ctor shape, verbatim. The two-part admission gate keeps its original
    /// evaluation order (ctor shape first, `expr_contains_call` only after it matches).
    fn materialize_option_ctor_global(
        &mut self,
        var: VarId,
        ty: &Ty,
        init: &IrExpr,
    ) -> Option<ValueId> {
        // An OPTION-ctor heap global (`let MAYBE = some(Cfg { name: "opt" })` —
        // the crossmod option_record_toplet): a call-free `some(...)`/`none`
        // initializer builds through the SAME ctor builder a local `let o =
        // some(..)` uses (allocs + stores only, zero `CallFn` — the count gate
        // stays exact), which registers the Option's own drop route. Tracked in
        // `materialized_options` so a `match m.MAYBE { some(c) => … }` over the
        // fresh copy EXECUTES (reads the real len-as-tag).
        if !matches!(init.kind, IrExprKind::OptionSome { .. } | IrExprKind::OptionNone) {
            return None;
        }
        if crate::lower::expr_contains_call(init) {
            return None;
        }
        let dst = self.try_lower_option_ctor(init, ty)?;
        if !self.live_heap_handles.contains(&dst) {
            self.live_heap_handles.push(dst);
        }
        self.value_shapes.insert(dst, crate::lower::VariantShape::Option);
        self.memo_global(var, dst);
        Some(dst)
    }

    /// Extracted from `value_or_global` (codopsy8 complexity sweep, arm 2 of 3): the
    /// list-of-records shape, verbatim. The two-part admission gate keeps its original
    /// evaluation order (`List` shape first, `expr_contains_call` only after it matches).
    fn materialize_record_list_global(&mut self, var: VarId, init: &IrExpr) -> Option<ValueId> {
        // A LIST-OF-RECORDS heap global (`let CFGS = [Cfg { name: "a" }, Cfg { name:
        // "b" }]` — cross_module_toplet_byvalue's #486 list-of-records shape): a
        // call-free `List` initializer whose elements are all record ctors builds
        // through the SAME builder a local `let xs = [Cfg{..}, ..]` uses
        // (`try_lower_record_list_literal` — per-element `try_lower_record_construct`
        // MOVED into owned i64 slots, zero `CallFn`, so the gate's `mir == ir` count
        // stays exact), which registers the list's own recursive `$__drop_list_<R>`
        // drop route (each element freed via `$__drop_<R>`). The fresh owned copy
        // frees at scope end; the real module global is untouched.
        if !matches!(init.kind, IrExprKind::List { .. }) {
            return None;
        }
        if crate::lower::expr_contains_call(init) {
            return None;
        }
        let dst = self.try_lower_record_list_literal(init)?;
        if !self.live_heap_handles.contains(&dst) {
            self.live_heap_handles.push(dst);
        }
        self.memo_global(var, dst);
        Some(dst)
    }

    /// Extracted from `value_or_global` (codopsy8 complexity sweep, arm 3 of 3): the
    /// SCALAR module-level global arm verbatim — const-fold the initializer's REAL value
    /// (rolling `self.ops` back to the mark when the fold fails), else wall. The caller now
    /// clones the init before dispatching, which the const-fold path did for itself.
    fn materialize_scalar_global(
        &mut self,
        var: VarId,
        init: &IrExpr,
    ) -> Result<ValueId, LowerError> {
        fn init_has_call(e: &IrExpr) -> bool {
            use almide_ir::visit::{walk_expr, IrVisitor};
            struct C(bool);
            impl IrVisitor for C {
                fn visit_expr(&mut self, e: &IrExpr) {
                    if matches!(
                        e.kind,
                        IrExprKind::Call { .. }
                            | IrExprKind::TailCall { .. }
                            | IrExprKind::RuntimeCall { .. }
                    ) {
                        self.0 = true;
                    }
                    walk_expr(self, e);
                }
            }
            let mut c = C(false);
            c.visit_expr(e);
            c.0
        }
        if !init_has_call(init) {
            let mark = self.ops.len();
            if let Some(dst) = self.lower_scalar_value(init) {
                self.memo_global(var, dst);
                return Ok(dst);
            }
            self.ops.truncate(mark);
        }
        Err(LowerError::Unsupported(format!(
            "scalar module-level global {var:?} has a non-const-foldable initializer                  (a call would be uncounted; a deferred Const-0 would be silently wrong)                  not in this brick"
        )))
    }

    /// The correct release op for a heap value at scope/frame end, by its tracking set (the SINGLE
    /// source of truth for drop-op selection — used by `emit_scope_end_drops`, `drop_arm_locals`, and
    /// the variant-match subject drop). Order matters: the recursive value-drops are checked BEFORE
    /// the flat `DropListStr`, since a `value.as_array` Result / a `List[Value]` is ALSO a
    /// `heap_elem_list`, but a flat per-slot `rc_dec` there would leak the nested element Values.
    /// The NAMED record type of `ty` iff it needs the recursive `$__drop_<R>` (some field is a
    /// `Map`/`Value`/record/`List[heap]` — [`record_field_needs_recursive_drop`]). A record VALUE of
    /// such a type is registered in `variant_drop_handles` so `drop_op_for` routes it to the recursive
    /// `Op::DropVariant` instead of the flat `DropListStr` (which would leak its nested heap fields).
    pub(crate) fn record_drop_type_name(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let name = match ty {
            Ty::Named(n, _) => n.as_str().to_string(),
            Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
            _ => return None,
        };
        // Return the CANONICAL registry key (a bare cross-module spelling resolves to the
        // qualified decl name) — this string is the drop-routing identity, and the
        // generators name `$__drop_<R>` from the QUALIFIED decl.
        let canonical =
            crate::lower::canonical_record_key(&self.record_layouts, &name)?.to_string();
        let (names, tys) = self.aggregate_field_tys(ty)?;
        // A GENERIC decl has no shared `$__drop_<R>` (the heap mask differs per
        // instantiation — `Pair[Int, String]` vs `Pair[String, Int]`): route to the
        // per-shape `__drop_anonrec_<hash>` over the SUBSTITUTED fields, the same
        // identity `collect_recursive_anon_records` registers for generation.
        if self.record_layouts.get(canonical.as_str()).is_some_and(|(gs, _)| !gs.is_empty()) {
            let pairs: Vec<(almide_lang::intern::Sym, Ty)> =
                names.into_iter().zip(tys.iter().cloned()).collect();
            if crate::lower::anon_record_needs_recursive_drop(&pairs) {
                return Some(crate::lower::anon_record_drop_name(&pairs));
            }
            return None;
        }
        tys.iter()
            .any(record_field_needs_recursive_drop)
            .then_some(canonical)
    }

    /// The recursive-drop handle name for a record VALUE of type `ty` — a NAMED recursive record's
    /// `<name>` (→ `$__drop_<name>`, generated from its `type` decl) OR a synthesized
    /// `anonrec_<hash>` for an ANONYMOUS record that owns any heap field whose nested heap a flat
    /// one-level mask would LEAK (`{ data: Bytes, state: Cfb8State }` — aes cfb8;
    /// `__drop_anonrec_<hash>` is emitted by `generate_record_drop_sources` from
    /// `collect_recursive_anon_records`). `None` for a non-record / scalar-only record (the flat
    /// masked `DropListStr` is sound). The synthesis predicate is structural
    /// (`record_field_needs_recursive_drop`), so this lowering-side decision matches the
    /// generation-side one exactly.
    pub(crate) fn record_or_anon_drop_type_name(&self, ty: &Ty) -> Option<String> {
        if let Some(name) = self.record_drop_type_name(ty) {
            return Some(name);
        }
        if let Ty::Record { fields } = ty {
            if crate::lower::anon_record_needs_recursive_drop(fields) {
                return Some(crate::lower::anon_record_drop_name(fields));
            }
        }
        None
    }

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
        let ty = self.variant_drop_handles.get(&v)?;
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
            (self.value_result_lists.contains(&v), |v| Op::DropResultListValue { v }),
            (self.value_result_results.contains(&v), |v| Op::DropResultValue { v }),
            (self.str_int_result_results.contains(&v), |v| Op::DropResultStrInt { v }),
            (self.value_int_result_results.contains(&v), |v| Op::DropResultValueInt { v }),
            (self.list_value_int_result_results.contains(&v), |v| Op::DropResultListValueInt { v }),
            (self.list_str_int_result_results.contains(&v), |v| Op::DropResultListStrInt { v }),
            (self.list_str_result_results.contains(&v), |v| Op::DropResultListStr { v }),
            (self.value_elem_lists.contains(&v), |v| Op::DropListValue { v }),
            (self.str_value_elem_lists.contains(&v), |v| Op::DropListStrValue { v }),
            (self.str_str_elem_lists.contains(&v), |v| Op::DropListStrStr { v }),
            (self.list_list_str_lists.contains(&v), |v| Op::DropListListStr { v }),
            (
                self.heap_elem_lists.contains(&v) || self.record_masks.contains_key(&v),
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
