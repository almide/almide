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
