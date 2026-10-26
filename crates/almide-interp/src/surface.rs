//! The public stdlib surface whose Almide body lives in a BUNDLED module.
//!
//! `stdlib_pool` lowers the self-host REGISTRY sources only, so a public fn
//! the stdlib writes in its bundled module (`stdlib/int.almd`'s `int.pow`,
//! `stdlib/float.almd`'s `float.from`) has no lowered body here, and every
//! fixture that calls one abstained with `<m>.<f>: no lowered body`. The
//! removal of the off-target spellings (`int.to_float`, `float.to_int`, the
//! sized `to_*` / `from_*` cells, `math.pow`, `int.bits_to_float`, …) moved
//! their callers onto exactly this surface.
//!
//! Each arm below is the bundled body, step for step:
//!
//! - a THIN WRAPPER over a registry body (`int.pow` = `math.__pow`) forwards
//!   to that body through the ordinary module dispatch, so the vote is still
//!   the linked self-hosted definition;
//! - a body with its own logic (`int.parse_radix`, the `bytes.get_*` window
//!   check, `list.binary_search`'s branchless loop) is restated over `Value`s
//!   with the source line it mirrors;
//! - the `<T>.from` family is generic over its `Numeric` source and reads the
//!   source through the protocol pivots (`__int_exact`, `__trunc`, `__int_sat`,
//!   `__to_float`, …) — a call the pool cannot lower without monomorphizing.
//!   The pivot is picked here by the argument's STATIC type, the same choice
//!   mono makes for the backends, and each pivot is its declaring module's
//!   one-liner (`stdlib/<type>.almd`, "Numeric / Integer requirements").
//!
//! Nothing here touches process state: the only host read is the wall clock
//! (`datetime.now_ms`), the same clock the prim floor serves to
//! `prim.clock_time_get`.

use std::rc::Rc;

use almide_base::intern::{sym, Sym};
use almide_ir::{BinOp, IrExpr};
use almide_lang::types::Ty;

use crate::value::Value;
use crate::{Flow, Interpreter};

/// 2^63 and 2^64 as f64 — the bounds the stdlib pivots compare against
/// (`9223372036854775808.0`, `18446744073709551616.0`).
const TWO63: f64 = 9223372036854775808.0;
const TWO64: f64 = 18446744073709551616.0;

impl<'a> Interpreter<'a> {
    /// `<T>.from` / `from_checked` / `from_saturating` / `from_wrapping` with
    /// a numeric target `T`: the generic body, its pivots chosen by the
    /// argument's static type. `None` = not one of these calls (or a source
    /// shape this does not read) — the caller dispatches as before.
    pub(crate) fn eval_numeric_carrier(
        &mut self,
        module: Sym,
        func: Sym,
        args: &[IrExpr],
        evaled: &[Value],
    ) -> Option<Flow> {
        if evaled.len() != 1 || self.module_fns.contains_key(&(module, func)) {
            return None;
        }
        let target = target_of(module.as_str())?;
        let op = carrier_op(func.as_str())?;
        let src = Num::read(&args.first()?.ty, evaled.first()?)?;
        let flow = carrier(module.as_str(), target, op, src)?;
        if matches!(flow, Flow::Abort(_)) {
            self.panicked.set(true);
        }
        Some(flow)
    }

    /// A public bundled-module fn with no lowered body, answered from its
    /// bundled definition. `None` = not one of them; the caller keeps the
    /// `no lowered body` abstain.
    pub(crate) fn eval_bundled_surface(&mut self, module: Sym, func: Sym, args: &[Value]) -> Option<Flow> {
        let f = func.as_str();
        match module.as_str() {
            "int" => self.surface_int(f, args),
            "float" => self.surface_float(f, args),
            "float64" => self.surface_float(f, args),
            "float32" => self.surface_float32(f, args),
            "string" => surface_string(f, args),
            "datetime" => surface_datetime(f, args),
            "list" => self.surface_list(f, args),
            "deque" => surface_deque(f, args),
            "io" => surface_io(f, args),
            "bytes" => surface_bytes(f, args),
            _ => None,
        }
    }

