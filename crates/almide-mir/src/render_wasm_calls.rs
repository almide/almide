/// The per-function naming environment the call renderers read — the callee
/// param counts and value repr/float classes travel as one value (a caller
/// could otherwise pair maps from different functions).
pub(crate) struct WasmEnv<'a> {
    pub param_counts: &'a BTreeMap<String, usize>,
    pub reprs: &'a BTreeMap<ValueId, Repr>,
    pub floats: &'a BTreeSet<ValueId>,
}

// The CALL-family wasm op renderers — Dup/Call/CallIndirect/CallFn/
// CallImport and the checked IntBinOp expansion with its strength
// reductions. include!-spliced from render_wasm_p2.rs.

/// Group 2 of [`render_op`]: reference/aliasing (`Dup`) and every CALL shape.
/// Split further into `_light` (`Dup`/`Call`/`CallIndirect`/`CallFn`/`CallImport`)
/// and `_intbinop` (`IntBinOp` alone — it was the dominant share of this group's
/// complexity) — `Op` has no repeated variant across the two, so the split
/// carries none of the guard-order risk a duplicated-discriminant match would.
fn render_op_call(op: &Op, env: &WasmEnv<'_>, tail_call: bool, fuser: &mut Fuser) -> String {
    match op {
        Op::Dup { .. }
        | Op::Call { .. }
        | Op::CallIndirect { .. }
        | Op::CallFn { .. }
        | Op::CallImport { .. } => {
            render_op_call_light(op, env, tail_call)
        }
        Op::IntBinOp { .. } => render_op_call_intbinop(op, fuser),
        _ => unreachable!("render_op_call: {op:?} is not in this group"),
    }
}

/// The fields of one `Op::CallIndirect` — they only travel together.
struct CallIndirectParts<'a> {
    dst: &'a Option<ValueId>,
    table_idx: &'a ValueId,
    args: &'a [CallArg],
    result: &'a Option<Repr>,
}

/// The indirect (closure) call arm of [`render_op_call_light`]: push the
/// args (heap Ptrs widened to the uniform i64 closure ABI), then dispatch
/// through the module function table with the closure type of this arity and
/// result class; a function-tail call transfers the frame via
/// `return_call_indirect` (C-178). Bodies verbatim.
fn render_call_indirect_wasm(
    p: CallIndirectParts<'_>,
    reprs: &BTreeMap<ValueId, Repr>,
    floats: &BTreeSet<ValueId>,
    tail_call: bool,
) -> String {
    let CallIndirectParts { dst, table_idx, args, result } = p;
// The closure ABI is uniform i64 (`$closure_fnN` = N i64 params). A HEAP arg (a Ptr,
// an i32 local) is WIDENED to i64 to match; the lambda narrows it back at entry
// (render_wasm_fn's lambda heap-param coercion).
let argstr = args
    .iter()
    .map(|a| match a {
        // Widen only a genuinely-i32 Ptr local; an i64 (address-repr'd)
        // handle already matches the uniform closure ABI.
        CallArg::Handle(v) if reprs.get(v).map_or(true, |r| r.is_heap()) => {
            format!("(i64.extend_i32_u (local.get {}))", local(*v))
        }
        other => render_arg_wasm(other, reprs, floats),
    })
    .collect::<Vec<_>>()
    .join(" ");
let arity = args.len();
// Pick the closure type by arity AND result class: `_v` = void (a `() -> Unit`
// closure — the lifted lambda has NO wasm result, so the dispatch type must be
// resultless and the call must NOT be dropped), `_h` = heap/i32, else scalar i64.
let suffix = match result {
    None => "_v",
    Some(r) if r.is_heap() => "_h",
    Some(_) => "",
};
// A FUNCTION-TAIL closure call (`tail_call_indexes`, same predicate
// as the CallFn arm): `return_call_indirect` transfers the frame,
// so a recursion cycle that hops through a closure runs in
// constant stack like the named mutual chain (C-178). The tail
// guard implies dst-reaches-ret, so the callee's result class
// (scalar `""` / heap `"_h"`) matches the caller's declared
// result and the type annotation stays the arity type.
if tail_call && dst.is_some() {
    return format!(
        "    (return_call_indirect (type $closure_fn{arity}{suffix}) {argstr} (i32.wrap_i64 (local.get {})))\n",
        local(*table_idx)
    );
}
// The table index is a wasm i32; the MIR value is the uniform i64, so wrap it.
let call = format!(
    "(call_indirect (type $closure_fn{arity}{suffix}) {argstr} (i32.wrap_i64 (local.get {})))",
    local(*table_idx)
);
match (dst, result) {
    (Some(d), _) => format!("    (local.set {} {call})\n", local(*d)),
    (None, None) => format!("    {call}\n"),
    (None, Some(_)) => format!("    (drop {call})\n"),
}
        

}

