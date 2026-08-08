//! The per-tier native op renderers — each takes one MIR op and an
//! [`OpSink`] and returns `Ok(false)` for "not this tier's op", so
//! `render_fn`'s dispatcher chains them: metering (fuel/timeout/budget),
//! the T1-3 Result carrier, the §13 termination convention, and the
//! value-producing scalar/float tiers. Arm bodies are verbatim from the
//! former inline `render_fn` op match.

use crate::lower::LowerError;
use crate::render_native::{
    render_dup, render_float_bin, render_float_cmp, render_float_un, render_int_binop,
    render_set_local, OpSink,
};
use crate::render_native::{var, wall, NTy};
use crate::render_native_shims::{
    BUDGET_SHIM, CHARGE_DYN_SHIM, CHARGE_SHIM, COUNTER_SHIM, CUT_RET_MARKER, FUEL_LT0_SHIM,
    TIMEOUT_SHIM,
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
        _ => return Ok(false),
    }
    Ok(true)
}

/// The T1-3 native Result carrier ops (`native_result_rewrite`). `Ok(false)`
/// = not this tier's op. Arm bodies verbatim from [`render_fn`].
pub(crate) fn render_native_result_op(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
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
    #[allow(unused_variables)]
    let used_shims = used_shims;
    match op {
        // ── T1-3 native Result carrier (native_result_rewrite) ──
        Op::Prim {
            kind: crate::PrimKind::ResMakeOk,
            dst: Some(d),
            args,
        } => {
            tys.insert(*d, NTy::Res);
            line!(
                "let {}: Result<i64, String> = Ok({});",
                var(*d),
                var(args[0])
            );
        }
        Op::Prim {
            kind: crate::PrimKind::ResMakeErrStr,
            dst: Some(d),
            args,
        } => {
            let src = match tys.get(&args[0]) {
                Some(NTy::Str) => format!("{}.clone()", var(args[0])),
                Some(NTy::StrRef) => format!("{}.to_string()", var(args[0])),
                other => {
                    return Err(wall(format!(
                        "native: ResMakeErrStr over a non-string payload ({other:?})"
                    )))
                }
            };
            tys.insert(*d, NTy::Res);
            line!("let {}: Result<i64, String> = Err({});", var(*d), src);
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
            line!(
                "let {}: &str = match &{} {{ Err(e) => e.as_str(), Ok(_) => \"\" }};",
                var(*d),
                var(args[0])
            );
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
        used_shims,
    } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    #[allow(unused_variables)]
    let used_shims = used_shims;
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
                line!("let mut {}: String = String::from({s:?});", var(*dst));
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
            for e in elems {
                if tys.get(e) != Some(&NTy::I64) {
                    return Err(wall("native: ListLit with a non-scalar element"));
                }
            }
            tys.insert(*dst, NTy::Vec);
            let items = elems.iter().map(|e| var(*e)).collect::<Vec<_>>().join(", ");
            line!("let mut {}: Vec<i64> = vec![{items}];", var(*dst));
        }
        Op::SetLocal { local, src } => render_set_local(local, src, tys, out, indent)?,
        Op::IntBinOp { dst, op, a, b } => {
            tys.insert(*dst, NTy::I64);
            let rendered = render_int_binop(op, *a, *b, used_shims)?;
            line!("let mut {}: i64 = {};", var(*dst), rendered);
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
        // `float.from_int` — int (i64) to f64, carried per the float floor.
        Op::Prim {
            kind: crate::PrimKind::F64FromInt,
            dst: Some(d),
            args,
        } if args.len() == 1 => {
            tys.insert(*d, NTy::F64);
            line!("let mut {}: f64 = ({} as f64);", var(*d), var(args[0]));
        }
        Op::Prim {
            kind: crate::PrimKind::FloatUn(op),
            dst: Some(d),
            args,
        } if args.len() == 1 => render_float_un(op, d, args, tys, out, indent)?,
        Op::Prim {
            kind: crate::PrimKind::FloatCmp(op),
            dst: Some(d),
            args,
        } if args.len() == 2 => render_float_cmp(op, d, args, tys, out, indent)?,
        _ => return Ok(false),
    }
    Ok(true)
}
