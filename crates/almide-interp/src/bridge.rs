//! Scalar / string / math native bridge.
//!
//! The monomorphic, concrete-typed runtime surface (`int.*`, `float.*`,
//! `string.*`, `math.*`) is dispatched here by `(module, func)` name. Each glue
//! reproduces the native runtime fn's behavior directly over `Value` — the
//! runtime fns are mostly one-liners over Rust std, so the bridge stays small
//! and stays byte-identical to native (the std behavior IS the oracle).
//!
//! Deliberately NOT taking a path dependency on the `almide_rt` crate in this
//! phase: that crate pulls in rustls/webpki (network/TLS), bloating the build
//! and importing effectful surface the interp does not need. The pure scalar
//! fns are reproduced inline. Wiring `almide_rt` as a path dep to cover the
//! full ~200-fn surface is the documented next-phase expansion.
//!
//! Returns `None` when `(module, func)` is not a bridged scalar fn (the caller
//! then tries an almide-bodied stdlib fn or reports `Unsupported`).

use crate::value::{float_to_string, Value};
use crate::Flow;

/// Dispatch a scalar/string/math `(module, func)` with evaluated args.
pub(crate) fn dispatch(module: &str, func: &str, args: &[Value]) -> Option<Flow> {
    match module {
        "int" => int_fn(func, args),
        "float" => float_fn(func, args),
        "string" => string_fn(func, args),
        "math" => math_fn(func, args),
        "bool" => bool_fn(func, args),
        "path" => path_fn(func, args),
        "bytes" => bytes_fn(func, args),
        "uint64" => uint64_fn(func, args),
        "int8" | "int16" | "int32" | "int64" | "uint8" | "uint16" | "uint32"
        | "float32" | "float64" => sized_numeric_fn(module, func, args),
        // The scalar prim floor — consumed by the self-hosted stdlib bodies the
        // `stdlib_pool` evaluates, not by fixture code directly.
        "prim" => prim_fn(func, args),
        _ => None,
    }
}

/// The SIZED-numeric observers (`int8.to_string`, `uint16.to_string`, …).
/// Every one of these widths fits the interpreter's `i64`/`f64` carrier
/// exactly — the values arriving here were already wrapped to the declared
/// width by the arithmetic that produced them (C-180) — so printing is the
/// canonical print of the carrier. Bridged so a fixture that merely NAMES a
/// sized width still evaluates in the third oracle instead of abstaining.
fn sized_numeric_fn(module: &str, func: &str, args: &[Value]) -> Option<Flow> {
    match func {
        "to_string" => {
            if module.starts_with("float") {
                let f = as_float(args.first())?;
                Some(Flow::val(Value::str(float_to_string(f))))
            } else {
                let n = as_int(args.first())?;
                Some(Flow::val(Value::str(n.to_string())))
            }
        }
        _ => None,
    }
}

/// `uint64.*` — the UNSIGNED 64-bit observers (#872, C-179). The interpreter
/// carries integers in the same `i64` slot the IR does, so the upper half of
/// `UInt64` arrives here as a NEGATIVE `i64` bit pattern: read it back as
/// `u64` rather than pivoting through the signed value (which printed `-1`
/// for `u64::MAX`). Bridged so the third oracle EVALUATES the lane instead of
/// abstaining — an abstention is a hole in the executable spec.
fn uint64_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let n = as_int(args.first())? as u64;
    let f = match func {
        "to_string" => Flow::val(Value::str(n.to_string())),
        "to_float32" | "to_float64" => Flow::val(Value::Float(n as f64)),
        _ => return None,
    };
    Some(f)
}

