
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
            let instr = match iop {
                IntOp::Add => "i64.add",
                IntOp::Sub => "i64.sub",
                IntOp::Mul => "i64.mul",
                IntOp::Div | IntOp::Mod => return None,
                IntOp::Lt => "i64.lt_s",
                IntOp::Le => "i64.le_s",
                IntOp::Gt => "i64.gt_s",
                IntOp::Ge => "i64.ge_s",
                IntOp::Eq => "i64.eq",
                IntOp::Ne => "i64.ne",
                IntOp::And => "i64.and",
                IntOp::Or => "i64.or",
                IntOp::Xor => "i64.xor",
                IntOp::Shl => "i64.shl",
                IntOp::Shr => "i64.shr_s",
                IntOp::ShrU => "i64.shr_u",
            };
            let ea = fuser.take(*a, &mut reads);
            let eb = fuser.take(*b, &mut reads);
            let core = format!("({instr} {ea} {eb})");
            let e = if matches!(
                iop,
                IntOp::Lt | IntOp::Le | IntOp::Gt | IntOp::Ge | IntOp::Eq | IntOp::Ne
            ) {
                format!("(i64.extend_i32_u {core})")
            } else {
                core
            };
            Some((*dst, e, reads))
        }
        Op::Prim { kind, dst: Some(d), args } => {
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
                    let e = match op {
                        FUnOp::Abs => format!("(f64.abs {x})"),
                        FUnOp::Sqrt => format!("(f64.sqrt {x})"),
                        FUnOp::Floor => format!("(f64.floor {x})"),
                        FUnOp::Ceil => format!("(f64.ceil {x})"),
                        FUnOp::Neg => format!("(f64.neg {x})"),
                    };
                    e
                }
                PrimKind::FloatBin(op) => {
                    let a = farg(fuser, &mut reads, 0);
                    let b = farg(fuser, &mut reads, 1);
                    let instr = match op {
                        FBinOp::Add => "f64.add",
                        FBinOp::Sub => "f64.sub",
                        FBinOp::Mul => "f64.mul",
                        FBinOp::Div => "f64.div",
                        FBinOp::Min => "f64.min",
                        FBinOp::Max => "f64.max",
                        FBinOp::CopySign => "f64.copysign",
                    };
                    format!("({instr} {a} {b})")
                }
                PrimKind::FloatCmp(op) => {
                    let a = farg(fuser, &mut reads, 0);
                    let b = farg(fuser, &mut reads, 1);
                    let instr = match op {
                        FCmpOp::Lt => "f64.lt",
                        FCmpOp::Le => "f64.le",
                        FCmpOp::Gt => "f64.gt",
                        FCmpOp::Ge => "f64.ge",
                        FCmpOp::Eq => "f64.eq",
                        FCmpOp::Ne => "f64.ne",
                    };
                    return Some((
                        *d,
                        format!("(i64.extend_i32_u ({instr} {a} {b}))"),
                        reads,
                    ));
                }
                PrimKind::F64FromInt | PrimKind::IntToFloat => {
                    let x = fuser.take(args[0], &mut reads);
                    format!("(f64.convert_i64_s {x})")
                }
                PrimKind::FloatToInt => {
                    let x = farg(fuser, &mut reads, 0);
                    return Some((*d, format!("(i64.trunc_sat_f64_s {x})"), reads));
                }
                _ => return None,
            };
            // f64-valued result: keep the f64 form for a float-classified dst,
            // else reinterpret back into the i64-uniform slot.
            let e = if floats.contains(d) {
                inner
            } else {
                format!("(i64.reinterpret_f64 {inner})")
            };
            Some((*d, e, reads))
        }
        _ => None,
    }
}

