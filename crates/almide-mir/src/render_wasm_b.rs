
/// The `(import …)` declarations for every distinct `@extern(wasm, module, name)`
/// host function the program calls (an [`Op::CallImport`]). The import signature is
/// the import's wasm valtypes (`abi`/`result_abi`, mapped from the declared Almide
/// types at lowering), so the declared `(func (param …) (result …))` matches exactly
/// what the call site supplies. Deduped by symbol + sorted (host-deterministic). A
/// program with no host import renders the empty string (byte-identical to before).
fn render_extern_imports(prog: &MirProgram) -> String {
    let mut decls: BTreeMap<String, String> = BTreeMap::new();
    for f in &prog.functions {
        for op in &f.ops {
            if let Op::CallImport { module, name, abi, result_abi, .. } = op {
                let sym = import_symbol(module, name);
                let params = if abi.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (param {})",
                        abi.iter().map(|a| a.wat()).collect::<Vec<_>>().join(" ")
                    )
                };
                let result = result_abi
                    .map(|r| format!(" (result {})", r.wat()))
                    .unwrap_or_default();
                decls.entry(sym.clone()).or_insert_with(|| {
                    format!(
                        "  (import {module:?} {name:?} (func ${sym}{params}{result}))\n"
                    )
                });
            }
        }
    }
    decls.into_values().collect()
}

