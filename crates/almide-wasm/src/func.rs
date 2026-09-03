//! The literal pool, per-function lowering plan/driver, and the callable
//! signature classifier — split from lib.rs for the complexity budget.

use std::collections::{HashMap, HashSet};

use almide_ir::{IrFunction, VarId};
use almide_types::types::Ty;
use wasm_encoder::{Function, ValType};

use crate::emitter::Emitter;
use crate::types_table::TypeTable;
use crate::*;

/// String literals placed in linear memory as REAL layout blocks.
pub(crate) struct Pool {
    pub(crate) data: Vec<u8>,
    pub(crate) interned: HashMap<String, u32>,
}

impl Pool {
    pub(crate) fn new() -> Self {
        // Reserve the null guard + itoa scratch: the layout's NULL_ADDR (0)
        // must never name a live block, and the scratch must not overlap
        // pool blocks.
        Pool { data: vec![0; POOL_START as usize], interned: HashMap::new() }
    }

    /// Intern `s` as a block; returns the block BASE address (deduped).
    pub(crate) fn intern(&mut self, s: &str) -> u32 {
        if let Some(&base) = self.interned.get(s) {
            return base;
        }
        let base = self.data.len() as u32;
        let bytes = s.as_bytes();
        let len = bytes.len() as u32;
        let mut header = vec![0u8; almide_layout::PAYLOAD as usize];
        header[almide_layout::RC.offset as usize..][..4].copy_from_slice(&1u32.to_le_bytes());
        header[almide_layout::LEN.offset as usize..][..4].copy_from_slice(&len.to_le_bytes());
        header[almide_layout::CAP.offset as usize..][..4].copy_from_slice(&len.to_le_bytes());
        self.data.extend_from_slice(&header);
        self.data.extend_from_slice(bytes);
        base
    }

    /// A static BLOCK with the given payload bytes (dedup by content) —
    /// capture-free closure blocks live in the pool, zero runtime alloc.
    pub(crate) fn intern_block(&mut self, payload: &[u8]) -> u32 {
        let key = format!("\u{0}blk:{payload:?}");
        if let Some(&base) = self.interned.get(&key) {
            return base;
        }
        let base = self.data.len() as u32;
        let len = payload.len() as u32;
        let mut header = vec![0u8; almide_layout::PAYLOAD as usize];
        header[almide_layout::RC.offset as usize..][..4].copy_from_slice(&1u32.to_le_bytes());
        header[almide_layout::LEN.offset as usize..][..4].copy_from_slice(&len.to_le_bytes());
        header[almide_layout::CAP.offset as usize..][..4].copy_from_slice(&len.to_le_bytes());
        self.data.extend_from_slice(&header);
        self.data.extend_from_slice(payload);
        self.interned.insert(key, base);
        base
    }
}

/// #1627: does this module-space initializer carry inner binds? Those
/// binds index the MODULE's VarTable, so the value cannot lower inline in
/// the calling frame — it needs its own.
fn initializer_needs_own_frame(il: &crate::InitLet, ctx: &Ctx) -> Result<bool, EmitError> {
    if il.space == 0 {
        return Ok(false);
    }
    let mut probe: Vec<(VarId, SliceTy)> = Vec::new();
    let mut probe_seen: HashSet<VarId> = HashSet::new();
    collect_binds(&il.tl.value, &mut probe, &mut probe_seen, ctx.types)?;
    Ok(!probe.is_empty())
}