/// The `CallFn` arm of [`render_op_call_light`]: the elided-marker no-op,
/// the function-tail `return_call` transfer (#864, C-178), and the plain
/// direct call. Bodies verbatim.
fn render_call_fn_wasm(
    dst: &Option<ValueId>,
    name: &str,
    args: &[CallArg],
    result: &Option<Repr>,
    env: &WasmEnv<'_>,
    tail_call: bool,
) -> String {
    let WasmEnv { param_counts, reprs, floats, .. } = *env;
// A caps-accounting ELIDED-CALL MARKER (`record_elided_calls`) is an
// `Op::CallFn { dst: None, args: [], result: None }` whose NAME carries
// the elided callee's caps identity — it must keep that name for the
// caps gate, but it must NOT render as a real `(call $name)`: when
// `$name` declares parameters, a 0-arg call underflows the wasm stack
// and wasmtime rejects the module. Render NOTHING for such a marker.
//
// A GENUINE 0-arg void call to a 0-PARAMETER function has the IDENTICAL
// shape (`dst:None, args:[], result:None`) and IS valid wasm — it must
// still render. The discriminator: a real call always supplies its
// callee's params, so only a marker calls a param-taking function with
// zero args.
let is_elided_marker = dst.is_none()
    && args.is_empty()
    && result.is_none()
    && param_counts.get(name).copied().unwrap_or(0) > 0;
if is_elided_marker {
    return String::new();
}
let argstr = args
    .iter()
    .map(|a| render_arg_wasm(a, reprs, floats))
    .collect::<Vec<_>>()
    .join(" ");
// A FUNCTION-TAIL call (`tail_call_indexes`): `return_call`
// transfers the frame — the merges after it are unreachable on
// this path (their `local.get` of the never-set dst is dead but
// valid wasm), so a mutual-tail-recursion chain runs in constant
// stack (#864). Self tail-recursion still takes the TCO loop
// rewrite upstream and never reaches here.
if tail_call && dst.is_some() {
    return format!("    (return_call ${name} {argstr})\n");
}
match dst {
    Some(d) => format!("    (local.set {} (call ${name} {argstr}))\n", local(*d)),
    None => format!("    (call ${name} {argstr})\n"),
}
        

}

fn render_op_call_light(op: &Op, env: &WasmEnv<'_>, tail_call: bool) -> String {
    let WasmEnv { param_counts: _, reprs, floats } = *env;
    match op {
        // An alias SHARES the object and bumps its refcount (A1.3-render): dst and
        // src become two handles to the SAME block, rc += 1 — matching the cert's
        // Alias = +1 and exercising the proven rc machine on a shared cell (whereas
        // eager-copy kept every cell at 1). In-place mutation is guarded by cow.
        Op::Dup { dst, src } => format!(
            "    (local.set {d} (local.get {s}))\n    (call $rc_inc (local.get {s}))\n",
            d = local(*dst),
            s = local(*src)
        ),
        // A runtime call → a wasm `call` of the (bootstrap) runtime function.
        Op::Call { dst, func, args, .. } => render_call(*dst, func, args),
        // An indirect (closure) call: push the args, then the table index, and dispatch
        // through the module function table with the closure signature OF THIS ARITY
        // (`$closure_fnN`, N = arg count). The table + every `(type $closure_fnN)` are
        // emitted by render_wasm_program for each arity present; `table_idx` is the runtime
        // slot of the lifted lambda.
        Op::CallIndirect { dst, table_idx, args, result } => {
            render_call_indirect_wasm(CallIndirectParts { dst, table_idx, args, result }, reprs, floats, tail_call)
        }
        Op::CallFn { dst, name, args, result } => {
            render_call_fn_wasm(dst, name, args, result, env, tail_call)
        }
        // A host wasm IMPORT call (`@extern(wasm, module, name)`). Emit a `(call
        // $__import_module_name …)`; the matching `(import …)` is declared at module
        // scope by render_wasm_program. The MIR is i64-uniform for scalars / i32 for
        // heap handles, so each arg is COERCED to its import valtype (`abi`, parallel to
        // `args`): a Float arg's i64 local holds the f64 BITS → `f64.reinterpret_i64`; a
        // Bool arg → `i32.wrap_i64`; an Int/heap arg passes through. The result is
        // coerced back to the MIR dst valtype (a heap dst i32, else a scalar i64).
        Op::CallImport { dst, module, name, args, abi, result, result_abi } => {
            let sym = crate::render_wasm::import_symbol(module, name);
            let argstr = args
                .iter()
                .zip(abi.iter())
                .map(|(a, ty)| render_import_arg_wasm(a, *ty, floats))
                .collect::<Vec<_>>()
                .join(" ");
            let call = format!("(call ${sym} {argstr})");
            match (dst, result_abi) {
                (Some(d), Some(rt)) => {
                    // Coerce the import's result valtype back to the i64-uniform / i32-heap
                    // MIR local: an f64 result → its i64 bits; an i32 Bool result → i64;
                    // an i32 heap pointer or i64 → the dst local directly.
                    let dst_heap = result.map(|r| r.is_heap()).unwrap_or(false);
                    let coerced = match rt {
                        crate::WasmAbi::F64 => format!("(i64.reinterpret_f64 {call})"),
                        crate::WasmAbi::I32 if !dst_heap => format!("(i64.extend_i32_u {call})"),
                        _ => call,
                    };
                    format!("    (local.set {} {coerced})\n", local(*d))
                }
                // A Unit-returning import (`-> Unit`, no MIR result) is a void call.
                _ => format!("    {call}\n"),
            }
        }
        _ => unreachable!("render_op_call_light: {op:?} is not in this group"),
    }
}