/// Render one MIR function with its signature (params, locals, result).
pub fn render_wasm_fn(
    func: &MirFunction,
    label_off: &BTreeMap<String, (u32, u32)>,
    func_slots: &BTreeMap<String, u32>,
    param_counts: &BTreeMap<String, usize>,
) -> String {
    let reprs = value_reprs_wasm(func);
    let floats = classify_f64_locals(func);
    // A LIFTED LAMBDA (`__lambda_*`) is dispatched through the function table against the uniform
    // i64 closure signature (`$closure_fnN`), so its params MUST all be i64. A HEAP param (a Ptr)
    // is received as an i64 raw param and NARROWED to its Ptr value local at entry (the dual of the
    // CallIndirect's `i64.extend_i32_u` widen); a scalar param is already i64. Regular functions
    // keep their natural per-repr signature.
    let is_lambda = func.name.starts_with("__lambda_");
    let mut lambda_narrow = String::new();
    let mut lambda_heap_locals: Vec<String> = Vec::new();
    let params = func
        .params
        .iter()
        .map(|p| {
            if is_lambda && p.repr.is_heap() {
                lambda_heap_locals.push(format!("(local {} i32)", local(p.value)));
                lambda_narrow.push_str(&format!(
                    "    (local.set {v} (i32.wrap_i64 (local.get {v}_raw)))\n",
                    v = local(p.value)
                ));
                format!("(param {}_raw i64)", local(p.value))
            } else if is_lambda {
                format!("(param {} i64)", local(p.value))
            } else {
                format!("(param {} {})", local(p.value), wasm_ty(p.repr))
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let result = func
        .ret
        .map(|r| format!(" (result {})", wasm_ty(reprs.get(&r).copied().unwrap_or(SCALAR_REPR))))
        .unwrap_or_default();
    // locals = values defined in the body that are not params (first-def order).
    let mut seen: BTreeSet<ValueId> = func.params.iter().map(|p| p.value).collect();
    let mut locals = Vec::new();
    for op in &func.ops {
        if let Some(d) = defined_value(op) {
            if seen.insert(d) {
                let ty = if floats.contains(&d) {
                    "f64"
                } else {
                    wasm_ty(reprs.get(&d).copied().unwrap_or(SCALAR_REPR))
                };
                locals.push(format!("(local {} {ty})", local(d)));
            }
        }
    }
    // A recursive List[String] drop needs two i32 scratch locals (loop index + length); they
    // are function-wide (DropListStr ops never nest) and only declared when one is present.
    // `DropResultListStr` (Result[List[String], String]) also loops the Ok payload list with
    // $dlsi/$dlsn, so it joins this gate.
    if func.ops.iter().any(|op| matches!(op,
        Op::DropListStr { .. } | Op::DropResultListStrInt { .. } | Op::DropResultListStr { .. })) {
        locals.push("(local $dlsi i32) (local $dlsn i32)".to_string());
    }
    // DropResultListStrInt reuses the List[List[String]] scratch ($dlli = tuple handle, $dllinner =
    // the inner List handle) for its nested Ok-tuple List free; `DropResultListStr` reuses just $dlli
    // (the Ok payload List handle — no inner $dllinner, its payload is the direct list). Declare them
    // when no DropListListStr did.
    // `DropListIntStr` (List[(Int,String)]) loops with $dlli/$dlln/$dllinner too (no $dlsi/$dlsn —
    // its per-element free is a single rc_dec of the tuple's String slot, not a nested loop).
    if func.ops.iter().any(|op| matches!(op,
        Op::DropResultListStrInt { .. } | Op::DropResultListStr { .. } | Op::DropListIntStr { .. }
        | Op::DropListStrInt { .. }))
        && !func.ops.iter().any(|op| matches!(op, Op::DropListListStr { .. }))
    {
        locals.push("(local $dlli i32) (local $dlln i32) (local $dllinner i32)".to_string());
    }
    // A recursive `List[List[String]]` drop is a NESTED loop: the OUTER loop over the rows needs its
    // own index/length/inner-handle scratch (`$dlsi`/`$dlsn` serve the INNER cell loop). It also uses
    // the inner-loop locals, so declare those too when no plain DropListStr already did.
    if func.ops.iter().any(|op| matches!(op, Op::DropListListStr { .. })) {
        locals.push("(local $dlli i32) (local $dlln i32) (local $dllinner i32)".to_string());
        if !func.ops.iter().any(|op| matches!(op,
            Op::DropListStr { .. } | Op::DropResultListStr { .. })) {
            locals.push("(local $dlsi i32) (local $dlsn i32)".to_string());
        }
    }
    // #806 step 4: bounds-check elision plans (render_wasm_bce.rs) — versioned
    // loops re-render their region twice, so the op walk is a RANGE renderer.
    let bce = analyze_bce(func);
    // A lifted lambda's heap params become i32 value locals (narrowed from their i64 raw params).
    locals.extend(lambda_heap_locals);
    let locals_decl = locals.join(" ");
    // The heap-param narrowing runs first, before any body op reads the Ptr value local.
    let mut body = lambda_narrow;
    // The loop-markers (LoopStart/LoopBreakUnless/LoopEnd) reconstruct the standard
    // wasm while shape `(block $brk (loop $cont … (br_if $brk (eqz cond)) … (br $cont)))`.
    // A unique id per loop keeps nested loops' labels distinct; the stack tracks which
    // open loop a break/back-edge closes.
    //
    // #806 step 3b: a loop condition computed by the IMMEDIATELY preceding compare
    // whose Bool is used ONLY by the break renders as one direct `br_if` on the
    // (negated) compare — dropping the extend/local.set/local.get/eqz churn that
    // sat in EVERY hot loop's header. Int compares negate exactly (total order);
    // float compares wrap in `i32.eqz` instead (¬(a<b) ≠ (a≥b) under NaN).
    // Render-level only: the MIR and its certificate are untouched.
    let mut fused_break: BTreeMap<usize, String> = BTreeMap::new();
    let mut fused_skip: BTreeSet<usize> = BTreeSet::new();
    // Total occurrences (def + uses) per value — shared by the 3b br_if
    // fusion (exactly 2 = def + the break) and the 3c tree fuser (exactly 2 =
    // def + one consumer).
    let mut occ: BTreeMap<ValueId, usize> = BTreeMap::new();
    {
        let mut vals: Vec<ValueId> = Vec::new();
        for op in &func.ops {
            vals.clear();
            op_values(op, &mut vals);
            for v in &vals {
                *occ.entry(*v).or_insert(0) += 1;
            }
        }
        for i in 1..func.ops.len() {
            let Op::LoopBreakUnless { cond } = &func.ops[i] else { continue };
            // exactly two occurrences program-wide: the def (dst) + this use.
            if occ.get(cond).copied() != Some(2) {
                continue;
            }
            match &func.ops[i - 1] {
                Op::IntBinOp { dst, op, a, b } if dst == cond => {
                    let neg = match op {
                        IntOp::Lt => "i64.ge_s",
                        IntOp::Le => "i64.gt_s",
                        IntOp::Gt => "i64.le_s",
                        IntOp::Ge => "i64.lt_s",
                        IntOp::Eq => "i64.ne",
                        IntOp::Ne => "i64.eq",
                        _ => continue,
                    };
                    fused_break.insert(
                        i,
                        format!("({neg} (local.get {}) (local.get {}))", local(*a), local(*b)),
                    );
                    fused_skip.insert(i - 1);
                }
                Op::Prim { kind: PrimKind::FloatCmp(op), dst: Some(d), args } if d == cond => {
                    let f = |a: usize| {
                        if floats.contains(&args[a]) {
                            format!("(local.get {})", local(args[a]))
                        } else {
                            format!("(f64.reinterpret_i64 (local.get {}))", local(args[a]))
                        }
                    };
                    let instr = match op {
                        FCmpOp::Lt => "f64.lt",
                        FCmpOp::Le => "f64.le",
                        FCmpOp::Gt => "f64.gt",
                        FCmpOp::Ge => "f64.ge",
                        FCmpOp::Eq => "f64.eq",
                        FCmpOp::Ne => "f64.ne",
                    };
                    fused_break
                        .insert(i, format!("(i32.eqz ({instr} {} {}))", f(0), f(1)));
                    fused_skip.insert(i - 1);
                }
                _ => {}
            }
        }
    }
    let ctx = RenderFnCtx {
        func,
        label_off,
        func_slots,
        param_counts,
        reprs: &reprs,
        floats: &floats,
        occ: &occ,
        fused_break: &fused_break,
        fused_skip: &fused_skip,
        bce: &bce,
    };
    let mut st = RenderFnState {
        fuser: Fuser::new(),
        if_stack: Vec::new(),
        loop_stack: Vec::new(),
        loop_ctr: 0,
    };
    st.fuser.scan_consts(&func.ops);
    st.fuser.scan_evens(&func.ops);
    render_op_range(&ctx, &mut st, 0, func.ops.len(), None, &mut body);
    st.fuser.flush_all(&mut body);
    let tail = func.ret.map(|r| format!("    (local.get {})\n", local(r))).unwrap_or_default();
    format!("  (func ${} {params}{result} {locals_decl}\n{body}{tail}  )\n", func.name)
}

/// The per-function IMMUTABLE render context [`render_op_range`] threads —
/// everything `render_wasm_fn` computes once before the op walk.
struct RenderFnCtx<'a> {
    func: &'a MirFunction,
    label_off: &'a BTreeMap<String, (u32, u32)>,
    func_slots: &'a BTreeMap<String, u32>,
    param_counts: &'a BTreeMap<String, usize>,
    reprs: &'a BTreeMap<ValueId, Repr>,
    floats: &'a BTreeSet<ValueId>,
    occ: &'a BTreeMap<ValueId, usize>,
    fused_break: &'a BTreeMap<usize, String>,
    fused_skip: &'a BTreeSet<usize>,
    bce: &'a BTreeMap<usize, BcePlan>,
}

/// The MUTABLE walk state: the expression fuser and the control-marker
/// stacks. Loop label ids come from one function-wide counter, so the two
/// copies of a versioned region get distinct `$brk`/`$cont` labels.
struct RenderFnState {
    fuser: Fuser,
    if_stack: Vec<Option<ValueId>>,
    loop_stack: Vec<u32>,
    loop_ctr: u32,
}

/// Render `func.ops[start..end]` into `body`. A `LoopStart` carrying a
/// [`BcePlan`] emits the guarded two-copy versioned form and recurses over
/// its region — once with the plan's elide set (fast), once without (the
/// byte-exact original). `region` is `Some((root, elide))` inside a copy:
/// `root` stops the plan re-applying to its own `LoopStart` (each level
/// versions exactly once), while a NESTED planned loop still applies its own
/// guard, composing its elide set with the enclosing copy's.
#[allow(clippy::too_many_lines)]
fn render_op_range(
    ctx: &RenderFnCtx,
    st: &mut RenderFnState,
    start: usize,
    end: usize,
    region: Option<(usize, &BTreeSet<usize>)>,
    body: &mut String,
) {
    // The if-markers (IfThen/Else/EndIf) render to a NESTED wasm `if`/`else` — a
    // stateful reconstruction of the flat marker stream. A scalar `if` is an
    // expression `(local.set $dst (if (result i64) cond (then …val) (else …val)))`;
    // each arm leaves its value on the stack. Only the taken arm executes.
    let arm_val = |v: &Option<ValueId>| {
        v.map(|v| format!("      (local.get {})\n", local(v))).unwrap_or_default()
    };
    let mut i = start;
    'op_loop: while i < end {
        let op_idx = i;
        let op = &ctx.func.ops[op_idx];
        i += 1;
        if ctx.fused_skip.contains(&op_idx) {
            continue;
        }
        match op {
            Op::LoopStart => {
                st.fuser.flush_all(body);
                let is_region_root = region.is_some_and(|(root, _)| root == op_idx);
                if !is_region_root {
                    if let Some(plan) = ctx.bce.get(&op_idx) {
                        // Versioned loop: guard once at entry, then the fast
                        // (elided) copy or the byte-exact original. A nested
                        // plan composes with the enclosing copy's elide set.
                        let outer: Option<&BTreeSet<usize>> = region.map(|(_, e)| e);
                        let fast: BTreeSet<usize> = match outer {
                            Some(o) => o.union(&plan.elide).copied().collect(),
                            None => plan.elide.clone(),
                        };
                        let slow: BTreeSet<usize> = outer.cloned().unwrap_or_default();
                        body.push_str(&format!("    (if {}\n      (then\n", plan.guard));
                        render_op_range(ctx, st, op_idx, plan.end_idx + 1, Some((op_idx, &fast)), body);
                        body.push_str("      )\n      (else\n");
                        render_op_range(ctx, st, op_idx, plan.end_idx + 1, Some((op_idx, &slow)), body);
                        body.push_str("      ))\n");
                        i = plan.end_idx + 1;
                        continue;
                    }
                }
                let id = st.loop_ctr;
                st.loop_ctr += 1;
                st.loop_stack.push(id);
                body.push_str(&format!("    (block $brk{id}\n    (loop $cont{id}\n"));
            }
            Op::LoopBreakUnless { cond } => {
                st.fuser.flush_all(body);
                let id = *st.loop_stack.last().expect("LoopBreakUnless outside a loop");
                if let Some(fc) = ctx.fused_break.get(&op_idx) {
                    body.push_str(&format!("    (br_if $brk{id} {fc})\n"));
                } else {
                    body.push_str(&format!(
                        "    (br_if $brk{id} (i64.eqz (local.get {})))\n",
                        local(*cond)
                    ));
                }
            }
            Op::LoopEnd => {
                st.fuser.flush_all(body);
                let id = st.loop_stack.pop().expect("LoopEnd without LoopStart");
                // unconditional back-edge to the loop top, then close `loop` and `block`.
                body.push_str(&format!("    (br $cont{id})\n    ))\n"));
            }
            Op::IfThen { cond, dst } => {
                st.fuser.flush_all(body);
                st.if_stack.push(*dst);
                // The result type follows the dst repr: a heap-result `if` yields an i32
                // handle, a scalar one an i64 (value_reprs_wasm fixed dst from the arm val).
                let res = match dst {
                    Some(d) => format!(
                        " (result {})",
                        wasm_ty(ctx.reprs.get(d).copied().unwrap_or(SCALAR_REPR))
                    ),
                    None => String::new(),
                };
                let set = dst.map(|d| format!("(local.set {} ", local(d))).unwrap_or_default();
                body.push_str(&format!(
                    "    {set}(if{res} (i64.ne (local.get {c}) (i64.const 0))\n      (then\n",
                    c = local(*cond),
                ));
            }
            Op::Else { val } => {
                st.fuser.flush_all(body);
                body.push_str(&format!("{}      )\n      (else\n", arm_val(val)));
            }
            Op::EndIf { val } => {
                st.fuser.flush_all(body);
                let dst = st.if_stack.pop().expect("EndIf without IfThen");
                // close: else-arm value, `)` else, `)` if, and `)` local.set if scalar.
                let close = if dst.is_some() { "))\n" } else { ")\n" };
                body.push_str(&format!("{}      ){close}", arm_val(val)));
            }
            _ => {
                // #806 step 3c bookkeeping — see [`Fuser`]. Writes of this op:
                let mut writes: Vec<ValueId> = Vec::new();
                if let Some(d) = defined_value(op) {
                    writes.push(d);
                }
                if let Op::SetLocal { local: l, .. } = op {
                    writes.push(*l);
                }
                // The loop guard already discharged this access's range check.
                let elided = region.is_some_and(|(_, e)| e.contains(&op_idx))
                    && matches!(op, Op::ListGetScalar { .. } | Op::ListSetScalar { .. });
                // A pending being REWRITTEN must materialize first (write order).
                st.fuser.flush_values(&writes, body);
                if splice_capable(op) {
                    let consumed: Vec<ValueId> = match op {
                        Op::IntBinOp { a, b, .. } => vec![*a, *b],
                        Op::SetLocal { src, .. } => vec![*src],
                        Op::Prim { args, .. } => args.clone(),
                        _ => Vec::new(),
                    }
                    .into_iter()
                    .filter(|v| st.fuser.pending.contains_key(v))
                    .collect();
                    st.fuser.flush_reading(&writes, &consumed, body);
                    // Defer a single-use pure-scalar def (def + 1 use = 2 occurrences).
                    // Guard-clause flattening of the former 4-deep nested-if (no `else`
                    // anywhere: any unmet condition falls through to the `body.push_str`
                    // below, unchanged — `break` exits the labeled block and resumes there;
                    // `continue` (unlabeled) passes through the non-loop label to the
                    // enclosing walk, exactly as the original inline `continue` did). No
                    // behavior change — see docs/roadmap/active/code-health-codopsy.md.
                    'try_defer: {
                        let Some(d) = defined_value(op) else {
                            break 'try_defer;
                        };
                        if ctx.occ.get(&d).copied() != Some(2) || ctx.func.ret == Some(d) {
                            break 'try_defer;
                        }
                        let Some((dst, e, reads)) = fusable_expr(op, &mut st.fuser, ctx.floats)
                        else {
                            break 'try_defer;
                        };
                        st.fuser.pending.insert(dst, (e, reads));
                        st.fuser.order.push(dst);
                        continue 'op_loop;
                    }
                    body.push_str(&render_op(op, ctx.label_off, ctx.func_slots, ctx.param_counts, &ctx.func.heap_slot_masks, ctx.reprs, ctx.floats, &mut st.fuser));
                } else {
                    // A non-splicing op reads through plain `local.get`: any
                    // pending it touches materializes first, as does any
                    // pending reading a local it writes.
                    let mut vals: Vec<ValueId> = Vec::new();
                    op_values(op, &mut vals);
                    st.fuser.flush_values(&vals, body);
                    st.fuser.flush_reading(&writes, &[], body);
                    if elided {
                        body.push_str(&render_list_access_unchecked(op, ctx.floats));
                    } else {
                        body.push_str(&render_op(op, ctx.label_off, ctx.func_slots, ctx.param_counts, &ctx.func.heap_slot_masks, ctx.reprs, ctx.floats, &mut st.fuser));
                    }
                }
            }
        }
    }
}