pub(crate) fn defined_value(op: &Op) -> Option<ValueId> {
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
        match op {
            Op::Alloc { dst, repr, .. } => {
                m.insert(*dst, *repr);
            }
            Op::Dup { dst, src } => {
                let r = m.get(src).copied().unwrap_or(Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT });
                m.insert(*dst, r);
            }
            Op::Const { dst }
            | Op::ConstInt { dst, .. }
            | Op::FuncRef { dst, .. }
            | Op::IntBinOp { dst, .. } => {
                m.insert(*dst, SCALAR_REPR);
            }
            // Rung-4 list ops: a literal is a fresh heap block; a scalar element load
            // is an i64 value.
            Op::ListLit { dst, .. } => {
                m.insert(*dst, Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT });
            }
            Op::ListGetScalar { dst, .. } => {
                m.insert(*dst, SCALAR_REPR);
            }
            // A `LoadHandle` result is a heap PTR (i32 handle); an `ArgsGetList` result is a
            // freshly-allocated heap `List[String]` PTR; a `ReadTextFile` result is a
            // freshly-allocated heap `Result[String, String]` PTR; a `ReadDir` result is a
            // freshly-allocated heap `Result[List[String], String]` PTR — all keep Ptr repr (no
            // i64 zero-extend). Every other prim result (a load, fd_write errno, or
            // handle→address) is a scalar i64.
            Op::Prim {
                dst: Some(dst),
                kind: PrimKind::LoadHandle
                    | PrimKind::ArgsGetList
                    | PrimKind::ArgsGetListFull
                    | PrimKind::EnvGet
                    | PrimKind::ReadLine
                    | PrimKind::ReadNBytes
                    | PrimKind::ReadTextFile
                    | PrimKind::ReadDir
                    | PrimKind::WriteTextFile
                    | PrimKind::MakeDir
                    | PrimKind::RemoveAll,
                ..
            } => {
                m.insert(*dst, Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT });
            }
            Op::Prim { dst: Some(dst), .. } => {
                m.insert(*dst, SCALAR_REPR);
            }
            // An `if` result: seed scalar, recorded on the stack; the real repr (scalar
            // i64 or heap-result i32) is fixed from the arm value at the matching `EndIf`.
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
            // A call's result repr is the callee's RETURN repr, carried on the op
            // (`result`) — the same field the ownership analysis reads to know a call
            // hands back a heap object. A String/List-returning call is a Ptr (i32),
            // NOT a scalar; typing it i64 mismatched `$alloc`'s i32 handle.
            Op::CallFn { dst: Some(d), result, .. } => {
                m.insert(*d, result.unwrap_or(SCALAR_REPR));
            }
            // An indirect (closure) call's result repr is likewise carried on the op.
            Op::CallIndirect { dst: Some(d), result, .. } => {
                m.insert(*d, result.unwrap_or(SCALAR_REPR));
            }
            _ => {}
        }
    }
    m
}