/// CHECKED signed division/remainder of [`render_op_call_intbinop`]:
/// divisor-0 / MIN÷-1 abort via `$__div_trap` (C-001/C-035), inline-expanded
/// (#806); a CONSTANT divisor elides the vacuous checks and `÷ 2^k`
/// strength-reduces to the exact correction-shift sequence. Bodies verbatim.
fn render_signed_divmod(
    op: IntOp,
    dst: ValueId,
    a: ValueId,
    b: ValueId,
    args: &str,
    fuser: &mut Fuser,
) -> String {
    let instr = if matches!(op, IntOp::Div) { "i64.div_s" } else { "i64.rem_s" };
    // #806 step 3c: a CONSTANT nonzero divisor decides both checks
    // statically — elide them (zero-check vacuous; MIN÷-1 only when
    // c == -1). `÷ 2^k` (k ≥ 1) additionally strength-reduces to the
    // EXACT correction-shift sequence (valid for every dividend,
    // negative included) — Cranelift does neither, and the hardware
    // sdiv alone cost ~25% of spectralnorm's inner loop.
    match fuser.const_of(b) {
        Some(c) if c != 0 && c != -1 => {
            if matches!(op, IntOp::Div) && c > 1 && (c as u64).is_power_of_two() {
                let k = (c as u64).trailing_zeros();
                // A provably-EVEN dividend divided by 2 needs no
                // negative-rounding correction: the quotient is
                // exact, and truncation == floor == `shr_s` for
                // every sign (incl. i64::MIN). See Fuser::evens.
                if c == 2 && fuser.is_even(a) {
                    return format!(
                        "    (local.set {d} (i64.shr_s (local.get {a}) (i64.const 1)))\n",
                        a = local(a),
                        d = local(dst),
                    );
                }
                return format!(
                    "    (local.set {d} (i64.shr_s (i64.add (local.get {a})\n\
                     \x20       (i64.shr_u (i64.shr_s (local.get {a}) (i64.const 63)) (i64.const {nk})))\n\
                     \x20       (i64.const {k})))\n",
                    a = local(a),
                    d = local(dst),
                    nk = 64 - k,
                );
            }
            return format!(
                "    (local.set {d} ({instr} {args}))\n",
                d = local(dst),
            );
        }
        Some(-1) => {
            return format!(
                "    (if (i32.and (i64.eq (local.get {a}) (i64.const -9223372036854775808))\n\
                 \x20                (i64.eq (local.get {b}) (i64.const -1)))\n\
                 \x20     (then (call $__div_trap (i32.const {OVERFLOW_MSG_ADDR}) (i32.const 24))))\n\
                 \x20   (local.set {d} ({instr} {args}))\n",
                a = local(a),
                b = local(b),
                d = local(dst),
            );
        }
        _ => {}
    }
    format!(
        "    (if (i64.eqz (local.get {b}))\n\
         \x20     (then (call $__div_trap (i32.const {DIVZERO_MSG_ADDR}) (i32.const 24))))\n\
         \x20   (if (i32.and (i64.eq (local.get {a}) (i64.const -9223372036854775808))\n\
         \x20                (i64.eq (local.get {b}) (i64.const -1)))\n\
         \x20     (then (call $__div_trap (i32.const {OVERFLOW_MSG_ADDR}) (i32.const 24))))\n\
         \x20   (local.set {d} ({instr} {args}))\n",
        a = local(a),
        b = local(b),
        d = local(dst),
    )


}