const SCALAR_REPR: Repr = Repr::Scalar { width: crate::ScalarWidth::Double };

fn wasm_ty(repr: Repr) -> &'static str {
    if repr.is_heap() {
        "i32"
    } else {
        "i64"
    }
}

/// The value an op defines (binds), if any.
/// Every [`ValueId`] an op touches (dst + all operands), exhaustively — the
/// generic occurrence walk the render-level peepholes (#806 step 3b) use to
/// prove a value is single-use before fusing its def into its use site.
fn op_values(op: &Op, out: &mut Vec<ValueId>) {
    let args_vals = |args: &[CallArg], out: &mut Vec<ValueId>| {
        for a in args {
            match a {
                CallArg::Handle(v) | CallArg::Scalar(v) => out.push(*v),
                CallArg::Imm(_) | CallArg::Label(_) => {}
            }
        }
    };
    match op {
        Op::Alloc { dst, init, .. } => {
            out.push(*dst);
            match init {
                Init::DynStr { len } | Init::DynList { len } | Init::DynListStr { len } => {
                    out.push(*len)
                }
                Init::OptSome { payload } => out.push(*payload),
                Init::Opaque
                | Init::OptNone
                | Init::IntList(_)
                | Init::Bytes(_)
                | Init::Str(_) => {}
            }
        }
        Op::Const { dst } | Op::ConstInt { dst, .. } | Op::FuncRef { dst, .. } => out.push(*dst),
        Op::Dup { dst, src } => {
            out.push(*dst);
            out.push(*src);
        }
        Op::Drop { v }
        | Op::DropListStr { v }
        | Op::DropValue { v }
        | Op::DropListValue { v }
        | Op::DropListStrValue { v }
        | Op::DropListStrStr { v }
        | Op::DropListIntStr { v }
        | Op::DropListStrInt { v }
        | Op::DropResultListValue { v }
        | Op::DropResultValue { v }
        | Op::DropResultStrInt { v }
        | Op::DropResultValueInt { v }
        | Op::DropResultListValueInt { v }
        | Op::DropResultListStrInt { v }
        | Op::DropResultListStr { v }
        | Op::DropListListStr { v }
        | Op::DropVariant { v, .. }
        | Op::DropWrapperRec { v, .. }
        | Op::Consume { v }
        | Op::Borrow { v }
        | Op::MakeUnique { v } => out.push(*v),
        Op::Pure { dst, uses } => {
            out.push(*dst);
            out.extend(uses.iter().copied());
        }
        Op::Call { dst, args, .. } | Op::CallFn { dst, args, .. } | Op::CallImport { dst, args, .. } => {
            if let Some(d) = dst {
                out.push(*d);
            }
            args_vals(args, out);
        }
        Op::CallIndirect { dst, table_idx, args, .. } => {
            if let Some(d) = dst {
                out.push(*d);
            }
            out.push(*table_idx);
            args_vals(args, out);
        }
        Op::ListLit { dst, elems } => {
            out.push(*dst);
            out.extend(elems.iter().copied());
        }
        Op::ListGetScalar { dst, list, idx } => {
            out.push(*dst);
            out.push(*list);
            out.push(*idx);
        }
        Op::ListSetScalar { list, idx, val } => {
            out.push(*list);
            out.push(*idx);
            out.push(*val);
        }
        Op::IntBinOp { dst, a, b, .. } => {
            out.push(*dst);
            out.push(*a);
            out.push(*b);
        }
        Op::Prim { dst, args, .. } => {
            if let Some(d) = dst {
                out.push(*d);
            }
            out.extend(args.iter().copied());
        }
        Op::IfThen { cond, dst } => {
            out.push(*cond);
            if let Some(d) = dst {
                out.push(*d);
            }
        }
        Op::Else { val } | Op::EndIf { val } => {
            if let Some(v) = val {
                out.push(*v);
            }
        }
        Op::LoopBreakUnless { cond } => out.push(*cond),
        Op::LoopStart | Op::LoopEnd => {}
        Op::SetLocal { local, src } => {
            out.push(*local);
            out.push(*src);
        }
    }
}