/// `path.*` — pure path-STRING manipulation, so the third oracle can evaluate it
/// (nothing here touches the filesystem). Added to stop the bundled-module and
/// docs fixtures abstaining: an abstention is a hole in the executable spec.
fn path_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let s = as_str(args.first())?;
    let f = match func {
        "join" => {
            let child = as_str(args.get(1))?;
            let base = s.trim_end_matches('/');
            Flow::val(Value::str(format!("{}/{}", base, child.trim_start_matches('/'))))
        }
        "dirname" => Flow::val(Value::str(match s.rfind('/') {
            Some(0) => "/".to_string(),
            Some(i) => s[..i].to_string(),
            None => ".".to_string(),
        })),
        "basename" => Flow::val(Value::str(
            s.rsplit('/').next().unwrap_or(s).to_string(),
        )),
        "is_absolute" => Flow::val(Value::Bool(s.starts_with('/'))),
        "extension" => {
            let base = s.rsplit('/').next().unwrap_or(s);
            // A leading-dot name (`.gitignore`) has no extension.
            let ext = base.rfind('.').filter(|i| *i > 0).map(|i| base[i + 1..].to_string());
            Flow::val(Value::Option(ext.map(|e| Box::new(Value::str(e)))))
        }
        "stem" => {
            let base = s.rsplit('/').next().unwrap_or(s);
            let stem = match base.rfind('.').filter(|i| *i > 0) {
                Some(i) => &base[..i],
                None => base,
            };
            Flow::val(Value::str(stem.to_string()))
        }
        _ => return None,
    };
    Some(f)
}

/// `bytes.*` — the subset with a pure list-of-int model. A `Bytes` value is a
/// `List[Int]` in the interp, so `from_list` is the identity and `to_string_lossy`
/// decodes. The mutating and raw-pointer surface stays out of scope.
fn bytes_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "from_list" => Flow::val(args.first()?.clone()),
        "to_list" => Flow::val(args.first()?.clone()),
        "len" => Flow::val(Value::Int(args.first()?.as_iter_items()?.len() as i64)),
        "new" => Flow::val(Value::list(vec![Value::Int(0); as_int(args.first())?.max(0) as usize])),
        "from_string" => Flow::val(Value::list(
            as_str(args.first())?.bytes().map(|b| Value::Int(b as i64)).collect(),
        )),
        "to_string_lossy" => {
            let raw: Vec<u8> = args
                .first()?
                .as_iter_items()?
                .iter()
                .map(|v| match v { Value::Int(i) => *i as u8, _ => 0 })
                .collect();
            Flow::val(Value::str(String::from_utf8_lossy(&raw).into_owned()))
        }
        _ => return None,
    };
    Some(f)
}

// ── helpers to pull typed args ──────────────────────────────────