/// #1627: register the binds-carrying module initializer as a synthetic
/// zero-param entry on the lifted pipeline — lowered in its own frame
/// with the module's space and name — and call_indirect it exactly as a
/// closure would (env 0: nothing to capture; no hop charge: native
/// charges nothing for reaching a top-let's value).
fn emit_modinit_call(em: &mut crate::emitter::Emitter<'_>, il: &crate::InitLet, declared: SliceTy) {
    let j = em.work.register_lambda(crate::LiftedLambda {
        params: Vec::new(),
        ret: Some(declared),
        effect_raw: None,
        body: il.tl.value.clone(),
        captures: Vec::new(),
        var_space: il.space,
        cur_module: il.module.clone(),
        charge_hop: false,
    });
    let slot = em.work.slot(crate::TableEntry::Lambda(j));
    let ti = em.work.itype(vec![ValType::I32], Some(declared.val_type()));
    em.f.instructions().i32_const(0); // env: unused
    em.f.instructions().i32_const(slot as i32);
    em.f.instructions().call_indirect(0, ti);
}

/// Lower one function body (used for `main` and every program function):
/// params become the leading locals, collected Binds follow, then the
/// scratch locals (interp cursor, tmp i32, match/unwrap subjects).
/// How one function's body meets its wasm signature.
#[derive(Clone)]
pub(crate) struct FnPlan {
    pub(crate) ret: Option<SliceTy>,
    /// The module this function belongs to (None = entry program).
    pub(crate) cur_module: Option<String>,
    /// Some(raw) = effect fn: the body yields RAW `raw`, then wraps
    /// `ok(..)` into the declared Result-block return.
    pub(crate) effect_raw: Option<SliceTy>,
    /// USER code — its loop heads / dyn ops charge the deterministic meter.
    pub(crate) metered: bool,
    /// Charge one unit at entry (non-exempt user fn, or any closure hop).
    pub(crate) charge_entry: bool,
    /// `main`: a propagated `!` error aborts with the native frame.
    pub(crate) in_main: bool,
    /// Lifted lambda: raw wasm param 0 is the closure ENV block; the
    /// prelude loads each capture into a fresh local (value snapshot).
    pub(crate) env_captures: Option<Vec<(VarId, SliceTy, u32, bool)>>,
    /// Which VarTable this body's VarIds index (0 = entry program,
    /// i+1 = `ir.modules[i]`) — the global-lookup space (#1596).
    pub(crate) var_space: u32,
    /// Some(name) = the witness sweep may record this fn (#1696 phase A):
    /// the straightline gate still decides; None = never (display
    /// helpers, lifted lambdas — later phases).
    pub(crate) witness_name: Option<String>,
}

