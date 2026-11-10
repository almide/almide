//! The per-tier native op renderers — each takes one MIR op and an
//! [`OpSink`] and returns `Ok(false)` for "not this tier's op", so
//! `render_fn`'s dispatcher chains them: metering (fuel/timeout/budget),
//! the T1-3 Result carrier, the §13 termination convention, and the
//! value-producing scalar/float tiers. Arm bodies are verbatim from the
//! former inline `render_fn` op match.

use crate::lower::LowerError;
use crate::render_native::{
    render_dup, render_float_bin, render_float_cmp, render_float_un, render_int_binop,
    render_set_local, scalar_bits, OpSink,
};
use crate::render_native::{var, wall, NTy};
use crate::render_native_shims::{
    BUDGET_SHIM, CHARGE_DYN_SHIM, CHARGE_SHIM, COUNTER_SHIM, CUT_RET_MARKER, FUEL_LT0_SHIM,
    REGION_ARENA_SHIM, TIMEOUT_SHIM,
};
use crate::{Init, Op};
use std::fmt::Write;

/// The metering ops — fuel charges (T1-1 strict cut + T5-1 wall deadline),
/// the T3-5 dynamic charge, and the timeout/budget region primitives — every
/// arm registers its runtime shim. `Ok(false)` = not this tier's op. Arm
/// bodies are verbatim from the former inline [`render_fn`] op match.
pub(crate) fn render_native_meter_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let OpSink {
        tys,
        out,
        indent,
        used_shims,
    } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        Op::Charge { site, cost } => {
            used_shims.push(COUNTER_SHIM);
            used_shims.push(CHARGE_SHIM);
            let tr = crate::charge_probe::probe_enabled();
            used_shims.push(FUEL_LT0_SHIM);
            line!("__almd_charge({site}, {cost}, {tr});");
            // T1-1 strict cut (see the wasm arm): the dummy return value's
            // type is known only after the body typed `func.ret`, so a
            // marker is emitted here and patched at the end of render_fn.
            line!("if __almd_fuel_lt0() {{ {CUT_RET_MARKER} }}");
            // T5-1: the wall-deadline check rides the same cut mechanism.
            if crate::charge_probe::timeout_used() {
                used_shims.push(TIMEOUT_SHIM.as_str());
                line!("if __almd_wall_hit() {{ {CUT_RET_MARKER} }}");
            }
        }
        // T3-5 dynamic charge — the native twin of the wasm arm above:
        // 1 + byte_len/16 of the result string, same trace + cut rules.
        Op::ChargeDyn { site, src } => {
            used_shims.push(COUNTER_SHIM);
            used_shims.push(CHARGE_SHIM);
            used_shims.push(CHARGE_DYN_SHIM);
            used_shims.push(FUEL_LT0_SHIM);
            let tr = crate::charge_probe::probe_enabled();
            let sref = match tys.get(src) {
                Some(NTy::Str | NTy::StrRef) => format!("{}.len() as i64", var(*src)),
                other => {
                    return Err(wall(format!(
                        "native: ChargeDyn over a non-string value ({other:?})"
                    )))
                }
            };
            line!("__almd_charge_dyn({site}, {sref}, {tr});");
            line!("if __almd_fuel_lt0() {{ {CUT_RET_MARKER} }}");
            if crate::charge_probe::timeout_used() {
                used_shims.push(TIMEOUT_SHIM.as_str());
                line!("if __almd_wall_hit() {{ {CUT_RET_MARKER} }}");
            }
        }

        Op::Prim {
            kind: crate::PrimKind::TimeoutEnter,
            dst: Some(d),
            args,
        } => {
            used_shims.push(TIMEOUT_SHIM.as_str());
            tys.insert(*d, NTy::I64);
            line!("let {} = __almd_timeout_enter({});", var(*d), var(args[0]));
        }
        Op::Prim {
            kind: crate::PrimKind::TimeoutExit,
            dst: Some(d),
            args,
        } => {
            used_shims.push(TIMEOUT_SHIM.as_str());
            tys.insert(*d, NTy::I64);
            line!("let {} = __almd_timeout_exit({});", var(*d), var(args[0]));
        }
        Op::Prim {
            kind: crate::PrimKind::TimeoutHit,
            dst: Some(d),
            ..
        } => {
            used_shims.push(TIMEOUT_SHIM.as_str());
            tys.insert(*d, NTy::I64);
            line!("let {} = __almd_timeout_hit();", var(*d));
        }
        Op::Prim {
            kind: crate::PrimKind::BudgetEnter,
            dst: Some(d),
            args,
        } => {
            used_shims.push(COUNTER_SHIM);
            used_shims.push(BUDGET_SHIM.as_str());
            tys.insert(*d, NTy::I64);
            line!("let {} = __almd_budget_enter({});", var(*d), var(args[0]));
        }
        Op::Prim {
            kind: crate::PrimKind::BudgetExhausted,
            dst: Some(d),
            ..
        } => {
            used_shims.push(COUNTER_SHIM);
            used_shims.push(BUDGET_SHIM.as_str());
            tys.insert(*d, NTy::I64);
            line!("let {} = __almd_budget_exhausted();", var(*d));
        }
        Op::Prim {
            kind: crate::PrimKind::BudgetExit,
            dst: Some(d),
            args,
        } => {
            used_shims.push(COUNTER_SHIM);
            used_shims.push(BUDGET_SHIM.as_str());
            tys.insert(*d, NTy::I64);
            line!("let {} = __almd_budget_exit({});", var(*d), var(args[0]));
        }
        Op::Prim {
            kind: crate::PrimKind::BudgetSpend,
            dst: Some(d),
            ..
        } => {
            used_shims.push(COUNTER_SHIM);
            used_shims.push(BUDGET_SHIM.as_str());
            tys.insert(*d, NTy::I64);
            line!("let {} = __almd_budget_spend();", var(*d));
        }
        // #4006 C2: the region window's arena mark — opened at the window
        // fn's entry, rewound after its last op (native_region_window.rs).
        Op::Prim {
            kind: crate::PrimKind::RegionSave,
            dst: Some(d),
            ..
        } => {
            used_shims.push(REGION_ARENA_SHIM.as_str());
            line!("let {} = almide_region_save();", var(*d));
        }
        Op::Prim {
            kind: crate::PrimKind::RegionRestore,
            args,
            ..
        } if args.len() == 1 => {
            used_shims.push(REGION_ARENA_SHIM.as_str());
            line!("almide_region_restore({});", var(args[0]));
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// The T1-3 native Result carrier ops (`native_result_rewrite`), with the
/// String-Ok carrier, the storage slots, narrow arithmetic and the scalar
/// Option carrier. `Ok(false)` = not this tier's op. Arm bodies verbatim from
/// [`render_fn`], grouped by family: each prim kind belongs to one family, so
/// the router below picks the one helper that can take it.
pub(crate) fn render_native_result_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    use crate::PrimKind as K;
    let Op::Prim { kind, .. } = op else { return Ok(false) };
    match kind {
        K::ResMakeOk | K::ResMakeErrStr | K::ResCopy | K::ResTag | K::ResOkScalar | K::ResErrStr => {
            render_result_carrier_op(op, s)
        }
        K::ResStrMake | K::ResStrSetTag | K::NativeSlotLoad { .. } | K::NativeSlotStore { .. } => {
            render_result_str_and_slot_op(op, s)
        }
        K::NativeNarrowBin { .. } => render_narrow_bin_op(op, s),
        K::NativeNarrowPow { .. } => render_narrow_pow_op(op, s),
        K::OptMakeSome | K::OptMakeNone | K::OptCopy | K::OptTag | K::OptPayload => render_option_carrier_op(op, s),
        _ => Ok(false),
    }
}

