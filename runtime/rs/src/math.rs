// math extern — Rust native implementations

// Trigonometry
// sin/cos/tan delegate to the vendored musl-libm reference (runtime/rs/src/libm.rs)
// instead of the platform `f64::sin`/`cos`/`tan`. The system libm's last-ULP result
// is platform-specific, so it can't be a stable cross-target oracle. The vendored
// algorithm is deterministic across platforms AND bit-identical to the WASM port
// (`emit_wasm/rt_libm.rs`), which mirrors libm.rs function-for-function.
#[inline(always)] pub fn almide_rt_math_sin(x: f64) -> f64 { almide_rt_libm_sin(x) }
#[inline(always)] pub fn almide_rt_math_cos(x: f64) -> f64 { almide_rt_libm_cos(x) }
#[inline(always)] pub fn almide_rt_math_tan(x: f64) -> f64 { almide_rt_libm_tan(x) }
#[inline(always)] pub fn almide_rt_math_asin(x: f64) -> f64 { x.asin() }
#[inline(always)] pub fn almide_rt_math_acos(x: f64) -> f64 { x.acos() }
#[inline(always)] pub fn almide_rt_math_atan(x: f64) -> f64 { almide_rt_libm_atan(x) }
#[inline(always)] pub fn almide_rt_math_atan2(y: f64, x: f64) -> f64 { y.atan2(x) }
#[inline(always)] pub fn almide_rt_math_tanh(x: f64) -> f64 { almide_rt_libm_tanh(x) }

// Logarithms / exponentials
// log/log2/log10/exp delegate to the vendored musl-libm reference
// (runtime/rs/src/libm.rs) for the same cross-platform-deterministic +
// bit-identical-to-WASM guarantee as sin/cos/tan. Platform f64::ln/log2/log10/exp
// differ in the last ULP per OS, so they can't be a stable cross-target oracle.
#[inline(always)] pub fn almide_rt_math_log(x: f64) -> f64 { almide_rt_libm_log(x) }
#[inline(always)] pub fn almide_rt_math_log2(x: f64) -> f64 { almide_rt_libm_log2(x) }
#[inline(always)] pub fn almide_rt_math_log10(x: f64) -> f64 { almide_rt_libm_log10(x) }
#[inline(always)] pub fn almide_rt_math_exp(x: f64) -> f64 { almide_rt_libm_exp(x) }
// Integer pow (math.pow on Int) is TOTAL like integer `/`/`%` (C-001 family):
// a NEGATIVE exponent has no integer result, so it ABORTS with `Error: negative
// exponent` + exit 1 on BOTH targets instead of the old `exp as u32` u32-wrap
// (which silently produced garbage and diverged from the wasm loop). For a
// non-negative exponent it is exponentiation-by-squaring with WRAPPING multiply,
// matching the rest of the wrap-arithmetic contract: it agrees bit-for-bit with
// the old `base.pow(exp as u32)` on every in-range case (overflow-checks=off) and
// extends deterministically to exponents `>= 2^32` (the full i64 count, e.g.
// `2^(2^32) = 0`) where the old u32-truncation and the wasm loop disagreed.
/// `Error: <msg>` + exit-1 abort message for a negative `math.pow` exponent.
/// Same wording byte-for-byte as the wasm `__pow_trap` so the two targets'
/// stderr is identical (the C-001 totality discipline).
pub const POW_NEGATIVE_EXPONENT_MSG: &str = "negative exponent";
#[inline(always)]
pub fn almide_rt_math_pow(base: i64, exp: i64) -> i64 {
    if exp < 0 {
        eprintln!("Error: {}", POW_NEGATIVE_EXPONENT_MSG);
        std::process::exit(1);
    }
    let mut result: i64 = 1;
    let mut b = base;
    let mut e = exp as u64;
    while e > 0 {
        if e & 1 == 1 {
            result = result.wrapping_mul(b);
        }
        e >>= 1;
        if e > 0 {
            b = b.wrapping_mul(b);
        }
    }
    result
}

// Rounding
#[inline(always)] pub fn almide_rt_math_abs(x: i64) -> i64 { x.abs() }
#[inline(always)] pub fn almide_rt_math_ceil(x: f64) -> f64 { x.ceil() }
#[inline(always)] pub fn almide_rt_math_floor(x: f64) -> f64 { x.floor() }
#[inline(always)] pub fn almide_rt_math_round(x: f64) -> f64 { x.round() }
#[inline(always)] pub fn almide_rt_math_sqrt(x: f64) -> f64 { x.sqrt() }

