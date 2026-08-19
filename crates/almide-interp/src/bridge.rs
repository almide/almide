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
/// `List[Int]` in the interp, so `to_list` is the identity and `to_string_lossy`
/// decodes. The mutating and raw-pointer surface stays out of scope.
fn bytes_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        // `to_list` is the identity — every stored element is already a
        // truncated octet. `from_list` is NOT: the native runtime maps each
        // element `x as u8` (almide_rt_bytes_from_list), so an out-of-range
        // element must truncate HERE too — the identity kept `-128` in the
        // model, and every read voted `-128` where both targets print `128`
        // (#1505, the 0x80-as-signed wrong vote).
        "from_list" => {
            let items = args.first()?.as_iter_items()?;
            let out: Vec<Value> = items
                .iter()
                .map(|v| match v {
                    Value::Int(n) => Value::Int((*n as u8) as i64),
                    other => other.clone(),
                })
                .collect();
            Flow::val(Value::List(std::rc::Rc::new(out)))
        }
        "to_list" => Flow::val(args.first()?.clone()),
        "len" => Flow::val(Value::Int(args.first()?.as_iter_items()?.len() as i64)),
        // C-197: an unsatisfiable size is the defined abort on every leg —
        // the infallible `vec![_; n]` panicked with a capacity overflow on
        // `bytes.new(i64::MAX)` instead (fuzz seed 500705518626 index 711).
        "new" => {
            let n = as_int(args.first())?.max(0) as usize;
            let mut v: Vec<Value> = Vec::new();
            if v.try_reserve_exact(n).is_err() {
                Flow::Abort("out of memory".to_string())
            } else {
                v.resize(n, Value::Int(0));
                Flow::val(Value::list(v))
            }
        }
        "from_string" => Flow::val(Value::list(
            as_str(args.first())?.bytes().map(|b| Value::Int(b as i64)).collect(),
        )),
        "to_string_lossy" => {
            let raw = as_byte_buf(args.first())?;
            Flow::val(Value::str(String::from_utf8_lossy(&raw).into_owned()))
        }
        // The WASM-only arena pair. Native is literally `almide_rt_bytes_heap_save()
        // -> 0` and `almide_rt_bytes_heap_restore(_) {}` (runtime/rs/src/bytes.rs) —
        // there is no arena to check-point outside wasm, and the interp models
        // `Bytes` as an owned `List` with no arena either. Mirroring the native
        // no-ops is the faithful vote; anything else would invent a behaviour
        // neither backend has on this leg (#1021).
        "heap_save" => Flow::val(Value::Int(0)),
        "heap_restore" => Flow::val(Value::Unit),
        "read_uint16" | "read_uint32" | "read_int32" | "read_float32" => {
            return bytes_read_fn(func, args)
        }
        _ => return None,
    };
    Some(f)
}

/// A `Bytes` receiver as raw octets. The interp models `Bytes` as `List[Int]`,
/// so every element is truncated with `as u8` exactly as the native runtime does.
fn as_byte_buf(v: Option<&Value>) -> Option<Vec<u8>> {
    Some(
        v?.as_iter_items()?
            .iter()
            .map(|v| match v { Value::Int(i) => *i as u8, _ => 0 })
            .collect(),
    )
}

/// The Endian-parameterized sized reads (#1098): dispatch on the ctor of the
/// `Endian` argument and mirror the native guards exactly — an out-of-range
/// window reads the documented 0 / 0.0 default, never aborts
/// (`almide_rt_bytes_read_u16_le`'s `checked_add` shape).
fn bytes_read_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let raw = as_byte_buf(args.first())?;
    let pos = as_int(args.get(1))?;
    let big = endian_is_big(args.get(2)?)?;
    let width = if func == "read_uint16" { 2 } else { 4 };
    let p = pos as usize;
    let window = (pos >= 0)
        .then(|| p.checked_add(width))
        .flatten()
        .filter(|end| *end <= raw.len())
        .map(|_| &raw[p..p + width]);
    let Some(w) = window else {
        // Out of range: the documented default for the read's result type.
        let zero = if func == "read_float32" { Value::Float(0.0) } else { Value::Int(0) };
        return Some(Flow::val(zero));
    };
    Some(Flow::val(decode_sized(func, w, big)?))
}

/// The `Endian` variant argument as "is big-endian".
fn endian_is_big(v: &Value) -> Option<bool> {
    let Value::Variant { ctor, .. } = v else { return None };
    match ctor.as_str() {
        "LittleEndian" => Some(false),
        "BigEndian" => Some(true),
        _ => None,
    }
}

