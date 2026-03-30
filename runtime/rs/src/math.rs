// math extern — Rust native implementations

// Trigonometry
#[inline(always)] pub fn almide_rt_math_sin(x: f64) -> f64 { x.sin() }
#[inline(always)] pub fn almide_rt_math_cos(x: f64) -> f64 { x.cos() }
#[inline(always)] pub fn almide_rt_math_tan(x: f64) -> f64 { x.tan() }
#[inline(always)] pub fn almide_rt_math_asin(x: f64) -> f64 { x.asin() }
#[inline(always)] pub fn almide_rt_math_acos(x: f64) -> f64 { x.acos() }
#[inline(always)] pub fn almide_rt_math_atan(x: f64) -> f64 { x.atan() }
#[inline(always)] pub fn almide_rt_math_atan2(y: f64, x: f64) -> f64 { y.atan2(x) }

// Logarithms / exponentials
#[inline(always)] pub fn almide_rt_math_log(x: f64) -> f64 { x.ln() }
#[inline(always)] pub fn almide_rt_math_log2(x: f64) -> f64 { x.log2() }
#[inline(always)] pub fn almide_rt_math_log10(x: f64) -> f64 { x.log10() }
#[inline(always)] pub fn almide_rt_math_exp(x: f64) -> f64 { x.exp() }
#[inline(always)] pub fn almide_rt_math_pow(base: i64, exp: i64) -> i64 { base.pow(exp as u32) }

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

// Float min/max
#[inline(always)] pub fn almide_rt_math_fmin(a: f64, b: f64) -> f64 { a.min(b) }
#[inline(always)] pub fn almide_rt_math_fmax(a: f64, b: f64) -> f64 { a.max(b) }
#[inline(always)] pub fn almide_rt_math_fpow(base: f64, exp: f64) -> f64 { base.powf(exp) }

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
pub fn almide_rt_math_log_gamma(x: f64) -> f64 {
    // Lanczos approximation (g=7, n=9 coefficients)
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
    let t = x + 7.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + ag.ln()
}
