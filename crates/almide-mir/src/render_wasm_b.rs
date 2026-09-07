
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

/// The f64 comparison, negated. No negated f64 instruction exists, so the
/// comparison is wrapped in `i32.eqz`. A non-float-typed operand local holds the
/// bits and is reinterpreted.
fn negated_float_cmp(op: FCmpOp, args: &[ValueId], floats: &BTreeSet<ValueId>) -> String {
    let operand = |a: usize| {
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
    format!("(i32.eqz ({instr} {} {}))", operand(0), operand(1))
}

/// The NEGATED comparison a `LoopBreakUnless` can fuse with, when the op right
/// before it defines exactly that condition. `None` when the shape does not
/// fuse (a non-comparison producer, or a comparison with no negated twin).
fn negated_break_test(
    def: &Op,
    cond: &ValueId,
    floats: &BTreeSet<ValueId>,
) -> Option<String> {
    match def {
        Op::IntBinOp { dst, op, a, b } if dst == cond => {
            let neg = match op {
                IntOp::Lt => "i64.ge_s",
                IntOp::Le => "i64.gt_s",
                IntOp::Gt => "i64.le_s",
                IntOp::Ge => "i64.lt_s",
                IntOp::Eq => "i64.ne",
                IntOp::Ne => "i64.eq",
                _ => return None,
            };
            Some(format!("({neg} (local.get {}) (local.get {}))", local(*a), local(*b)))
        }
        Op::Prim { kind: PrimKind::FloatCmp(op), dst: Some(d), args } if d == cond => {
            Some(negated_float_cmp(*op, args, floats))
        }
        _ => None,
    }
}

/// Render one MIR function with its signature (params, locals, result).
/// #806 step 3b planning: a loop condition computed by the IMMEDIATELY
/// preceding compare whose Bool is used ONLY by the break renders as one
/// direct `br_if` on the (negated) compare — dropping the extend/local.set/
/// local.get/eqz churn in EVERY hot loop's header. Int compares negate
/// exactly (total order); float compares wrap in `i32.eqz` instead
/// (¬(a<b) ≠ (a≥b) under NaN). Render-level only: the MIR and its
/// certificate are untouched. Also returns the total value-occurrence map
/// shared with the 3c tree fuser.
#[allow(clippy::type_complexity)]
fn plan_break_fusion(
    func: &MirFunction,
    floats: &BTreeSet<ValueId>,
) -> (BTreeMap<ValueId, usize>, BTreeMap<usize, String>, BTreeSet<usize>) {
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
            let Some(test) = negated_break_test(&func.ops[i - 1], cond, floats) else { continue };
            fused_break.insert(i, test);
            fused_skip.insert(i - 1);
        }
    }
    (occ, fused_break, fused_skip)
}

/// The local declarations of [`render_wasm_fn`]: every body-defined value
/// (first-def order, typed by repr/float class), plus the per-family
/// recursive-drop scratch locals. Section comments verbatim.
fn declare_fn_locals(
    func: &MirFunction,
    reprs: &BTreeMap<ValueId, Repr>,
    floats: &BTreeSet<ValueId>,
) -> Vec<String> {
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
    locals.extend(drop_scratch_locals(func));
    locals
}

/// The recursive-drop scratch locals of [`declare_fn_locals`]: each drop
/// family loops with fixed scratch registers, function-wide (drops never
/// nest) and declared only when the family is present. Gates verbatim.
fn drop_scratch_locals(func: &MirFunction) -> Vec<String> {
    let mut locals = Vec::new();
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
    locals
}

pub fn render_wasm_fn(
    func: &MirFunction,
    func_slots: &BTreeMap<String, u32>,
    param_counts: &BTreeMap<String, usize>,
    // `true` = a local-reuse-rewritten function (#1554): skip break fusion
    // and BCE, whose plans pattern-match on single-def value identities the
    // slot merge no longer guarantees.
    plain: bool,
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
    let mut locals = declare_fn_locals(func, &reprs, &floats);
    // #806 step 4: bounds-check elision plans (render_wasm_bce.rs) — versioned
    // loops re-render their region twice, so the op walk is a RANGE renderer.
    let bce = if plain { BTreeMap::new() } else { analyze_bce(func) };
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
    let (occ, fused_break, fused_skip) = if plain {
        (BTreeMap::new(), BTreeMap::new(), BTreeSet::new())
    } else {
        plan_break_fusion(func, &floats)
    };
    let tail_calls = tail_call_indexes(func);
    let ctx = RenderFnCtx {
        func,
        tail_calls: &tail_calls,
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
        switch_ctr: 0,
    };
    st.fuser.scan_consts(&func.ops);
    st.fuser.scan_evens(&func.ops);
    render_op_range(&ctx, &mut st, 0, func.ops.len(), None, &mut body);
    st.fuser.flush_all(&mut body);
    let tail = func.ret.map(|r| format!("    (local.get {})\n", local(r))).unwrap_or_default();
    if std::env::var("ALMIDE_DBG_WAT").is_ok_and(|p| func.name.contains(&p)) {
        eprintln!("  (func ${} {params}{result} {locals_decl}\n{body}{tail}  )", func.name);
    }
    format!("  (func ${} {params}{result} {locals_decl}\n{body}{tail}  )\n", func.name)
}

/// The per-function IMMUTABLE render context [`render_op_range`] threads —
/// everything `render_wasm_fn` computes once before the op walk.
struct RenderFnCtx<'a> {
    func: &'a MirFunction,
    tail_calls: &'a BTreeSet<usize>,
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
    /// #882: the same function-wide-counter discipline for the `$sw…` labels a
    /// recognized dense match emits — a versioned region renders its switch
    /// twice, and the two copies must not share a label.
    switch_ctr: u32,
}
