//! Call lowering: user functions, variant constructors, the
//! println/eprintln special forms, and the `list.*` runtime forms.

use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStringPart};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::types_table::NamedDef;
use crate::*;

impl Emitter<'_> {
    pub(crate) fn lower_call(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower_call_at(target, args, false)
    }

    pub(crate) fn lower_call_at(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
        tail: bool,
    ) -> Result<Option<SliceTy>, EmitError> {
        match target {
            CallTarget::Named { name } if name.as_str() == "println" && args.len() == 1 => {
                self.lower_print(&args[0], F_PRINTLN_IMPORT, F_PRINTLN_BLOCK)?;
                Ok(None)
            }
            CallTarget::Named { name } if name.as_str() == "eprintln" && args.len() == 1 => {
                self.lower_print(&args[0], F_EPRINTLN_IMPORT, F_EPRINTLN_BLOCK)?;
                Ok(None)
            }
            CallTarget::Named { name } => {
                let name = name.as_str();
                // Variant constructor? (checker keeps ctor and fn names apart)
                if let Some(&(ti, ci)) = self.types.ctors.get(name) {
                    let (size, tag, fields) = {
                        let NamedDef::Variant(v) = &self.types.defs[ti as usize] else {
                            return unsup("ctor-of-record");
                        };
                        let c = &v.cases[ci as usize];
                        let fs: Vec<(SliceTy, u32)> =
                            c.fields.iter().map(|f| (f.ty, f.offset)).collect();
                        (c.size, c.tag, fs)
                    };
                    if args.len() != fields.len() {
                        return unsup(&format!("ctor-arity:{name}"));
                    }
                    let hold = self.hold_i32()?;
                    self.f
                        .instructions()
                        .i32_const(size as i32)
                        .call(F_ALLOC)
                        .local_tee(hold)
                        .i32_const(tag as i32)
                        .i32_store(slot_memarg(almide_layout::SUM_TAG));
                    for (a, (fty, off)) in args.iter().zip(fields) {
                        self.f.instructions().local_get(hold);
                        self.lower(a, Some(fty))?;
                        self.store_ty_slot(fty, off);
                    }
                    self.f.instructions().local_get(hold);
                    self.release_i32();
                    return Ok(Some(SliceTy::Named(ti)));
                }
                let Some(&i) = self.table.by_name.get(name) else {
                    return unsup(&format!("call:{name}"));
                };
                let info = &self.table.infos[i];
                if let Some(r) = &info.refuse {
                    return unsup(&format!("call-fn:{name}:{r}"));
                }
                if args.len() != info.params.len() {
                    return unsup(&format!("call-arity:{name}"));
                }
                let (index, ret, params) = (info.wasm_index, info.ret, info.params.clone());
                for (a, want) in args.iter().zip(params) {
                    self.lower(a, Some(want))?;
                }
                self.calls.insert(name.to_string());
                // Tail position with a matching return type → return_call:
                // constant stack for arbitrarily deep (incl. mutual)
                // recursion, the C-292 contract.
                if tail && ret.is_some() && ret == self.fn_ret {
                    self.f.instructions().return_call(index);
                } else {
                    self.f.instructions().call(index);
                }
                Ok(ret)
            }
            // Stdlib special forms the runtime helpers cover directly.
            CallTarget::Module { module, func, .. }
                if module.as_str() == "int" && func.as_str() == "to_string" && args.len() == 1 =>
            {
                self.lower(&args[0], Some(INT))?;
                self.f.instructions().call(F_INT_TO_STRING);
                Ok(Some(STR))
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string"
                    && matches!(func.as_str(), "len" | "length")
                    && args.len() == 1 =>
            {
                self.lower(&args[0], Some(STR))?;
                self.f.instructions().call(F_STR_LEN_CHARS);
                Ok(Some(INT))
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "list" => {
                self.lower_list_call(func.as_str(), args)
            }
            CallTarget::Module { module, func, .. } => {
                unsup(&format!("call:{}.{}", module.as_str(), func.as_str()))
            }
            _ => unsup("call:computed-or-method"),
        }
    }

    /// `list.*` special forms over the runtime helpers.
    pub(crate) fn lower_list_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
            ("len", [xs]) => {
                let elem = match self.lower(xs, None)? {
                    SliceTy::List(h) => self.types.el(h),
                    other => return unsup(&format!("list-len-of:{other:?}")),
                };
                self.f
                    .instructions()
                    .i32_load(len_memarg())
                    .i32_const(elem.slot_size() as i32)
                    .i32_div_u()
                    .i64_extend_i32_u();
                Ok(Some(INT))
            }
            ("get", [xs, idx]) => {
                let h = match self.lower(xs, None)? {
                    SliceTy::List(h) => h,
                    other => return unsup(&format!("list-get-of:{other:?}")),
                };
                self.lower(idx, Some(INT))?;
                let helper = match self.types.el(h).slot_size() {
                    8 => F_LIST_GET_8,
                    _ => F_LIST_GET_4,
                };
                self.f.instructions().call(helper);
                Ok(Some(SliceTy::Option(h)))
            }
            ("get_or", [xs, idx, default]) => self.lower_list_get_or(xs, idx, default),
            // `list.push` MUTATES through its `mut` param on the oracle
            // (the growth fixture pushes as bare statements). Lowered as a
            // write-back: var = $push(var, v). Requires a plain var arg.
            ("push", [xs, v]) => {
                let IrExprKind::Var { id } = &xs.kind else {
                    return unsup("list-push-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                let SliceTy::List(h) = var_ty else {
                    return unsup(&format!("list-push-of:{var_ty:?}"));
                };
                let elem = self.types.el(h);
                self.f.instructions().local_get(var_idx);
                self.lower(v, Some(elem))?;
                let helper = match elem.slot_size() {
                    8 => F_LIST_PUSH_8,
                    _ => F_LIST_PUSH_4,
                };
                self.f.instructions().call(helper).local_set(var_idx);
                Ok(None)
            }
            ("join", [xs, sep]) => {
                match self.lower(xs, None)? {
                    SliceTy::List(h) if self.types.el(h) == STR => {}
                    other => return unsup(&format!("list-join-of:{other:?}")),
                }
                self.lower(sep, Some(STR))?;
                self.f.instructions().call(F_LIST_JOIN);
                Ok(Some(STR))
            }
            ("map", [xs, cb]) => self.lower_list_map(xs, cb),
            ("filter", [xs, cb]) => self.lower_list_filter(xs, cb),
            ("fold", [xs, init, cb]) => self.lower_list_fold(xs, init, cb),
            _ => unsup(&format!("call:list.{func}")),
        }
    }

    /// A literal-lambda HOF callback: (param locals, body). Fn-typed
    /// VALUES are a later mechanism — the direct-lambda form is the
    /// dominant idiom (153:31 in the corpus) and inlines with zero
    /// closure machinery: captures are just enclosing locals in scope.
    fn hof_lambda<'e>(
        &mut self,
        cb: &'e IrExpr,
        arity: usize,
    ) -> Result<(Vec<u32>, &'e IrExpr), EmitError> {
        let IrExprKind::Lambda { params, body, .. } = &cb.kind else {
            return unsup("list-hof-nonlambda");
        };
        if params.len() != arity {
            return unsup("list-hof-arity");
        }
        let mut idxs = Vec::new();
        for (var, _) in params {
            let Some(&(idx, _)) = self.locals.get(var) else {
                return unsup("bind:unmapped");
            };
            idxs.push(idx);
        }
        Ok((idxs, body))
    }

    /// Shared loop header: xs → holds (base, count, idx); returns them.
    fn hof_loop_open(
        &mut self,
        xs: &IrExpr,
    ) -> Result<(SliceTy, u32, u32, u32), EmitError> {
        let elem = match self.lower(xs, None)? {
            SliceTy::List(h) => self.types.el(h),
            other => return unsup(&format!("list-hof-of:{other:?}")),
        };
        let bh = self.hold_i32()?;
        let ch = self.hold_i32()?;
        let ih = self.hold_i32()?;
        self.f.instructions().local_tee(bh);
        self.f
            .instructions()
            .i32_load(len_memarg())
            .i32_const(elem.slot_size() as i32)
            .i32_div_u()
            .local_set(ch)
            .i32_const(0)
            .local_set(ih);
        Ok((elem, bh, ch, ih))
    }

    /// Loop-body prologue: guard + load current element into `param`.
    fn hof_elem_into(&mut self, elem: SliceTy, bh: u32, ch: u32, ih: u32, param: u32) {
        self.f.instructions().local_get(ih).local_get(ch).i32_ge_u().br_if(1);
        self.f
            .instructions()
            .local_get(bh)
            .local_get(ih)
            .i32_const(elem.slot_size() as i32)
            .i32_mul()
            .i32_add();
        self.load_ty_slot(elem, 0);
        self.f.instructions().local_set(param);
    }

    fn hof_step(&mut self, ih: u32) {
        self.f.instructions().local_get(ih).i32_const(1).i32_add().local_set(ih).br(0).end().end();
    }

    fn lower_list_map(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let Some(u) = slice_ty_of(&body.ty, self.types) else {
            return unsup(&format!("list-map-ret:{}", ty_name(&body.ty)));
        };
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        // result = alloc(count * stride_u), same element count as source
        let rh = self.hold_i32()?;
        self.f
            .instructions()
            .local_get(ch)
            .i32_const(u.slot_size() as i32)
            .i32_mul()
            .call(F_ALLOC)
            .local_set(rh);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        // dest addr, then value, then store
        self.f
            .instructions()
            .local_get(rh)
            .local_get(ih)
            .i32_const(u.slot_size() as i32)
            .i32_mul()
            .i32_add();
        self.lower(body, Some(u))?;
        self.store_ty_slot(u, 0);
        self.hof_step(ih);
        self.f.instructions().local_get(rh);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(SliceTy::List(self.types.intern(u))))
    }

    fn lower_list_filter(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let rh = self.hold_i32()?;
        self.f.instructions().i32_const(0).call(F_ALLOC).local_set(rh); // []
        let push = match elem.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower(body, Some(BOOL))?;
        self.f.instructions().if_(BlockType::Empty);
        self.f
            .instructions()
            .local_get(rh)
            .local_get(params[0])
            .call(push)
            .local_set(rh);
        self.f.instructions().end();
        self.hof_step(ih);
        self.f.instructions().local_get(rh);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(SliceTy::List(self.types.intern(elem))))
    }

    fn lower_list_fold(
        &mut self,
        xs: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 2)?;
        let (acc_p, x_p) = (params[0], params[1]);
        let Some(b) = slice_ty_of(&init.ty, self.types) else {
            return unsup(&format!("list-fold-acc:{}", ty_name(&init.ty)));
        };
        self.lower(init, Some(b))?;
        self.f.instructions().local_set(acc_p);
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, x_p);
        self.lower(body, Some(b))?;
        self.f.instructions().local_set(acc_p);
        self.hof_step(ih);
        self.f.instructions().local_get(acc_p);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(b))
    }

    /// `list.get_or(xs, i, d)`: (xs.get(i)) ?? d, inlined via the get
    /// helper — extracted for complexity budget.
    pub(crate) fn lower_list_get_or(
        &mut self,
        xs: &IrExpr,
        idx: &IrExpr,
        default: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let elem = match self.lower(xs, None)? {
            SliceTy::List(h) => self.types.el(h),
            other => return unsup(&format!("list-get-of:{other:?}")),
        };
        self.lower(idx, Some(INT))?;
        let helper = match elem.slot_size() {
            8 => F_LIST_GET_8,
            _ => F_LIST_GET_4,
        };
        self.f
            .instructions()
            .call(helper)
            .local_tee(self.scr_i32_local)
            .i32_eqz()
            .if_(BlockType::Result(elem.val_type()));
        self.lower(default, Some(elem))?;
        self.f.instructions().else_().local_get(self.scr_i32_local);
        self.load_ty_slot(elem, almide_layout::OPTION_FIELD);
        self.f.instructions().end();
        Ok(Some(elem))
    }

    /// `println`/`eprintln`: interpolations build in the line buffer;
    /// everything else must lower to a String block and goes through the
    /// stream's block-print helper.
    pub(crate) fn lower_print(&mut self, arg: &IrExpr, import: u32, block_print: u32) -> Result<(), EmitError> {
        if let IrExprKind::StringInterp { parts } = &arg.kind {
            let start = self.lower_interp_build(parts)?;
            // print(start, cursor - start), then release the buffer region.
            self.f
                .instructions()
                .local_get(start)
                .local_get(self.cursor_local)
                .local_get(start)
                .i32_sub()
                .call(import)
                .local_get(start)
                .global_set(G_LINE_CURSOR);
            self.release_i32();
            return Ok(());
        }
        self.lower(arg, Some(STR))?;
        self.f.instructions().call(block_print);
        Ok(())
    }

    /// Build interpolation parts into the line buffer from the CURRENT
    /// global cursor (stack-disciplined: nested value-position builds
    /// start after our partial content and restore on their exit).
    /// Returns the hold local carrying the build's start; the caller
    /// consumes the region [start, cursor_local), then must restore
    /// `G_LINE_CURSOR = start` and `release_i32()`.
    pub(crate) fn lower_interp_build(
        &mut self,
        parts: &[IrStringPart],
    ) -> Result<u32, EmitError> {
        let start = self.hold_i32()?;
        self.f
            .instructions()
            .global_get(G_LINE_CURSOR)
            .local_tee(start)
            .local_set(self.cursor_local);
        for part in parts {
            match part {
                IrStringPart::Lit { value } => {
                    if value.is_empty() {
                        continue;
                    }
                    let base = self.pool.intern(value);
                    let len = value.len() as i32;
                    self.f
                        .instructions()
                        .local_get(self.cursor_local)
                        .i32_const((base + almide_layout::PAYLOAD) as i32)
                        .i32_const(len)
                        .call(F_APPEND_COPY)
                        .local_set(self.cursor_local);
                }
                IrStringPart::Expr { expr } => {
                    // Publish our cursor so a nested build starts past it.
                    self.f
                        .instructions()
                        .local_get(self.cursor_local)
                        .global_set(G_LINE_CURSOR);
                    self.f.instructions().local_get(self.cursor_local);
                    match self.lower(expr, None)? {
                        INT => {
                            self.f
                                .instructions()
                                .call(F_APPEND_I64)
                                .local_set(self.cursor_local);
                        }
                        STR => {
                            // stack: cur, base → cur, payload, len
                            self.f
                                .instructions()
                                .local_tee(self.tmp_i32_local)
                                .i32_const(almide_layout::PAYLOAD as i32)
                                .i32_add()
                                .local_get(self.tmp_i32_local)
                                .i32_load(len_memarg())
                                .call(F_APPEND_COPY)
                                .local_set(self.cursor_local);
                        }
                        BOOL => {
                            self.f
                                .instructions()
                                .call(F_APPEND_BOOL)
                                .local_set(self.cursor_local);
                        }
                        other => return unsup(&format!("interp-part:{other:?}")),
                    }
                }
            }
        }
        Ok(start)
    }

}
