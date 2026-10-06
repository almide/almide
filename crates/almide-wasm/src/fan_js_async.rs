//! Overlapped waits for a `fan` over async JS hooks on `--host js` (#3383).
//!
//! An `@extern(wasm, "js", NAME, returns: promise)` hook (#3371) is a
//! request to a system outside the process, so the order its requests
//! arrive in is the environment's ω (ADR-0024 D2/D3, amended 2026-10-05):
//! it does not force a fan sequential. Under JSPI (ADR-0024 D8 on the JS
//! host) a fan whose every element is ONE such call on values it already
//! has is lowered as the p3 prefetch protocol is (fan.rs, ops 40/41/42),
//! with three generated imports of [`FAN_MODULE`] as the substrate:
//!
//! - `start:NAME(args…) -> slot` — plain; calls the hook, keeps its Promise;
//! - `wait()` — the ONLY Suspending import; settles every started slot;
//! - `take:NAME(slot) -> ret` — plain; the settled value, marshalled like the
//!   hook's own return (a rejection is the err of a fallible extern and
//!   abandons the instance for an infallible one, #3356).
//!
//! Every element starts in arm order, the module suspends once, and the
//! values are taken in arm order: every element runs (D1), the first `Err`
//! in arm order is the result (D6), so the observation is the sequential
//! lowering's (N7). `fan.any` takes until its first ok; the remaining
//! slots were awaited by `wait` (N5) and are dropped by the glue at the
//! next batch, never marshalled.
//!
//! Anything outside that shape stays sequential, as does every fan when
//! `ALMIDE_FAN_SEQUENTIAL` is set (the ADR-0024 D7 ablation switch, which
//! the overlap gate uses to compare both lowerings). Without `--host js`
//! nothing here fires, so the module bytes of every other build are the
//! ones they were.

use almide_ir::{CallTarget, IrExpr, IrExprKind};
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::work::Helper;
use crate::host_exports::FAN_MODULE;
use crate::*;

/// Is `f` an `@extern(wasm, …, returns: promise)` (#3371)?
pub(crate) fn returns_promise(f: &almide_ir::IrFunction) -> bool {
    f.extern_attrs.iter().any(|a| a.target.as_str() == "wasm" && a.returns_promise)
}

/// One element's hook call: the table fn of the async extern, its argument
/// expressions, and whether the element wraps an infallible hook's value
/// in `ok(…)` (the callback form of `fan.map` / `fan.any`).
pub(crate) struct HookCall<'a> {
    hook: usize,
    args: &'a [IrExpr],
    wrap_ok: bool,
}

/// Is the overlapped lowering available for this emission?
fn overlap_enabled() -> bool {
    crate::host_exports::js_host() && !almide_base::env::flag("ALMIDE_FAN_SEQUENTIAL")
}

/// An argument the element already has: a variable or a literal. Moving its
/// evaluation before the other elements' calls is unobservable.
fn computed(a: &IrExpr) -> bool {
    matches!(
        a.kind,
        IrExprKind::Var { .. }
            | IrExprKind::LitInt { .. }
            | IrExprKind::LitFloat { .. }
            | IrExprKind::LitStr { .. }
            | IrExprKind::LitBool { .. }
    )
}

/// The declared protocol imports of the finished module: one per
/// registered `FanStart` / `FanWait` / `FanTake` helper, whose stub body
/// the `imports::declare` post-pass removes.
pub(crate) fn declared_protocol(table: &FnTable, work: &FnWork) -> Vec<crate::imports::Declared> {
    let base = work.helper_base.get();
    let hook_name = |hook: usize| table.infos[hook].import.as_ref().map_or(String::new(), |(_, n)| n.clone());
    work.helpers
        .borrow()
        .iter()
        .enumerate()
        .filter_map(|(pos, h)| {
            let name = match h {
                Helper::FanStart { hook, .. } => format!("start:{}", hook_name(*hook as usize)),
                Helper::FanWait => "wait".to_string(),
                Helper::FanTake { hook, .. } => format!("take:{}", hook_name(*hook as usize)),
                _ => return None,
            };
            Some(crate::imports::Declared { index: base + pos as u32, module: FAN_MODULE.to_string(), name })
        })
        .collect()
}

/// A protocol helper's wasm params (None = not one).
pub(crate) fn helper_params(h: &Helper) -> Option<Vec<ValType>> {
    match h {
        Helper::FanStart { params, .. } => Some(params.clone()),
        Helper::FanWait => Some(Vec::new()),
        Helper::FanTake { .. } => Some(vec![ValType::I32]),
        _ => None,
    }
}

/// A protocol helper's wasm result (the outer None = not one).
pub(crate) fn helper_result(h: &Helper) -> Option<Option<ValType>> {
    match h {
        Helper::FanStart { .. } => Some(Some(ValType::I32)),
        Helper::FanWait => Some(None),
        Helper::FanTake { ret, .. } => Some(*ret),
        _ => None,
    }
}

