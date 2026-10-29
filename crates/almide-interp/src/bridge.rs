//! The scalar floor under the self-hosted bodies, and the bridge's measured residue.
//!
//! The reference interpreter's third vote comes from the SELF-HOSTED stdlib
//! bodies — the same sources the wasm leg links and the native leg compiles
//! (`almide_lang::self_host_registry`, lowered once by `stdlib_pool`) — and
//! `dispatch_module.rs` consults that body FIRST at the pool boundary
//! (#2185). This file is what a body cannot be, in two tiers:
//!
//! - INSIDE the pool tier (`pool_depth > 0`), the bridge is part of the FLOOR
//!   a body consumes, exactly like `prim.*`: the json decoder's `string.len`,
//!   a codec path's `int.to_string`, `math.sin` under a self-hosted `math.*`.
//!   The tier evaluates one body over the block heap; nesting another body
//!   there is what it cannot yet do faithfully (the first body-first
//!   measurement voted WRONG on 9 fixtures — json paths, glob, header lookup,
//!   codec error paths — and crashed on 2), so a body's helper calls are
//!   answered over `Value`s here.
//! - AT the boundary, the bridge answers only where the body abstains or does
//!   not exist: `bytes.zeros` past the arena ceiling, the sized-numeric
//!   `to_string` observers.
//!
//! Every arm here is one the spec/wasm_cross corpus REACHES on one of those
//! two tiers; the ~90 others the file used to carry (`path.*`, `uint64.*`,
//! `bool.*`, the int/float/string/math arms no body consumes) were Rust
//! restatements of bodies the interp evaluates itself and are gone. The gate
//! that keeps it so is
//! `tests/wasm_runtime_interp_ledger.rs::interp_bridge_fallback_ledger`: every
//! non-`prim` name the bridge answers is recorded per run
//! (`RunOutcome::bridge_fallbacks`, tagged floor or fallback) and held equal
//! to `interp-bridge-fallback-ledger.txt` in both directions — a new
//! dependence on a Rust copy is a reviewed ledger edit, and an entry the
//! corpus stops reaching is a dead arm to delete.
//!
//! Each glue reproduces the native runtime fn's behavior directly over
//! `Value` (the runtime fns are one-liners over Rust std, so the floor stays
//! byte-identical to native). Deliberately NOT taking a path dependency on the
//! `almide_rt` crate: it pulls in rustls/webpki (network/TLS) and effectful
//! surface the interp does not need.
//!
//! Returns `None` when `(module, func)` is not served here (the caller then
//! abstains with the body's own reason).

use crate::value::{float_to_string, Value};
use crate::Flow;

/// Dispatch a scalar/string/math `(module, func)` with evaluated args.
pub(crate) fn dispatch(module: &str, func: &str, args: &[Value]) -> Option<Flow> {
    match module {
        "int" => int_fn(func, args),
        "float" => float_fn(func, args),
        "string" => string_fn(func, args),
        "math" => math_fn(func, args),
        "bytes" => bytes_fn(func, args),
        "int8" | "int16" | "int32" | "int64" | "uint8" | "uint16" | "uint32"
        | "float32" | "float64" => sized_numeric_fn(module, func, args),
        // The scalar prim floor — consumed by the self-hosted stdlib bodies the
        // `stdlib_pool` evaluates, not by fixture code directly.
        "prim" => prim_fn(func, args),
        // The env floor's writers: true `@intrinsic` stubs with no lowered body
        // (the readers go through `prim.env_get`).
        "env" => env_fn(func, args),
        _ => None,
    }
}

thread_local! {
    // The variables the running program set with `env.set` (`Some`) or
    // removed with `env.remove` (`None`, #3908). They are the program's
    // environment, layered over the process's: the interp runs in-process
    // beside the harness that spawns the native and wasm legs, and a write
    // to the real environment would reach those children (#3808).
    static ENV_OVERLAY: std::cell::RefCell<std::collections::HashMap<String, Option<String>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Forget every `env.set` of the previous program; a new interpreter starts
/// from the process environment.
pub(crate) fn reset_env_overlay() {
    ENV_OVERLAY.with(|o| o.borrow_mut().clear());
}

/// What the running program did to `name`: `Some(Some(v))` it set v,
/// `Some(None)` it removed the name, `None` it left the name alone.
pub(crate) fn env_overlay_get(name: &str) -> Option<Option<String>> {
    ENV_OVERLAY.with(|o| o.borrow().get(name).cloned())
}

/// The program's whole environment, as `env.vars`' leaf answers it: the
/// process's `name=value` entries with the overlay applied, by name.
fn env_overlay_environ() -> Vec<String> {
    let mut all: std::collections::BTreeMap<String, String> = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect();
    ENV_OVERLAY.with(|o| {
        for (k, v) in o.borrow().iter() {
            match v {
                Some(v) => all.insert(k.clone(), v.clone()),
                None => all.remove(k),
            };
        }
    });
    all.into_iter().map(|(k, v)| format!("{k}={v}")).collect()
}

/// Why `name` cannot be in an environment (#4200's rule, which `env.set`
/// and `env.remove` share), or `None`.
fn env_name_refusal(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some("empty name")
    } else if name.contains('=') {
        Some("name contains '='")
    } else if name.contains('\0') {
        Some("name contains NUL")
    } else {
        None
    }
}

