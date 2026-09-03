// ── bridge.rs, part 2: the float / math / bool / string fns ──
//
// include!-spliced into `bridge.rs` at module level (the 800-line file
// discipline, #1856; `as_int` / `as_float` / `as_str` / `abort_args` and the
// vendored-libm entry stay in bridge.rs and are shared). bridge.rs keeps the
// `dispatch` table and the int / sized-int / bytes / path / prim families.

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
/// (llvm.minnum/maxnum has unspecified ±0-tie order). Ties follow the
/// IEEE-754-2019 zero ordering (C-049, ALS-T23).
fn float_order_fn(func: &str, args: &[Value]) -> Option<Flow> {
    match func {
        "min" => {
            let (a, b) = (as_float(args.first())?, as_float(args.get(1))?);
            Some(Flow::val(Value::Float(pick_min(a, b))))
        }
        "max" => {
            let (a, b) = (as_float(args.first())?, as_float(args.get(1))?);
            Some(Flow::val(Value::Float(pick_max(a, b))))
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

/// The min/max bodies shared by [`float_order_fn`]: a NaN operand loses,
/// and a TIE (the ±0 pair included) follows IEEE-754-2019 zero ordering
/// (C-049, ALS-T23): min = -0.0, max = +0.0, commutative — the same
/// decision tree the native runtime and both wasm legs implement.
fn pick_min(a: f64, b: f64) -> f64 {
    if a.is_nan() { b }
    else if b.is_nan() { a }
    else if a < b { a }
    else if b < a { b }
    else if a.is_sign_negative() { a } else { b }
}

fn pick_max(a: f64, b: f64) -> f64 {
    if a.is_nan() { b }
    else if b.is_nan() { a }
    else if a > b { a }
    else if b > a { b }
    else if a.is_sign_positive() { a } else { b }
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