pub(crate) fn lower_fn(
    params: &[(VarId, SliceTy)],
    plan: FnPlan,
    body: &IrExpr,
    top_lets: &[crate::InitLet],
    ctx: &Ctx,
    pool: &mut Pool,
) -> Result<(Function, HashSet<usize>), EmitError> {
    let FnPlan { ret, effect_raw, in_main, env_captures, cur_module, metered, charge_entry, var_space, witness_name } =
        plan;
    let cur_module = cur_module.as_deref();
    let env_shift: u32 = u32::from(env_captures.is_some());
    let mut locals: HashMap<VarId, (u32, SliceTy)> = HashMap::new();
    let mut seen: HashSet<VarId> = HashSet::new();
    for (i, (var, ty)) in params.iter().enumerate() {
        locals.insert(*var, (i as u32 + env_shift, *ty));
        seen.insert(*var);
    }

    // C-319: shared-cell vars (captured ∩ mutated) — their locals hold
    // the CELL ADDRESS (i32); env-captured cells arrive pre-flagged.
    let mut cell_vars = crate::cells::cell_vars_of(body);
    let mut binds: Vec<(VarId, SliceTy)> = Vec::new();
    if let Some(caps) = &env_captures {
        for (var, ty, _, is_cell) in caps {
            if *is_cell {
                cell_vars.insert(*var);
            }
            if seen.insert(*var) {
                binds.push((*var, *ty));
            }
        }
    }
    for il in top_lets {
        let tl = &il.tl;
        if slice_ty_of(&tl.ty, ctx.types).is_none() {
            return unsup(&format!("bind-ty:{}", ty_name(&tl.ty)));
        };
        if il.space == 0 {
            // The top-let var itself is a GLOBAL; only its initializer's
            // inner binds need main locals.
            seen.insert(tl.var);
            collect_binds(&tl.value, &mut binds, &mut seen, ctx.types)?;
        }
        // A MODULE-space initializer (#1596/#1627) allocates NOTHING in
        // this frame: bind-free values lower inline against globals only,
        // and binds-carrying values become synthetic lifted entries whose
        // own frame holds their locals (the emission loop below).
    }
    collect_binds(body, &mut binds, &mut seen, ctx.types)?;

    // C-320: a region ARM fn (its body binds budget_enter) repairs the
    // meter at a CUT — the cut's early return would skip the arm's own
    // trailing budget_exit, leaking depth/fuel and staling the verdict
    // (the incumbent's #1572). The saved-fuel bind var feeds the repair.
    let region_saved_var: Option<VarId> = body_region_enter_var(body);

    // Bound-range deferral (C-238): head-only range binds leave the
    // locals table entirely — they live as (start, end) i64 pairs after
    // the hold pools, and every for-in over them counts.
    let deferred = crate::ranges::deferred_ranges_of(body);
    binds.retain(|(v, _)| !deferred.contains_key(v));

    let mut local_decls: Vec<(u32, ValType)> = Vec::new();
    for (i, (var, ty)) in binds.iter().enumerate() {
        locals.insert(*var, (env_shift + (params.len() + i) as u32, *ty));
        // A cell var's local holds the cell ADDRESS.
        local_decls.push((
            1,
            if cell_vars.contains(var) { ValType::I32 } else { ty.val_type() },
        ));
    }
    let base = env_shift + (params.len() + binds.len()) as u32;
    let (cursor_local, tmp_i32_local, scr_i32_local, scr_i64_local, scr_f64_local) =
        (base, base + 1, base + 2, base + 3, base + 4);
    local_decls.push((3, ValType::I32)); // cursor, tmp, scr_i32
    local_decls.push((1, ValType::I64)); // scr_i64
    local_decls.push((1, ValType::F64)); // scr_f64
    // Hold pools: stack-disciplined scratch for constructs that must keep
    // an address/counter live ACROSS sub-expression lowering (list
    // literals, index bases, for-in state). Depth beyond the pool is an
    // honest unsup, never a corruption.
    let hold_i32_base = base + 5;
    let hold_i64_base = hold_i32_base + HOLD_I32_POOL;
    let hold_f64_base = hold_i64_base + HOLD_I64_POOL;
    local_decls.push((HOLD_I32_POOL, ValType::I32));
    local_decls.push((HOLD_I64_POOL, ValType::I64));
    local_decls.push((HOLD_F64_POOL, ValType::F64));
    // Deferred-range (start, end) i64 pairs after the pools.
    let mut deferred_ranges: HashMap<VarId, (u32, u32, bool)> = HashMap::new();
    let mut next_extra = hold_f64_base + HOLD_F64_POOL;
    // C-320 repair locals: depth-at-entry (i32).
    let region_depth_entry = if region_saved_var.is_some() {
        local_decls.push((1, ValType::I32));
        let l = next_extra;
        next_extra += 1;
        Some(l)
    } else {
        None
    };
    {
        let mut next = next_extra;
        let mut vars: Vec<_> = deferred.iter().collect();
        vars.sort_by_key(|(v, _)| **v);
        for (v, inc) in vars {
            deferred_ranges.insert(*v, (next, next + 1, *inc));
            next += 2;
        }
        if !deferred_ranges.is_empty() {
            local_decls.push((2 * deferred_ranges.len() as u32, ValType::I64));
        }
    }

    let mut f = Function::new(local_decls);
    let mut calls: HashSet<usize> = HashSet::new();
    {
        let mut em = Emitter {
            pool,
            locals: &locals,
            rc_param_ceiling: env_shift + params.len() as u32,
            rc_droppable_params: Vec::new(),
            rc_owned: std::collections::BTreeSet::new(),
            table: ctx.table,
            types: ctx.types,
            calls: &mut calls,
            fn_ret: ret,
            cursor_local,
            tmp_i32_local,
            scr_i32_local,
            scr_i64_local,
            hold_i32_base,
            hold_i32_depth: 0,
            hold_i64_base,
            hold_i64_depth: 0,
            hold_f64_base,
            hold_f64_depth: 0,
            scr_f64_local,
            loop_ctl: None,
            in_tail: false,
            branch_depth: 0,
            witness: None,
            cur_module,
            var_space,
            in_main,
            work: ctx.work,
            globals: ctx.globals,
            deferred_ranges: &deferred_ranges,
            metered,
            cells: &cell_vars,
            region_repair: region_saved_var.and_then(|v| {
                let saved = locals.get(&v)?.0;
                Some((saved, region_depth_entry.expect("allocated with the var")))
            }),
            f: &mut f,
        };
        // #1696 phase A: arm the witness recorder when the sweep is
        // collecting and the straightline gate admits this body (no
        // effect wrap, no captures, no top-let prelude — every excluded
        // form has RC sites the two hooks do not cover yet).
        if let Some(_name) = &witness_name
            && crate::witness::collecting()
            && effect_raw.is_none()
            && env_captures.is_none()
            && top_lets.is_empty()
            && crate::witness::straightline_subset(
                body,
                ret.is_some_and(crate::witness::heapish_ret),
            )
            .is_none()
        {
            let mut w = crate::witness::WitnessRecorder::new();
            for (k, &(_, pty)) in params.iter().enumerate() {
                if em.rc_droppable(pty) {
                    w.param_owned(env_shift + k as u32);
                }
            }
            em.witness = Some(w);
        }
        populate_tail_release_set(&mut em, cur_module, env_shift, params, body);
        if let Some((_, dl)) = em.region_repair {
            em.f.instructions().global_get(G_DET_DEPTH).local_set(dl);
        }
        if let Some(caps) = &env_captures {
            // env (raw param 0) → capture locals: by-value snapshot,
            // except C-319 cells, whose 4-byte ADDRESS is what travels.
            for (var, ty, off, is_cell) in caps {
                let (idx, _) = em.locals[var];
                em.f.instructions().local_get(0);
                if *is_cell {
                    em.f.instructions().i32_load(slot_memarg(*off));
                } else {
                    em.load_ty_slot(*ty, *off);
                }
                em.f.instructions().local_set(idx);
            }
        }
        for il in top_lets {
            let tl = &il.tl;
            let Some(&(gidx, declared)) = ctx.globals.get(&(il.space, tl.var)) else {
                return unsup(&format!("bind-ty:{}", ty_name(&tl.ty)));
            };
            // #1627: a MODULE-space initializer whose value carries inner
            // binds cannot lower inline (its binds index the module's
            // VarTable, not this frame's locals map) — it becomes a
            // synthetic entry in its own frame instead.
            if initializer_needs_own_frame(il, ctx)? {
                emit_modinit_call(&mut em, il, declared);
            } else {
                // A module initializer lowers in ITS OWN space: its Var
                // reads index that module's table and its bare Named calls
                // resolve module-qualified first (#1596).
                let saved_space = em.var_space;
                let saved_module = em.cur_module;
                em.var_space = il.space;
                em.cur_module = il.module.as_deref();
                let lowered = em.lower(&tl.value, Some(declared));
                em.var_space = saved_space;
                em.cur_module = saved_module;
                lowered?;
            }
            if matches!(
                declared,
                SliceTy::List(_)
                    | SliceTy::Map(..)
                    | SliceTy::Set(_)
                    | SliceTy::Scalar(Scalar::Bytes)
            ) {
                em.f.instructions().call(F_BLOCK_COPY);
            }
            em.f.instructions().global_set(gidx);
        }
        if charge_entry {
            em.emit_det_charge_const(1);
        }
        match (ret, effect_raw) {
            (None, _) => em.lower_stmt_expr(body)?,
            (Some(want), None) => {
                em.lower_tail(body, Some(want))?;
                // RC-3: a droppable result that may BORROW a local
                // takes +1 before the epilogue releases the owners.
                if em.rc_droppable(want)
                    && !crate::rc_ownership::rc_certainly_fresh(&crate::rc_ownership::rc_tail(body).kind)
                {
                    em.rc_inc_top();
                    if em.witness.is_some() {
                        let tail = crate::rc_ownership::rc_tail(body);
                        let src = if let almide_ir::IrExprKind::Var { id } = &tail.kind {
                            em.locals.get(id).map(|&(l, _)| l)
                        } else {
                            None
                        };
                        if let Some(w) = em.witness.as_mut() {
                            match src {
                                Some(l) if w.ret_move(l) => {}
                                _ => w.poison(),
                            }
                        }
                    }
                }
            }
            (Some(want), Some(raw)) => {
                // Tail marker ON: a RAW-typed tail call still cannot
                // `return_call` into this Result-returning frame (the
                // Named arm's ret == fn_ret guard keeps it a plain call),
                // but `f(…)!` in tail position with a MATCHING Result
                // type sees through Try/Unwrap and return_calls — the
                // effect-TCO contract (#557 / C-069, O(1) stack).
                if raw == SliceTy::Unit {
                    // A Unit-effect body is statement-shaped; the ok
                    // payload materializes after it runs.
                    em.lower_stmt_expr(body)?;
                    em.f.instructions().i32_const(0);
                } else {
                    em.lower_tail(body, Some(raw))?;
                    // RC-3: the raw payload rides inside the ok carrier
                    // past the epilogue — same borrow rule as the pure
                    // arm, and the +1 must precede the wrap.
                    if em.rc_droppable(raw)
                        && !crate::rc_ownership::rc_certainly_fresh(&crate::rc_ownership::rc_tail(body).kind)
                    {
                        em.rc_inc_top();
                    }
                }
                em.wrap_ok(raw, want)?;
            }
        }
        // RC-3 epilogue: the fall-through exit releases every local the
        // Bind/Assign routes made an owner, then the droppable PARAMS —
        // the callee-owned half of the argument convention (call sites
        // inc borrowed args; fresh temporaries are consumed here).
        // Early returns and tail calls skip this — a leak, never a
        // dangle. BTreeSet order keeps the release deterministic.
        let owned = std::mem::take(&mut em.rc_owned);
        for &idx in &owned {
            em.f.instructions().local_get(idx).call(F_DEC_FLAT);
            em.witness_dec(idx);
        }
        for (k, &(_, pty)) in params.iter().enumerate() {
            // A param the Assign routes made an rc_owned member (the
            // mut-param writeback) was released by the pass above —
            // a second dec here is the #1770 double free.
            if em.rc_droppable(pty) && !owned.contains(&(env_shift + k as u32)) {
                // env_shift: a lifted lambda's raw param 0 is the closure
                // ENV block — dec'ing it freed the closure after its
                // first invoke (call_indirect then read a freelist
                // pointer: "uninitialized element", the C-319 trio).
                em.f.instructions().local_get(env_shift + k as u32).call(F_DEC_FLAT);
                em.witness_dec(env_shift + k as u32);
            }
        }
        // The armed recorder's certificate goes to the sink — poisoned
        // or not (the floor test fails loudly on the sentinel).
        if let (Some(w), Some(name)) = (em.witness.take(), &witness_name) {
            crate::witness::push(name, w.certificate());
        }
        // Hold-balance invariant: every arm releases exactly what it
        // held. An over-release WRAPS the u32 depth and poisons every
        // later hold as a fake depth failure (string.get held 4, released
        // 5, and three unrelated fixtures walled on "hold-depth-i32") —
        // the BUG: prefix keeps this out of the honest-wall histogram.
        if em.hold_i32_depth != 0 || em.hold_i64_depth != 0 || em.hold_f64_depth != 0 {
            return unsup(&format!(
                "BUG:hold-imbalance:i32={},i64={},f64={}",
                em.hold_i32_depth, em.hold_i64_depth, em.hold_f64_depth
            ));
        }
    }
    f.instructions().end();
    Ok((f, calls))
}

