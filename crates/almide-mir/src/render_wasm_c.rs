
/// Wasm mnemonic SUFFIXES for the float op families — shared by the fuser
/// (f64) and the f64/f32 prim renderers (`(f64.{name} …)` / `(f32.{name} …)`).
fn float_un_name(op: FUnOp) -> &'static str {
    match op {
        FUnOp::Abs => "abs",
        FUnOp::Sqrt => "sqrt",
        FUnOp::Floor => "floor",
        FUnOp::Ceil => "ceil",
        FUnOp::Neg => "neg",
    }
}

fn float_bin_name(op: FBinOp) -> &'static str {
    match op {
        FBinOp::Add => "add",
        FBinOp::Sub => "sub",
        FBinOp::Mul => "mul",
        FBinOp::Div => "div",
        FBinOp::Min => "min",
        FBinOp::Max => "max",
        FBinOp::CopySign => "copysign",
    }
}

fn float_cmp_name(op: FCmpOp) -> &'static str {
    match op {
        FCmpOp::Lt => "lt",
        FCmpOp::Le => "le",
        FCmpOp::Gt => "gt",
        FCmpOp::Ge => "ge",
        FCmpOp::Eq => "eq",
        FCmpOp::Ne => "ne",
    }
}

/// Build the deferred expression for a fusable single-use def, splicing
/// already-pending operands. Returns `None` when the op is not a fusable
/// pure-scalar def (the caller renders it normally).
fn fusable_expr(
    op: &Op,
    fuser: &mut Fuser,
    floats: &BTreeSet<ValueId>,
) -> Option<(ValueId, String, BTreeSet<ValueId>)> {
    let mut reads = BTreeSet::new();
    match op {
        Op::ConstInt { dst, value } => {
            let e = if floats.contains(dst) {
                format!("(f64.const {})", wat_f64_const(*value as u64))
            } else {
                format!("(i64.const {value})")
            };
            Some((*dst, e, reads))
        }
        Op::IntBinOp { dst, op: iop, a, b } => {
            // Div/Mod read their operands several times (trap checks) — never fusable.
            if matches!(iop, IntOp::Div | IntOp::Mod | IntOp::DivU | IntOp::ModU) {
                return None;
            }
            // The shared table's extend flag covers the signed AND unsigned
            // comparisons (#872) — omitting the unsigned lane here spliced a raw
            // i32 where the i64 scalar model was expected (an INVALID module,
            // not a wrong value).
            let (instr, is_cmp) = int_binop_instr(*iop);
            let ea = fuser.take(*a, &mut reads);
            let eb = fuser.take(*b, &mut reads);
            let core = format!("({instr} {ea} {eb})");
            let e = if is_cmp { format!("(i64.extend_i32_u {core})") } else { core };
            Some((*dst, e, reads))
        }
        Op::Prim { kind, dst: Some(d), args } => fusable_float_prim(kind, *d, args, fuser, floats),
        _ => None,
    }
}

