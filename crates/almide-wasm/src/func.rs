//! The literal pool, per-function lowering plan/driver, and the callable
//! signature classifier — split from lib.rs for the complexity budget.

use std::collections::{HashMap, HashSet};

use almide_ir::{IrFunction, IrTopLet, VarId};
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
}

pub(crate) fn lower_fn(
    params: &[(VarId, SliceTy)],
    plan: FnPlan,
    body: &IrExpr,
    top_lets: &[IrTopLet],
    ctx: &Ctx,
    pool: &mut Pool,
) -> Result<(Function, HashSet<usize>), EmitError> {
    let FnPlan { ret, effect_raw, in_main, env_captures, cur_module, metered, charge_entry } = plan;
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
    for tl in top_lets {
        if slice_ty_of(&tl.ty, ctx.types).is_none() {
            return unsup(&format!("bind-ty:{}", ty_name(&tl.ty)));
        };
        // The top-let var itself is a GLOBAL; only its initializer's
        // inner binds need main locals.
        seen.insert(tl.var);
        collect_binds(&tl.value, &mut binds, &mut seen, ctx.types)?;
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
            in_tail: false,
            cur_module,
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
        for tl in top_lets {
            let Some(&(gidx, declared)) = ctx.globals.get(&tl.var) else {
                return unsup(&format!("bind-ty:{}", ty_name(&tl.ty)));
            };
            em.lower(&tl.value, Some(declared))?;
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
                }
                em.wrap_ok(raw, want)?;
            }
        }
    }
    f.instructions().end();
    Ok((f, calls))
}

pub(crate) fn fn_signature(f: &IrFunction, types: &TypeTable) -> Result<(Vec<SliceTy>, Option<SliceTy>), String> {
    if f.generics.is_some() {
        return Err("generic".into());
    }
    let mut params = Vec::new();
    for p in &f.params {
        if p.is_mut {
            return Err("mut-param".into());
        }
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