/// Decode an in-range window for one of the sized reads. `w` is exactly the
/// read's width, so the array conversions cannot fail.
fn decode_sized(func: &str, w: &[u8], big: bool) -> Option<Value> {
    let v = match func {
        "read_uint16" => {
            let b = [w[0], w[1]];
            Value::Int((if big { u16::from_be_bytes(b) } else { u16::from_le_bytes(b) }) as i64)
        }
        "read_uint32" => {
            let b = [w[0], w[1], w[2], w[3]];
            Value::Int((if big { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) }) as i64)
        }
        "read_int32" => {
            let b = [w[0], w[1], w[2], w[3]];
            Value::Int((if big { i32::from_be_bytes(b) } else { i32::from_le_bytes(b) }) as i64)
        }
        "read_float32" => {
            let b = [w[0], w[1], w[2], w[3]];
            Value::Float((if big { f32::from_be_bytes(b) } else { f32::from_le_bytes(b) }) as f64)
        }
        _ => return None,
    };
    Some(v)
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
    // The sized→Int widenings (`int.from_uint16`, `int.from_int32`, …): every
    // native impl is a widening/bit-preserving cast into i64 — the carrier the
    // interp already holds, with the value pre-wrapped to its declared width by
    // whatever produced it (C-180). Identity here, exactly like the reverse
    // `to_*` casts' truncation lives with the producers.
    if let Some(rest) = func.strip_prefix("from_") {
        if matches!(
            rest,
            "uint8" | "uint16" | "uint32" | "uint64" | "int8" | "int16" | "int32" | "int64"
        ) {
            return Some(Flow::val(Value::Int(as_int(args.first())?)));
        }
    }
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
    // Four disjoint name families. A miss in one falls through to the next and
    // ends as the same `None` an unmatched name would produce, so the chain is
    // equivalent to one flat table with the arms in this order.
    prim_bitwise_fn(func, args)
        .or_else(|| prim_repr_fn(func, args))
        .or_else(|| prim_float_arith_fn(func, args))
        .or_else(|| prim_float_cmp_fn(func, args))
}

/// Bitwise / shifts: i64-uniform; wasm's `i64.shl` family masks the count
/// to 6 bits, so the floor does too (a Rust bare `<<` would panic instead).
fn prim_bitwise_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let f: fn(i64, i64) -> i64 = match func {
        "band" => |a, b| a & b,
        "bor" => |a, b| a | b,
        "bxor" => |a, b| a ^ b,
        "bshl" => |a, b| a.wrapping_shl(b as u32 & 63),
        "bshr" => |a, b| a.wrapping_shr(b as u32 & 63),
        "bshr_u" => |a, b| ((a as u64).wrapping_shr(b as u32 & 63)) as i64,
        _ => return None,
    };
    Some(Flow::val(Value::Int(f(as_int(args.first())?, as_int(args.get(1))?))))
}

/// Numeric REPRESENTATION changes — width and bit-pattern conversions that
/// carry no arithmetic of their own.
///
/// Int ⇄ Float is `f64.convert_i64_s` / saturating `i64.trunc_sat_f64_s`
/// (Rust's float→int `as` is exactly the saturating truncate); `fbits` /
/// `ffrombits` are the raw f64 ⇄ i64 reinterpret (`float.to_bits` /
/// `int.bits_to_float`). For f32 the language-level Float32 is CARRIED widened
/// (one f64 in this oracle, the widened bits in the backends), the narrowing
/// rounds to nearest, and the pattern lives in the low 32 bits.
fn prim_repr_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "i2f" => Value::Float(as_int(args.first())? as f64),
        "f2i" => Value::Int(as_float(args.first())? as i64),
        "fbits" => Value::Int(as_float(args.first())?.to_bits() as i64),
        "ffrombits" => Value::Float(f64::from_bits(as_int(args.first())? as u64)),
        "f2f32" => Value::Float((as_float(args.first())? as f32) as f64),
        "f32_2f" => Value::Float(as_float(args.first())?),
        "i2f32" => Value::Float((as_int(args.first())? as f32) as f64),
        "f32bits" => Value::Int(((as_float(args.first())? as f32).to_bits()) as i64),
        "bits_to_f32" => Value::Float(f32::from_bits(as_int(args.first())? as u32) as f64),
        _ => return None,
    };
    Some(Flow::val(f))
}