/// #806 step 3c: the expression-tree fuser. A single-use PURE scalar def
/// (const / non-trapping int op / f64 op) is DEFERRED instead of emitted as a
/// `local.set`, and spliced as a nested expression at its one consumer —
/// collapsing the per-op `local.set`/`local.get` churn of hot arithmetic
/// chains into wasm expression trees. Safety is enforced by flushing, never
/// by reordering effects: a pending expr reads ONLY locals (no memory), so it
/// is flushed (materialized as the original `local.set`) before (a) any
/// control marker (block boundary), (b) any op that REDEFINES a local it
/// reads (unless that op is its own consumer — operand evaluation precedes
/// the write), and (c) any op that would read it through a non-splicing
/// position. Render-level only: the MIR and its certificate are untouched.
pub(crate) struct Fuser {
    /// dst → (rendered expr, the locals the expr reads). The expr is typed
    /// exactly as the local would have been (f64 for float-classified dsts).
    pending: BTreeMap<ValueId, (String, BTreeSet<ValueId>)>,
    /// def order, for deterministic flushing.
    order: Vec<ValueId>,
    /// SSA-const values: `ConstInt` dsts never reassigned by a `SetLocal`.
    /// Lets the Div/Mod render elide the (statically decided) zero / MIN÷-1
    /// checks for a constant divisor and strength-reduce `÷ 2^k` to the exact
    /// correction-shift sequence — wasmtime's Cranelift does neither, and the
    /// serialized hardware sdiv alone cost ~25% of spectralnorm's inner loop.
    consts: BTreeMap<ValueId, i64>,
    /// Values PROVABLY EVEN (mod 2^64 — wrap preserves parity): a product of
    /// consecutive integers `x*(x±1)`, a product with an even constant, or a
    /// left shift. For an even dividend, `÷ 2` needs no negative-rounding
    /// correction: truncating division of an EXACT quotient equals `shr_s 1`
    /// for every sign — so the Div render drops the 4-op correction (the
    /// spectralnorm triangular index `ij*(ij+1)/2` sits in the innermost loop).
    evens: BTreeSet<ValueId>,
}

