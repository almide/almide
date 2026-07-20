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
        _ => None,
    }
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

fn float_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        // The `.0`-suffixed form — the ONLY display path that uses it.
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
    let f = match func {
        // CODEPOINT unit (#419, C-065): string.len/index_of count chars, not
        // bytes, on BOTH targets — the interp lagging this was the third
        // judge's FIRST catch (fuzz 3-way: both targets agreed, interp voted
        // bytes).
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
        "repeat" => Flow::val(Value::str(
            // Negative counts clamp to 0 (C-054; mirrors runtime/rs string.rs).
            as_str(args.first())?.repeat(as_int(args.get(1))?.max(0) as usize),
        )),
        // Codepoint-count take, the C-054 unsigned discipline (mirrors
        // runtime/rs almide_rt_string_take: `chars().take(n as usize)` — a
        // negative n is enormous as usize, so take(-1) keeps the whole string).
        "take" => Flow::val(Value::str(
            as_str(args.first())?
                .chars()
                .take(as_int(args.get(1))? as usize)
                .collect::<String>(),
        )),
        "count" => Flow::val(Value::Int(
            as_str(args.first())?.matches(as_str(args.get(1))?).count() as i64,
        )),
        // index_of returns Option[Int] of the CODEPOINT index (#419 unified
        // the unit; the old byte-offset comment predated that change).
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