fn as_int(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Int(n)) => Some(*n),
        _ => None,
    }
}
fn as_float(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Float(f)) => Some(*f),
        _ => None,
    }
}
fn as_str(v: Option<&Value>) -> Option<&str> {
    match v {
        Some(Value::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn abort_args(module: &str, func: &str) -> Flow {
    Flow::Abort(format!("internal: bad args to {}.{}", module, func))
}

// ── int ─────────────────────────────────────────────────────────

fn int_fn(func: &str, args: &[Value]) -> Option<Flow> {
    int_fn_a(func, args)
        .or_else(|| int_fn_b(func, args))
}

/// The SCALAR prim floor — the whole prim vocabulary a self-hosted stdlib body
/// (`stdlib_pool`) may consume in this oracle. Everything here is a total,
/// deterministic scalar→scalar op with one exact Rust spelling, cited against
/// the MIR op docs (`almide-mir/src/lib_b.rs`) the two backends render from.
///
/// Deliberately NOT here, and why `None` is the right answer for them: the heap
/// and effect prims (`prim.handle`, `prim.load*`/`store*`, `prim.alloc_*`,
/// `prim.rc_*`, `prim.fd_write`, `prim.random_get`…). The interp models a container
/// as a `Value`, not linear memory, so a handle has no faithful value — bridging
/// `prim.handle` as the identity would let `prim.handle(b) + 12` add 12 to a
/// list and vote WRONG, which is worse than the honest abstain the `None`
/// produces (the caller's fixture skips with the prim named). `prim.die` rides
/// out with them: its argument is always `prim.handle(<message>)`, so the eval
/// abstains on the handle before die is reached.
fn prim_fn(func: &str, args: &[Value]) -> Option<Flow> {
    // Bitwise / shifts: i64-uniform; wasm's `i64.shl` family masks the count
    // to 6 bits, so the floor does too (a Rust bare `<<` would panic instead).
    let int2 = |f: fn(i64, i64) -> i64| -> Option<Flow> {
        Some(Flow::val(Value::Int(f(as_int(args.first())?, as_int(args.get(1))?))))
    };
    // f64 arithmetic / comparison: real f64s in the interp carrier.
    let float2 = |f: fn(f64, f64) -> f64| -> Option<Flow> {
        Some(Flow::val(Value::Float(f(as_float(args.first())?, as_float(args.get(1))?))))
    };
    let fcmp = |f: fn(&f64, &f64) -> bool| -> Option<Flow> {
        Some(Flow::val(Value::Bool(f(&as_float(args.first())?, &as_float(args.get(1))?))))
    };
    match func {
        "band" => int2(|a, b| a & b),
        "bor" => int2(|a, b| a | b),
        "bxor" => int2(|a, b| a ^ b),
        "bshl" => int2(|a, b| a.wrapping_shl(b as u32 & 63)),
        "bshr" => int2(|a, b| a.wrapping_shr(b as u32 & 63)),
        "bshr_u" => int2(|a, b| ((a as u64).wrapping_shr(b as u32 & 63)) as i64),
        // Int ⇄ Float: `f64.convert_i64_s` / saturating `i64.trunc_sat_f64_s`
        // (Rust's float→int `as` is exactly the saturating truncate).
        "i2f" => Some(Flow::val(Value::Float(as_int(args.first())? as f64))),
        "f2i" => Some(Flow::val(Value::Int(as_float(args.first())? as i64))),
        // The raw f64 ⇄ i64 BIT reinterpret (`float.to_bits` / `int.bits_to_float`).
        "fbits" => Some(Flow::val(Value::Int(as_float(args.first())?.to_bits() as i64))),
        "ffrombits" => {
            Some(Flow::val(Value::Float(f64::from_bits(as_int(args.first())? as u64))))
        }
        // The f32 conventions: the language-level Float32 is CARRIED widened (one
        // f64 in this oracle, the widened bits in the backends), the narrowing
        // rounds to nearest, and the pattern lives in the low 32 bits.
        "f2f32" => Some(Flow::val(Value::Float((as_float(args.first())? as f32) as f64))),
        "f32_2f" => Some(Flow::val(Value::Float(as_float(args.first())?))),
        "i2f32" => Some(Flow::val(Value::Float((as_int(args.first())? as f32) as f64))),
        "f32bits" => Some(Flow::val(Value::Int(
            ((as_float(args.first())? as f32).to_bits()) as i64,
        ))),
        "bits_to_f32" => Some(Flow::val(Value::Float(
            f32::from_bits(as_int(args.first())? as u32) as f64,
        ))),
        "fadd" => float2(|a, b| a + b),
        "fsub" => float2(|a, b| a - b),
        "fmul" => float2(|a, b| a * b),
        "fdiv" => float2(|a, b| a / b),
        "fcopysign" => float2(f64::copysign),
        "fneg" => Some(Flow::val(Value::Float(-as_float(args.first())?))),
        "fabs" => Some(Flow::val(Value::Float(as_float(args.first())?.abs()))),
        "fsqrt" => Some(Flow::val(Value::Float(as_float(args.first())?.sqrt()))),
        "fceil" => Some(Flow::val(Value::Float(as_float(args.first())?.ceil()))),
        "ffloor" => Some(Flow::val(Value::Float(as_float(args.first())?.floor()))),
        "feq" => fcmp(|a, b| a == b),
        "fne" => fcmp(|a, b| a != b),
        "flt" => fcmp(|a, b| a < b),
        "fgt" => fcmp(|a, b| a > b),
        "fge" => fcmp(|a, b| a >= b),
        "fle" => fcmp(|a, b| a <= b),
        _ => None,
    }
}

/// The first half of the `int.*` arm table.
///
/// Extracted from `int_fn` (arm-table halving): arms verbatim and in
/// source order, so the router's order is the only ordering that matters.
fn int_fn_a(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "to_string" => {
            let n = as_int(args.first())?;
            Flow::val(Value::str(n.to_string()))
        }
        "to_float" => {
            let n = as_int(args.first())?;
            Flow::val(Value::Float(n as f64))
        }
        "abs" => Flow::val(Value::Int(as_int(args.first())?.abs())),
        "min" => Flow::val(Value::Int(as_int(args.first())?.min(as_int(args.get(1))?))),
        "max" => Flow::val(Value::Int(as_int(args.first())?.max(as_int(args.get(1))?))),
        "clamp" => {
            let n = as_int(args.first())?;
            let lo = as_int(args.get(1))?;
            let hi = as_int(args.get(2))?;
            // ALS-T6: an inverted range is the abort form (a raw Rust clamp
            // here would panic the harness instead of voting).
            if lo > hi {
                return Some(Flow::Abort("clamp requires min <= max".to_string()));
            }
            Flow::val(Value::Int(n.clamp(lo, hi)))
        }
        "to_hex" => Flow::val(Value::str(format!("{:x}", as_int(args.first())?))),
        _ => return None,
    };
    Some(f)
}

/// The second half of the `int.*` arm table.
///
/// Extracted from `int_fn` (arm-table halving): arms verbatim and in
/// source order, so the router's order is the only ordering that matters.
fn int_fn_b(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "parse" | "from_string" => {
            let s = as_str(args.first())?;
            // Returns Result[Int, String] — matches almide_rt_int_parse.
            match s.trim().parse::<i64>() {
                Ok(n) => Flow::val(Value::Result(Ok(Box::new(Value::Int(n))))),
                Err(e) => Flow::val(Value::Result(Err(Box::new(Value::str(e.to_string()))))),
            }
        }
        // Bitwise.
        "band" => Flow::val(Value::Int(as_int(args.first())? & as_int(args.get(1))?)),
        "bor" => Flow::val(Value::Int(as_int(args.first())? | as_int(args.get(1))?)),
        "bxor" => Flow::val(Value::Int(as_int(args.first())? ^ as_int(args.get(1))?)),
        "bnot" => Flow::val(Value::Int(!as_int(args.first())?)),
        "bshl" => Flow::val(Value::Int(as_int(args.first())? << as_int(args.get(1))?)),
        "bshr" => Flow::val(Value::Int(as_int(args.first())? >> as_int(args.get(1))?)),
        _ => return None,
    };
    Some(f)
}

// ── float ───────────────────────────────────────────────────────

/// The sized-float CONVERSIONS (`float.to_float32`/`to_float64`/
/// `from_float32`/`from_float64`): the interpreter carries every float in one
/// `f64`, and `Float32`/`Float64` are the SAME carrier at the language level
/// (the narrowing to f32 precision is the emitters' concern), so each of these
/// is the identity here. Bridged so a fixture that merely NAMES a sized float
/// still evaluates in the third oracle instead of abstaining.
fn float_sized_conv(func: &str, args: &[Value]) -> Option<Flow> {
    let n = as_float(args.first())?;
    match func {
        "to_float64" | "from_float64" => Some(Flow::val(Value::Float(n))),
        // f32 round-trips through the narrower precision, exactly as both
        // emitters do — the value a `Float32` can actually hold.
        "to_float32" | "from_float32" => Some(Flow::val(Value::Float(n as f32 as f64))),
        _ => None,
    }
}

fn float_fn(func: &str, args: &[Value]) -> Option<Flow> {
    if let Some(f) = float_sized_conv(func, args) {
        return Some(f);
    }
    float_fn_core(func, args).or_else(|| float_fn_convert(func, args))
}

/// Rounding, sign, and the total-order predicates.
///
/// Extracted from `float_fn` (name-router split, arms verbatim and in source order).
fn float_fn_core(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {

        _ => return None,
    };
    Some(f)
}

/// Conversions to/from other numeric types and to string.
///
/// Extracted from `float_fn` (name-router split, arms verbatim and in source order).
fn float_fn_convert(func: &str, args: &[Value]) -> Option<Flow> {
    float_fn_convert_a(func, args)
        .or_else(|| float_fn_convert_b(func, args))
}

/// The first half of `float_fn_convert`'s arm table.
///
/// Extracted from `float_fn_convert` (arm-table halving): arms verbatim and in
/// source order, so the router's order is the only ordering that matters.
fn float_fn_convert_a(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "to_string" => Flow::val(Value::str(float_to_string(as_float(args.first())?))),
        "to_int" => Flow::val(Value::Int(as_float(args.first())? as i64)),
        "from_int" => Flow::val(Value::Float(as_int(args.first())? as f64)),
        "abs" => Flow::val(Value::Float(as_float(args.first())?.abs())),
        "ceil" => Flow::val(Value::Float(as_float(args.first())?.ceil())),
        "floor" => Flow::val(Value::Float(as_float(args.first())?.floor())),
        "round" => Flow::val(Value::Float(as_float(args.first())?.round())),
        "sqrt" => Flow::val(Value::Float(as_float(args.first())?.sqrt())),
        // Explicit NaN/tie tree mirroring runtime/rs/src/float.rs
        // almide_rt_float_min/max — NOT f64::min/max (llvm.minnum/maxnum has
        // unspecified ±0-tie order). Ties return the FIRST operand (C-049).
        _ => return None,
    };
    Some(f)
}

