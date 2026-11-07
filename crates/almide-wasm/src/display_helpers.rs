//! The display-helper build phase: the bodies of every registered display,
//! scan and named helper, built on one Emitter shell (split from display.rs).

use super::*;

/// The display-helper build phase: bodies for every registered
/// `DisplayNamed` (fixed point — a body may register more). Called after
/// each successful `lower_fn`; a failing body refuses THAT fn, keeping
/// per-fn refusal granularity, and is marked Failed so later callers
/// refuse themselves (assembly stubs the promised index).
pub(crate) fn build_display_helpers(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
) -> Result<std::collections::HashSet<usize>, EmitError> {
    let mut all_calls = std::collections::HashSet::new();
    loop {
        let (todo, todo_named): (Vec<(u32, u32)>, Vec<(crate::work::NamedOp, u32)>) = {
            let hs = work.helpers.borrow();
            let bodies = work.display_bodies.borrow();
            let named_bodies = work.named_bodies.borrow();
            let d = hs
                .iter()
                .filter_map(|h| match h {
                    Helper::DisplayNamed { ti, irk } if !bodies.contains_key(&(*ti, *irk)) => Some((*ti, *irk)),
                    _ => None,
                })
                .collect();
            let n = hs
                .iter()
                .filter_map(|h| match h {
                    Helper::NamedOp { op, ti } if !named_bodies.contains_key(&(*op, *ti)) => {
                        Some((*op, *ti))
                    }
                    _ => None,
                })
                .collect();
            (d, n)
        };
        let todo_scan: Vec<crate::ETy> = {
            let hs = work.helpers.borrow();
            let scan_bodies = work.scan_bodies.borrow();
            hs.iter()
                .filter_map(|h| match h {
                    Helper::ScanDeep { key } if !scan_bodies.contains_key(key) => Some(*key),
                    _ => None,
                })
                .collect()
        };
        let built_ty = build_display_ty_helpers(table, types, work, pool, &mut all_calls)?;
        if todo.is_empty() && todo_named.is_empty() && todo_scan.is_empty() && !built_ty {
            return Ok(all_calls);
        }
        for key in todo {
            match build_one_display_helper(table, types, work, pool, key) {
                Ok((f, calls)) => {
                    all_calls.extend(calls.iter().copied());
                    work.display_bodies
                        .borrow_mut()
                        .insert(key, crate::work::DisplayBuild::Built(f));
                }
                Err(e) => {
                    work.display_bodies.borrow_mut().insert(key, crate::work::DisplayBuild::Failed);
                    return Err(e);
                }
            }
        }
        for (op, ti) in todo_named {
            match build_one_named_helper(table, types, work, pool, op, ti) {
                Ok((f, calls)) => {
                    all_calls.extend(calls.iter().copied());
                    work.named_bodies
                        .borrow_mut()
                        .insert((op, ti), crate::work::DisplayBuild::Built(f));
                }
                Err(e) => {
                    work.named_bodies
                        .borrow_mut()
                        .insert((op, ti), crate::work::DisplayBuild::Failed);
                    return Err(e);
                }
            }
        }
        for key in todo_scan {
            match build_one_scan_helper(table, types, work, pool, key) {
                Ok((f, calls)) => {
                    all_calls.extend(calls.iter().copied());
                    work.scan_bodies.borrow_mut().insert(key, crate::work::DisplayBuild::Built(f));
                }
                Err(e) => {
                    work.scan_bodies.borrow_mut().insert(key, crate::work::DisplayBuild::Failed);
                    return Err(e);
                }
            }
        }
    }
}

/// The local layout an Emitter-built helper body runs on: `params` raw
/// params, then `i32s` i32 locals, then one i64, one f64, then the three
/// hold pools. The bodies below differ ONLY in the width of that param +
/// i32 block, so everything after it is DERIVED here rather than restated
/// per body — the one time a body computed those bases by hand, the
/// 4-param shift put `hold_i32_base` on an f64 slot and only the
/// validator noticed.
#[derive(Clone, Copy)]
pub(crate) struct Shell {
    pub(crate) params: u32,
    pub(crate) i32s: u32,
    pub(crate) cursor: u32,
    pub(crate) tmp_i32: u32,
    pub(crate) scr_i32: u32,
}

impl Shell {
    /// Two raw params, and the three i32 locals after them ARE the
    /// Emitter's cursor / tmp / scratch: `(a, b) -> i32` (the named-op
    /// bodies) and `(block, cursor) -> cursor` (display).
    pub(crate) const PAIR: Shell = Shell { params: 2, i32s: 3, cursor: 2, tmp_i32: 3, scr_i32: 4 };
    /// `(block, stride, off, needle) -> address|0`: four params, and two
    /// of the five i32 locals (4 = p, 5 = end) are the scan's own walk, so
    /// the Emitter's cursor takes the spare slot 8.
    pub(crate) const SCAN: Shell = Shell { params: 4, i32s: 5, cursor: 8, tmp_i32: 6, scr_i32: 7 };
}