/// `__set`'s write into the overlay: `env.remove`'s mark, the value NUL
/// (#3908), unsets the name.
fn env_overlay_put(name: &str, value: &str) {
    let v = (value != "\0").then(|| value.to_string());
    ENV_OVERLAY.with(|o| o.borrow_mut().insert(name.to_string(), v));
}

/// `env.set` writes the program's environment, as the other two legs do
/// (native setenv; wasm the embedded host's environ), so a fixture that sets
/// then reads a key (fan_ordered_arms.almd) observes the same bytes on every
/// leg. The interp keeps those writes in its overlay rather than the process
/// environment, which the native and wasm legs it is compared against inherit.
/// `env.sleep_ms` pauses for real: the pause is not an observation of its own,
/// and a fixture's ordering argument (a sleeping reader beside a writer) reads
/// the same here as on the targets.
fn env_fn(func: &str, args: &[Value]) -> Option<Flow> {
    match (func, args) {
        // `env.set` (C-421, #4200): the pairs no environment holds are an err
        // and store nothing — the rule restated here, independently of the
        // Almide body the backends run, so the third oracle checks it.
        ("set", [Value::Str(name), Value::Str(value)]) => {
            let why = env_name_refusal(name).or_else(|| value.contains('\0').then_some("value contains NUL"));
            Some(Flow::val(match why {
                Some(why) => Value::Result(Err(Box::new(Value::str(format!("env.set: {why}"))))),
                None => {
                    env_overlay_put(name, value);
                    Value::Result(Ok(Box::new(Value::Unit)))
                }
            }))
        }
        // `env.remove` (#3908): `set`'s name rule and err texts under its own
        // name; an unset name is ok.
        ("remove", [Value::Str(name)]) => Some(Flow::val(match env_name_refusal(name) {
            Some(why) => Value::Result(Err(Box::new(Value::str(format!("env.remove: {why}"))))),
            None => {
                ENV_OVERLAY.with(|o| o.borrow_mut().insert(name.to_string(), None));
                Value::Result(Ok(Box::new(Value::Unit)))
            }
        })),
        // `env.set`'s and `env.remove`'s private op, reached through their bodies.
        ("__set", [Value::Str(name), Value::Str(value)]) => {
            env_overlay_put(name, value);
            Some(Flow::val(Value::Unit))
        }
        // `env.vars` (#3908): the program's environment keyed by name, in
        // name order — restated here, as `set` is, since the interp lowers
        // no env body; `__environ` is the leaf that body reads.
        ("vars", []) => {
            let entries: Vec<(Value, Value)> = env_overlay_environ()
                .into_iter()
                .filter_map(|e| e.split_once('=').map(|(k, v)| (Value::str(k), Value::str(v))))
                .collect();
            Some(Flow::val(Value::Map(std::rc::Rc::new(entries))))
        }
        ("__environ", []) => {
            Some(Flow::val(Value::list(env_overlay_environ().into_iter().map(Value::str).collect())))
        }
        ("sleep_ms", [Value::Int(ms)]) => {
            std::thread::sleep(std::time::Duration::from_millis((*ms).max(0) as u64));
            Some(Flow::val(Value::Unit))
        }
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
            // `float32.to_string` is not bridged: its self-hosted body (the f32
            // Schubfach, C-372) evaluates in the pool tier.
            if module == "float32" {
                None
            } else if module.starts_with("float") {
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



/// `bytes.*` — the three the corpus still reaches: `to_list` (the identity —
/// a `Bytes` is a `List[Int]` here) and `len` as the floor inside bodies, and
/// `new`, whose body abstains past the interp arena ceiling where both
/// backends take the C-197 defined abort.
fn bytes_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "to_list" => Flow::val(args.first()?.clone()),
        "len" => Flow::val(Value::Int(args.first()?.as_iter_items()?.len() as i64)),
        // `almide_rt_bytes_to_string_lossy` — `String::from_utf8_lossy`. The
        // body's String block has no faithful read-back at the boundary
        // (`return sync`), so a lossy decode written as
        // `bytes.to_string_lossy(bytes.from_list(..))` (the replacement of
        // the removed `string.from_bytes`) is answered here.
        "to_string_lossy" => {
            let octets: Option<Vec<u8>> = args
                .first()?
                .as_iter_items()?
                .iter()
                .map(|v| match v {
                    Value::Int(n) => Some(*n as u8),
                    _ => None,
                })
                .collect();
            Flow::val(Value::str(String::from_utf8_lossy(&octets?).into_owned()))
        }
        // C-197: an unsatisfiable size is the defined abort on every leg —
        // the infallible `vec![_; n]` panicked with a capacity overflow on
        // `bytes.zeros(i64::MAX)` instead (fuzz seed 500705518626 index 711).
        "zeros" => {
            let n = as_int(args.first())?.max(0) as usize;
            let mut v: Vec<Value> = Vec::new();
            if v.try_reserve_exact(n).is_err() {
                Flow::Abort("out of memory".to_string())
            } else {
                v.resize(n, Value::Int(0));
                Flow::val(Value::list(v))
            }
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

// ── int ─────────────────────────────────────────────────────────

fn int_fn(func: &str, args: &[Value]) -> Option<Flow> {
    // The sized→Int widenings (`int.__from_uint16`, `int.__from_int32`, …): every
    // native impl is a widening/bit-preserving cast into i64 — the carrier the
    // interp already holds, with the value pre-wrapped to its declared width by
    // whatever produced it (C-180). Identity here, exactly like the reverse
    // `to_*` casts' truncation lives with the producers.
    if let Some(rest) = func.strip_prefix("__from_") {
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
/// `float.from_bits`). For f32 the language-level Float32 is CARRIED widened
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
        "__raw_to_float" => {
            let n = as_int(args.first())?;
            Flow::val(Value::Float(n as f64))
        }
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
            // ASCII whitespace only (str::trim_ascii, C-413).
            match s.trim_ascii().parse::<i64>() {
                Ok(n) => Flow::val(Value::Result(Ok(Box::new(Value::Int(n))))),
                Err(e) => Flow::val(Value::Result(Err(Box::new(Value::str(e.to_string()))))),
            }
        }
        // Bitwise.
        "band" => Flow::val(Value::Int(as_int(args.first())? & as_int(args.get(1))?)),
        "bor" => Flow::val(Value::Int(as_int(args.first())? | as_int(args.get(1))?)),
        "bxor" => Flow::val(Value::Int(as_int(args.first())? ^ as_int(args.get(1))?)),
        "bshl" => Flow::val(Value::Int(as_int(args.first())? << as_int(args.get(1))?)),
        "bshr" => Flow::val(Value::Int(as_int(args.first())? >> as_int(args.get(1))?)),
        _ => return None,
    };
    Some(f)
}

// ── float ───────────────────────────────────────────────────────

/// The sized-float CONVERSIONS (`float.__raw_to_float32`/`__to_float64`/
/// `__from_float32`/`__from_float64`): the interpreter carries every float in one
/// `f64`, and `Float32`/`Float64` are the SAME carrier at the language level
/// (the narrowing to f32 precision is the emitters' concern), so each of these
/// is the identity here. Bridged so a fixture that merely NAMES a sized float
/// still evaluates in the third oracle instead of abstaining.
fn float_sized_conv(func: &str, args: &[Value]) -> Option<Flow> {
    let n = as_float(args.first())?;
    match func {
        "__to_float64" | "__from_float64" => Some(Flow::val(Value::Float(n))),
        // f32 round-trips through the narrower precision, exactly as both
        // emitters do — the value a `Float32` can actually hold.
        "__raw_to_float32" | "__from_float32" => Some(Flow::val(Value::Float(n as f32 as f64))),
        _ => None,
    }
}

fn float_fn(func: &str, args: &[Value]) -> Option<Flow> {
    float_sized_conv(func, args)
        .or_else(|| float_unary_fn(func, args))
        .or_else(|| float_text_fn(func, args))
}

/// The one-operand surface: width conversion and the classification
/// predicates.
fn float_unary_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "__from_int" => Value::Float(as_int(args.first())? as f64),
        "is_nan" => Value::Bool(as_float(args.first())?.is_nan()),
        "is_infinite" => Value::Bool(as_float(args.first())?.is_infinite()),
        _ => return None,
    };
    Some(Flow::val(f))
}




/// The text surface: rendering to a string and parsing back.
fn float_text_fn(func: &str, args: &[Value]) -> Option<Flow> {
    let f = match func {
        "to_string" => Flow::val(Value::str(float_to_string(as_float(args.first())?))),
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
        "ln" | "log" => vl::almide_rt_libm_log(x),
        "log10" => vl::almide_rt_libm_log10(x),
        "__fpow" | "powf" | "__pow" => {
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
    // `sqrt` / `abs` / `pi` / `e` and the rest of `math.*` come from the
    // self-hosted bodies now (#2185); only the five transcendentals a body
    // consumes as its floor stay here.
    //
    // asin / acos / atan2 / sinh / cosh / hypot / cbrt / expm1 / log1p and the
    // inverse hyperbolics are pure-Almide bodies in stdlib/math.almd (musl
    // transcriptions over the floor above), so the interp evaluates the body
    // like either backend does; none of them reaches this bridge.
    //
    // The float `**` OPERATOR is the same floor and routes to the same
    // `pow` in the binop path (`eval_match.rs`, `BinOp::PowFloat`) — #924's
    // rule stands: a transcendental reachable through an OPERATOR must agree
    // in both places.
    if let Some(v) = math_vendored_libm(func, args) {
        return Some(Flow::val(Value::Float(v)));
    }
    None
}

// ── string ──────────────────────────────────────────────────────

/// `string.to_upper` under C-412: an ASCII scalar maps by the ASCII rules; a
/// non-ASCII scalar takes its full Unicode uppercase unless that mapping holds
/// an ASCII scalar (U+0131 ı → I, U+017F ſ → S, U+FB01 ﬁ → FI, …), and then it
/// stays unchanged — the native runtime's `almide_rt_string_to_upper`.
fn to_upper_no_ascii_fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let m = c.to_uppercase();
        if !c.is_ascii() && m.clone().any(|x| x.is_ascii()) {
            out.push(c)
        } else {
            out.extend(m)
        }
    }
    out
}

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
        "to_upper" => Flow::val(Value::str(to_upper_no_ascii_fold(as_str(args.first())?))),
        "trim" => Flow::val(Value::str(as_str(args.first())?.trim().to_string())),
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
                .take(as_int(args.get(1))?.max(0) as usize)
                .collect::<String>(),
        )),
        // `drop`/`take_end`/`drop_end` are `take`'s siblings — a negative count is
        // 0 (#4147) — and `slice` clamps its start/end into [0, len], with a
        // reversed range empty.
        "drop" => Flow::val(Value::str(
            as_str(args.first())?
                .chars()
                .skip(as_int(args.get(1))?.max(0) as usize)
                .collect::<String>(),
        )),
        "take_end" | "drop_end" => {
            let cs: Vec<char> = as_str(args.first())?.chars().collect();
            let raw = as_int(args.get(1))?;
            // A negative count is 0 (#4147).
            let k = if raw < 0 { 0 } else { (raw as usize).min(cs.len()) };
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
        "to_list" => Flow::val(Value::list(
            as_str(args.first())?
                .chars()
                .map(|c| Value::str(c.to_string()))
                .collect(),
        )),
        _ => return None,
    };
    Some(f)
}