/// The `Result<i64, AlmideString>` carrier arms of [`render_native_result_op`].
fn render_result_carrier_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let OpSink {
        tys,
        out,
        indent,
        used_shims: _,
    } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        // ── T1-3 native Result carrier (native_result_rewrite) ──
        Op::Prim {
            kind: crate::PrimKind::ResMakeOk,
            dst: Some(d),
            args,
        } => {
            // The Ok slot holds the MIR scalar: a Float as its bits.
            let payload = match tys.get(&args[0]) {
                Some(NTy::F64) => format!("{}.to_bits() as i64", var(args[0])),
                _ => var(args[0]),
            };
            tys.insert(*d, NTy::Res);
            line!(
                "let mut {}: Result<i64, AlmideString> = Ok({});",
                var(*d),
                payload
            );
        }
        Op::Prim {
            kind: crate::PrimKind::ResMakeErrStr,
            dst: Some(d),
            args,
        } => {
            let src = match tys.get(&args[0]) {
                Some(NTy::Str) => format!("{}.clone()", var(args[0])),
                Some(NTy::StrRef) => format!("AlmideString::from({})", var(args[0])),
                other => {
                    return Err(wall(format!(
                        "native: ResMakeErrStr over a non-string payload ({other:?})"
                    )))
                }
            };
            tys.insert(*d, NTy::Res);
            line!("let mut {}: Result<i64, AlmideString> = Err({});", var(*d), src);
        }
        Op::Prim {
            kind: crate::PrimKind::ResCopy,
            dst: Some(d),
            args,
        } => {
            // A copy of either carrier, typed as its source.
            let t = match tys.get(&args[0]) {
                Some(NTy::ResStr) => NTy::ResStr,
                _ => NTy::Res,
            };
            let spelled = if t == NTy::ResStr { "Result<AlmideString, AlmideString>" } else { "Result<i64, AlmideString>" };
            tys.insert(*d, t);
            line!("let mut {}: {} = {}.clone();", var(*d), spelled, var(args[0]));
        }
        Op::Prim {
            kind: crate::PrimKind::ResTag,
            dst: Some(d),
            args,
        } => {
            tys.insert(*d, NTy::I64);
            line!("let {}: i64 = {}.is_err() as i64;", var(*d), var(args[0]));
        }
        Op::Prim {
            kind: crate::PrimKind::ResOkScalar,
            dst: Some(d),
            args,
        } => {
            tys.insert(*d, NTy::I64);
            line!(
                "let {}: i64 = match &{} {{ Ok(x) => *x, Err(_) => 0 }};",
                var(*d),
                var(args[0])
            );
        }
        Op::Prim {
            kind: crate::PrimKind::ResErrStr,
            dst: Some(d),
            args,
        } => {
            // A BORROW of the Err payload (the verifier aliases it to the
            // Result's object); "" on the Ok side (unreached — the tag
            // dispatch guards it).
            tys.insert(*d, NTy::StrRef);
            if tys.get(&args[0]) == Some(&NTy::ResStr) {
                // The String-Ok carrier keeps its payload in one slot on both
                // sides; the tag dispatch decides which the program reads.
                line!(
                    "let {}: &str = match &{} {{ Ok(s) | Err(s) => s.as_str() }};",
                    var(*d),
                    var(args[0])
                );
                // A copy of this read where the carrier dies moves the string
                // out of its one slot (render_native_loop's payload move),
                // whichever side the tag dispatch read it under.
                crate::render_native_addr::note_payload_take(
                    *d,
                    args[0],
                    format!("match &mut {} {{ Ok(s) | Err(s) => std::mem::take(s) }}", var(args[0])),
                );
            } else {
                line!(
                    "let {}: &str = match &{} {{ Err(e) => e.as_str(), Ok(_) => \"\" }};",
                    var(*d),
                    var(args[0])
                );
                // Noted for the by-value move out of a dying Result param
                // (render_native_loop); a local of this kind is not a payload
                // carrier (`render_native_move::payload_carrier`).
                crate::render_native_addr::note_payload_take(
                    *d,
                    args[0],
                    format!("match &mut {} {{ Err(e) => std::mem::take(e), Ok(_) => AlmideString::new() }}", var(args[0])),
                );
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// The String-Ok carrier and storage-slot arms of [`render_native_result_op`].
fn render_result_str_and_slot_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let OpSink {
        tys,
        out,
        indent,
        used_shims,
    } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        // ── #4006 family C: the String-Ok Result carrier (native_result_str_rewrite) ──
        Op::Prim {
            kind: crate::PrimKind::ResStrMake,
            dst: Some(d),
            args,
        } => {
            let src = match tys.get(&args[0]) {
                Some(NTy::Str) => format!("{}.clone()", var(args[0])),
                Some(NTy::StrRef) => format!("AlmideString::from({})", var(args[0])),
                other => {
                    return Err(wall(format!(
                        "native: a String-Ok Result over a non-String payload ({other:?})"
                    )))
                }
            };
            tys.insert(*d, NTy::ResStr);
            line!("let mut {}: Result<AlmideString, AlmideString> = Ok({});", var(*d), src);
        }
        Op::Prim {
            kind: crate::PrimKind::ResStrSetTag,
            args,
            ..
        } => {
            if tys.get(&args[0]) != Some(&NTy::ResStr) || tys.get(&args[1]) != Some(&NTy::I64) {
                return Err(wall("native: a String-Ok Result tag write outside its window"));
            }
            line!(
                "if {} != 0 {{ if let Ok(s) = std::mem::replace(&mut {}, Ok(AlmideString::new())) {{ {} = Err(s); }} }}",
                var(args[1]),
                var(args[0]),
                var(args[0])
            );
        }
        // ── #4006 family I: a scalar storage slot (native_slot_rewrite) ──
        Op::Prim {
            kind: crate::PrimKind::NativeSlotLoad { slot },
            dst: Some(d),
            ..
        } => {
            used_shims.push(crate::native_slot_rewrite::slot_static(*slot));
            tys.insert(*d, NTy::I64);
            line!("let mut {}: i64 = ALMD_SLOT_{}.load(std::sync::atomic::Ordering::Relaxed);", var(*d), slot);
        }
        Op::Prim {
            kind: crate::PrimKind::NativeSlotStore { slot },
            args,
            ..
        } => {
            // The slot holds the MIR scalar: a Float as its bits.
            let v = match tys.get(&args[0]) {
                Some(NTy::I64) => var(args[0]),
                Some(NTy::F64) => format!("{}.to_bits() as i64", var(args[0])),
                other => {
                    return Err(wall(format!(
                        "native: a non-scalar value stored into a storage slot ({other:?})"
                    )))
                }
            };
            used_shims.push(crate::native_slot_rewrite::slot_static(*slot));
            line!("ALMD_SLOT_{}.store({}, std::sync::atomic::Ordering::Relaxed);", slot, v);
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// The narrow-integer arithmetic arm of [`render_native_result_op`].
fn render_narrow_bin_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let OpSink {
        tys,
        out,
        indent,
        used_shims: _,
    } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        // ── #4006: narrow integer arithmetic (native_narrow_rewrite) ──
        Op::Prim {
            kind: crate::PrimKind::NativeNarrowBin { op, bits, signed },
            dst: Some(d),
            args,
        } if args.len() == 2 => {
            let ty = match (*bits, *signed) {
                (8, true) => "i8",
                (16, true) => "i16",
                (32, true) => "i32",
                (8, false) => "u8",
                (16, false) => "u16",
                (32, false) => "u32",
                _ => return Err(wall(format!("native: a narrow op at width {bits}"))),
            };
            let f = match op {
                crate::IntOp::Add => "wrapping_add",
                crate::IntOp::Sub => "wrapping_sub",
                crate::IntOp::Mul => "wrapping_mul",
                other => return Err(wall(format!("native: a narrow {other:?}"))),
            };
            if args.iter().any(|a| tys.get(a) != Some(&NTy::I64)) {
                return Err(wall("native: a narrow op over a non-integer operand"));
            }
            tys.insert(*d, NTy::I64);
            line!("let mut {}: i64 = (({} as {ty}).{f}({} as {ty})) as i64;", var(*d), var(args[0]), var(args[1]));
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// The scalar-Option carrier arms of [`render_native_result_op`].
pub(crate) fn render_option_carrier_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let OpSink {
        tys,
        out,
        indent,
        used_shims: _,
    } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        // ── #4006 family B: the native scalar-Option carrier (native_option_rewrite) ──
        Op::Prim {
            kind: crate::PrimKind::OptMakeSome,
            dst: Some(d),
            args,
        } => {
            // The slot holds the MIR scalar: a Float as its bits.
            let payload = match tys.get(&args[0]) {
                Some(NTy::I64) => var(args[0]),
                Some(NTy::F64) => format!("{}.to_bits() as i64", var(args[0])),
                other => {
                    return Err(wall(format!(
                        "native: OptMakeSome over a non-scalar payload ({other:?})"
                    )))
                }
            };
            tys.insert(*d, NTy::Opt);
            line!("let mut {}: Option<i64> = Some({});", var(*d), payload);
        }
        Op::Prim {
            kind: crate::PrimKind::OptMakeNone,
            dst: Some(d),
            ..
        } => {
            tys.insert(*d, NTy::Opt);
            line!("let mut {}: Option<i64> = None;", var(*d));
        }
        Op::Prim {
            kind: crate::PrimKind::OptCopy,
            dst: Some(d),
            args,
        } => {
            tys.insert(*d, NTy::Opt);
            line!("let mut {}: Option<i64> = {};", var(*d), opt_operand(tys, args[0])?);
        }
        Op::Prim {
            kind: crate::PrimKind::OptTag,
            dst: Some(d),
            args,
        } => {
            let o = opt_operand(tys, args[0])?;
            tys.insert(*d, NTy::I64);
            line!("let {}: i64 = {}.is_some() as i64;", var(*d), o);
        }
        Op::Prim {
            kind: crate::PrimKind::OptPayload,
            dst: Some(d),
            args,
        } => {
            let o = opt_operand(tys, args[0])?;
            tys.insert(*d, NTy::I64);
            line!("let {}: i64 = {}.unwrap_or(0);", var(*d), o);
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// The §13 termination-convention ops: `ProcExit` (a user exit code), and
/// the message half — `Handle` over a STRING local (an alias; the native
/// render has no address model) followed by `Die` (the wasm `$__die` twin:
/// the string to stderr verbatim, exit 1). `Ok(false)` = not this tier's op.
pub(crate) fn render_native_termination_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let OpSink {
        tys,
        out,
        indent,
        used_shims: _,
    } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        // The §13 termination convention's exit half (assert desugar tail,
        // time-ctor negative trap): a user exit code, no message of its own.
        Op::Prim {
            kind: crate::PrimKind::ProcExit,
            dst: None,
            args,
        } => {
            line!("std::process::exit({} as i32);", var(args[0]));
        }
        // The §13 termination convention's MESSAGE half — the err-abort
        // window of a `?`-propagation in main (`Handle(msg)` + `Die`).
        // The native render has no address model: a handle over a STRING
        // local is an alias (the value IS the string), and `Die` is the
        // wasm `$__die` twin — the string to stderr verbatim, exit 1.
        Op::Prim {
            kind: crate::PrimKind::Handle,
            dst: Some(d),
            args,
        } if matches!(tys.get(&args[0]), Some(NTy::Str | NTy::StrRef)) => {
            let src = match tys.get(&args[0]) {
                Some(NTy::Str) => format!("{}.as_str()", var(args[0])),
                _ => var(args[0]).to_string(),
            };
            tys.insert(*d, NTy::StrRef);
            line!("let {}: &str = {};", var(*d), src);
        }
        Op::Prim {
            kind: crate::PrimKind::Die,
            dst: None,
            args,
        } if matches!(tys.get(&args[0]), Some(NTy::Str | NTy::StrRef)) => {
            // The Die ABI carries panic's raw "PANIC: " message. Like the
            // embedded host's die trap, contain that abort during a request.
            // ProcExit remains the program's explicit, unconditional exit.
            line!("if {}.starts_with(\"PANIC: \") {{ almide_contained_unwind(|| {}.to_string()); }}", var(args[0]), var(args[0]));
            line!("eprint!(\"{{}}\", {});", var(args[0]));
            line!("std::process::exit(1);");
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// One VALUE-PRODUCING op (const, alloc, dup, list literal, int/float
/// arithmetic, local rebind) rendered into the sink. `Ok(false)` = not this
/// tier's op — the caller tries the flow tier, then walls. Arm bodies are
/// verbatim from the former inline [`render_fn`] op match.
pub(crate) fn render_native_scalar_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let OpSink {
        tys,
        out,
        indent,
        used_shims,
    } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        Op::ConstInt { dst, value } => {
            tys.insert(*dst, NTy::I64);
            line!("let mut {}: i64 = {}i64;", var(*dst), value);
        }
        Op::Alloc { dst, init, .. } => match init {
            Init::Str(s) => {
                tys.insert(*dst, NTy::Str);
                // The shared literal escaper (#3438): `{:?}` also escapes the
                // bidi controls today, but one rule for every native renderer
                // keeps that a property of the code, not of std's Debug tables.
                let lit = almide_base::rust_lit::escape_rust_str(s);
                // An immortal static block (`rc == 0`, ADR-0013): a literal
                // costs no allocation and no free, as the standard codegen's
                // borrowed `&str` literal does; an edit copies it first.
                line!(
                    "let mut {}: AlmideString = {{ static S: crate::almide_str_block::AlmideStrStatic<{}> = crate::almide_str_block::AlmideStrStatic::new(\"{lit}\"); AlmideString::from_static(&S) }};",
                    var(*dst),
                    s.len()
                );
            }
            // #4006: a constant int list (a literal the lowering folded, an
            // empty `[]`, a top-level list `let`) — the same Vec a ListLit renders.
            Init::IntList(vs) => {
                tys.insert(*dst, NTy::Vec);
                let items = vs.iter().map(|v| format!("{v}i64")).collect::<Vec<_>>().join(", ");
                if crate::render_native_move::is_stack_lit(*dst) {
                    // Borrowed only (crate::render_native_move::set_stack_lits).
                    line!("let mut {}: [i64; {}] = [{items}];", var(*dst), vs.len());
                } else {
                    line!("let mut {}: Vec<i64> = vec![{items}];", var(*dst));
                }
            }
            other => {
                return Err(wall(format!(
                    "native: Alloc {other:?} — outside the rung subset"
                )))
            }
        },
        Op::Dup { dst, src } => render_dup(dst, src, tys, out, indent)?,
        // Rung-4 scalar-list literal: the natural Vec spelling. Elements are raw
        // i64 slot values (the wasm leg stores the same bits).
        Op::ListLit { dst, elems } => {
            // A real `f64` element goes into its slot as its bits — the value
            // the MIR (and the wasm leg) stores there.
            let mut items = Vec::with_capacity(elems.len());
            for e in elems {
                items.push(match tys.get(e) {
                    Some(NTy::I64) => var(*e),
                    Some(NTy::F64) => format!("{}.to_bits() as i64", var(*e)),
                    _ => return Err(wall("native: ListLit with a non-scalar element")),
                });
            }
            tys.insert(*dst, NTy::Vec);
            let n = items.len();
            let items = items.join(", ");
            if crate::render_native_move::is_stack_lit(*dst) {
                // Borrowed only (crate::render_native_move::set_stack_lits).
                line!("let mut {}: [i64; {n}] = [{items}];", var(*dst));
            } else {
                line!("let mut {}: Vec<i64> = vec![{items}];", var(*dst));
            }
        }
        Op::SetLocal { local, src } => render_set_local(local, src, tys, out, indent)?,
        Op::IntBinOp { dst, op, a, b } => {
            let (l, r) = (scalar_bits(*a, tys)?, scalar_bits(*b, tys)?);
            match crate::render_native::cmp_int_expr(op, &l, &r) {
                Some(e) => line!("{}", crate::render_native_cond::bind(*dst, &e)),
                None => line!("let mut {}: i64 = {};", var(*dst), render_int_binop(op, *a, *b, tys, used_shims)?),
            }
            tys.insert(*dst, NTy::I64);
        }
        _ => {
            return render_native_float_op(
                op,
                OpSink {
                    tys,
                    out,
                    indent,
                    used_shims,
                },
            )
        }
    }
    Ok(true)
}

/// Rung-5 float floor: MIR floats are i64 BITS; native computes in real
/// f64. Every op below is IEEE-754-exact on both targets (hardware ops,
/// identical bit results), so byte-identity holds through
/// `float.to_string`. Min/Max/CopySign are excluded: Rust's `f64::min`
/// NaN semantics differ from wasm `f64.min` (they only occur inside
/// self-host bodies, which never render natively).
pub(crate) fn render_native_float_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let OpSink {
        tys, out, indent, ..
    } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    match op {
        Op::Prim {
            kind: crate::PrimKind::FloatBin(op),
            dst: Some(d),
            args,
        } if args.len() == 2 => render_float_bin(op, d, args, tys, out, indent)?,
        // Both integer-to-f64 primitives use the same representation boundary.
        Op::Prim {
            kind: crate::PrimKind::F64FromInt | crate::PrimKind::IntToFloat,
            dst: Some(d),
            args,
        } if args.len() == 1 => {
            if tys.get(&args[0]) != Some(&NTy::I64) {
                return Err(wall("native: integer-to-float conversion requires an i64 operand"));
            }
            tys.insert(*d, NTy::F64);
            line!("let mut {}: f64 = ({} as f64);", var(*d), var(args[0]));
        }
        Op::Prim {
            kind: crate::PrimKind::FloatUn(op),
            dst: Some(d),
            args,
        } if args.len() == 1 => render_float_un(op, d, args, tys, out, indent)?,
        Op::Prim { kind: crate::PrimKind::FloatToInt, dst: Some(d), args } if args.len() == 1 => {
            let value = match tys.get(&args[0]) {
                Some(NTy::F64) => var(args[0]),
                Some(NTy::I64) => format!("f64::from_bits({} as u64)", var(args[0])),
                _ => return Err(wall("native: float-to-int conversion requires a float value or its bits")),
            };
            tys.insert(*d, NTy::I64);
            line!("let mut {}: i64 = ({value}) as i64;", var(*d));
        }
        Op::Prim {
            kind: crate::PrimKind::FloatCmp(op),
            dst: Some(d),
            args,
        } if args.len() == 2 => render_float_cmp(op, d, args, tys, out, indent)?,
        // Exercised by float32_arithmetic_rounds -> float_fmod. Preserve
        // the bits of an f64 local; an i64 operand already holds those bits.
        Op::Prim { kind: crate::PrimKind::FloatBits, dst: Some(d), args } if args.len() == 1 => {
            let bits = match tys.get(&args[0]) {
                Some(NTy::I64) => var(args[0]),
                Some(NTy::F64) => format!("{}.to_bits() as i64", var(args[0])),
                other => return Err(wall(format!("native: float bits of a non-scalar value ({other:?})"))),
            };
            tys.insert(*d, NTy::I64);
            line!("let mut {}: i64 = {bits};", var(*d));
        }
        _ => return crate::render_native_float32::render(op, OpSink { tys, out, indent, used_shims: s.used_shims }),
    }
    Ok(true)
}

/// An Option-carrier operand, refusing anything the rewrite did not type `Opt`.
fn opt_operand(tys: &std::collections::BTreeMap<crate::ValueId, NTy>, v: crate::ValueId) -> Result<String, LowerError> {
    match tys.get(&v) {
        Some(NTy::Opt) => Ok(var(v)),
        other => Err(wall(format!("native: Option read over a non-Option value ({other:?})"))),
    }
}

/// Native exponentiation keeps the full exponent, the source abort, and a
/// concrete wrapping accumulator type. It never delegates a narrow multiply
/// to an i64 shim and never truncates the exponent to u32.
fn render_narrow_pow_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let Op::Prim { kind: crate::PrimKind::NativeNarrowPow { bits, signed }, dst: Some(dst), args } = op
        else { return Ok(false) };
    if args.len() != 2 || args.iter().any(|v| s.tys.get(v) != Some(&NTy::I64)) {
        return Err(wall("native: narrow power requires two integer operands"));
    }
    if !matches!(bits, 8 | 16 | 32) { return Err(wall("native: unsupported narrow power width")); }
    let ty = format!("{}{bits}", if *signed { "i" } else { "u" });
    let (a, b) = (var(args[0]), var(args[1]));
    s.tys.insert(*dst, NTy::I64);
    for _ in 0..s.indent { s.out.push_str("    "); }
    writeln!(s.out, "let mut {}: i64 = {{ let mut __base = {a} as {ty}; let __exp = {b}; \
        if __exp < 0 {{ almide_abort(\"negative exponent\"); }} \
        let mut __n = __exp as u64; let mut __r: {ty} = 1; \
        while __n > 0 {{ if __n & 1 == 1 {{ __r = __r.wrapping_mul(__base); }} \
        __n >>= 1; if __n > 0 {{ __base = __base.wrapping_mul(__base); }} }} __r as i64 }};", var(*dst)).expect("writing to a String cannot fail");
    Ok(true)
}