    /// A thin wrapper's one call, through the same module dispatch a
    /// fixture-spelled call of the internal name takes.
    fn forward(&mut self, module: &str, func: &str, args: &[Value]) -> Flow {
        self.dispatch_module_resolved(sym(module), sym(func), args.to_vec())
    }

    fn surface_int(&mut self, f: &str, args: &[Value]) -> Option<Flow> {
        match f {
            // `fn pow(base: Int, exp: Int) -> Int = math.__pow(base, exp)`
            "pow" if args.len() == 2 => Some(self.forward("math", "__pow", args)),
            "wrap" => {
                let (Some(Value::Int(x)), Some(Value::Int(bits))) = (args.first(), args.get(1)) else {
                    return None;
                };
                Some(self.int_wrap(*x, *bits))
            }
            "parse_radix" => {
                let (Some(Value::Str(s)), Some(Value::Int(radix))) = (args.first(), args.get(1)) else {
                    return None;
                };
                Some(self.int_parse_radix(s, *radix))
            }
            _ => None,
        }
    }

    /// `int.wrap` (stdlib/int.almd): `bits <= 0` aborts, `bits >= 64` keeps
    /// x, else `int.band(x, int.bnot(int.bshl(-1, bits)))`.
    fn int_wrap(&mut self, x: i64, bits: i64) -> Flow {
        if bits <= 0 {
            return self.panic_flow("wrap width must be positive".to_string());
        }
        if bits >= 64 {
            return Flow::val(Value::Int(x));
        }
        Flow::val(Value::Int(x & !((-1i64) << bits)))
    }

    /// `int.parse_radix` (stdlib/int.almd) with its local helpers
    /// `check_radix`, `strip_radix_prefix`, `accumulate_negative` and
    /// `accumulate_digits`: the digits are accumulated as a non-positive
    /// value so `min_value` is reachable.
    fn int_parse_radix(&mut self, s: &str, radix: i64) -> Flow {
        if !(2..=36).contains(&radix) {
            return self.panic_flow(format!("radix must be in 2..36, got {radix}"));
        }
        let bytes = s.trim().as_bytes();
        let err = |m: &str| Flow::val(Value::Result(Err(Box::new(Value::str(m)))));
        let (digits, negative) = match bytes.first() {
            None => return err("cannot parse integer from empty string"),
            Some(&b'-') => (&bytes[1..], true),
            Some(&b'+') => (&bytes[1..], false),
            Some(_) => (bytes, false),
        };
        let digits = match digits {
            [b'0', p, _, ..] if prefix_radix(*p) == radix => &digits[2..],
            _ => digits,
        };
        if digits.is_empty() {
            return err("invalid digit found in string");
        }
        let mut acc: i64 = 0;
        for b in digits {
            let d = digit_value(*b);
            if d >= radix {
                return err("invalid digit found in string");
            }
            if acc < (i64::MIN + d) / radix {
                return err(if negative {
                    "number too small to fit in target type"
                } else {
                    "number too large to fit in target type"
                });
            }
            acc = acc.wrapping_mul(radix).wrapping_sub(d);
        }
        if negative {
            Flow::val(Value::Result(Ok(Box::new(Value::Int(acc)))))
        } else if acc == i64::MIN {
            err("number too large to fit in target type")
        } else {
            Flow::val(Value::Result(Ok(Box::new(Value::Int(-acc)))))
        }
    }

    /// `float.*` and `float64.*` (the same f64 carrier).
    fn surface_float(&mut self, f: &str, args: &[Value]) -> Option<Flow> {
        match f {
            // `fn pow(base: Float, exp: Float) -> Float = math.__fpow(base, exp)`
            "pow" if args.len() == 2 => Some(self.forward("math", "__fpow", args)),
            // `fn from_bits(bits: Int) -> Float = int.__bits_to_float(bits)`
            // (float64's adds an identity `float.__to_float64`).
            "from_bits" if args.len() == 1 => Some(self.forward("int", "__bits_to_float", args)),
            _ => None,
        }
    }