/// The plain int-binop instruction table: (wasm instruction, is-comparison).
/// Div/Mod never reach it — they inline-expand in the caller.
fn int_binop_instr(op: IntOp) -> (&'static str, bool) {
    match op {
        IntOp::Add => ("i64.add", false),
        IntOp::Sub => ("i64.sub", false),
        IntOp::Mul => ("i64.mul", false),
        IntOp::Div | IntOp::Mod | IntOp::DivU | IntOp::ModU => {
            unreachable!("inline-expanded in render_op_call_intbinop")
        }
        IntOp::Lt => ("i64.lt_s", true),
        IntOp::LtU => ("i64.lt_u", true),
        IntOp::LeU => ("i64.le_u", true),
        IntOp::GtU => ("i64.gt_u", true),
        IntOp::GeU => ("i64.ge_u", true),
        IntOp::Le => ("i64.le_s", true),
        IntOp::Gt => ("i64.gt_s", true),
        IntOp::Ge => ("i64.ge_s", true),
        IntOp::Eq => ("i64.eq", true),
        IntOp::Ne => ("i64.ne", true),
        IntOp::And => ("i64.and", false),
        IntOp::Or => ("i64.or", false),
        IntOp::Xor => ("i64.xor", false),
        IntOp::Shl => ("i64.shl", false),
        IntOp::Shr => ("i64.shr_s", false),
        IntOp::ShrU => ("i64.shr_u", false),
    }
}

fn render_op_call_intbinop(op: &Op, fuser: &mut Fuser) -> String {
    match op {
        Op::IntBinOp { dst, op, a, b } => {
            // #806 step 3c: splice pending single-use defs into the operands
            // (Div/Mod below read operands several times, so they stay plain
            // `local.get` — the caller flushed any pending among them).
            let args = if matches!(op, IntOp::Div | IntOp::Mod | IntOp::DivU | IntOp::ModU) {
                format!("(local.get {}) (local.get {})", local(*a), local(*b))
            } else {
                format!("{} {}", fuser.operand(*a), fuser.operand(*b))
            };
            // CHECKED division/remainder: divisor 0 / MIN÷-1 abort via $__div_trap
            // with the native-identical stderr line + exit 1 (C-001/C-035) — never a
            // bare i64.div_s hard trap (exit 134, no message). The checks + op are
            // INLINE-EXPANDED (#806): the old `call $__chk_div` put a function call
            // in every hot-loop `/`/`%` (wasmtime does not inline across wasm
            // calls); the expansion is instruction-for-instruction the SAME
            // semantics as `$__chk_div`/`$__chk_rem`. Operands are locals, so the
            // re-evaluations cost nothing and no scratch local is needed.
            // UNSIGNED division/remainder (#872): the same divisor-zero abort as
            // the signed pair ($__div_trap, native-identical stderr) — there is
            // no MIN÷-1 overflow case in the unsigned domain, and no signed
            // strength-reduction applies (the constant is a bit pattern).
            if matches!(op, IntOp::DivU | IntOp::ModU) {
                let instr = if matches!(op, IntOp::DivU) { "i64.div_u" } else { "i64.rem_u" };
                return format!(
                    "    (if (i64.eqz (local.get {b}))\n\
                     \x20     (then (call $__div_trap (i32.const {DIVZERO_MSG_ADDR}) (i32.const 24))))\n\
                     \x20   (local.set {d} ({instr} {args}))\n",
                    b = local(*b),
                    d = local(*dst),
                );
            }
            if matches!(op, IntOp::Div | IntOp::Mod) {
                return render_signed_divmod(*op, *dst, *a, *b, &args, fuser);
            }
            // A comparison yields an i32 0/1 → zero-extend to the i64 scalar model.
            let (instr, is_cmp) = int_binop_instr(*op);
            let expr = if is_cmp {
                format!("(i64.extend_i32_u ({instr} {args}))")
            } else {
                format!("({instr} {args})")
            };
            format!("    (local.set {d} {expr})\n", d = local(*dst))
        }
        _ => unreachable!("render_op_call_intbinop: {op:?} is not in this group"),
    }
}