/// The float-prim tier of [`fusable_expr`]: f64 unary/binary/compare/convert
/// prims over the i64-uniform slot model. Operands classified as floats splice
/// raw; scalar-slot operands reinterpret in, and an f64-valued result
/// reinterprets back out unless the dst itself is float-classified.
fn fusable_float_prim(
    kind: &PrimKind,
    d: ValueId,
    args: &[ValueId],
    fuser: &mut Fuser,
    floats: &BTreeSet<ValueId>,
) -> Option<(ValueId, String, BTreeSet<ValueId>)> {
    let mut reads = BTreeSet::new();
    let mut farg = |fuser: &mut Fuser, reads: &mut BTreeSet<ValueId>, i: usize| {
        let raw = fuser.take(args[i], reads);
        if floats.contains(&args[i]) {
            raw
        } else {
            format!("(f64.reinterpret_i64 {raw})")
        }
    };
    let inner = match kind {
        PrimKind::FloatUn(op) => {
            let x = farg(fuser, &mut reads, 0);
            format!("(f64.{} {x})", float_un_name(*op))
        }
        PrimKind::FloatBin(op) => {
            let a = farg(fuser, &mut reads, 0);
            let b = farg(fuser, &mut reads, 1);
            format!("(f64.{} {a} {b})", float_bin_name(*op))
        }
        PrimKind::FloatCmp(op) => {
            let a = farg(fuser, &mut reads, 0);
            let b = farg(fuser, &mut reads, 1);
            let e = format!("(i64.extend_i32_u (f64.{} {a} {b}))", float_cmp_name(*op));
            return Some((d, e, reads));
        }
        PrimKind::F64FromInt | PrimKind::IntToFloat => {
            let x = fuser.take(args[0], &mut reads);
            format!("(f64.convert_i64_s {x})")
        }
        PrimKind::FloatToInt => {
            let x = farg(fuser, &mut reads, 0);
            return Some((d, format!("(i64.trunc_sat_f64_s {x})"), reads));
        }
        _ => return None,
    };
    // f64-valued result: keep the f64 form for a float-classified dst,
    // else reinterpret back into the i64-uniform slot.
    let e = if floats.contains(&d) {
        inner
    } else {
        format!("(i64.reinterpret_f64 {inner})")
    };
    Some((d, e, reads))
}

pub(crate) fn defined_value(op: &Op) -> Option<ValueId> {
    // EXHAUSTIVE on purpose (#777): the old `_ => None` catch-all meant a new
    // defining Op variant silently reported "defines nothing", which is the
    // kind of registry drift F3 is about — every consumer of this fn (DCE,
    // region passes, the def-before-use gate) would quietly treat the value as
    // never-defined. A new variant is now a compile error here.
    //
    // `SetLocal` is deliberately NOT a definition: it REASSIGNS an existing
    // slot (MIR is not single-assignment across loop iterations), and the
    // passes keyed on "the op that created this value" must not match it. The
    // def-before-use gate accounts for it separately as a redefinition.
    match op {
        Op::Alloc { dst, .. }
        | Op::Dup { dst, .. }
        | Op::Const { dst }
        | Op::ConstInt { dst, .. }
        | Op::FuncRef { dst, .. }
        | Op::IntBinOp { dst, .. }
        | Op::ListLit { dst, .. }
        | Op::ListGetScalar { dst, .. }
        | Op::Pure { dst, .. } => Some(*dst),
        Op::CallFn { dst, .. } | Op::Call { dst, .. } => *dst,
        Op::CallImport { dst, .. } => *dst,
        Op::CallIndirect { dst, .. } => *dst,
        Op::Prim { dst, .. } => *dst,
        Op::IfThen { dst, .. } => *dst,
        Op::ChargeDyn { .. }
        | Op::Drop { .. }
        | Op::DropListStr { .. }
        | Op::DropValue { .. }
        | Op::DropListValue { .. }
        | Op::DropListStrValue { .. }
        | Op::DropListStrStr { .. }
        | Op::DropListIntStr { .. }
        | Op::DropListStrInt { .. }
        | Op::DropResultListValue { .. }
        | Op::DropResultValue { .. }
        | Op::DropResultStrInt { .. }
        | Op::DropResultValueInt { .. }
        | Op::DropResultListValueInt { .. }
        | Op::DropResultListStrInt { .. }
        | Op::DropResultListStr { .. }
        | Op::DropListListStr { .. }
        | Op::DropVariant { .. }
        | Op::DropWrapperRec { .. }
        | Op::Consume { .. }
        | Op::Borrow { .. }
        | Op::MakeUnique { .. }
        | Op::ListSetScalar { .. }
        | Op::Else { .. }
        | Op::EndIf { .. }
        | Op::LoopStart
        | Op::LoopBreakUnless { .. }
        | Op::LoopEnd
        | Op::SetLocal { .. } => None,
        Op::Charge { .. } => None,
    }
}