    /// `float32.from_bits(bits) = float.__raw_to_float32(int.__bits_to_f32(bits))`:
    /// the widened f32 the second step narrows back without rounding.
    fn surface_float32(&mut self, f: &str, args: &[Value]) -> Option<Flow> {
        match f {
            "from_bits" if args.len() == 1 => Some(match self.forward("int", "__bits_to_f32", args) {
                Flow::Value(Value::Float(x)) => Flow::val(Value::Float((x as f32) as f64)),
                other => other,
            }),
            _ => None,
        }
    }

    /// `list.binary_search(xs, target) = list.binary_search_by(xs, target, (x) => x)`:
    /// the branchless loop of `binary_search_by`, ordered by `__key_order`
    /// (equal is 0, else `list.sort`'s order).
    fn surface_list(&mut self, f: &str, args: &[Value]) -> Option<Flow> {
        match f {
            "binary_search" if args.len() == 2 => {
                let items = args.first()?.as_iter_items()?;
                let target = args.get(1)?;
                let order = crate::value::TotalOrder::new(&self.variant_tags);
                let key_order = |e: &Value| -> i64 {
                    if e == target {
                        0
                    } else {
                        match order.cmp(e, target) {
                            Some(std::cmp::Ordering::Less) => -1,
                            _ => 1,
                        }
                    }
                };
                let (mut base, mut size) = (0usize, items.len());
                while size > 1 {
                    let half = size / 2;
                    let mid = base + half;
                    if key_order(&items[mid]) <= 0 {
                        base = mid;
                    }
                    size -= half;
                }
                let found = !items.is_empty() && key_order(&items[base]) == 0;
                let r = found.then(|| Box::new(Value::Int(base as i64)));
                Some(Flow::val(Value::Option(r)))
            }
            _ => None,
        }
    }

    fn panic_flow(&mut self, msg: String) -> Flow {
        self.panicked.set(true);
        Flow::Abort(msg)
    }

    /// An arithmetic operator over Matrix operands, as the backends lower it
    /// (`almide-mir` `matrix_binop_rewrite`): both Matrix → `matrix.__add` /
    /// `__sub` / `__mul` / `__div`; one Matrix and a scalar (`ScaleMatrix`) →
    /// `matrix.scale(m, k)` with the Matrix first and an Int `k` converted
    /// (`int.__raw_to_float`). `None` = not a Matrix operator.
    pub(crate) fn eval_matrix_binop(
        &mut self,
        op: BinOp,
        left_ty: &Ty,
        right_ty: &Ty,
        l: &Value,
        r: &Value,
    ) -> Option<Flow> {
        let (lm, rm) = (is_matrix_ty(left_ty), is_matrix_ty(right_ty));
        if !lm && !rm {
            return None;
        }
        if matches!(op, BinOp::ScaleMatrix) {
            let (m, k) = if lm { (l, r) } else { (r, l) };
            let k = match k {
                Value::Int(n) => Value::Float(*n as f64),
                other => other.clone(),
            };
            return Some(self.forward("matrix", "scale", &[m.clone(), k]));
        }
        if !(lm && rm) {
            return None;
        }
        let func = match op {
            BinOp::AddMatrix | BinOp::AddInt | BinOp::AddFloat => "__add",
            BinOp::SubMatrix | BinOp::SubInt | BinOp::SubFloat => "__sub",
            BinOp::MulMatrix | BinOp::MulInt | BinOp::MulFloat => "__mul",
            BinOp::DivInt | BinOp::DivFloat => "__div",
            _ => return None,
        };
        Some(self.forward("matrix", func, &[l.clone(), r.clone()]))
    }
}

/// A Matrix-typed operand, bare or applied (`Matrix[Float]`).
pub(crate) fn is_matrix_ty(t: &Ty) -> bool {
    matches!(t, Ty::Matrix)
        || matches!(t, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Matrix, _))
}

// ── the `<T>.from` family ───────────────────────────────────────────