/// A protocol helper's body: the loud stub its declared import replaces.
pub(crate) fn helper_body(h: &Helper) -> Option<wasm_encoder::Function> {
    if !matches!(h, Helper::FanStart { .. } | Helper::FanWait | Helper::FanTake { .. }) {
        return None;
    }
    let mut f = wasm_encoder::Function::new([]);
    f.instructions().unreachable().end();
    Some(f)
}

impl Emitter<'_> {
    /// `fan.*` module calls: the overlap when it applies, else the fan
    /// lowerings of fan.rs. Ok(None) = not handled here.
    pub(crate) fn lower_fan_module_call(&mut self, func: &str, args: &[IrExpr]) -> Result<Option<Option<Lowered>>, EmitError> {
        match self.lower_fan_overlap_call(func, args)? {
            Some(t) => Ok(Some(Some(Lowered::owned(t)))),
            None => self.lower_fan_call(func, args),
        }
    }

    /// `fan.map` / `fan.any` whose element is one async-hook call, on
    /// `--host js` (ALMIDE_DBG_FAN names the route, as fan.rs does).
    fn lower_fan_overlap_call(&mut self, func: &str, args: &[IrExpr]) -> Result<Option<SliceTy>, EmitError> {
        let ("map" | "any" | "any_map", [xs, cb]) = (func, args) else { return Ok(None) };
        let Some(call) = self.fan_overlap_elem(cb) else { return Ok(None) };
        if almide_base::env::flag("ALMIDE_DBG_FAN") {
            eprintln!("[fan-dbg] fan.{func}: js-host overlap lowering engaged");
        }
        let t = if func == "map" { self.lower_fan_map_overlap(xs, cb, &call)? } else { self.lower_fan_any_overlap(xs, cb, &call)? };
        Ok(Some(t))
    }

    /// The async extern a call targets, by table index.
    fn async_hook(&self, target: &CallTarget) -> Option<usize> {
        let i = match target {
            CallTarget::Named { name } => self.resolve_named_fn(name.as_str())?,
            CallTarget::Module { module, func, .. } => *self.table.by_name.get(&format!("{module}.{func}"))?,
            _ => return None,
        };
        let info = &self.table.infos[i];
        let promise = info.import.as_ref().is_some_and(|(m, n)| crate::host_exports::is_async_import(m, n));
        (promise && info.refuse.is_none()).then_some(i)
    }

    /// `hook(args…)` with every argument already computed, and a value the
    /// fan can collect: a scalar, or a fallible hook's Result over one.
    fn hook_call<'a>(&self, e: &'a IrExpr) -> Option<HookCall<'a>> {
        let IrExprKind::Call { target, args, .. } = &e.kind else { return None };
        let hook = self.async_hook(target)?;
        let info = &self.table.infos[hook];
        if !args.iter().all(computed) || args.len() != info.params.len() {
            return None;
        }
        let payload = match info.ret? {
            SliceTy::Result(o, er) if self.types.el(er) == STR => self.types.el(o),
            t => t,
        };
        matches!(payload, SliceTy::Scalar(_)).then_some(HookCall { hook, args, wrap_ok: false })
    }

    /// The element of `fan.map` / `fan.any` (`(x) => hook(…)`, a fallible
    /// hook, or `(x) => ok(hook(…))`, an infallible one) when the overlap
    /// applies.
    pub(crate) fn fan_overlap_elem<'a>(&self, cb: &'a IrExpr) -> Option<HookCall<'a>> {
        if !overlap_enabled() {
            return None;
        }
        let IrExprKind::Lambda { params, body, .. } = &cb.kind else { return None };
        if params.len() != 1 {
            return None;
        }
        let body = crate::fan::strip_callback_try(body);
        match &body.kind {
            IrExprKind::ResultOk { expr } => {
                let call = self.hook_call(expr)?;
                let ret = self.table.infos[call.hook].ret;
                matches!(ret, Some(SliceTy::Scalar(_))).then_some(HookCall { wrap_ok: true, ..call })
            }
            _ => {
                let call = self.hook_call(body)?;
                matches!(self.table.infos[call.hook].ret, Some(SliceTy::Result(..))).then_some(call)
            }
        }
    }

    /// Every arm of a `fan { … }` block is one hook call: the plan.
    pub(crate) fn fan_overlap_block<'a>(&self, arms: &'a [IrExpr]) -> Option<Vec<HookCall<'a>>> {
        if !overlap_enabled() || arms.is_empty() {
            return None;
        }
        arms.iter().map(|a| self.hook_call(a)).collect()
    }

    /// `start:NAME(args…)` — the slot is left on the stack.
    fn fan_start(&mut self, call: &HookCall<'_>) -> Result<(), EmitError> {
        let params = self.table.infos[call.hook].params.clone();
        let helper = self.work.helper(Helper::FanStart {
            hook: call.hook as u32,
            params: params.iter().map(|t| t.val_type()).collect(),
        });
        self.arm_scope(|em| {
            for (a, want) in call.args.iter().zip(&params) {
                em.lower_arg(a, Some(*want), ArgMode::Borrow)?;
            }
            em.f.instructions().call(helper);
            Ok(())
        })
    }

    fn fan_wait(&mut self) {
        let helper = self.work.helper(Helper::FanWait);
        self.f.instructions().call(helper);
    }

    /// `take:NAME(slot)` with the slot on the stack: the hook's value (its
    /// import ABI: a scalar, or a fallible hook's Result block) is left on
    /// the stack and its type returned.
    pub(crate) fn fan_take(&mut self, call: &HookCall<'_>) -> Result<SliceTy, EmitError> {
        let Some(ret) = self.table.infos[call.hook].ret else {
            return unsup("fan-overlap-unit");
        };
        let helper = self.work.helper(Helper::FanTake { hook: call.hook as u32, ret: Some(ret.val_type()) });
        self.f.instructions().call(helper);
        Ok(ret)
    }

    /// The block form's phase A: one slot hold per arm (in arm order), every
    /// hook started, one wait. The caller takes in arm order and releases
    /// the holds after its own.
    pub(crate) fn fan_overlap_block_start(&mut self, calls: &[HookCall<'_>]) -> Result<Vec<u32>, EmitError> {
        self.witness_decline("fan:js-overlap");
        let mut slots = Vec::new();
        for call in calls {
            let h = self.hold_i32()?;
            self.fan_start(call)?;
            self.f.instructions().local_set(h);
            slots.push(h);
        }
        self.fan_wait();
        Ok(slots)
    }

    /// The payload type of an element's value, and the element's Result
    /// type (the callback's) — `b` and `Result[b, String]`.
    fn elem_types(&mut self, call: &HookCall<'_>) -> Result<(SliceTy, SliceTy), EmitError> {
        match self.table.infos[call.hook].ret {
            Some(r @ SliceTy::Result(o, _)) if !call.wrap_ok => Ok((self.types.el(o), r)),
            Some(b @ SliceTy::Scalar(_)) if call.wrap_ok => {
                let r = SliceTy::Result(self.types.intern(b), self.types.intern(STR));
                Ok((b, r))
            }
            other => unsup(&format!("fan-overlap-ret:{other:?}")),
        }
    }

    /// Phase A of the callback forms: a slot buffer of `ch` words, every
    /// element's hook started in list order, one wait. Returns the buffer.
    fn fan_overlap_phase_a(&mut self, call: &HookCall<'_>, cb: &IrExpr, (elem, bh, ch, ih): (SliceTy, u32, u32, u32)) -> Result<u32, EmitError> {
        let (params, _) = self.hof_lambda(cb, 1)?;
        let hs = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(ch).i32_const(4).i32_mul().call(F_ALLOC).local_set(hs);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.f.instructions().local_get(hs).local_get(ih).i32_const(4).i32_mul().i32_add();
        self.fan_start(call)?;
        self.f.instructions().i32_store(slot_memarg(0));
        self.hof_step(ih);
        self.fan_wait();
        self.f.instructions().i32_const(0).local_set(ih);
        Ok(hs)
    }

    /// Phase B's loop head: past the last element, break; else the slot of
    /// element `ih` is taken (its value on the stack).
    fn fan_overlap_take_at(&mut self, call: &HookCall<'_>, hs: u32, ch: u32, ih: u32) -> Result<SliceTy, EmitError> {
        {
            let mut i = self.f.instructions();
            i.local_get(ih).local_get(ch).i32_ge_u().br_if(1);
            i.local_get(hs).local_get(ih).i32_const(4).i32_mul().i32_add();
            i.i32_load(slot_memarg(0));
        }
        self.fan_take(call)
    }

    /// Push the payload `b` held in `hv` onto the list accumulator `hacc`.
    fn fan_overlap_push(&mut self, hacc: u32, hv: u32, b: SliceTy) {
        let mut i = self.f.instructions();
        i.local_get(hacc).local_get(hv);
        if b.val_type() == ValType::F64 {
            i.i64_reinterpret_f64();
        }
        let push = if b.slot_size() == 8 { F_LIST_PUSH_8 } else { F_LIST_PUSH_4 };
        i.call(push).local_set(hacc);
    }

    /// `fan.map(xs, (x) => hook(…))` overlapped: start every element, wait
    /// once, take in list order — the first err is the result and the slots
    /// after it are not taken (their hooks already ran, D1).
    pub(crate) fn lower_fan_map_overlap(&mut self, xs: &IrExpr, cb: &IrExpr, call: &HookCall<'_>) -> Result<SliceTy, EmitError> {
        self.witness_decline("fan:js-overlap");
        let (b, _) = self.elem_types(call)?;
        let lp = self.hof_loop_open(xs)?;
        let (_, _, ch, ih) = lp;
        let hs = self.fan_overlap_phase_a(call, cb, lp)?;
        let hr = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let hv = self.hold_val(b)?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).local_set(hr);
            i.i32_const(0).call(F_ALLOC).local_set(hacc);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.fan_overlap_take_at(call, hs, ch, ih)?;
        if call.wrap_ok {
            self.f.instructions().local_set(hv);
            self.fan_overlap_push(hacc, hv, b);
        } else {
            let hp = self.hold_i32()?;
            {
                let mut i = self.f.instructions();
                i.local_set(hp);
                i.local_get(hp).i32_load(slot_memarg(almide_layout::SUM_TAG));
                i.if_(BlockType::Empty);
                // the first err IS the result: no later slot is taken.
                i.local_get(hp).local_set(hr);
                i.br(2);
                i.end();
                i.local_get(hp);
            }
            self.load_ty_slot(b, almide_layout::SUM_FIELD);
            self.f.instructions().local_set(hv);
            self.fan_overlap_push(hacc, hv, b);
            // the payload's credit moved into the list; the shell goes.
            self.f.instructions().local_get(hp).call(F_DEC_FLAT);
            self.release_i32();
        }
        self.hof_step(ih);
        let acc_dec = self.dec_fn_of(SliceTy::List(self.types.intern(b)));
        {
            let mut i = self.f.instructions();
            i.local_get(hs).call(F_DEC_FLAT);
            i.local_get(hr).i32_eqz().if_(BlockType::Empty);
            i.i32_const(16).call(F_ALLOC).local_tee(hr).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hr).local_get(hacc).i32_store(slot_memarg(almide_layout::SUM_FIELD));
            i.else_();
            i.local_get(hacc).call(acc_dec);
            i.end();
            i.local_get(hr);
        }
        self.release_val(b);
        // hacc, hr, hs, then the loop's ih, ch, bh.
        for _ in 0..6 {
            self.release_i32();
        }
        let lb = self.types.intern(SliceTy::List(self.types.intern(b)));
        Ok(SliceTy::Result(lb, self.types.intern(STR)))
    }

    /// `fan.any(xs, (x) => hook(…))` overlapped: start every element, wait
    /// once, take in list order until the first ok — an err skips its
    /// element; all-fail is the C-004 ledger Err. Untaken slots were
    /// awaited by `wait` and are dropped by the glue.
    pub(crate) fn lower_fan_any_overlap(&mut self, xs: &IrExpr, cb: &IrExpr, call: &HookCall<'_>) -> Result<SliceTy, EmitError> {
        self.witness_decline("fan:js-overlap");
        let (b, res) = self.elem_types(call)?;
        let lp = self.hof_loop_open(xs)?;
        let (_, _, ch, ih) = lp;
        let hs = self.fan_overlap_phase_a(call, cb, lp)?;
        let hr = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).local_set(hr);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.fan_overlap_take_at(call, hs, ch, ih)?;
        if call.wrap_ok {
            // an infallible hook's value wins: ok(value).
            let hv = self.hold_val(b)?;
            self.f.instructions().local_set(hv);
            self.f.instructions().i32_const(16).call(F_ALLOC).local_tee(hr).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_TAG));
            self.f.instructions().local_get(hr).local_get(hv);
            self.store_ty_slot(b, almide_layout::SUM_FIELD);
            self.f.instructions().br(1);
            self.release_val(b);
        } else {
            let res_dec = self.dec_fn_of(res);
            let hp = self.hold_i32()?;
            let mut i = self.f.instructions();
            i.local_set(hp);
            i.local_get(hp).i32_load(slot_memarg(almide_layout::SUM_TAG)).i32_eqz();
            i.if_(BlockType::Empty);
            i.local_get(hp).local_set(hr).br(2);
            i.end();
            // a losing err is released with its message.
            i.local_get(hp).call(res_dec);
            self.release_i32();
        }
        self.hof_step(ih);
        let msg = self.pool.intern("fan.any: all candidates failed");
        {
            let mut i = self.f.instructions();
            i.local_get(hs).call(F_DEC_FLAT);
            i.local_get(hr).i32_eqz().if_(BlockType::Empty);
            i.i32_const(16).call(F_ALLOC).local_tee(hr).i32_const(1).i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hr).i32_const(msg as i32).i32_store(slot_memarg(almide_layout::SUM_FIELD));
            i.end();
            i.local_get(hr);
        }
        // hr, hs, then the loop's ih, ch, bh.
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(res)
    }
}