impl Fuser {
    pub(crate) fn new() -> Self {
        Fuser {
            pending: BTreeMap::new(),
            order: Vec::new(),
            consts: BTreeMap::new(),
            evens: BTreeSet::new(),
        }
    }
    /// Pre-scan the function for SSA-const locals (a `ConstInt` def with no
    /// `SetLocal` reassignment — reassigned loop seeds are removed).
    pub(crate) fn scan_consts(&mut self, ops: &[Op]) {
        for op in ops {
            if let Op::ConstInt { dst, value } = op {
                self.consts.insert(*dst, *value);
            }
        }
        for op in ops {
            if let Op::SetLocal { local, .. } = op {
                self.consts.remove(local);
            }
        }
    }
    pub(crate) fn const_of(&self, v: ValueId) -> Option<i64> {
        self.consts.get(&v).copied()
    }
    /// Pre-scan for provably-even values (see the `evens` field doc). Uses
    /// the same single-def discipline as `scan_consts`: a dst defined more
    /// than once or reassigned by any `SetLocal` is never classified.
    /// Parity is preserved by two's-complement wrapping (2^64 is even), so
    /// `x*(x±1)` stays even even when the add/mul wrap.
    pub(crate) fn scan_evens(&mut self, ops: &[Op]) {
        let mut def_idx: BTreeMap<ValueId, usize> = BTreeMap::new();
        let mut multi: BTreeSet<ValueId> = BTreeSet::new();
        for (i, op) in ops.iter().enumerate() {
            if let Some(d) = defined_value(op) {
                if def_idx.insert(d, i).is_some() {
                    multi.insert(d);
                }
            }
            if let Op::SetLocal { local, .. } = op {
                multi.insert(*local);
            }
        }
        let mut evens: BTreeSet<ValueId> = BTreeSet::new();
        for op in ops {
            let Op::IntBinOp { dst, op: iop, a, b } = op else { continue };
            if multi.contains(dst) {
                continue;
            }
            let even = match iop {
                IntOp::Mul => {
                    consecutive_values(ops, &def_idx, &multi, &self.consts, *a, *b)
                        || consecutive_values(ops, &def_idx, &multi, &self.consts, *b, *a)
                        || self.consts.get(a).is_some_and(|c| c % 2 == 0)
                        || self.consts.get(b).is_some_and(|c| c % 2 == 0)
                }
                IntOp::Shl => self.consts.get(b).is_some_and(|c| (1..64).contains(c)),
                _ => false,
            };
            if even {
                evens.insert(*dst);
            }
        }
        self.evens = evens;
    }
    pub(crate) fn is_even(&self, v: ValueId) -> bool {
        self.evens.contains(&v)
    }
    /// Read operand `v`: consume its pending expr if one exists, else a plain
    /// `local.get`. Accumulates the transitive read-set into `reads`.
    fn take(&mut self, v: ValueId, reads: &mut BTreeSet<ValueId>) -> String {
        if let Some((e, rs)) = self.pending.remove(&v) {
            self.order.retain(|x| *x != v);
            reads.extend(rs);
            e
        } else {
            reads.insert(v);
            format!("(local.get {})", local(v))
        }
    }
    /// Operand read for render_op arms that do not need read-set tracking.
    pub(crate) fn operand(&mut self, v: ValueId) -> String {
        let mut reads = BTreeSet::new();
        self.take(v, &mut reads)
    }
    fn emit(&mut self, v: ValueId, body: &mut String) {
        if let Some((e, _)) = self.pending.remove(&v) {
            self.order.retain(|x| *x != v);
            body.push_str(&format!("    (local.set {} {e})\n", local(v)));
        }
    }
    fn flush_all(&mut self, body: &mut String) {
        for v in std::mem::take(&mut self.order) {
            if let Some((e, _)) = self.pending.remove(&v) {
                body.push_str(&format!("    (local.set {} {e})\n", local(v)));
            }
        }
    }
    /// Flush pendings that READ any of `written`, except those in `consumed`
    /// (about to be spliced into the writing op itself, whose operand
    /// evaluation precedes the write).
    fn flush_reading(&mut self, written: &[ValueId], consumed: &[ValueId], body: &mut String) {
        let victims: Vec<ValueId> = self
            .order
            .iter()
            .filter(|v| {
                !consumed.contains(v)
                    && self.pending.get(v).is_some_and(|(_, rs)| {
                        written.iter().any(|w| rs.contains(w))
                    })
            })
            .copied()
            .collect();
        for v in victims {
            self.emit(v, body);
        }
    }
    /// Flush pendings whose dst appears in `vals` (an op will read them
    /// through a position that cannot splice).
    fn flush_values(&mut self, vals: &[ValueId], body: &mut String) {
        let victims: Vec<ValueId> =
            self.order.iter().filter(|v| vals.contains(v)).copied().collect();
        for v in victims {
            self.emit(v, body);
        }
    }
}