#[cfg(test)]
mod env_overlay_tests {
    use super::*;

    /// `env.set` is the program's own environment, not the host process's:
    /// the oracle runs the interp in-process and then spawns the native and
    /// wasm legs, which inherit the process environment (#3808).
    #[test]
    fn env_set_stays_inside_the_interpreter() {
        let key = "INTERP_ENV_OVERLAY_PROBE";
        reset_env_overlay();
        env_fn("__set", &[Value::str(key.to_string()), Value::str("written".to_string())]);
        assert_eq!(std::env::var(key).ok(), None, "env.set leaked into the host process");
        assert_eq!(env_overlay_get(key), Some(Some("written".to_string())));
        reset_env_overlay();
        assert_eq!(env_overlay_get(key), None);
    }

    /// `env.remove` unsets a name inside the interpreter only: the overlay
    /// records the removal, `env.vars`' leaf drops the name, and the host
    /// process keeps the variable it had (#3908).
    #[test]
    fn env_remove_stays_inside_the_interpreter() {
        let key = "PATH";
        let before = std::env::var(key).ok();
        reset_env_overlay();
        env_fn("remove", &[Value::str(key.to_string())]);
        assert_eq!(env_overlay_get(key), Some(None));
        assert!(!env_overlay_environ().iter().any(|e| e.starts_with("PATH=")), "a removed name is still listed");
        assert_eq!(std::env::var(key).ok(), before, "env.remove leaked into the host process");
        env_fn("__set", &[Value::str(key.to_string()), Value::str("\0".to_string())]);
        assert_eq!(env_overlay_get(key), Some(None), "the NUL mark is a removal");
        reset_env_overlay();
    }
}