pub(crate) fn fn_signature(f: &IrFunction, types: &TypeTable) -> Result<(Vec<SliceTy>, Option<SliceTy>), String> {
    if f.generics.is_some() {
        return Err("generic".into());
    }
    // The C-132 move-mode pass rewrote eligible mut-param fns (their
    // `mutated_params` is CLEARED, the write-back is explicit in the
    // tree); a REMAINING entry marks the excluded shapes — two `mut`
    // params, a same-scope duplicate name, a declared-Result effect fn
    // with a non-String err carrier — the same key the incumbent's v1
    // wall uses. (The can-err effect fn was admitted by #1576's ruling.)
    if !f.mutated_params.is_empty() {
        return Err("mut-param".into());
    }
    let mut params = Vec::new();
    for p in &f.params {
        let Some(sty) = slice_ty_of(&p.ty, types) else {
            return Err(format!("param-ty:{}", ty_name(&p.ty)));
        };
        params.push(sty);
    }
    let ret = match &f.ret_ty {
        Ty::Unit if f.is_effect => {
            Some(SliceTy::Result(types.intern(SliceTy::Unit), types.intern(STR)))
        }
        Ty::Unit => None,
        other => match slice_ty_of(other, types) {
            // Effect convention: the wasm value of an effect fn is ALWAYS
            // one Result block — the interp's raw-value-or-Flow::Return(Err)
            // pair becomes tag dispatch on one static type. A declared
            // `T!E` return is already Result-shaped; a raw `T` wraps as
            // `Result(T, String)` (the default error carrier).
            Some(sty @ SliceTy::Result(..)) => Some(sty),
            Some(sty) if f.is_effect => {
                Some(SliceTy::Result(types.intern(sty), types.intern(STR)))
            }
            Some(sty) => Some(sty),
            None => return Err(format!("ret-ty:{}", ty_name(other))),
        },
    };
    Ok((params, ret))
}