/// The second half of `float_fn_convert`'s arm table.
///
/// Extracted from `float_fn_convert` (arm-table halving): arms verbatim and in
/// source order, so the router's order is the only ordering that matters.
fn float_fn_convert_b(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "min" => {
            let (a, b) = (as_float(args.first())?, as_float(args.get(1))?);
            Flow::val(Value::Float(if a.is_nan() { b } else if b.is_nan() { a } else if a > b { b } else { a }))
        }
        "max" => {
            let (a, b) = (as_float(args.first())?, as_float(args.get(1))?);
            Flow::val(Value::Float(if a.is_nan() { b } else if b.is_nan() { a } else if a < b { b } else { a }))
        }
        "clamp" => {
            let n = as_float(args.first())?;
            let lo = as_float(args.get(1))?;
            let hi = as_float(args.get(2))?;
            // ALS-T6: lo > hi OR a NaN bound is the abort form — `!(lo <= hi)`
            // covers both (a raw f64::clamp here would panic the harness).
            if !(lo <= hi) {
                return Some(Flow::Abort("clamp requires min <= max".to_string()));
            }
            Flow::val(Value::Float(n.clamp(lo, hi)))
        }
        "sign" => Flow::val(Value::Float(as_float(args.first())?.signum())),
        "is_nan" => Flow::val(Value::Bool(as_float(args.first())?.is_nan())),
        "is_infinite" => Flow::val(Value::Bool(as_float(args.first())?.is_infinite())),
        "to_fixed" => {
            let n = as_float(args.first())?;
            let d = as_int(args.get(1))?;
            // ALS-T6: out-of-domain decimals abort (mirrors runtime/rs float.rs).
            // 0..=4096 — the 1e6 bound was NOT total (format! caps runtime
            // precision at u16::MAX; an f64's exact expansion is ≤ ~1074 digits).
            if !(0..=4096).contains(&d) {
                return Some(Flow::Abort("to_fixed requires decimals in 0..=4096".to_string()));
            }
            Flow::val(Value::str(format!("{:.1$}", n, d as usize)))
        }
        "parse" => {
            let s = as_str(args.first())?;
            match s.trim().parse::<f64>() {
                Ok(n) => Flow::val(Value::Result(Ok(Box::new(Value::Float(n))))),
                Err(e) => Flow::val(Value::Result(Err(Box::new(Value::str(e.to_string()))))),
            }
        }
        _ => return None,
    };
    Some(f)
}