// Constants
#[inline(always)] pub fn almide_rt_math_pi() -> f64 { std::f64::consts::PI }
#[inline(always)] pub fn almide_rt_math_e() -> f64 { std::f64::consts::E }
#[inline(always)] pub fn almide_rt_math_inf() -> f64 { f64::INFINITY }
#[inline(always)] pub fn almide_rt_math_is_nan(x: f64) -> bool { x.is_nan() }

// Int min/max/sign
#[inline(always)] pub fn almide_rt_math_min(a: i64, b: i64) -> i64 { a.min(b) }
#[inline(always)] pub fn almide_rt_math_max(a: i64, b: i64) -> i64 { a.max(b) }
#[inline(always)] pub fn almide_rt_math_sign(n: i64) -> i64 { if n > 0 { 1 } else if n < 0 { -1 } else { 0 } }

// Float min/max — explicit NaN/tie decision tree, mirrored bit-for-bit by the
// wasm emitter (`emit_float_min_max` in emit_wasm/calls_numeric.rs).
// Deliberately NOT `f64::min`/`f64::max`: those are the llvm.minnum/maxnum
// intrinsics whose ±0-tie order is UNSPECIFIED — under `#[inline(always)]`
// x86 selects `maxsd` (returns the SECOND operand on ties), silently
// contradicting both the non-inlined library call and the wasm emit.
// Ties return the FIRST operand: max(0,-0)=0, max(-0,0)=-0 (C-049).
#[inline(always)] pub fn almide_rt_math_fmin(a: f64, b: f64) -> f64 {
    if a.is_nan() { b } else if b.is_nan() { a } else if a > b { b } else { a }
}
#[inline(always)] pub fn almide_rt_math_fmax(a: f64, b: f64) -> f64 {
    if a.is_nan() { b } else if b.is_nan() { a } else if a < b { b } else { a }
}
// Float pow delegates to the vendored musl-libm `pow` (deterministic +
// bit-identical to the WASM port). This also makes all the special cases
// (0/inf/nan/neg-base, odd/even integer exponent) match exactly cross-target.
#[inline(always)] pub fn almide_rt_math_fpow(base: f64, exp: f64) -> f64 { almide_rt_libm_pow(base, exp) }

// Factorial / combinatorics
pub fn almide_rt_math_factorial(n: i64) -> i64 {
    (1..=n).product()
}
pub fn almide_rt_math_choose(n: i64, k: i64) -> i64 {
    if k < 0 || k > n { return 0; }
    let k = k.min(n - k) as u64;
    let mut result: u64 = 1;
    for i in 0..k {
        result = result * (n as u64 - i) / (i + 1);
    }
    result as i64
}
// Lanczos approximation (g=7, n=9 coefficients). Both the native and the wasm
// log_gamma compute this SAME polynomial; the only ULP-level divergence was the
// three `ln(...)` calls. Native used the PLATFORM `f64::ln` (per-OS last-ULP),
// wasm used the VENDORED musl-libm `log` — so they could differ by ~1 ULP. The
// fix routes native through the SAME vendored `almide_rt_libm_log` and pins the
// `0.5·ln(2π)` term to its exact f64 bit-pattern (the platform `ln` of `2π`
// happens to equal this literal, but hard-coding it removes the platform call),
// making native == wasm bit-for-bit. See emit_wasm/calls_numeric.rs "log_gamma".
const LANCZOS_G_OFFSET: f64 = 7.5; // t = x + g + 0.5  with g = 7
/// `0.5 · ln(2π)`, pinned to its exact f64 bit-pattern (= `(2π).ln() * 0.5`),
/// shared verbatim with the wasm emit so the constant term cannot drift.
const HALF_LN_2PI: f64 = 0.9189385332046727;
pub fn almide_rt_math_log_gamma(x: f64) -> f64 {
    // Lanczos computes Γ(x+1), so shift input by -1 to get Γ(x)
    let x = x - 1.0;
    let coeffs = [
        0.99999999999980993, 676.5203681218851, -1259.1392167224028,
        771.32342877765313, -176.61502916214059, 12.507343278686905,
        -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7,
    ];
    let mut ag = coeffs[0];
    for (i, &c) in coeffs[1..].iter().enumerate() {
        ag += c / (x + (i + 1) as f64);
    }
    let t = x + LANCZOS_G_OFFSET;
    HALF_LN_2PI + (x + 0.5) * almide_rt_libm_log(t) - t + almide_rt_libm_log(ag)
}