/// Build one helper body on `shell`'s local layout: `body` emits into a
/// fresh Emitter, and the closed function comes back with the call set it
/// accumulated. ONE scaffold for every Emitter-built helper in this
/// module.
/// #2758: the frame's witness, armed while a sweep collects, is audited
/// against the closed body (witness_helper.rs).
pub(crate) fn build_helper_body(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
    (shell, hw): (Shell, Option<crate::witness::helper::HelperWitness>),
    body: impl FnOnce(&mut Emitter<'_>) -> Result<(), EmitError>,
) -> Result<(wasm_encoder::Function, std::collections::HashSet<usize>), EmitError> {
    use crate::emitter::{HOLD_F64_POOL, HOLD_I32_POOL, HOLD_I64_POOL};
    use wasm_encoder::ValType;
    let local_decls = [
        (shell.i32s, ValType::I32),
        (1, ValType::I64),
        (1, ValType::F64),
        (HOLD_I32_POOL, ValType::I32),
        (HOLD_I64_POOL, ValType::I64),
        (HOLD_F64_POOL, ValType::F64),
    ];
    // Immediately after the param + i32 block: the i64, then the f64,
    // then the three hold pools in declaration order.
    let scr_i64 = shell.params + shell.i32s;
    let scr_f64 = scr_i64 + 1;
    let holds = scr_f64 + 1;
    let mut f = wasm_encoder::Function::new(local_decls);
    let anchor_serial = crate::witness::anchor::begin_fn();
    let mut calls = std::collections::HashSet::new();
    let empty_locals = std::collections::HashMap::new();
    let empty_constants = std::collections::HashMap::new();
    let empty_globals = std::collections::HashMap::new();
    let empty_ranges = std::collections::HashMap::new();
    let empty_cells = std::collections::HashSet::new();
    let (recorded, footprint);
    {
        let mut em = Emitter {
            var_space: 0,
            pool,
            locals: &empty_locals,
            rc_param_ceiling: 0,
            tail_release_allowed: false,
            rc_frame_params: Vec::new(),
            tail_consumed: Default::default(),
            loop_back_releasable: Default::default(),
            self_index: None,
            rc_owned: std::collections::BTreeSet::new(),
            owned_ty: std::collections::HashMap::new(),
            owned_call_marks: Default::default(),
            if_sites: Default::default(),
            display_src: None,
            borrowed_temps: Vec::new(),
            exit_ledger: Vec::new(),
            arm_rests: Default::default(),
            borrow_base: 0, // helper bodies lower no arm argument
            table,
            types,
            calls: &mut calls,
            fn_ret: None,
            cursor_local: shell.cursor,
            tmp_i32_local: shell.tmp_i32,
            scr_i32_local: shell.scr_i32,
            scr_i64_local: scr_i64,
            in_main: false,
            build_depth: 0,
            work,
            globals: &empty_globals,
            global_constants: &empty_constants,
            deferred_ranges: &empty_ranges,
            metered: false,
            cells: &empty_cells,
            tomb_vars: &empty_cells,
            raw_map_read: false,
            tomb_tail: Default::default(),
            moves: Default::default(),
            region_repair: None,
            loop_ctl: None,
            hoisted_counts: HashMap::new(),
            cow_flags: HashMap::new(),
            cow_prejudged: HashSet::new(),
            bounds_facts: None,
            payload_ptrs: HashMap::new(),
            in_tail: false,
            try_see_through: false,
            branch_depth: 0,
            witness: hw.as_ref().map(|h| h.arm()),
            cur_module: None,
            hold_i32_base: holds,
            hold_i32_depth: 0,
            hold_i64_base: holds + HOLD_I32_POOL,
            hold_i64_depth: 0,
            hold_f64_base: holds + HOLD_I32_POOL + HOLD_I64_POOL,
            hold_f64_depth: 0,
            scr_f64_local: scr_f64,
            f: &mut f,
        };
        body(&mut em)?;
        footprint = hw.as_ref().map(|_| crate::witness::helper::footprint(&em));
        recorded = em.witness.take();
    }
    f.instructions().end();
    crate::witness::helper::finish(hw, recorded, footprint, &f);
    crate::witness::anchor::seal(anchor_serial, &f);
    Ok((f, calls))
}

/// One `(block, stride, off, needle) -> address|0` DEEP scan body: walk
/// the entries comparing the key slot by the type-directed `==`.
pub(crate) fn build_one_scan_helper(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
    key: crate::ETy,
) -> Result<(wasm_encoder::Function, std::collections::HashSet<usize>), EmitError> {
    use wasm_encoder::BlockType;
    // #2758: the entry block is lent (param 0); the needle's own RC calls,
    // like every other, are held to the byte audit.
    let hw = crate::witness::helper::HelperWitness::new(format!("<scan:{key:?}>"), "scan", &[0]);
    build_helper_body(table, types, work, pool, (Shell::SCAN, hw), |em| {
        // params: 0=block, 1=stride, 2=off, 3=needle; locals 4=p, 5=end
        let (blk, stride, off, needle, p_, end_) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
        let kt = em.types.el(key);
        {
            let mut i = em.f.instructions();
            i.local_get(blk)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .local_tee(p_);
            i.local_get(blk).i32_load(len_memarg()).i32_add().local_set(end_);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(p_).local_get(end_).i32_ge_u().br_if(1);
            i.local_get(p_)
                .local_get(off)
                .i32_add()
                .i32_load(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 });
            i.local_get(needle);
        }
        em.emit_val_eq(kt)?;
        {
            let mut i = em.f.instructions();
            i.if_(BlockType::Empty);
            i.local_get(p_).return_();
            i.end();
            i.local_get(p_).local_get(stride).i32_add().local_set(p_);
            i.br(0).end().end();
            i.i32_const(almide_layout::NULL_ADDR as i32);
        }
        Ok(())
    })
}

