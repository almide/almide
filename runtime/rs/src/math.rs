// math extern — Rust native implementations

// Trigonometry
pub fn almide_rt_math_sin(x: f64) -> f64 { x.sin() }
pub fn almide_rt_math_cos(x: f64) -> f64 { x.cos() }
pub fn almide_rt_math_tan(x: f64) -> f64 { x.tan() }
pub fn almide_rt_math_asin(x: f64) -> f64 { x.asin() }
pub fn almide_rt_math_acos(x: f64) -> f64 { x.acos() }
pub fn almide_rt_math_atan(x: f64) -> f64 { x.atan() }
pub fn almide_rt_math_atan2(y: f64, x: f64) -> f64 { y.atan2(x) }

// Logarithms / exponentials
pub fn almide_rt_math_log(x: f64) -> f64 { x.ln() }
pub fn almide_rt_math_log2(x: f64) -> f64 { x.log2() }
pub fn almide_rt_math_log10(x: f64) -> f64 { x.log10() }
pub fn almide_rt_math_exp(x: f64) -> f64 { x.exp() }
pub fn almide_rt_math_pow(base: f64, exp: f64) -> f64 { base.powf(exp) }

// Rounding
pub fn almide_rt_math_abs(x: i64) -> i64 { x.abs() }
pub fn almide_rt_math_ceil(x: f64) -> f64 { x.ceil() }
pub fn almide_rt_math_floor(x: f64) -> f64 { x.floor() }
pub fn almide_rt_math_round(x: f64) -> f64 { x.round() }
pub fn almide_rt_math_sqrt(x: f64) -> f64 { x.sqrt() }

// Constants
pub fn almide_rt_math_pi() -> f64 { std::f64::consts::PI }
pub fn almide_rt_math_e() -> f64 { std::f64::consts::E }
pub fn almide_rt_math_inf() -> f64 { f64::INFINITY }
pub fn almide_rt_math_is_nan(x: f64) -> bool { x.is_nan() }

// Int min/max/sign
pub fn almide_rt_math_min(a: i64, b: i64) -> i64 { a.min(b) }
pub fn almide_rt_math_max(a: i64, b: i64) -> i64 { a.max(b) }
pub fn almide_rt_math_sign(n: i64) -> i64 { if n > 0 { 1 } else if n < 0 { -1 } else { 0 } }

// Float min/max
pub fn almide_rt_math_fmin(a: f64, b: f64) -> f64 { a.min(b) }
pub fn almide_rt_math_fmax(a: f64, b: f64) -> f64 { a.max(b) }
pub fn almide_rt_math_fpow(base: f64, exp: f64) -> f64 { base.powf(exp) }

// Combinatorics
pub fn almide_rt_math_factorial(n: i64) -> i64 {
    if n <= 1 { 1 } else { (2..=n).product() }
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
    // Stirling's approximation for ln(Gamma(x))
    if x <= 0.0 { return f64::INFINITY; }
    let x = x - 1.0;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * x.ln() - x
        + 1.0 / (12.0 * x) - 1.0 / (360.0 * x * x * x)
}