/// The prim results that are heap PTRs (i32 handles): a `LoadHandle` result;
/// an `ArgsGetList` result (a freshly-allocated heap `List[String]`); a
/// `ReadTextFile` result (a heap `Result[String, String]`); a `ReadDir` result
/// (a heap `Result[List[String], String]`); and their region/env/io kin — all
/// keep Ptr repr (no i64 zero-extend). Every other prim result (a load,
/// fd_write errno, or handle→address) is a scalar i64.
fn prim_result_is_ptr(kind: &PrimKind) -> bool {
    matches!(
        kind,
        PrimKind::LoadHandle
            | PrimKind::RegionAllocC { .. }
            | PrimKind::RegionLoadH { .. }
            | PrimKind::ArgsGetList
            | PrimKind::ArgsGetListFull
            | PrimKind::EnvGet
            | PrimKind::ReadLine
            | PrimKind::ReadNBytes
            | PrimKind::ReadTextFile
            | PrimKind::ReadDir
            | PrimKind::WriteTextFile
            | PrimKind::MakeDir
            | PrimKind::RemoveAll
    )
}

/// The repr a single op BIRTHS (`dst` → repr), if any — the value-defining
/// arms of [`value_reprs_wasm`]. `if` results are handled by the caller (seed
/// scalar at `IfThen`, fix from the arm value at `EndIf`) — not here.
fn op_birth_repr(op: &Op, m: &BTreeMap<ValueId, Repr>) -> Option<(ValueId, Repr)> {
    match op {
        Op::Alloc { dst, repr, .. } => Some((*dst, *repr)),
        Op::Dup { dst, src } => {
            let r = m.get(src).copied().unwrap_or(Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT });
            Some((*dst, r))
        }
        Op::Const { dst }
        | Op::ConstInt { dst, .. }
        | Op::FuncRef { dst, .. }
        | Op::IntBinOp { dst, .. } => Some((*dst, SCALAR_REPR)),
        // Rung-4 list ops: a literal is a fresh heap block; a scalar element load
        // is an i64 value.
        Op::ListLit { dst, .. } => Some((*dst, Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT })),
        Op::ListGetScalar { dst, .. } => Some((*dst, SCALAR_REPR)),
        Op::Prim { dst: Some(dst), kind, .. } if prim_result_is_ptr(kind) => {
            Some((*dst, Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }))
        }
        Op::Prim { dst: Some(dst), .. } => Some((*dst, SCALAR_REPR)),
        // A call's result repr is the callee's RETURN repr, carried on the op
        // (`result`) — the same field the ownership analysis reads to know a call
        // hands back a heap object. A String/List-returning call is a Ptr (i32),
        // NOT a scalar; typing it i64 mismatched `$alloc`'s i32 handle.
        Op::CallFn { dst: Some(d), result, .. } => Some((*d, result.unwrap_or(SCALAR_REPR))),
        // An indirect (closure) call's result repr is likewise carried on the op.
        Op::CallIndirect { dst: Some(d), result, .. } => Some((*d, result.unwrap_or(SCALAR_REPR))),
        _ => None,
    }
}

/// Infer each value's Repr (params + op results) for local/param/result typing.
fn value_reprs_wasm(func: &MirFunction) -> BTreeMap<ValueId, Repr> {
    let mut m = BTreeMap::new();
    // The `if`-result `dst` repr follows the ARM values (a heap-result `if` yields an i32
    // handle, a scalar one an i64): seed `dst` scalar at `IfThen`, then OVERWRITE it from
    // the arm value's repr at `EndIf`. The stack pairs each `EndIf` with its `IfThen` dst.
    let mut if_result_stack: Vec<Option<ValueId>> = Vec::new();
    for p in &func.params {
        m.insert(p.value, p.repr);
    }
    for op in &func.ops {
        if let Some((dst, r)) = op_birth_repr(op, &m) {
            m.insert(dst, r);
            continue;
        }
        match op {
            Op::IfThen { dst, .. } => {
                if_result_stack.push(*dst);
                if let Some(dst) = dst {
                    m.insert(*dst, SCALAR_REPR);
                }
            }
            Op::EndIf { val: Some(v) } => {
                if let Some(Some(dst)) = if_result_stack.pop() {
                    if let Some(r) = m.get(v).copied() {
                        m.insert(dst, r);
                    }
                }
            }
            Op::EndIf { val: None } => {
                if_result_stack.pop();
            }
            _ => {}
        }
    }
    m
}