// ── math ────────────────────────────────────────────────────────

fn math_fn(func: &str, args: &[Value]) -> Option<Flow> {
    // TRANSCENDENTALS DIVERGE FROM THE ORACLE. Both backends deliberately route
    // `math.sin/cos/tan/exp/log*/pow` (and float `**`) through a VENDORED
    // musl-libm (`runtime/rs/src/libm.rs`, mirrored by `emit_wasm/rt_libm.rs`)
    // rather than the platform `f64::sin/…`, because the system libm's last-ULP
    // result is platform-specific and provides no stable oracle (the StrictMath
    // / fdlibm decision). Rust `std`'s `f64::sin` calls that same platform libm,
    // so if the interp used it here it would diverge from the native==wasm
    // consensus in the last ULP (e.g. `0.799441007199113` vs
    // `0.7994410071991129`) and cast a WRONG third vote into the cross-target
    // oracle. The interp does not vendor the libm (it would couple this lean
    // crate to `almide_rt`'s heavyweight TLS deps and risk silent drift from the
    // oracle), so it honestly reports `Unsupported` for the platform-libm
    // transcendentals; the 3-way gate logs a reasoned skip.
    //
    // SAFE here: `sqrt` is IEEE-754 correctly-rounded (identical on every libm /
    // platform), `abs` is exact, and `pi` / `e` are constants — all match the
    // backends bit-for-bit.
    if matches!(
        func,
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2"
            | "sinh" | "cosh" | "tanh"
            | "exp" | "exp2" | "expm1"
            | "ln" | "log" | "log2" | "log10" | "log1p"
            | "pow" | "fpow" | "powf" | "cbrt" | "hypot"
    ) {
        return Some(Flow::Unsupported(format!(
            "transcendental `math.{func}` (backends use vendored musl-libm; \
             interp's platform libm diverges in the last ULP — no oracle match)"
        )));
    }
    let f = match func {
        "pi" => Flow::val(Value::Float(std::f64::consts::PI)),
        "e" => Flow::val(Value::Float(std::f64::consts::E)),
        "sqrt" => Flow::val(Value::Float(as_float(args.first())?.sqrt())),
        "abs" => Flow::val(Value::Float(as_float(args.first())?.abs())),
        _ => return None,
    };
    Some(f)
}