/// A numeric source value, read through its STATIC type: the interp carries
/// every integer in one i64 (a UInt64 as its bit pattern) and every float in
/// one f64 (a Float32 widened, exactly).
#[derive(Clone, Copy)]
enum Num {
    /// Int, Int8..Int64, UInt8..UInt32 — the value itself.
    Signed(i64),
    /// UInt64 — the u64 its slot's bits spell.
    U64(u64),
    /// Float / Float64.
    Float(f64),
    /// Float32, widened.
    F32(f64),
}

impl Num {
    fn read(ty: &Ty, v: &Value) -> Option<Num> {
        match (ty, v) {
            (Ty::UInt64, Value::Int(n)) => Some(Num::U64(*n as u64)),
            (
                Ty::Int | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32,
                Value::Int(n),
            ) => Some(Num::Signed(*n)),
            (Ty::Float32, Value::Float(f)) => Some(Num::F32(*f)),
            (Ty::Float | Ty::Float64, Value::Float(f)) => Some(Num::Float(*f)),
            _ => None,
        }
    }
}

/// The numeric carrier a module names.
#[derive(Clone, Copy)]
enum Target {
    /// An integer carrier other than UInt64: its type name, range, width and
    /// signedness.
    Int { ty: &'static str, lo: i64, hi: i64, bits: u32, signed: bool },
    UInt64,
    /// Float and Float64 — one f64.
    Float,
    Float32,
}

fn target_of(module: &str) -> Option<Target> {
    let int = |ty: &'static str, lo: i64, hi: i64, bits: u32, signed: bool| Target::Int { ty, lo, hi, bits, signed };
    Some(match module {
        "int" => int("Int", i64::MIN, i64::MAX, 64, true),
        "int64" => int("Int64", i64::MIN, i64::MAX, 64, true),
        "int8" => int("Int8", i8::MIN as i64, i8::MAX as i64, 8, true),
        "int16" => int("Int16", i16::MIN as i64, i16::MAX as i64, 16, true),
        "int32" => int("Int32", i32::MIN as i64, i32::MAX as i64, 32, true),
        "uint8" => int("UInt8", 0, u8::MAX as i64, 8, false),
        "uint16" => int("UInt16", 0, u16::MAX as i64, 16, false),
        "uint32" => int("UInt32", 0, u32::MAX as i64, 32, false),
        "uint64" => Target::UInt64,
        "float" => Target::Float,
        "float64" => Target::Float,
        "float32" => Target::Float32,
        _ => return None,
    })
}

#[derive(Clone, Copy)]
enum CarrierOp {
    /// `from`: toward zero; NaN, an infinity or out of range aborts.
    Plain,
    /// `from_checked`: some only when the target holds x exactly.
    Checked,
    /// `from_saturating`: toward zero, clamped; NaN is 0.
    Saturating,
    /// `from_wrapping` (Integer sources): the low bits.
    Wrapping,
}

fn carrier_op(func: &str) -> Option<CarrierOp> {
    Some(match func {
        "from" => CarrierOp::Plain,
        "from_checked" => CarrierOp::Checked,
        "from_saturating" => CarrierOp::Saturating,
        "from_wrapping" => CarrierOp::Wrapping,
        _ => return None,
    })
}

fn some_or_none(v: Option<Value>) -> Flow {
    Flow::val(Value::Option(v.map(Box::new)))
}

fn out_of_range(module: &str, ty: &str) -> Flow {
    Flow::Abort(format!("{module}.from: the value is not a number in the range of {ty}"))
}

/// One cell of the family. `None` = a cell the stdlib does not declare (a
/// float target's `from_saturating`, a float source's `from_wrapping`).
fn carrier(module: &str, target: Target, op: CarrierOp, x: Num) -> Option<Flow> {
    Some(match (target, op) {
        (Target::Int { ty, lo, hi, .. }, CarrierOp::Plain) => {
            match int_exact(trunc(x)).filter(|n| (lo..=hi).contains(n)) {
                Some(n) => Flow::val(Value::Int(n)),
                None => out_of_range(module, ty),
            }
        }
        (Target::Int { lo, hi, .. }, CarrierOp::Checked) => {
            some_or_none(int_exact(x).filter(|n| (lo..=hi).contains(n)).map(Value::Int))
        }
        (Target::Int { lo, hi, .. }, CarrierOp::Saturating) => Flow::val(Value::Int(int_sat(x).clamp(lo, hi))),
        (Target::Int { bits, signed, .. }, CarrierOp::Wrapping) => {
            Flow::val(Value::Int(wrap_to(bits64(x)?, bits, signed)))
        }
        (Target::UInt64, CarrierOp::Plain) => match u64_exact(trunc(x)) {
            Some(u) => Flow::val(Value::Int(u as i64)),
            None => out_of_range(module, "UInt64"),
        },
        (Target::UInt64, CarrierOp::Checked) => some_or_none(u64_exact(x).map(|u| Value::Int(u as i64))),
        (Target::UInt64, CarrierOp::Saturating) => Flow::val(Value::Int(u64_sat(x) as i64)),
        (Target::UInt64, CarrierOp::Wrapping) => Flow::val(Value::Int(bits64(x)?)),
        (Target::Float, CarrierOp::Plain) => Flow::val(Value::Float(to_f64(x))),
        (Target::Float, CarrierOp::Checked) => some_or_none(f64_exact(x).map(Value::Float)),
        (Target::Float32, CarrierOp::Plain) => Flow::val(Value::Float(to_f32(x))),
        (Target::Float32, CarrierOp::Checked) => some_or_none(f32_exact(x).map(Value::Float)),
        (Target::Float | Target::Float32, CarrierOp::Saturating | CarrierOp::Wrapping) => return None,
    })
}

/// `__trunc`: the identity on integers, `float.trunc` / `float32.trunc`
/// (toward zero, exact) on floats.
fn trunc(x: Num) -> Num {
    match x {
        Num::Float(f) => Num::Float(f.trunc()),
        Num::F32(f) => Num::F32(f.trunc()),
        other => other,
    }
}

/// `__int_exact`: Int's `some(x)`, UInt64's `int.__from_uint64_checked`, and
/// `float.__int_exact` (an integral value in -2^63..2^63) for the floats.
fn int_exact(x: Num) -> Option<i64> {
    match x {
        Num::Signed(n) => Some(n),
        Num::U64(u) => i64::try_from(u).ok(),
        Num::Float(f) | Num::F32(f) => (f == f.trunc() && f >= -TWO63 && f < TWO63).then_some(f as i64),
    }
}

/// `__uint64_exact`: `int.__to_uint64_checked` (none below 0), UInt64's
/// `some(x)`, and `float.__uint64_exact` (an integral value in 0..2^64).
fn u64_exact(x: Num) -> Option<u64> {
    match x {
        Num::Signed(n) => u64::try_from(n).ok(),
        Num::U64(u) => Some(u),
        Num::Float(f) | Num::F32(f) => (f == f.trunc() && f >= 0.0 && f < TWO64).then_some(f as u64),
    }
}

/// `__int_sat`: the integer itself (UInt64 clamped to 2^63-1), and
/// `float.__int_sat` — NaN is 0, the bounds saturate, else toward zero.
fn int_sat(x: Num) -> i64 {
    match x {
        Num::Signed(n) => n,
        Num::U64(u) => i64::try_from(u).unwrap_or(i64::MAX),
        Num::Float(f) | Num::F32(f) => {
            if f.is_nan() {
                0
            } else if f >= TWO63 {
                i64::MAX
            } else if f <= -TWO63 {
                i64::MIN
            } else {
                f as i64
            }
        }
    }
}

/// `__uint64_sat`: `int.__to_uint64_saturating` (below 0 is 0), UInt64
/// itself, and `float.__to_uint64` (Rust's saturating `as u64`, NaN 0).
fn u64_sat(x: Num) -> u64 {
    match x {
        Num::Signed(n) => u64::try_from(n).unwrap_or(0),
        Num::U64(u) => u,
        Num::Float(f) | Num::F32(f) => f as u64,
    }
}

/// `__bits64` of an Integer source: its two's-complement 64 bits.
fn bits64(x: Num) -> Option<i64> {
    match x {
        Num::Signed(n) => Some(n),
        Num::U64(u) => Some(u as i64),
        Num::Float(_) | Num::F32(_) => None,
    }
}

/// The sized `from_wrapping` bodies: `int.wrap(b, w)`, then the signed
/// reading `if u >= 2^(w-1) then u - 2^w else u` for a signed target.
fn wrap_to(b: i64, bits: u32, signed: bool) -> i64 {
    if bits >= 64 {
        return b;
    }
    let u = b & ((1i64 << bits) - 1);
    if signed && u >= (1i64 << (bits - 1)) {
        u - (1i64 << bits)
    } else {
        u
    }
}

/// `uint64.__to_float64`: the lower half through the signed pivot; the upper
/// half as `hf + hf + (x % 2)` with `hf` the f64 of `x / 2` — the body's own
/// steps, roundings included.
fn u64_to_f64(u: u64) -> f64 {
    if (u as i64) >= 0 {
        (u as i64) as f64
    } else {
        let hf = ((u / 2) as i64) as f64;
        hf + hf + ((u % 2) as i64) as f64
    }
}

/// `uint64.__raw_to_float32`: the same split, narrowed to f32 at the end.
fn u64_to_f32(u: u64) -> f64 {
    if (u as i64) >= 0 {
        ((u as i64) as f32) as f64
    } else {
        let hf = ((u / 2) as i64) as f64;
        ((hf + hf + ((u % 2) as i64) as f64) as f32) as f64
    }
}

/// `__to_float`: `int.__raw_to_float`, `uint64.__to_float64`, or the float.
fn to_f64(x: Num) -> f64 {
    match x {
        Num::Signed(n) => n as f64,
        Num::U64(u) => u64_to_f64(u),
        Num::Float(f) | Num::F32(f) => f,
    }
}

/// `__to_float_exact`: Int's round trip below 2^63, UInt64's round trip
/// through `float.__uint64_exact`, a float's `none` for NaN only.
fn f64_exact(x: Num) -> Option<f64> {
    match x {
        Num::Signed(n) => {
            let f = n as f64;
            (f < TWO63 && f as i64 == n).then_some(f)
        }
        Num::U64(u) => {
            let f = u64_to_f64(u);
            (u64_exact(Num::Float(f)) == Some(u)).then_some(f)
        }
        Num::Float(f) | Num::F32(f) => (!f.is_nan()).then_some(f),
    }
}

/// `__to_float32`: `int.__raw_to_float32` (`as f32`), `uint64.__raw_to_float32`,
/// `float.__raw_to_float32`, Float32 itself.
fn to_f32(x: Num) -> f64 {
    match x {
        Num::Signed(n) => (n as f32) as f64,
        Num::U64(u) => u64_to_f32(u),
        Num::Float(f) => (f as f32) as f64,
        Num::F32(f) => f,
    }
}

/// `__to_float32_exact`: `int.__to_float32_checked` (the widened read-back
/// at or past 2^63 is none), UInt64's round trip, `float.__to_float32_checked`
/// (NaN and ±inf are none), and Float32's `none` for NaN only.
fn f32_exact(x: Num) -> Option<f64> {
    match x {
        Num::Signed(n) => {
            let back = (n as f32) as f64;
            (back < TWO63 && back as i64 == n).then_some(back)
        }
        Num::U64(u) => {
            let f = u64_to_f32(u);
            (u64_exact(Num::Float(f)) == Some(u)).then_some(f)
        }
        Num::Float(f) => {
            let back = (f as f32) as f64;
            (!f.is_nan() && !f.is_infinite() && back == f).then_some(back)
        }
        Num::F32(f) => (!f.is_nan()).then_some(f),
    }
}

// ── int.parse_radix's helpers ───────────────────────────────────────

/// `prefix_radix`: the radix a `0x` / `0o` / `0b` letter names, any case.
fn prefix_radix(b: u8) -> i64 {
    match b {
        b'x' | b'X' => 16,
        b'o' | b'O' => 8,
        b'b' | b'B' => 2,
        _ => 0,
    }
}

/// `digit_value`: an ASCII digit or letter, any case; 99 for any other byte.
fn digit_value(b: u8) -> i64 {
    match b {
        b'0'..=b'9' => (b - b'0') as i64,
        b'a'..=b'z' => (b - b'a') as i64 + 10,
        b'A'..=b'Z' => (b - b'A') as i64 + 10,
        _ => 99,
    }
}

// ── the value-only arms ─────────────────────────────────────────────

/// `string.from_codepoint(n)`: none outside the Unicode scalar values, else
/// the one-char string `string.__from_codepoint` encodes.
fn surface_string(f: &str, args: &[Value]) -> Option<Flow> {
    match (f, args) {
        ("from_codepoint", [Value::Int(n)]) => {
            let n = *n;
            let c = if !(0..=1114111).contains(&n) || (55296..=57343).contains(&n) {
                None
            } else {
                Some(Value::str(char::from_u32(n as u32)?.to_string()))
            };
            Some(some_or_none(c))
        }
        _ => None,
    }
}

/// `datetime.now_ms()`: wall-clock milliseconds since the Unix epoch — the
/// real host clock, as the prim floor serves `prim.clock_time_get` (a
/// fixture can only print relations between reads).
fn surface_datetime(f: &str, args: &[Value]) -> Option<Flow> {
    match (f, args) {
        ("now_ms", []) => {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            Some(Flow::val(Value::Int(ms)))
        }
        _ => None,
    }
}

/// `io.stdin()` / `io.stdout()` / `io.stderr()`: `Stdin(0)` / `Stdout(1)` /
/// `Stderr(2)`, `mod type`s over Int that both backends erase to the Int.
fn surface_io(f: &str, args: &[Value]) -> Option<Flow> {
    if !args.is_empty() {
        return None;
    }
    let fd = match f {
        "stdin" => 0,
        "stdout" => 1,
        "stderr" => 2,
        _ => return None,
    };
    Some(Flow::val(Value::Int(fd)))
}

/// `type Deque[A] = { front: List[A], back: List[A] }` and the readers that
/// build or read it whole (stdlib/deque.almd).
fn surface_deque(f: &str, args: &[Value]) -> Option<Flow> {
    let deque = |front: Vec<Value>, back: Vec<Value>| Value::Record {
        name: Some(sym("Deque")),
        fields: Rc::new(vec![(sym("front"), Value::list(front)), (sym("back"), Value::list(back))]),
    };
    let halves = |d: &Value| -> Option<(Vec<Value>, Vec<Value>)> {
        let Value::Record { fields, .. } = d else { return None };
        let field = |name: &str| fields.iter().find(|(k, _)| k.as_str() == name).and_then(|(_, v)| v.as_iter_items());
        Some((field("front")?, field("back")?))
    };
    Some(Flow::val(match (f, args) {
        ("new", []) => deque(Vec::new(), Vec::new()),
        ("from_list", [xs]) => deque(Vec::new(), xs.as_iter_items()?),
        ("to_list", [d]) => {
            let (mut front, back) = halves(d)?;
            front.reverse();
            front.extend(back);
            Value::list(front)
        }
        ("len", [d]) => {
            let (front, back) = halves(d)?;
            Value::Int((front.len() + back.len()) as i64)
        }
        ("is_empty", [d]) => {
            let (front, back) = halves(d)?;
            Value::Bool(front.is_empty() && back.is_empty())
        }
        _ => return None,
    }))
}

/// A `bytes.get_<k><bits>[_<le|be>]` / `next_…` layout: width in bytes,
/// big-endian, the scalar kind.
#[derive(Clone, Copy)]
struct ReadEnc {
    width: usize,
    big_endian: bool,
    kind: u8,
}

/// `u16_le`, `i32_be`, `f64_le`, `u8`, `i8`: a 1-byte scalar carries no
/// order, a wider one must; f16 is not read here (its decode is the body's
/// own `__read_f16_*` arithmetic).
fn read_enc(suffix: &str) -> Option<ReadEnc> {
    let (kind_bits, big_endian) = match suffix.rsplit_once('_') {
        Some((k, "le")) => (k, Some(false)),
        Some((k, "be")) => (k, Some(true)),
        _ => (suffix, None),
    };
    let (kind, digits) = kind_bits.split_at_checked(1)?;
    let kind = match kind {
        "u" => b'u',
        "i" => b'i',
        "f" => b'f',
        _ => return None,
    };
    let width = match digits.parse::<usize>().ok()? {
        8 => 1,
        16 => 2,
        32 => 4,
        64 => 8,
        _ => return None,
    };
    if kind == b'f' && width < 4 {
        return None;
    }
    match (width, big_endian) {
        (1, None) => Some(ReadEnc { width, big_endian: false, kind }),
        (1, Some(_)) | (_, None) => None,
        (_, Some(big_endian)) => Some(ReadEnc { width, big_endian, kind }),
    }
}

/// The octets of a Bytes value, `None` when it is not one (a block address
/// inside the pool tier is not decoded here).
fn octets(v: &Value) -> Option<Vec<u8>> {
    let Value::List(xs) = v else { return None };
    xs.iter()
        .map(|x| match x {
            Value::Int(n) => Some(*n as u8),
            _ => None,
        })
        .collect()
}

/// `get_<enc>(b, pos)`: `if fits(b, pos, w) then some(__read_<enc>(b, pos)) else none`,
/// `fits` being `pos >= 0 and pos <= bytes.len(b) - w`.
fn read_at(b: &[u8], pos: i64, enc: ReadEnc) -> Option<Value> {
    let w = enc.width;
    if !(0..=b.len() as i64 - w as i64).contains(&pos) {
        return None;
    }
    let p = pos as usize;
    let mut raw: u64 = 0;
    for i in 0..w {
        let octet = if enc.big_endian { b[p + i] } else { b[p + w - 1 - i] };
        raw = (raw << 8) | octet as u64;
    }
    Some(match (enc.kind, w) {
        (b'f', 4) => Value::Float(f32::from_bits(raw as u32) as f64),
        (b'f', _) => Value::Float(f64::from_bits(raw)),
        (b'i', 8) | (b'u', 8) => Value::Int(raw as i64),
        (b'i', _) => {
            let shift = 64 - 8 * w as u32;
            Value::Int(((raw << shift) as i64) >> shift)
        }
        _ => Value::Int(raw as i64),
    })
}

/// The `bytes` readers: `get_<enc>` (an Option), `next_<enc>` (that Option
/// and the position, advanced by the width only when something was read),
/// and `next_bytes(b, pos, n)` — `__take_at`'s window swapped into
/// `(Bytes?, next)`.
fn surface_bytes(f: &str, args: &[Value]) -> Option<Flow> {
    if f == "next_bytes" {
        let [b, Value::Int(pos), Value::Int(n)] = args else { return None };
        let b = octets(b)?;
        let (pos, n) = (*pos, *n);
        let window = usize::try_from(pos)
            .ok()
            .zip(usize::try_from(n).ok())
            .filter(|(p, n)| p.checked_add(*n).is_some_and(|e| e <= b.len()));
        let (v, next) = match window {
            Some((p, k)) => {
                let slice = b[p..p + k].iter().map(|o| Value::Int(*o as i64)).collect();
                (Some(Value::list(slice)), pos + n)
            }
            None => (None, pos),
        };
        return Some(Flow::val(Value::tuple(vec![Value::Option(v.map(Box::new)), Value::Int(next)])));
    }
    let (is_next, suffix) = match f.strip_prefix("get_") {
        Some(s) => (false, s),
        None => (true, f.strip_prefix("next_")?),
    };
    let enc = read_enc(suffix)?;
    let [b, Value::Int(pos)] = args else { return None };
    let v = read_at(&octets(b)?, *pos, enc);
    if !is_next {
        return Some(some_or_none(v));
    }
    let next = if v.is_some() { *pos + enc.width as i64 } else { *pos };
    Some(Flow::val(Value::tuple(vec![Value::Option(v.map(Box::new)), Value::Int(next)])))
}
