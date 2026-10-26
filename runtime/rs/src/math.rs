// math extern — Rust native implementations

// Trigonometry
// sin/cos/tan delegate to the vendored musl-libm reference (runtime/rs/src/libm.rs)
// instead of the platform `f64::sin`/`cos`/`tan`. The system libm's last-ULP result
// is platform-specific, so it can't be a stable cross-target oracle. The vendored
// algorithm is deterministic across platforms AND bit-identical to the self-hosted
// WASM port (`stdlib/math_trig.almd`), which mirrors libm.rs function-for-function.
#[inline(always)] pub fn almide_rt_math_sin(x: f64) -> f64 { almide_rt_libm_sin(x) }
#[inline(always)] pub fn almide_rt_math_cos(x: f64) -> f64 { almide_rt_libm_cos(x) }
#[inline(always)] pub fn almide_rt_math_tan(x: f64) -> f64 { almide_rt_libm_tan(x) }
#[inline(always)] pub fn almide_rt_math_atan(x: f64) -> f64 { almide_rt_libm_atan(x) }
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
pub const ALMIDE_POW_NEGATIVE_EXPONENT_MSG: &str = "negative exponent";
#[inline(always)]
pub fn almide_rt_math_pow(base: i64, exp: i64) -> i64 {
    if exp < 0 {
        almide_abort(ALMIDE_POW_NEGATIVE_EXPONENT_MSG);
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

#[inline(always)] pub fn almide_rt_math_ceil(x: f64) -> f64 { x.ceil() }
#[inline(always)] pub fn almide_rt_math_floor(x: f64) -> f64 { x.floor() }
#[inline(always)] pub fn almide_rt_math_round(x: f64) -> f64 { x.round() }
// A negative x MANUFACTURES a NaN: the ONE fixed +qNaN 0x7FF8000000000000
// (C-055), not the host's sqrt NaN (x86 -NaN, aarch64 +NaN). -0.0 and a NaN x
// take `f64::sqrt` (-0.0 stays -0.0, the NaN passes through). The sign is
// cleared with `abs` on the sqrt result, not by selecting a NaN constant:
// LLVM treats every NaN as interchangeable and folds
// `if x < 0.0 { NaN } else { x.sqrt() }` back into the bare sqrt (measured
// with rustc -O on aarch64), while `abs` is a sign-bit operation it keeps, and
// it stays branchless (a `black_box`ed constant cost nbody 30%). Every leg's
// invalid-operation NaN carries the default payload (quiet bit only), so the
// result is exactly 0x7FF8000000000000.
#[inline(always)] pub fn almide_rt_math_sqrt(x: f64) -> f64 { let r = x.sqrt(); if x < 0.0 { r.abs() } else { r } }

// Constants
#[inline(always)] pub fn almide_rt_math_pi() -> f64 { std::f64::consts::PI }
#[inline(always)] pub fn almide_rt_math_e() -> f64 { std::f64::consts::E }
#[inline(always)] pub fn almide_rt_math_inf() -> f64 { f64::INFINITY }
#[inline(always)] pub fn almide_rt_math_is_nan(x: f64) -> bool { x.is_nan() }

#[inline(always)] pub fn almide_rt_math_sign(n: i64) -> i64 { if n > 0 { 1 } else if n < 0 { -1 } else { 0 } }

// Float min/max — explicit NaN/tie decision tree, mirrored bit-for-bit by the
// self-hosted wasm leg (`float_min`/`float_max` in stdlib/float_core.almd).
// Deliberately NOT `f64::min`/`f64::max`: those are the llvm.minnum/maxnum
// intrinsics whose ±0-tie order is UNSPECIFIED — under `#[inline(always)]`
// x86 selects `maxsd` (returns the SECOND operand on ties), silently
// contradicting both the non-inlined library call and the wasm emit.
// Ties follow IEEE-754-2019 zero ordering (C-049, ALS-T23):
// min(±0)= -0.0, max(±0)= +0.0, commutative.
#[inline(always)] pub fn almide_rt_math_fmin(a: f64, b: f64) -> f64 {
    if a.is_nan() { b }
    else if b.is_nan() { a }
    else if a < b { a }
    else if b < a { b }
    // Tie (a == b, the ±0 pair included): IEEE-754-2019 minimum — a
    // negative-signed operand wins, commutatively (min(0,-0) = -0).
    else if a.is_sign_negative() { a } else { b }
}
// Float pow delegates to the vendored musl-libm `pow` (deterministic +
// bit-identical to the WASM port). This also makes all the special cases
// (0/inf/nan/neg-base, odd/even integer exponent) match exactly cross-target.
#[inline(always)] pub fn almide_rt_math_fpow(base: f64, exp: f64) -> f64 { almide_rt_libm_pow(base, exp) }

// Factorial / combinatorics
// #4212: the product wraps mod 2^64, so once n! holds 64 factors of two it is 0 and
// stays 0. Legendre: v2(n!) = n - popcount(n), nondecreasing in n; v2(65!) = 63 and
// v2(66!) = 64, so every n >= 66 answers 0 at once instead of looping n times (an
// input-chosen n of 10^18 ran for centuries). The self-host `math_factorial`
// (stdlib/math_int.almd) makes the same cut.
pub fn almide_rt_math_factorial(n: i64) -> i64 {
    if n >= 66 { return 0; }
    (1..=n).fold(1i64, |acc, x| acc.wrapping_mul(x))
}
// C(n, k) EXACTLY, reduced mod 2^64 the way every Int product is (C-170, C-056): the
// true binomial whenever it fits in an Int, and its two's-complement wrap when it does
// not (`choose(67, 33)` = C(67,33) - 2^64). #2491: the old running product
// `result * (n - i) / (i + 1)` multiplied before it divided, so the product left i64
// several steps before the answer did, and `choose(62, 31)` came out negative although
// C(62,31) < 2^63. A wrapped product cannot be divided exactly, so the product is kept
// in a form where division is not needed: each factor's power of two is counted
// (`twos`, which after step i is v2(C(n, i+1)) >= 0 because every partial product is a
// binomial coefficient) and its odd part is multiplied in mod 2^64; odd numbers are
// invertible mod 2^64, so the odd denominators are divided out once at the end by
// their Newton inverse. Every step is wrapping u64 arithmetic, the same ops the
// self-host `math_choose` (stdlib/math_int.almd) runs on the wasm legs. `n >= k >= 0`
// holds past the guard, so `n - i >= 1` and `i + 1 >= 1`.
pub fn almide_rt_math_choose(n: i64, k: i64) -> i64 {
    if k < 0 || k > n { return 0; }
    let k = k.min(n - k);
    // #4212: the loop below runs min(k, n-k) times, linear in the VALUE of k, so
    // an input-chosen n of 10^18 ran for centuries. Past 64 steps the answer comes
    // from the odd-factorial form instead, whose work is polylogarithmic in n.
    if k > 64 { return almide_rt_math_choose_odd_factorials(n as u64, k as u64) as i64; }
    let mut odd_num: u64 = 1;
    let mut odd_den: u64 = 1;
    let mut twos: u32 = 0;
    for i in 0..k {
        let a = (n - i) as u64;
        let b = (i + 1) as u64;
        let (ta, tb) = (a.trailing_zeros(), b.trailing_zeros());
        odd_num = odd_num.wrapping_mul(a >> ta);
        odd_den = odd_den.wrapping_mul(b >> tb);
        twos = twos + ta - tb;
    }
    let odd = odd_num.wrapping_mul(almide_rt_math_choose_inv(odd_den));
    // v2(C(n, k)) <= log2(n) < 63, so the shift never reaches the width.
    (if twos >= 64 { 0 } else { odd << twos }) as i64
}
// x = a (odd) is its own inverse mod 2^3; each Newton step doubles the correct
// low bits (3, 6, 12, 24, 48, 96), so five steps reach 64.
fn almide_rt_math_choose_inv(a: u64) -> u64 {
    let mut inv = a;
    for _ in 0..5 {
        inv = inv.wrapping_mul(2u64.wrapping_sub(a.wrapping_mul(inv)));
    }
    inv
}
// C(n, k) mod 2^64 in time polylogarithmic in n (#4212), the same value the loop
// above computes. C(n, k) = 2^t * O(n) / (O(k) O(n-k)) where O(m) is the odd part
// of m! and t = v2(C(n, k)) = popcount(k) + popcount(n-k) - popcount(n) (Kummer:
// the carries of k + (n-k) in base 2). O(m) = prod_{i>=0} F(m >> i), F(m) the
// product of the odd numbers <= m. F is a product over the binary blocks of
// [0, m+1): the odd numbers of a block [b*2^s, (b+1)*2^s) multiply to G_s(b),
// G_s(y) = prod_{odd j < 2^s} (2^s y + j). The coefficient of y^i in G_s carries
// 2^(s*i), so mod 2^64 only the terms with s*i < 64 survive, and
// G_(s+1)(y) = G_s(2y) * G_s(2y + 1) builds every G_s from G_1(y) = 2y + 1 in
// about 20k multiplications. All of it is wrapping u64 arithmetic, op for op the
// self-host `__choose_odd_factorials` (stdlib/math_int.almd) on the wasm legs.
fn almide_rt_math_choose_odd_factorials(n: u64, k: u64) -> u64 {
    let t = k.count_ones() + (n - k).count_ones() - n.count_ones();
    if t >= 64 { return 0; }
    let g = almide_rt_math_choose_block_polys();
    let odd = almide_rt_math_choose_odd_fact(&g, n)
        .wrapping_mul(almide_rt_math_choose_inv(almide_rt_math_choose_odd_fact(&g, k)))
        .wrapping_mul(almide_rt_math_choose_inv(almide_rt_math_choose_odd_fact(&g, n - k)));
    odd << t
}
// g[s] = G_s's surviving coefficients (lowest first), s in 1..=63; g[0] is unused.
fn almide_rt_math_choose_block_polys() -> Vec<Vec<u64>> {
    let mut g: Vec<Vec<u64>> = vec![vec![1], vec![1, 2]];
    for s in 1..63usize {
        let c = &g[s];
        // G_s(2y): coefficient i scaled by 2^i (i < 64 since s*i < 64).
        let a: Vec<u64> = c.iter().enumerate().map(|(i, &x)| x << i).collect();
        // G_s(2y + 1) by Horner: b = b * (2y + 1) + c_i, from the top coefficient.
        let mut b: Vec<u64> = Vec::new();
        for &x in c.iter().rev() {
            let mut nb = vec![0u64; b.len() + 1];
            for (j, &bj) in b.iter().enumerate() {
                nb[j] = nb[j].wrapping_add(bj);
                nb[j + 1] = nb[j + 1].wrapping_add(bj.wrapping_mul(2));
            }
            nb[0] = nb[0].wrapping_add(x);
            b = nb;
        }
        let d = 63 / (s + 1) + 1;
        let mut p = vec![0u64; d];
        for (i, &ai) in a.iter().enumerate().take(d) {
            for (j, &bj) in b.iter().enumerate().take(d - i) {
                p[i + j] = p[i + j].wrapping_add(ai.wrapping_mul(bj));
            }
        }
        g.push(p);
    }
    g
}
// O(m): the odd part of m!, mod 2^64.
fn almide_rt_math_choose_odd_fact(g: &[Vec<u64>], m: u64) -> u64 {
    let mut r: u64 = 1;
    let mut x = m;
    while x > 0 {
        // F(x): the odd numbers below N = x + 1, block by block from the top bit;
        // the 1-wide block (bit 0) holds an even number and is skipped.
        let big_n = x + 1;
        let mut base: u64 = 0;
        for s in (1..64u32).rev() {
            if (big_n >> s) & 1 == 1 {
                let y = base >> s;
                let v = g[s as usize].iter().rev().fold(0u64, |acc, &c| acc.wrapping_mul(y).wrapping_add(c));
                r = r.wrapping_mul(v);
                base += 1u64 << s;
            }
        }
        x >>= 1;
    }
    r
}
// Lanczos approximation (g=7, n=9 coefficients). Both the native and the wasm
// log_gamma compute this SAME polynomial; the only ULP-level divergence was the
// three `ln(...)` calls. Native used the PLATFORM `f64::ln` (per-OS last-ULP),
// wasm used the VENDORED musl-libm `log` — so they could differ by ~1 ULP. The
// fix routes native through the SAME vendored `almide_rt_libm_log` and pins the
// `0.5·ln(2π)` term to its exact f64 bit-pattern (the platform `ln` of `2π`
// happens to equal this literal, but hard-coding it removes the platform call),
// making native == wasm bit-for-bit. See stdlib/math_lgamma.almd.
const ALMIDE_LANCZOS_G_OFFSET: f64 = 7.5; // t = x + g + 0.5  with g = 7
/// `0.5 · ln(2π)`, pinned to its exact f64 bit-pattern (= `(2π).ln() * 0.5`),
/// shared verbatim with the wasm emit so the constant term cannot drift.
const ALMIDE_HALF_LN_2PI: f64 = 0.9189385332046727;
/// `ln|Γ(x)|` over the whole real line (#2493). The Lanczos series below is only
/// valid for `x >= 0.5`: under it the log arguments reach zero or go negative, and
/// `log_gamma(-0.5)` was NaN, `log_gamma(-6.5)` inf, `log_gamma(-1)` NaN. Below 0.5
/// the REFLECTION formula `ln|Γ(x)| = ln(π / |sin(πx)|) - ln Γ(1 - x)` moves the
/// work to `1 - x > 0.5`. `|sin(πx)|` is computed on the reduced fraction
/// (`f = frac(|x|)`, folded to `min(f, 1 - f)`, both exact), so it keeps full
/// relative accuracy next to the integers where `π·x` itself would round away the
/// answer. The non-positive integers are poles: `f = 0` gives `+inf`, and so does
/// `-inf` (its fraction is NaN) and `+inf` (the series would compute `inf - inf`),
/// matching C `lgamma`. NaN stays NaN (it fails the `< 0.5` test and the series
/// propagates it). Every op is shared with stdlib/math_lgamma.almd — the vendored
/// musl `sin` and `log` on both sides — so the result stays bit-identical (C-051).
pub fn almide_rt_math_log_gamma(x: f64) -> f64 {
    if x < 0.5 {
        let y = if x < 0.0 { -x } else { x };
        let f = y - y.floor();
        let f = if f > 0.5 { 1.0 - f } else { f };
        let s = almide_rt_libm_sin(std::f64::consts::PI * f);
        // `!(s > 0)`: a pole (s = 0) or -inf (s = NaN).
        if !(s > 0.0) { return f64::INFINITY; }
        return almide_rt_libm_log(std::f64::consts::PI / s) - almide_rt_math_log_gamma_lanczos(1.0 - x);
    }
    if x == f64::INFINITY { return f64::INFINITY; }
    almide_rt_math_log_gamma_lanczos(x)
}
fn almide_rt_math_log_gamma_lanczos(x: f64) -> f64 {
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
    let t = x + ALMIDE_LANCZOS_G_OFFSET;
    ALMIDE_HALF_LN_2PI + (x + 0.5) * almide_rt_libm_log(t) - t + almide_rt_libm_log(ag)
}
