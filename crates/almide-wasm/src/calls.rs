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
        self.lower_call_at(target, args, false, None)
    }

    pub(crate) fn lower_call_at(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
        tail: bool,
        ret_hint: Option<SliceTy>,
    ) -> Result<Option<SliceTy>, EmitError> {
        match target {
            // Computed callee: a function VALUE — call_indirect through
            // the funcref table (args first, +1-biased slot last).
            CallTarget::Computed { callee } => {
                let got = self.lower(callee, None)?;
                let SliceTy::Fn(sig) = got else {
                    return unsup(&format!("computed-callee-{got:?}"));
                };
                let def = self.types.fn_sig_def(sig);
                if args.len() != def.params.len() {
                    return unsup("computed-arity");
                }
                // Closure convention: env block is arg 0; the callee's
                // table slot is the block's first payload field.
                let h = self.hold_i32()?;
                self.f.instructions().local_set(h);
                self.f.instructions().local_get(h);
                for (a, p) in args.iter().zip(def.params.iter()) {
                    self.lower(a, Some(*p))?;
                }
                self.f.instructions().local_get(h).i32_load(slot_memarg(0));
                let mut ps: Vec<ValType> = vec![ValType::I32];
                ps.extend(def.params.iter().map(|t| t.val_type()));
                let ti = self.work.itype(ps, def.ret.map(SliceTy::val_type));
                // Encoder argument order is (table, type).
                if tail && def.ret.is_some() && def.ret == self.fn_ret {
                    self.f.instructions().return_call_indirect(0, ti);
                } else {
                    self.f.instructions().call_indirect(0, ti);
                }
                self.release_i32();
                let _ = ret_hint;
                Ok(def.ret)
            }
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
                // Variant constructor? Concrete ctors resolve by the global
                // map; GENERIC instances resolve by name within the call's
                // annotated type (the ret hint) — ctor names are ambiguous
                // across instances, the type context is not.
                let ctor = self.types.ctors.get(name).copied().or_else(|| {
                    let SliceTy::Named(ti) = ret_hint? else { return None };
                    let NamedDef::Variant(v) = self.types.def(ti) else { return None };
                    let ci = v.cases.iter().position(|c| c.name == name)?;
                    Some((ti, ci as u32))
                });
                if let Some((ti, ci)) = ctor {
                    let (size, tag, fields) = {
                        let NamedDef::Variant(v) = &self.types.def(ti) else {
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
                // Entry fns resolve by name; a miss falls back to the
                // module-fn simple-name index (intra-module calls arrive
                // as Named after lower_module).
                // Intra-module Named calls resolve within the CURRENT
                // module first (simple names collide across modules),
                // then the entry program's globals.
                let resolved = self
                    .cur_module
                    .and_then(|m| self.table.by_name.get(&format!("{m}.{name}")))
                    .or_else(|| self.table.by_name.get(name))
                    .copied();
                let Some(i) = resolved else {
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
                self.calls.insert(i);
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
                if module.as_str() == "process" && func.as_str() == "exit" =>
            {
                // The abort floor (C-153 family): surface the code to the
                // host import, then trap. The host records the code BEFORE
                // the unwind, so exit-code parity is exact; the trailing
                // `unreachable` keeps the stack polymorphic (nothing after
                // `process.exit` executes on any target).
                match args.first() {
                    Some(a) => {
                        self.lower(a, Some(INT))?;
                        self.f.instructions().i32_wrap_i64();
                    }
                    None => {
                        self.f.instructions().i32_const(1);
                    }
                }
                self.f.instructions().call(F_EXIT_IMPORT).unreachable();
                Ok(None)
            }
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
            CallTarget::Module { module, func, .. } if module.as_str() == "bytes" => {
                self.lower_bytes_call(func.as_str(), args)
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string" && func.as_str() == "to_bytes" && args.len() == 1 =>
            {
                self.lower(&args[0], Some(STR))?;
                self.f.instructions().call(F_BLOCK_COPY);
                Ok(Some(SliceTy::Scalar(Scalar::Bytes)))
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string"
                    && func.as_str() == "slice"
                    && (args.len() == 2 || args.len() == 3) =>
            {
                self.lower(&args[0], Some(STR))?;
                self.lower(&args[1], Some(INT))?;
                if let Some(e) = args.get(2) {
                    self.lower(e, Some(INT))?;
                } else {
                    // the surface's `end` default: i64::MAX ("to the end")
                    self.f.instructions().i64_const(i64::MAX);
                }
                self.f.instructions().call(F_STR_SLICE);
                Ok(Some(STR))
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string" && func.as_str() == "repeat" && args.len() == 2 =>
            {
                self.lower(&args[0], Some(STR))?;
                self.lower(&args[1], Some(INT))?;
                self.f.instructions().call(F_STR_REPEAT);
                Ok(Some(STR))
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "list" => {
                self.lower_list_call(func.as_str(), args)
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "map" => {
                self.lower_map_call(func.as_str(), args, ret_hint)
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "set" => {
                self.lower_set_call(func.as_str(), args, ret_hint)
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "prim" => {
                self.lower_prim_call(func.as_str(), args)
            }
            CallTarget::Module { module, func, .. } => {
                // Linked module functions live in the table under their
                // qualified name. A stdlib SURFACE call additionally
                // resolves through the self-host registry to its loaded
                // implementation (same registry the interp's bridge uses —
                // one IR, two sound resolutions). Anything else is an
                // honest wall.
                let key = format!("{}.{}", module.as_str(), func.as_str());
                let Some(i) = self.resolve_qualified(&key) else {
                    return unsup(&format!("call:{key}"));
                };
                let info = &self.table.infos[i];
                if let Some(r) = &info.refuse {
                    return unsup(&format!("call-fn:{key}:{r}"));
                }
                if args.len() != info.params.len() {
                    return unsup(&format!("call-arity:{key}"));
                }
                let (index, ret, params) = (info.wasm_index, info.ret, info.params.clone());
                for (a, want) in args.iter().zip(params) {
                    self.lower(a, Some(want))?;
                }
                self.calls.insert(i);
                if tail && ret.is_some() && ret == self.fn_ret {
                    self.f.instructions().return_call(index);
                } else {
                    self.f.instructions().call(index);
                }
                Ok(ret)
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
                // The 8-byte helper's value param is i64; an f64 element
                // crosses the call boundary as its BIT PATTERN (memory is
                // bytes — the consumer reloads the slot as f64).
                if elem.val_type() == wasm_encoder::ValType::F64 {
                    self.f.instructions().i64_reinterpret_f64();
                }
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
            ("enumerate", [xs]) => {
                let elem = match self.lower(xs, None)? {
                    SliceTy::List(h) => self.types.el(h),
                    other => return unsup(&format!("list-enumerate-of:{other:?}")),
                };
                let pair_ti = self.types.tuple(vec![INT, elem]);
                let pdef = self.types.tuple_def(pair_ti);
                let (ioff, eoff, psize) =
                    (pdef.fields[0].1, pdef.fields[1].1, pdef.size);
                let stride = elem.slot_size();
                let bh = self.hold_i32()?;
                let ch = self.hold_i32()?;
                let ih = self.hold_i32()?;
                let rh = self.hold_i32()?;
                let ph = self.hold_i32()?;
                self.f.instructions().local_tee(bh);
                self.f
                    .instructions()
                    .i32_load(len_memarg())
                    .i32_const(stride as i32)
                    .i32_div_u()
                    .local_set(ch)
                    .i32_const(0)
                    .local_set(ih);
                // result list of pair addresses
                self.f
                    .instructions()
                    .local_get(ch)
                    .i32_const(4)
                    .i32_mul()
                    .call(F_ALLOC)
                    .local_set(rh);
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.f.instructions().local_get(ih).local_get(ch).i32_ge_u().br_if(1);
                // pair block
                self.f.instructions().i32_const(psize as i32).call(F_ALLOC).local_set(ph);
                self.f
                    .instructions()
                    .local_get(ph)
                    .local_get(ih)
                    .i64_extend_i32_u();
                self.store_ty_slot(INT, ioff);
                self.f.instructions().local_get(ph);
                self.f
                    .instructions()
                    .local_get(bh)
                    .local_get(ih)
                    .i32_const(stride as i32)
                    .i32_mul()
                    .i32_add();
                self.load_ty_slot(elem, 0);
                self.store_ty_slot(elem, eoff);
                // store pair addr into result
                self.f
                    .instructions()
                    .local_get(rh)
                    .local_get(ih)
                    .i32_const(4)
                    .i32_mul()
                    .i32_add()
                    .local_get(ph);
                self.store_ty_slot(SliceTy::Tuple(pair_ti), 0);
                self.f
                    .instructions()
                    .local_get(ih)
                    .i32_const(1)
                    .i32_add()
                    .local_set(ih)
                    .br(0)
                    .end()
                    .end();
                self.f.instructions().local_get(rh);
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(Some(SliceTy::List(self.types.intern(SliceTy::Tuple(pair_ti)))))
            }
            ("slice", [xs, a, b]) => {
                let (h, elem) = match self.lower(xs, None)? {
                    SliceTy::List(h) => (h, self.types.el(h)),
                    other => return unsup(&format!("list-slice-of:{other:?}")),
                };
                let stride = elem.slot_size() as i64;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(a, Some(INT))?;
                let ah = self.hold_i64()?;
                self.f.instructions().local_set(ah);
                self.lower(b, Some(INT))?;
                let eh = self.hold_i64()?;
                // e = min(b, count); s = a; s < 0 or s >= e → []
                let mut ins = self.f.instructions();
                ins.local_tee(eh);
                ins.local_get(bh)
                    .i32_load(len_memarg())
                    .i64_extend_i32_u()
                    .i64_const(stride)
                    .i64_div_s();
                ins.local_get(eh);
                ins.local_get(bh)
                    .i32_load(len_memarg())
                    .i64_extend_i32_u()
                    .i64_const(stride)
                    .i64_div_s();
                ins.i64_lt_s().select().local_set(eh);
                // empty when a < 0 (usize-wrap semantics) or a >= e
                ins.local_get(ah).i64_const(0).i64_lt_s();
                ins.local_get(ah).local_get(eh).i64_ge_s();
                ins.i32_or().if_(BlockType::Result(ValType::I32));
                ins.i32_const(0).call(F_ALLOC);
                ins.else_();
                // alloc (e-a)*stride; copy from base + a*stride
                ins.local_get(eh)
                    .local_get(ah)
                    .i64_sub()
                    .i64_const(stride)
                    .i64_mul()
                    .i32_wrap_i64()
                    .call(F_ALLOC)
                    .local_tee(self.tmp_i32_local)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add();
                ins.local_get(bh)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(ah)
                    .i64_const(stride)
                    .i64_mul()
                    .i32_wrap_i64()
                    .i32_add();
                ins.local_get(eh)
                    .local_get(ah)
                    .i64_sub()
                    .i64_const(stride)
                    .i64_mul()
                    .i32_wrap_i64();
                ins.memory_copy(0, 0);
                ins.local_get(self.tmp_i32_local);
                ins.end();
                self.release_i64();
                self.release_i64();
                self.release_i32();
                Ok(Some(SliceTy::List(h)))
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

    /// Resolve a qualified stdlib call ("float.to_string") to a table
    /// index: linked module fns first, then the self-host registry's
    /// implementation index.
    pub(crate) fn resolve_qualified(&self, key: &str) -> Option<usize> {
        self.table.by_name.get(key).copied().or_else(|| {
            let impl_fn = almide_types::self_host_registry::self_host_runtime()
                .iter()
                .flat_map(|(_, maps)| maps.iter())
                .find(|(_, surface)| *surface == key)
                .map(|(impl_fn, _)| *impl_fn)?;
            // WHITELIST: linked impls join the resolvable set ONE AT A
            // TIME, each landing with its own parity evidence — the
            // signature heuristic missed incumbent-layout coupling twice
            // (8-byte list slots, header-writing string builders).
            const VERIFIED: &[&str] = &[
                "float_to_string",
                "float_to_string_compound",
                "float_to_fixed",
                "int_to_string",
                // Dragon4's own dependency closure (prim-only bodies).
                "math_log",
                "math_log2",
                "math_log10",
                // String->String, raw stores build STRING blocks only —
                // the string layout is digest-shared with the incumbent.
                "string_trim",
                // (Int, Int) -> List[Int]: raw stores build 8-byte Int
                // slots — the one list class both layouts share. Carries
                // its own C-169 ceiling die.
                "list_repeat",
                // String->String: byte-level string building; its tuple
                // helpers are module fns lowered by THIS emitter.
                "string_to_upper",
            ];
            // Second tier: signatures that TRIP the coupled-type proxy
            // below but whose bodies are AUDITED raw-write-free — every
            // sum is built via language-level ok()/err() (lowered by THIS
            // emitter with THIS layout) and every prim access is a
            // read-only load on a layout-shared block (string payload).
            // The proxy guards hand-written block internals; it misfires
            // on constructor-built sums.
            const VERIFIED_SUM_BUILDERS: &[&str] =
                &["string_to_int", "int_from_hex", "float_parse"];
            if !VERIFIED.contains(&impl_fn) && !VERIFIED_SUM_BUILDERS.contains(&impl_fn) {
                return None;
            }
            let i = self.table.impl_index.get(impl_fn).copied()?;
            // LAYOUT BOUNDARY: self-host impls encode the INCUMBENT's
            // block layout. Scalars, strings and List[scalar] match our
            // ratified layout byte-for-byte; sums/maps/sets/tuples/named
            // do NOT (the incumbent keeps the Result tag in the len slot
            // — found by the burn-up: result.unwrap_or(ok(5)) returned
            // the default). An impl whose signature carries a
            // layout-coupled type stays UNRESOLVED (honest wall) until
            // the layouts are deliberately reconciled.
            let info = &self.table.infos[i];
            let coupled = |t: &SliceTy| {
                match t {
                    SliceTy::Option(_)
                    | SliceTy::Result(..)
                    | SliceTy::Map(..)
                    | SliceTy::Set(_)
                    | SliceTy::Tuple(_)
                    | SliceTy::Named(_) => true,
                    // The incumbent packs EVERY list element into an
                    // 8-byte slot; ours are 4 for the i32 word class —
                    // List[Int]/List[Float] agree, List[String]/List[Bool]
                    // do not (string.join through a linked impl trapped).
                    SliceTy::List(h) => self.types.el(*h).slot_size() == 4,
                    _ => false,
                }
            };
            if !VERIFIED_SUM_BUILDERS.contains(&impl_fn)
                && (info.params.iter().any(coupled) || info.ret.as_ref().is_some_and(coupled))
            {
                return None;
            }
            Some(i)
        })
    }

    /// `${list}`: append "[e, e]" into the line buffer natively. Enters
    /// with `[cursor, list]` on the stack (the shared part preamble);
    /// leaves the cursor local updated.
    fn emit_list_display(&mut self, el: SliceTy) -> Result<(), EmitError> {
        let stride = el.slot_size() as i32;
        let open_b = self.pool.intern("[");
        let close_b = self.pool.intern("]");
        let sep = self.pool.intern(", ");
        let hb = self.hold_i32()?;
        let end = self.hold_i32()?;
        let cur = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_set(hb);
            // The preamble pushed the line cursor; appends below manage
            // cursor_local themselves.
            i.drop();
            // '['
            i.local_get(self.cursor_local)
                .i32_const(open_b as i32 + almide_layout::PAYLOAD as i32)
                .i32_const(1)
                .call(F_APPEND_COPY)
                .local_set(self.cursor_local);
            i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(cur);
            i.local_get(cur)
                .local_get(hb)
                .i32_load(len_memarg())
                .i32_add()
                .local_set(end);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
            // separator for every element after the first
            i.local_get(cur)
                .local_get(hb)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .i32_ne()
                .if_(BlockType::Empty);
            i.local_get(self.cursor_local)
                .i32_const(sep as i32 + almide_layout::PAYLOAD as i32)
                .i32_const(2)
                .call(F_APPEND_COPY)
                .local_set(self.cursor_local);
            i.end();
        }
        match el {
            INT => {
                let mut i = self.f.instructions();
                i.local_get(self.cursor_local);
                i.local_get(cur).i64_load(wasm_encoder::MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                });
                i.call(F_APPEND_I64).local_set(self.cursor_local);
            }
            FLOAT => {
                // Element formatting = the SAME compound form the oracle
                // uses for `${float}` (integer-valued floats drop ".0").
                let Some(idx) = self.resolve_qualified("float.to_string_compound") else {
                    return unsup("interp-part:ListFloat-unlinked");
                };
                let info = &self.table.infos[idx];
                if info.refuse.is_some() || info.ret != Some(STR) {
                    return unsup("interp-part:ListFloat-impl");
                }
                let widx = info.wasm_index;
                self.calls.insert(idx);
                let mut i = self.f.instructions();
                i.local_get(cur).f64_load(wasm_encoder::MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                });
                i.call(widx).local_set(self.tmp_i32_local);
                i.local_get(self.cursor_local)
                    .local_get(self.tmp_i32_local)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(self.tmp_i32_local)
                    .i32_load(len_memarg())
                    .call(F_APPEND_COPY)
                    .local_set(self.cursor_local);
            }
            _ => return unsup("interp-part:List-el"),
        }
        {
            let mut i = self.f.instructions();
            i.local_get(cur).i32_const(stride).i32_add().local_set(cur);
            i.br(0);
            i.end();
            i.end();
            // ']'
            i.local_get(self.cursor_local)
                .i32_const(close_b as i32 + almide_layout::PAYLOAD as i32)
                .i32_const(1)
                .call(F_APPEND_COPY)
                .local_set(self.cursor_local);
        }
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(())
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
                        // Float parts format through the SAME linked
                        // Dragon4 the oracle uses (float.to_string), then
                        // append as a string block.
                        FLOAT => {
                            let Some(i) = self.resolve_qualified("float.to_string_compound") else {
                                return unsup("interp-part:Float-unlinked");
                            };
                            let info = &self.table.infos[i];
                            if info.refuse.is_some() || info.ret != Some(STR) {
                                return unsup("interp-part:Float-impl");
                            }
                            let idx = info.wasm_index;
                            self.calls.insert(i);
                            self.f
                                .instructions()
                                .call(idx)
                                .local_tee(self.tmp_i32_local)
                                .i32_const(almide_layout::PAYLOAD as i32)
                                .i32_add()
                                .local_get(self.tmp_i32_local)
                                .i32_load(len_memarg())
                                .call(F_APPEND_COPY)
                                .local_set(self.cursor_local);
                        }
                        SliceTy::List(h)
                            if matches!(
                                self.types.el(h),
                                SliceTy::Scalar(Scalar::Int) | SliceTy::Scalar(Scalar::Float)
                            ) =>
                        {
                            // NATIVE display build: "[e, e]" appended
                            // straight into the line buffer. The linked
                            // list_to_string impls read the len header as
                            // COUNT (the incumbent's convention; ours is
                            // BYTES) — a read-side layout coupling the
                            // whitelist audit now checks for.
                            let el = self.types.el(h);
                            self.emit_list_display(el)?;
                        }
                        other => return unsup(&format!("interp-part:{other:?}")),
                    }
                }
            }
        }
        Ok(start)
    }

}