/// Record EVERY operand of one op in `set` — the verbatim `for a in args {
/// set.insert(*a) }` body every operand-classifying arm of [`classify_f64_op`]
/// ran inline. Extracted from `classify_f64_op` (codopsy round-2 complexity
/// sweep, shared leaf 1 of 2): the arm keeps the whole decision, namely WHICH
/// accumulator (`hard` or `poison`) it hands in.
fn record_f64_operands(values: &[ValueId], set: &mut BTreeSet<ValueId>) {
    for v in values {
        set.insert(*v);
    }
}

/// Record an op's OPTIONAL result value (a `dst`, or an `Else`/`EndIf` arm
/// value) in `set` — the verbatim `if let Some(d) = dst { set.insert(*d) }`
/// body; a result-less op records nothing, exactly as before. Extracted from
/// `classify_f64_op` (codopsy round-2 complexity sweep, shared leaf 2 of 2).
fn record_f64_result(value: &Option<ValueId>, set: &mut BTreeSet<ValueId>) {
    if let Some(v) = value {
        set.insert(*v);
    }
}

/// The [`Op::Prim`] family of [`classify_f64_op`] — the five `Op::Prim` arms
/// moved verbatim, in their original order (the catch-all still comes last, so
/// the four float kinds keep winning). Decides, per prim kind, whether the
/// operands and the result are pulled toward f64 (`hard`) or frozen into the
/// i64-uniform slot (`poison`). Extracted from `classify_f64_op` (codopsy
/// round-2 complexity sweep, group 1 of 3).
fn classify_f64_prim(
    kind: &PrimKind,
    dst: &Option<ValueId>,
    args: &[ValueId],
    hard: &mut BTreeSet<ValueId>,
    poison: &mut BTreeSet<ValueId>,
) {
    match kind {
        PrimKind::FloatUn(_) | PrimKind::FloatBin(_) => {
            record_f64_operands(args, hard);
            record_f64_result(dst, hard);
        }
        PrimKind::FloatCmp(_) => {
            record_f64_operands(args, hard);
            record_f64_result(dst, poison);
        }
        PrimKind::F64FromInt | PrimKind::IntToFloat => {
            record_f64_operands(args, poison);
            record_f64_result(dst, hard);
        }
        PrimKind::FloatToInt => {
            record_f64_operands(args, hard);
            record_f64_result(dst, poison);
        }
        // FloatBits / the f32 family are BIT-level (identity pass-throughs, low-32
        // patterns) — they need the i64-uniform slot. Every other prim borrows
        // addresses/handles or produces non-float scalars.
        _ => {
            record_f64_operands(args, poison);
            record_f64_result(dst, poison);
        }
    }
}

/// The [`Init`] half of [`classify_f64_op`]'s `Op::Alloc` arm: a DYNAMICALLY
/// sized block poisons its length operand and an `OptSome` its payload; every
/// statically sized init names no value at all. Extracted from
/// `classify_f64_op` (codopsy round-2 complexity sweep, group 2 of 3):
/// verbatim, and still exhaustive on `Init` so a new init form is a compile
/// error here rather than a silently unpoisoned operand.
fn poison_alloc_init_operands(init: &Init, poison: &mut BTreeSet<ValueId>) {
    match init {
        Init::DynStr { len }
        | Init::DynList { len }
        | Init::DynListStr { len } => {
            poison.insert(*len);
        }
        Init::OptSome { payload } => {
            poison.insert(*payload);
        }
        Init::Opaque
        | Init::Empty
        | Init::OptNone
        | Init::IntList(_)
        | Init::Bytes(_)
        | Init::Str(_) => {}
    }
}