/// `y == x ± 1` at the point both are consumed — the [`Fuser::scan_evens`]
/// consecutive-integers witness for `x*(x±1)` parity. `y`'s single def must
/// be `Add(w, 1)`/`Add(1, w)`/`Sub(w, 1)` where `w` NAMES THE SAME NUMBER as
/// `x`: either the same ValueId, or two single-def `IntBinOp`s of identical
/// shape (op + operand ids) sitting in one STRAIGHT-LINE stretch — no control
/// marker between them and nothing redefining their operands (a `SetLocal`
/// or a re-executed operand def would let the two evaluations diverge). This
/// is the `ij*(ij+1)` shape, where lowering materializes `i + j` twice.
fn consecutive_values(
    ops: &[Op],
    def_idx: &BTreeMap<ValueId, usize>,
    multi: &BTreeSet<ValueId>,
    consts: &BTreeMap<ValueId, i64>,
    x: ValueId,
    y: ValueId,
) -> bool {
    if multi.contains(&y) {
        return false;
    }
    let Some(&dy) = def_idx.get(&y) else { return false };
    let (w, one) = match &ops[dy] {
        Op::IntBinOp { op: IntOp::Add, a, b, .. } => {
            if consts.get(b) == Some(&1) {
                (*a, true)
            } else if consts.get(a) == Some(&1) {
                (*b, true)
            } else {
                return false;
            }
        }
        Op::IntBinOp { op: IntOp::Sub, a, b, .. } => (*a, consts.get(b) == Some(&1)),
        _ => return false,
    };
    if !one {
        return false;
    }
    if multi.contains(&w) || multi.contains(&x) {
        // A reassignable name can change between y's def and the multiply
        // that consumes the pair — no stable "same number" witness.
        return false;
    }
    if w == x {
        return true;
    }
    let (Some(&dw), Some(&dx)) = (def_idx.get(&w), def_idx.get(&x)) else { return false };
    let (sa, sb) = match (&ops[dw], &ops[dx]) {
        (
            Op::IntBinOp { op: o1, a: a1, b: b1, .. },
            Op::IntBinOp { op: o2, a: a2, b: b2, .. },
        ) if o1 == o2 && a1 == a2 && b1 == b2 => (*a1, *b1),
        _ => return false,
    };
    let (lo, hi) = if dw < dx { (dw, dx) } else { (dx, dw) };
    ops[lo + 1..hi].iter().all(|o| {
        let redefines = defined_value(o).is_some_and(|d| d == sa || d == sb);
        let writes = matches!(o, Op::SetLocal { local, .. } if *local == sa || *local == sb);
        let control = matches!(
            o,
            Op::LoopStart
                | Op::LoopEnd
                | Op::LoopBreakUnless { .. }
                | Op::IfThen { .. }
                | Op::Else { .. }
                | Op::EndIf { .. }
        );
        !redefines && !writes && !control
    })
}