/// f64 arithmetic: real f64s in the interp carrier.
fn prim_float_arith_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let binary: fn(f64, f64) -> f64 = match func {
        "fadd" => |a, b| a + b,
        "fsub" => |a, b| a - b,
        "fmul" => |a, b| a * b,
        "fdiv" => |a, b| a / b,
        "fcopysign" => f64::copysign,
        // The v1 wasm `f64.min` / `f64.max` semantics EXACTLY — these prims
        // render as FBinOp::Min/Max and the native leg never links them (it
        // reaches the same observables through its own runtime): NaN
        // PROPAGATES (unlike Rust's f64::min/max, which return the other
        // operand), and a ±0 tie answers -0 for min, +0 for max.
        "fmin" => |a, b| {
            if a.is_nan() || b.is_nan() {
                f64::NAN
            } else if a < b {
                a
            } else if b < a {
                b
            } else if a.is_sign_negative() {
                a
            } else {
                b
            }
        },
        "fmax" => |a, b| {
            if a.is_nan() || b.is_nan() {
                f64::NAN
            } else if a > b {
                a
            } else if b > a {
                b
            } else if a.is_sign_negative() {
                b
            } else {
                a
            }
        },
        _ => return prim_float_unary_fn(func, args),
    };
    Some(Flow::val(Value::Float(binary(
        as_float(args.first())?,
        as_float(args.get(1))?,
    ))))
}

/// The one-operand half of [`prim_float_arith_fn`].
fn prim_float_unary_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let unary: fn(f64) -> f64 = match func {
        "fneg" => |a| -a,
        "fabs" => f64::abs,
        "fsqrt" => f64::sqrt,
        "fceil" => f64::ceil,
        "ffloor" => f64::floor,
        "fnearest" => f64::round_ties_even,
        _ => return None,
    };
    Some(Flow::val(Value::Float(unary(as_float(args.first())?))))
}

/// f64 comparison — the IEEE predicates, NaN-unordered like both backends.
fn prim_float_cmp_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let cmp: fn(&f64, &f64) -> bool = match func {
        "feq" => |a, b| a == b,
        "fne" => |a, b| a != b,
        "flt" => |a, b| a < b,
        "fgt" => |a, b| a > b,
        "fge" => |a, b| a >= b,
        "fle" => |a, b| a <= b,
        _ => return None,
    };
    Some(Flow::val(Value::Bool(cmp(
        &as_float(args.first())?,
        &as_float(args.get(1))?,
    ))))
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
    float_sized_conv(func, args)
        .or_else(|| float_unary_fn(func, args))
        .or_else(|| float_order_fn(func, args))
        .or_else(|| float_text_fn(func, args))
}

/// The one-operand surface: rounding, sign, width conversion and the
/// classification predicates.
fn float_unary_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "to_int" => Value::Int(as_float(args.first())? as i64),
        "from_int" => Value::Float(as_int(args.first())? as f64),
        "abs" => Value::Float(as_float(args.first())?.abs()),
        "ceil" => Value::Float(as_float(args.first())?.ceil()),
        "floor" => Value::Float(as_float(args.first())?.floor()),
        "round" => Value::Float(as_float(args.first())?.round()),
        "sqrt" => Value::Float(as_float(args.first())?.sqrt()),
        "sign" => Value::Float(as_float(args.first())?.signum()),
        "is_nan" => Value::Bool(as_float(args.first())?.is_nan()),
        "is_infinite" => Value::Bool(as_float(args.first())?.is_infinite()),
        _ => return None,
    };
    Some(Flow::val(f))
}

/// `min` / `max` / `clamp` — the explicit NaN/tie tree mirroring
/// runtime/rs/src/float.rs `almide_rt_float_min`/`_max`, NOT `f64::min`/`max`
/// (llvm.minnum/maxnum has unspecified ±0-tie order). Ties return the FIRST
/// operand (C-049).
fn float_order_fn(func: &str, args: &[Value]) -> Option<Flow> {
    match func {
        "min" => {
            let (a, b) = (as_float(args.first())?, as_float(args.get(1))?);
            Some(Flow::val(Value::Float(pick_ordered(a, b, a > b))))
        }
        "max" => {
            let (a, b) = (as_float(args.first())?, as_float(args.get(1))?);
            Some(Flow::val(Value::Float(pick_ordered(a, b, a < b))))
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
            Some(Flow::val(Value::Float(n.clamp(lo, hi))))
        }
        _ => None,
    }
}

/// The min/max body shared by [`float_order_fn`]: a NaN operand loses, and
/// `take_b` decides the non-NaN comparison so a tie keeps `a` (the first
/// operand) either way.
fn pick_ordered(a: f64, b: f64, take_b: bool) -> f64 {
    if a.is_nan() {
        b
    } else if b.is_nan() || !take_b {
        a
    } else {
        b
    }
}

/// The text surface: rendering to a string and parsing back.
fn float_text_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "to_string" => Flow::val(Value::str(float_to_string(as_float(args.first())?))),
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