/// The call arms of [`classify_f64_op`] — verbatim, `table_idx` being the one
/// operand only `Op::CallIndirect` carries (passed `None` by the direct-call
/// arm), inserted in the original dst → table_idx → handle-args order.
///
/// A SCALAR call argument is FLEXIBLE (like a ListLit element): the
/// render crosses the i64-uniform ABI with ONE boundary reinterpret at
/// the call site, so an f64-classified value keeps its real-f64 local.
/// Poisoning args froze every local a call ever touched into i64 for
/// its WHOLE lifetime — nbody's two 34-arg `energy(...)` calls forced
/// the entire advance loop onto reinterpret round-trips. Handle args
/// (heap pointers) and the RESULT (the callee returns raw i64 bits)
/// stay poisoned.
///
/// Extracted from `classify_f64_op` (codopsy round-2 complexity sweep, group 3
/// of 3).
fn poison_call_operands(
    dst: &Option<ValueId>,
    table_idx: Option<&ValueId>,
    args: &[CallArg],
    poison: &mut BTreeSet<ValueId>,
) {
    record_f64_result(dst, poison);
    if let Some(t) = table_idx {
        poison.insert(*t);
    }
    for a in args {
        if let CallArg::Handle(v) = a {
            poison.insert(*v);
        }
    }
}