/// Read a FLOAT-op operand: splice a pending expr / plain `local.get`, in the
/// f64 form when the value is float-classified, else reinterpreted from the
/// i64-uniform slot.
fn float_operand(fuser: &mut Fuser, floats: &BTreeSet<ValueId>, v: ValueId) -> String {
    let raw = fuser.operand(v);
    if floats.contains(&v) {
        raw
    } else {
        format!("(f64.reinterpret_i64 {raw})")
    }
}

/// The splice-capable op kinds: every read position of these renders through
/// [`Fuser::operand`], so pendings among their operands are consumed, never
/// stale-read. `Div`/`Mod` are excluded — their checked render reads each
/// operand several times.
fn splice_capable(op: &Op) -> bool {
    match op {
        Op::IntBinOp { op, .. } => !matches!(op, IntOp::Div | IntOp::Mod),
        // No read positions at all — trivially splice-clean, and its dst is a
        // prime defer candidate (a single-use const in a hot loop).
        Op::ConstInt { .. } => true,
        Op::SetLocal { .. } => true,
        Op::Prim { kind, .. } => matches!(
            kind,
            PrimKind::FloatUn(_)
                | PrimKind::FloatBin(_)
                | PrimKind::FloatCmp(_)
                | PrimKind::F64FromInt
                | PrimKind::FloatToInt
                | PrimKind::IntToFloat
        ),
        _ => false,
    }
}