/// One `(a, b) -> i32` body for a RECURSIVE Named type: deep equality for
/// `NamedOp::Eq`, the total-order verdict for `NamedOp::Cmp`.
///
/// ONE builder, because the two bodies differ by one call (#2172). Both
/// exist for the same reason the display helper does: the emitter inlines
/// a type's shape, so a type that contains itself must become a CALL
/// somewhere. `path` starts with `ti`, so the self-referencing fields see
/// a cycle and call THIS helper's promised index instead of inlining.
pub(crate) fn build_one_named_helper(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
    op: crate::work::NamedOp,
    ti: u32,
) -> Result<(wasm_encoder::Function, std::collections::HashSet<usize>), EmitError> {
    // #2758: both operand blocks are lent (params 0 and 1).
    let hw = crate::witness::helper::HelperWitness::new(format!("<named-op:{op:?}:{ti}>"), "named-op", &[0, 1]);
    build_helper_body(table, types, work, pool, (Shell::PAIR, hw), |em| {
        // #4250: a type on a cycle compares a deep pair on a bounded stack.
        if matches!(op, crate::work::NamedOp::Eq) && em.types.named_on_cycle(ti) {
            return em.emit_named_eq_deep(ti);
        }
        em.f.instructions().local_get(0).local_get(1);
        match op {
            // `path` starts with `ti`, so a self-referencing field sees the
            // cycle and calls THIS helper's promised index.
            crate::work::NamedOp::Eq => em.emit_named_eq(ti, &mut vec![ti]),
            // The cmp walk needs no path: every `Named` is a call already.
            crate::work::NamedOp::Cmp => em.emit_named_cmp(ti),
            crate::work::NamedOp::EqTy => {
                em.emit_val_eq_level(em.types.el(crate::ETy::from_index(ti as usize)), &mut Vec::new())
            }
            crate::work::NamedOp::CmpTy => em.emit_val_cmp_level(em.types.el(crate::ETy::from_index(ti as usize))),
        }
    })
}

/// One `(block, cursor) -> cursor` display body.
pub(crate) fn build_one_display_helper(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
    (ti, irk): (u32, u32),
) -> Result<(wasm_encoder::Function, std::collections::HashSet<usize>), EmitError> {
    // #2758: the block is lent (param 0); the cursor (param 1) is an i32.
    let hw = crate::witness::helper::HelperWitness::new(format!("<display:{ti}>"), "display", &[0]);
    let ir = work.display_ir(irk);
    build_helper_body(table, types, work, pool, (Shell::PAIR, hw), |em| {
        em.f.instructions().local_get(1).local_set(2);
        em.f.instructions().local_get(0);
        let mut path = vec![ti];
        em.on_tail_spine(ti, |em| em.emit_display_named(ti, ir.as_ref(), &mut path))?;
        em.f.instructions().local_get(2);
        Ok(())
    })
}

/// The IR type of field `name` in a record IR type (`{ f: T }`, open or
/// closed) — the anonymous-record shape's leaf types (#3187).
pub(crate) fn record_field_ir<'t>(ir: Option<&'t Ty>, name: &str) -> Option<&'t Ty> {
    match ir? {
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            fields.iter().find(|(f, _)| f.as_str() == name).map(|(_, t)| t)
        }
        _ => None,
    }
}

/// The `i`-th component of a container's IR type — a List/Option/Set/Map/
/// Result argument or a tuple element — for the display walk (C-372). `None`
/// when the shape does not line up; the leaf then prints as a Float, never
/// guesses Float32.
pub(crate) fn ir_arg(ir: Option<&Ty>, i: usize) -> Option<&Ty> {
    match ir? {
        Ty::Applied(_, args) => args.get(i),
        Ty::Tuple(elems) => elems.get(i),
        _ => None,
    }
}