/// The per-`Op` classification arm of [`classify_f64_locals`]'s scan loop —
/// verbatim move. `hard`/`poison`/`edges` are the loop's accumulators,
/// write-only from every arm (a genuine fold): threading them as `&mut`
/// out-params called once per op preserves the exact original mutation
/// order, so this is safe despite the match having 20+ arms.
///
/// The arms are GROUPED by what they decide — patterns whose bodies were
/// byte-identical share one arm (they bind the same operand names), and the
/// `Op::Prim` family, an `Alloc`'s `Init` and the call operands each moved to a
/// helper. The match stays EXHAUSTIVE on `Op`: a new variant falling into a
/// `_ => {}` would be classified "poisons nothing", which is the direction that
/// RETYPES a value to f64 — i.e. a miscompile, not a missed optimization.
fn classify_f64_op(
    op: &Op,
    hard: &mut BTreeSet<ValueId>,
    poison: &mut BTreeSet<ValueId>,
    edges: &mut Vec<(ValueId, ValueId)>,
) {
    match op {
        Op::Prim { kind, dst, args } => classify_f64_prim(kind, dst, args, hard, poison),
        Op::ConstInt { .. } | Op::Const { .. } | Op::Charge { .. } | Op::ChargeDyn { .. } => {}
        Op::SetLocal { local, src } => edges.push((*local, *src)),
        // A list ELEMENT slot is flexible either way (`f64.load`/`f64.store`);
        // it is the list HANDLE and the INDEX that can never be an f64.
        Op::ListGetScalar { dst: _, list, idx } | Op::ListSetScalar { list, idx, val: _ } => {
            poison.insert(*list);
            poison.insert(*idx);
        }
        // The ops that name exactly ONE non-float value: a `ListLit`'s fresh
        // heap block (its `elems` stay flexible — one boundary reinterpret), a
        // loop-exit Bool, and a function-table index.
        Op::ListLit { dst: v, elems: _ }
        | Op::LoopBreakUnless { cond: v }
        | Op::FuncRef { dst: v, .. } => {
            poison.insert(*v);
        }
        Op::IntBinOp { dst, a, b, .. } => {
            poison.insert(*dst);
            poison.insert(*a);
            poison.insert(*b);
        }
        Op::IfThen { cond, dst } => {
            poison.insert(*cond);
            record_f64_result(dst, poison);
        }
        Op::Else { val } | Op::EndIf { val } => {
            record_f64_result(val, poison);
        }
        Op::LoopStart | Op::LoopEnd => {}
        Op::Alloc { dst, init, .. } => {
            poison.insert(*dst);
            poison_alloc_init_operands(init, poison);
        }
        Op::Dup { dst, src } => {
            poison.insert(*dst);
            poison.insert(*src);
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
        | Op::MakeUnique { v } => {
            poison.insert(*v);
        }
        Op::Pure { dst, uses } => {
            poison.insert(*dst);
            record_f64_operands(uses, poison);
        }
        // A SCALAR call argument is FLEXIBLE (like a ListLit element) — see
        // [`poison_call_operands`], which carries that rule for both call arms.
        Op::Call { dst, args, .. } | Op::CallFn { dst, args, .. } | Op::CallImport { dst, args, .. } => {
            poison_call_operands(dst, None, args, poison);
        }
        Op::CallIndirect { dst, table_idx, args, .. } => {
            poison_call_operands(dst, Some(table_idx), args, poison);
        }
    }
}

/// #806 step 3a: the set of locals this function can declare as REAL `f64`
/// wasm locals instead of i64-uniform bit slots. The uniform model pays 2-3
/// `reinterpret`s (GPR↔XMM moves Cranelift does not eliminate through locals)
/// per float op — measured 2.1× alone on spectralnorm's inner loop.
///
/// Classification is a conservative fixpoint over `SetLocal` copy edges:
/// - HARD-float sites (f64-op operands/results) pull a value toward f64.
/// - FLEXIBLE sites can emit either type (`ConstInt` bits, `ListGet/SetScalar`
///   element slots via `f64.load`/`f64.store`, `ListLit` elems via one
///   boundary reinterpret, `Const`'s zero default, `SetLocal` copies).
/// - EVERYTHING else — params/ret (the i64-uniform ABI), calls, allocs,
///   drops, int ops, if-merged values, bit-identity ops (`FloatBits`), the
///   f32 family — POISONS the value: it stays i64 and the affected float
///   arms keep today's reinterpret emission. A poisoned + hard value is
///   simply not retyped, so soundness never depends on the classification
///   being sharp. Byte-behavior is unchanged: reinterpret/load/store are
///   bit-preserving, and the arithmetic instructions are identical.
pub(crate) fn classify_f64_locals(func: &MirFunction) -> BTreeSet<ValueId> {
    let mut hard: BTreeSet<ValueId> = BTreeSet::new();
    let mut poison: BTreeSet<ValueId> = func.params.iter().map(|p| p.value).collect();
    if let Some(r) = func.ret {
        poison.insert(r);
    }
    let mut edges: Vec<(ValueId, ValueId)> = Vec::new();
    for op in &func.ops {
        classify_f64_op(op, &mut hard, &mut poison, &mut edges);
    }
    // Propagate both properties across copy components to a fixpoint: a
    // component with any poisoned member stays i64 throughout; one with a
    // hard-float member (and no poison) is f64 throughout.
    loop {
        let mut changed = false;
        for (a, b) in &edges {
            if poison.contains(a) != poison.contains(b) {
                poison.insert(*a);
                poison.insert(*b);
                changed = true;
            }
            if hard.contains(a) != hard.contains(b) {
                hard.insert(*a);
                hard.insert(*b);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    hard.difference(&poison).copied().collect()
}

/// Format the EXACT f64 value held by `bits` as a WAT hexfloat literal —
/// bit-precise for every case (normals, subnormals, ±0, ±inf, NaN payloads),
/// so `(f64.const …)` materializes the identical bit pattern the i64-uniform
/// slot carried. Emitting `(f64.reinterpret_i64 (i64.const bits))` instead is
/// NOT folded by Cranelift — it executed a movabs + GPR→XMM move per loop
/// iteration (measured ~1s on spectralnorm's inner loop).
fn wat_f64_const(bits: u64) -> String {
    let sign = if bits >> 63 == 1 { "-" } else { "" };
    let exp = ((bits >> 52) & 0x7ff) as i64;
    let man = bits & 0xf_ffff_ffff_ffff;
    if exp == 0x7ff {
        return if man == 0 {
            format!("{sign}inf")
        } else {
            format!("{sign}nan:0x{man:x}")
        };
    }
    if exp == 0 {
        return if man == 0 {
            format!("{sign}0x0p+0")
        } else {
            // subnormal: fraction digits are man / 2^52, scaled by 2^-1022.
            format!("{sign}0x0.{man:013x}p-1022")
        };
    }
    format!("{sign}0x1.{man:013x}p{:+}", exp - 1023)
}

fn local(v: ValueId) -> String {
    format!("$v{}", v.0)
}

/// The wasm `$func` symbol an `@extern(wasm, module, name)` IMPORT is declared and
/// called under. Mangled `$__import_<module>_<name>` so it cannot collide with a
/// user/runtime function of the same bare `name` (the wrapper fn keeps its own
/// name and `(call $__import_…)`s this). Single source for the import declaration
/// (render_wasm_program), the call render (`render_op`), and the translation-
/// validation pattern.
pub fn import_symbol(module: &str, name: &str) -> String {
    format!("__import_{module}_{name}")
}


fn render_arg_wasm(
    arg: &CallArg,
    reprs: &BTreeMap<ValueId, Repr>,
    floats: &BTreeSet<ValueId>,
) -> String {
    match arg {
        // A Handle arg names a heap BLOCK (i32 pointer param). The value may live
        // in an i64 local when it came through `PrimKind::Handle` (the eq engine's
        // slot model holds heap operands as i64 byte-ADDRESSES — `list.eq_list_*`
        // over top-level vars emitted `(call $… (local.get $v:i64))` against an
        // i32 param: invalid wasm that hid behind the v0 fallback). Wrap exactly
        // those; a Ptr-repr'd local passes through unchanged (byte-identical).
        CallArg::Handle(v) => {
            if reprs.get(v).is_some_and(|r| !r.is_heap()) {
                format!("(i32.wrap_i64 (local.get {}))", local(*v))
            } else {
                format!("(local.get {})", local(*v))
            }
        }
        // A scalar arg is FLEXIBLE for the f64 classifier (classify_f64_op does
        // NOT poison it): an f64-classified value crosses the i64-uniform ABI
        // with this ONE boundary reinterpret instead of dragging its whole
        // lifetime onto i64 round-trips (nbody's 34-arg energy() calls).
        CallArg::Scalar(v) => {
            if floats.contains(v) {
                format!("(i64.reinterpret_f64 (local.get {}))", local(*v))
            } else {
                format!("(local.get {})", local(*v))
            }
        }
        CallArg::Imm(n) => format!("(i64.const {n})"),
        CallArg::Label(l) => panic!("label arg {l:?} not valid for a user call"),
    }
}

/// Render one `Op::CallImport` arg, COERCED from its i64-uniform / i32-heap MIR
/// local to the import-signature valtype `ty`. A scalar MIR local is i64: an `F64`
/// import param reads the f64 BITS it holds (`f64.reinterpret_i64`), an `I32` Bool
/// param narrows (`i32.wrap_i64`), an `I64` param passes through. A heap handle is
/// already an i32 pointer for an `I32` param. An immediate matches the valtype's
/// constant form.
fn render_import_arg_wasm(
    arg: &CallArg,
    ty: crate::WasmAbi,
    floats: &BTreeSet<ValueId>,
) -> String {
    match arg {
        // A heap handle is an i32 pointer — exactly the `I32` import valtype.
        // (A handle to an i64/f64 param is a type error the lowering never emits.)
        CallArg::Handle(v) => format!("(local.get {})", local(*v)),
        CallArg::Scalar(v) if floats.contains(v) => float_scalar_import_arg(*v, ty),
        CallArg::Scalar(v) => slot_scalar_import_arg(*v, ty),
        CallArg::Imm(n) => imm_import_arg(*n, ty),
        CallArg::Label(l) => panic!("label arg {l:?} not valid for a host import call"),
    }
}

/// An f64-classified scalar lives in a REAL f64 local (scalar call args are
/// flexible, not poisoned): an F64 import param reads it directly, an I64
/// param takes its bits, an I32 (Bool) param cannot legally carry a float —
/// the wrap goes through the bits for form's sake.
fn float_scalar_import_arg(v: ValueId, ty: crate::WasmAbi) -> String {
    use crate::WasmAbi;
    match ty {
        WasmAbi::F64 => format!("(local.get {})", local(v)),
        WasmAbi::I64 => format!("(i64.reinterpret_f64 (local.get {}))", local(v)),
        WasmAbi::I32 => {
            format!("(i32.wrap_i64 (i64.reinterpret_f64 (local.get {})))", local(v))
        }
    }
}

/// An i64-slot scalar: read direct for I64, reinterpret for F64, wrap for I32.
fn slot_scalar_import_arg(v: ValueId, ty: crate::WasmAbi) -> String {
    use crate::WasmAbi;
    match ty {
        WasmAbi::I64 => format!("(local.get {})", local(v)),
        WasmAbi::F64 => format!("(f64.reinterpret_i64 (local.get {}))", local(v)),
        WasmAbi::I32 => format!("(i32.wrap_i64 (local.get {}))", local(v)),
    }
}

/// An immediate: materialize the const at the import param's valtype.
fn imm_import_arg(n: i64, ty: crate::WasmAbi) -> String {
    use crate::WasmAbi;
    match ty {
        WasmAbi::I64 => format!("(i64.const {n})"),
        WasmAbi::F64 => format!("(f64.reinterpret_i64 (i64.const {n}))"),
        WasmAbi::I32 => format!("(i32.const {n})"),
    }
}

fn render_call(
    dst: Option<ValueId>,
    func: &RtFn,
    args: &[CallArg],
    label_off: &BTreeMap<String, (u32, u32)>,
    floats: &BTreeSet<ValueId>,
) -> String {
    match (func, args) {
        (RtFn::ListSet, [CallArg::Handle(t), CallArg::Imm(idx), CallArg::Imm(val)]) => format!(
            "    (call $list_set (local.get {t}) (i32.const {idx}) (i64.const {val}))\n",
            t = local(*t)
        ),
        (RtFn::ListPush, [CallArg::Handle(t), CallArg::Imm(val)]) => {
            // push may move the buffer → rebind the handle local (dst == target).
            let target = dst.unwrap_or(*t);
            format!(
                "    (local.set {d} (call $list_push (local.get {t}) (i64.const {val})))\n",
                d = local(target),
                t = local(*t)
            )
        }
        (RtFn::PrintList, [CallArg::Handle(v), CallArg::Label(label)]) => {
            let (off, len) = label_off[label];
            format!(
                "    (call $print_list (local.get {v}) (i32.const {off}) (i32.const {len}))\n",
                v = local(*v)
            )
        }
        (RtFn::PrintInt, [CallArg::Scalar(v)]) => {
            // An f64-classified value never legally reaches print_int, but the
            // flexible-scalar-arg rule still owes the i64 BITS at the boundary.
            if floats.contains(v) {
                format!(
                    "    (call $print_int (i64.reinterpret_f64 (local.get {})))\n",
                    local(*v)
                )
            } else {
                format!("    (call $print_int (local.get {}))\n", local(*v))
            }
        }
        (RtFn::PrintStr, [CallArg::Handle(v)]) => {
            format!("    (call $print_str (local.get {}))\n", local(*v))
        }
        _ => panic!("malformed runtime call {func:?} with args {args:?}"),
    }
}

include!("render_wasm_p2.rs");
include!("render_wasm_p2_b.rs");
include!("render_wasm_p3.rs");