/// The var bound to `almide_rt_prim_budget_enter` at the TOP LEVEL of a
/// region-arm body (the frontend desugar's shape), if any.
fn body_region_enter_var(body: &IrExpr) -> Option<VarId> {
    let IrExprKind::Block { stmts, .. } = &body.kind else {
        return None;
    };
    for st in stmts {
        if let almide_ir::IrStmtKind::Bind { var, value, .. } = &st.kind
            && let IrExprKind::RuntimeCall { symbol, .. } = &value.kind
            && symbol.as_str() == "almide_rt_prim_budget_enter"
        {
            return Some(*var);
        }
    }
    None
}

/// Fill the tail-site param-release set (calls.rs `emit_tail_param_release`).
/// Sound by construction: ENTRY fns only, never lambdas, and only when the
/// body derives no raw addresses — a `prim.*` call like `prim.handle(s)`
/// hands the tail callee a raw pointer into a param's block, and releasing
/// that param at the tail site is a use-after-free (string.is_whitespace
/// read garbage codepoints exactly this way). Pool/registry bodies keep the
/// pre-existing accounting.
fn populate_tail_release_set(
    em: &mut Emitter<'_>,
    cur_module: Option<&str>,
    env_shift: u32,
    params: &[(VarId, SliceTy)],
    body: &IrExpr,
) {
    if cur_module.is_some() || env_shift != 0 || crate::rc_ownership::body_uses_prim(body) {
        return;
    }
    for (k, &(_, pty)) in params.iter().enumerate() {
        if em.rc_droppable(pty) {
            em.rc_droppable_params.push(env_shift + k as u32);
        }
    }
}
