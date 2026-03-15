// float extern — Rust native implementations

pub fn almide_rt_float_to_string(n: f64) -> String { format!("{}", n) }
pub fn almide_rt_float_parse(s: String) -> Result<f64, String> { s.trim().parse::<f64>().map_err(|e| e.to_string()) }
pub fn almide_rt_float_abs(n: f64) -> f64 { n.abs() }
pub fn almide_rt_float_ceil(n: f64) -> f64 { n.ceil() }
pub fn almide_rt_float_floor(n: f64) -> f64 { n.floor() }
pub fn almide_rt_float_round(n: f64) -> f64 { n.round() }
pub fn almide_rt_float_sqrt(n: f64) -> f64 { n.sqrt() }
pub fn almide_rt_float_min(a: f64, b: f64) -> f64 { a.min(b) }
pub fn almide_rt_float_max(a: f64, b: f64) -> f64 { a.max(b) }
pub fn almide_rt_float_clamp(n: f64, lo: f64, hi: f64) -> f64 { n.clamp(lo, hi) }
pub fn almide_rt_float_sign(n: f64) -> f64 { n.signum() }
pub fn almide_rt_float_to_int(n: f64) -> i64 { n as i64 }
pub fn almide_rt_float_from_int(n: i64) -> f64 { n as f64 }
pub fn almide_rt_float_to_fixed(n: f64, decimals: i64) -> String { format!("{:.1$}", n, decimals as usize) }

// math module functions (delegated to float operations)
pub fn almide_rt_math_abs(x: f64) -> f64 { x.abs() }
pub fn almide_rt_math_ceil(x: f64) -> f64 { x.ceil() }
pub fn almide_rt_math_floor(x: f64) -> f64 { x.floor() }
pub fn almide_rt_math_round(x: f64) -> f64 { x.round() }
pub fn almide_rt_math_sqrt(x: f64) -> f64 { x.sqrt() }
pub fn almide_rt_math_sin(x: f64) -> f64 { x.sin() }
pub fn almide_rt_math_cos(x: f64) -> f64 { x.cos() }
pub fn almide_rt_math_tan(x: f64) -> f64 { x.tan() }
pub fn almide_rt_math_asin(x: f64) -> f64 { x.asin() }
pub fn almide_rt_math_acos(x: f64) -> f64 { x.acos() }
pub fn almide_rt_math_atan(x: f64) -> f64 { x.atan() }
pub fn almide_rt_math_atan2(y: f64, x: f64) -> f64 { y.atan2(x) }
pub fn almide_rt_math_log(x: f64) -> f64 { x.ln() }
pub fn almide_rt_math_log2(x: f64) -> f64 { x.log2() }
pub fn almide_rt_math_log10(x: f64) -> f64 { x.log10() }
pub fn almide_rt_math_exp(x: f64) -> f64 { x.exp() }
pub fn almide_rt_math_pow(base: f64, exp: f64) -> f64 { base.powf(exp) }
pub fn almide_rt_math_pi() -> f64 { std::f64::consts::PI }
pub fn almide_rt_math_e() -> f64 { std::f64::consts::E }
pub fn almide_rt_math_inf() -> f64 { f64::INFINITY }
pub fn almide_rt_math_is_nan(x: f64) -> bool { x.is_nan() }