// ── bool ────────────────────────────────────────────────────────

fn bool_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "to_string" => match args.first() {
            Some(Value::Bool(b)) => Flow::val(Value::str(b.to_string())),
            _ => abort_args("bool", "to_string"),
        },
        _ => return None,
    };
    Some(f)
}

// ── string ──────────────────────────────────────────────────────

fn string_fn(func: &str, args: &[Value]) -> Option<Flow> {
    string_fn_whole(func, args)
        .or_else(|| string_fn_slice(func, args))
        .or_else(|| string_fn_structural(func, args))
}

/// Whole-string predicates and transforms — no index arithmetic.
///
/// Extracted from `string_fn` (name-router split, arms verbatim and in source
/// order). `None` means "not my group", so the router's order is the only
/// ordering that matters.
fn string_fn_whole(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "len" | "length" => Flow::val(Value::Int(as_str(args.first())?.chars().count() as i64)),
        "char_count" => Flow::val(Value::Int(as_str(args.first())?.chars().count() as i64)),
        "is_empty" => Flow::val(Value::Bool(as_str(args.first())?.is_empty())),
        "to_upper" => Flow::val(Value::str(as_str(args.first())?.to_uppercase())),
        "to_lower" => Flow::val(Value::str(as_str(args.first())?.to_lowercase())),
        "trim" => Flow::val(Value::str(as_str(args.first())?.trim().to_string())),
        "trim_start" => Flow::val(Value::str(as_str(args.first())?.trim_start().to_string())),
        "trim_end" => Flow::val(Value::str(as_str(args.first())?.trim_end().to_string())),
        "reverse" => Flow::val(Value::str(
            as_str(args.first())?.chars().rev().collect::<String>(),
        )),
        "contains" => Flow::val(Value::Bool(
            as_str(args.first())?.contains(as_str(args.get(1))?),
        )),
        "starts_with" => Flow::val(Value::Bool(
            as_str(args.first())?.starts_with(as_str(args.get(1))?),
        )),
        "ends_with" => Flow::val(Value::Bool(
            as_str(args.first())?.ends_with(as_str(args.get(1))?),
        )),
        "replace" => Flow::val(Value::str(
            as_str(args.first())?
                .replace(as_str(args.get(1))?, as_str(args.get(2))?),
        )),
        "repeat" => {
            // Negative counts clamp to 0 (C-054) and a result past the shared
            // 2^31-byte ceiling aborts in the T6 form (C-169) — both mirror
            // runtime/rs almide_rt_string_repeat exactly; without the ceiling
            // the interp materialized multi-GB strings and dissented from the
            // two backends' identical abort (nightly-fuzz OutputDivergence,
            // seed 1785045556318379299 index 11).
            let s = as_str(args.first())?;
            let n = as_int(args.get(1))?.max(0);
            if (s.len() as i64).saturating_mul(n) > (1_i64 << 31) {
                Flow::Abort("repeat result too large".to_string())
            } else {
                Flow::val(Value::str(s.repeat(n as usize)))
            }
        }
        // Codepoint-count take, the C-054 unsigned discipline (mirrors
        // runtime/rs almide_rt_string_take: `chars().take(n as usize)` — a
        // negative n is enormous as usize, so take(-1) keeps the whole string).
        _ => return None,
    };
    Some(f)
}