/// The per-`Op` classification arm of [`classify_f64_locals`]'s scan loop —
/// verbatim move. `hard`/`poison`/`edges` are the loop's accumulators,
/// write-only from every arm (a genuine fold): threading them as `&mut`
/// out-params called once per op preserves the exact original mutation
/// order, so this is safe despite the match having 20+ arms.
fn classify_f64_op(
    op: &Op,
    hard: &mut BTreeSet<ValueId>,
    poison: &mut BTreeSet<ValueId>,
    edges: &mut Vec<(ValueId, ValueId)>,
) {
    match op {
        Op::Prim { kind: PrimKind::FloatUn(_) | PrimKind::FloatBin(_), dst, args } => {
            for a in args {
                hard.insert(*a);
            }
            if let Some(d) = dst {
                hard.insert(*d);
            }
        }
        Op::Prim { kind: PrimKind::FloatCmp(_), dst, args } => {
            for a in args {
                hard.insert(*a);
            }
            if let Some(d) = dst {
                poison.insert(*d);
            }
        }
        Op::Prim { kind: PrimKind::F64FromInt | PrimKind::IntToFloat, dst, args } => {
            for a in args {
                poison.insert(*a);
            }
            if let Some(d) = dst {
                hard.insert(*d);
            }
        }
        Op::Prim { kind: PrimKind::FloatToInt, dst, args } => {
            for a in args {
                hard.insert(*a);
            }
            if let Some(d) = dst {
                poison.insert(*d);
            }
        }
        // FloatBits / the f32 family are BIT-level (identity pass-throughs, low-32
        // patterns) — they need the i64-uniform slot. Every other prim borrows
        // addresses/handles or produces non-float scalars.
        Op::Prim { dst, args, .. } => {
            for a in args {
                poison.insert(*a);
            }
            if let Some(d) = dst {
                poison.insert(*d);
            }
        }
        Op::ConstInt { .. } | Op::Const { .. } => {}
        Op::SetLocal { local, src } => edges.push((*local, *src)),
        Op::ListGetScalar { dst: _, list, idx } => {
            poison.insert(*list);
            poison.insert(*idx);
        }
        Op::ListSetScalar { list, idx, val: _ } => {
            poison.insert(*list);
            poison.insert(*idx);
        }
        Op::ListLit { dst, elems: _ } => {
            poison.insert(*dst);
        }
        Op::IntBinOp { dst, a, b, .. } => {
            poison.insert(*dst);
            poison.insert(*a);
            poison.insert(*b);
        }
        Op::IfThen { cond, dst } => {
            poison.insert(*cond);
            if let Some(d) = dst {
                poison.insert(*d);
            }
        }
        Op::Else { val } | Op::EndIf { val } => {
            if let Some(v) = val {
                poison.insert(*v);
            }
        }
        Op::LoopBreakUnless { cond } => {
            poison.insert(*cond);
        }
        Op::LoopStart | Op::LoopEnd => {}
        Op::Alloc { dst, init, .. } => {
            poison.insert(*dst);
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
                | Init::OptNone
                | Init::IntList(_)
                | Init::Bytes(_)
                | Init::Str(_) => {}
            }
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
            for u in uses {
                poison.insert(*u);
            }
        }
        // A SCALAR call argument is FLEXIBLE (like a ListLit element): the
        // render crosses the i64-uniform ABI with ONE boundary reinterpret at
        // the call site, so an f64-classified value keeps its real-f64 local.
        // Poisoning args froze every local a call ever touched into i64 for
        // its WHOLE lifetime — nbody's two 34-arg `energy(...)` calls forced
        // the entire advance loop onto reinterpret round-trips. Handle args
        // (heap pointers) and the RESULT (the callee returns raw i64 bits)
        // stay poisoned.
        Op::Call { dst, args, .. } | Op::CallFn { dst, args, .. } | Op::CallImport { dst, args, .. } => {
            if let Some(d) = dst {
                poison.insert(*d);
            }
            for a in args {
                if let CallArg::Handle(v) = a {
                    poison.insert(*v);
                }
            }
        }
        Op::CallIndirect { dst, table_idx, args, .. } => {
            if let Some(d) = dst {
                poison.insert(*d);
            }
            poison.insert(*table_idx);
            for a in args {
                if let CallArg::Handle(v) = a {
                    poison.insert(*v);
                }
            }
        }
        Op::FuncRef { dst, .. } => {
            poison.insert(*dst);
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
    use crate::WasmAbi;
    match arg {
        CallArg::Handle(v) => match ty {
            // A heap handle is an i32 pointer — exactly the `I32` import valtype.
            WasmAbi::I32 => format!("(local.get {})", local(*v)),
            // A heap handle to an i64/f64 param is a type error the lowering never emits.
            _ => format!("(local.get {})", local(*v)),
        },
        // An f64-classified scalar lives in a REAL f64 local (scalar call args
        // are flexible, not poisoned): an F64 import param reads it directly,
        // an I64 param takes its bits, an I32 (Bool) param cannot legally
        // carry a float — the wrap goes through the bits for form's sake.
        CallArg::Scalar(v) if floats.contains(v) => match ty {
            WasmAbi::F64 => format!("(local.get {})", local(*v)),
            WasmAbi::I64 => format!("(i64.reinterpret_f64 (local.get {}))", local(*v)),
            WasmAbi::I32 => {
                format!("(i32.wrap_i64 (i64.reinterpret_f64 (local.get {})))", local(*v))
            }
        },
        CallArg::Scalar(v) => match ty {
            WasmAbi::I64 => format!("(local.get {})", local(*v)),
            WasmAbi::F64 => format!("(f64.reinterpret_i64 (local.get {}))", local(*v)),
            WasmAbi::I32 => format!("(i32.wrap_i64 (local.get {}))", local(*v)),
        },
        CallArg::Imm(n) => match ty {
            WasmAbi::I64 => format!("(i64.const {n})"),
            WasmAbi::F64 => format!("(f64.reinterpret_i64 (i64.const {n}))"),
            WasmAbi::I32 => format!("(i32.const {n})"),
        },
        CallArg::Label(l) => panic!("label arg {l:?} not valid for a host import call"),
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