/// The vendored-musl-libm transcendentals, by stdlib name. `None` = not a
/// vendored transcendental (the caller then decides: honest abstain, or one of
/// the platform-exact ops like sqrt/abs).
fn math_vendored_libm(func: &str, args: &[Value]) -> Option<f64> {
    use crate::vendored_libm as vl;
    let x = as_float(args.first())?;
    Some(match func {
        "sin" => vl::almide_rt_libm_sin(x),
        "cos" => vl::almide_rt_libm_cos(x),
        "tan" => vl::almide_rt_libm_tan(x),
        "atan" => vl::almide_rt_libm_atan(x),
        "tanh" => vl::almide_rt_libm_tanh(x),
        "exp" => vl::almide_rt_libm_exp(x),
        "expm1" => vl::almide_rt_libm_expm1(x),
        "ln" | "log" => vl::almide_rt_libm_log(x),
        "log2" => vl::almide_rt_libm_log2(x),
        "log10" => vl::almide_rt_libm_log10(x),
        "fpow" | "powf" | "pow" => {
            let y = as_float(args.get(1))?;
            vl::almide_rt_libm_pow(x, y)
        }
        _ => return None,
    })
}

fn math_fn(func: &str, args: &[Value]) -> Option<Flow> {
    // The transcendental floor is the VENDORED musl-libm both backends run
    // (`crate::vendored_libm`, included from runtime/rs/src/libm.rs — see that
    // module's header for why include! beats a copy or a crate dep). Before
    // this the interp abstained here, because Rust `std`'s `f64::sin` calls the
    // PLATFORM libm and would diverge from the native==wasm consensus in the
    // last ULP (`0.799441007199113` vs `0.7994410071991129`), casting a WRONG
    // third vote. Computing the consensus algorithm restores the third judge.
    //
    // `sqrt` stays on `f64::sqrt` (IEEE-754 correctly rounded — identical on
    // every platform and equal to the wasm `f64.sqrt` opcode), `abs` is exact,
    // `pi`/`e` are constants.
    //
    // NOT bridged, and still honestly Unsupported: names the vendored file does
    // not provide (`asin`/`acos`/`atan2`/`sinh`/`cosh`/`exp2`/`log1p`/`cbrt`/
    // `hypot`). The runtime's own asin/acos/atan2 delegate to the PLATFORM
    // libm, so they have no stable oracle either — they are unreachable from
    // Almide today (no `@intrinsic` in stdlib/math.almd) and must not be
    // bridged here on a guess.
    //
    // The float `**` OPERATOR is the same floor and routes to the same
    // `pow` in the binop path (`eval_match.rs`, `BinOp::PowFloat`) — #924's
    // rule stands: a transcendental reachable through an OPERATOR must agree
    // in both places.
    if let Some(v) = math_vendored_libm(func, args) {
        return Some(Flow::val(Value::Float(v)));
    }
    if matches!(
        func,
        "asin" | "acos" | "atan2" | "sinh" | "cosh"
            | "exp2" | "log1p" | "cbrt" | "hypot"
    ) {
        return Some(Flow::Unsupported(format!(
            "transcendental `math.{func}` (no vendored musl-libm implementation;              the runtime's own delegates to the platform libm — no oracle match)"
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
        // Option-returning prefix strip (`args.option`'s `--name=` parse walked
        // into the pool body's prim.handle without this — #1217's recon).
        "strip_prefix" => {
            let s = as_str(args.first())?;
            let p = as_str(args.get(1))?;
            Flow::val(Value::Option(
                s.strip_prefix(p).map(|r| Box::new(Value::str(r.to_string()))),
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

#[cfg(test)]
mod bytes_model_tests {
    use super::*;

    /// #1505: `bytes.from_list` must truncate each element to an octet the
    /// way the native runtime's `x as u8` does — the identity model kept
    /// `-128`/`300` in the list and every later read voted a value both
    /// targets never print (0x80 came back `-128` where they print `128`).
    #[test]
    fn from_list_truncates_to_octets_like_the_native_cast() {
        let src = Value::List(std::rc::Rc::new(vec![
            Value::Int(1),
            Value::Int(128),
            Value::Int(255),
            Value::Int(-128),
            Value::Int(300),
        ]));
        let Some(Flow::Value(Value::List(out))) = bytes_fn("from_list", &[src]) else {
            panic!("from_list did not evaluate");
        };
        let got: Vec<i64> =
            out.iter().map(|v| match v { Value::Int(n) => *n, _ => panic!("non-int") }).collect();
        assert_eq!(got, vec![1, 128, 255, 128, 44]);
    }
}