/// Codepoint-indexed slicing and counting. Every clamp mirrors C-034: an
/// UNSIGNED count saturates (a negative one is enormous as a `usize`), and
/// `slice`'s start/end clamp SIGNED — the one documented exception.
///
/// Extracted from `string_fn` (name-router split, arms verbatim and in source
/// order). `None` means "not my group", so the router's order is the only
/// ordering that matters.
fn string_fn_slice(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "take" => Flow::val(Value::str(
            as_str(args.first())?
                .chars()
                .take(as_int(args.get(1))? as usize)
                .collect::<String>(),
        )),
        "count" => Flow::val(Value::Int(
            as_str(args.first())?.matches(as_str(args.get(1))?).count() as i64,
        )),
        // `drop`/`take_end`/`drop_end` are `take`'s unsigned-count siblings, and
        // `slice` clamps its start/end SIGNED (the one C-034 exception: the native
        // oracle is `(x.max(0) as usize).min(len)`), with a reversed range empty.
        "drop" => Flow::val(Value::str(
            as_str(args.first())?
                .chars()
                .skip(as_int(args.get(1))? as usize)
                .collect::<String>(),
        )),
        "take_end" | "drop_end" => {
            let cs: Vec<char> = as_str(args.first())?.chars().collect();
            let raw = as_int(args.get(1))?;
            // Unsigned: a negative count is enormous, so it saturates to all.
            let k = if raw < 0 { cs.len() } else { (raw as usize).min(cs.len()) };
            let kept: String = if func == "take_end" {
                cs[cs.len() - k..].iter().collect()
            } else {
                cs[..cs.len() - k].iter().collect()
            };
            Flow::val(Value::str(kept))
        }
        "slice" => {
            let cs: Vec<char> = as_str(args.first())?.chars().collect();
            let clamp = |i: i64| -> usize { (i.max(0) as usize).min(cs.len()) };
            let start = clamp(as_int(args.get(1))?);
            let end = match args.get(2) {
                Some(Value::Int(e)) => clamp(*e),
                _ => cs.len(),
            };
            let out: String = if end > start { cs[start..end].iter().collect() } else { String::new() };
            Flow::val(Value::str(out))
        }
        // index_of returns Option[Int] of the CODEPOINT index (#419 unified
        // the unit; the old byte-offset comment predated that change).
        _ => return None,
    };
    Some(f)
}

/// Search, split/join, and parsing — the arms that build or consume lists.
///
/// Extracted from `string_fn` (name-router split, arms verbatim and in source
/// order). `None` means "not my group", so the router's order is the only
/// ordering that matters.
fn string_fn_structural(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "index_of" => {
            let s = as_str(args.first())?;
            Flow::val(Value::Option(
                s.find(as_str(args.get(1))?)
                    .map(|b| Box::new(Value::Int(s[..b].chars().count() as i64))),
            ))
        }
        "last_index_of" => {
            let s = as_str(args.first())?;
            Flow::val(Value::Option(
                s.rfind(as_str(args.get(1))?)
                    .map(|b| Box::new(Value::Int(s[..b].chars().count() as i64))),
            ))
        }
        "split" => {
            let s = as_str(args.first())?;
            let sep = as_str(args.get(1))?;
            Flow::val(Value::list(
                s.split(sep).map(|p| Value::str(p.to_string())).collect(),
            ))
        }
        "lines" => Flow::val(Value::list(
            as_str(args.first())?
                .lines()
                .map(|l| Value::str(l.to_string()))
                .collect(),
        )),
        "chars" => Flow::val(Value::list(
            as_str(args.first())?
                .chars()
                .map(|c| Value::str(c.to_string()))
                .collect(),
        )),
        "join" => {
            // join(parts: List[String], sep: String)
            let parts = match args.first() {
                Some(Value::List(xs)) => xs,
                _ => return Some(abort_args("string", "join")),
            };
            let sep = as_str(args.get(1))?;
            let strs: Vec<String> = parts
                .iter()
                .map(|v| match v {
                    Value::Str(s) => (**s).clone(),
                    other => other.display_bare(),
                })
                .collect();
            Flow::val(Value::str(strs.join(sep)))
        }
        "capitalize" => {
            let s = as_str(args.first())?;
            let mut c = s.chars();
            let out = match c.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), c.as_str()),
                None => String::new(),
            };
            Flow::val(Value::str(out))
        }
        "to_int" => {
            let s = as_str(args.first())?;
            match s.trim().parse::<i64>() {
                Ok(n) => Flow::val(Value::Result(Ok(Box::new(Value::Int(n))))),
                Err(e) => Flow::val(Value::Result(Err(Box::new(Value::str(e.to_string()))))),
            }
        }
        _ => return None,
    };
    Some(f)
}
