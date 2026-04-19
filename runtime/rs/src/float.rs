// float extern — Rust native implementations

pub fn almide_rt_float_to_string(n: f64) -> String {
    let s = format!("{}", n);
    if n.fract() == 0.0 && !s.contains('.') && !s.contains("inf") && !s.contains("NaN") {
        format!("{}.0", s)
    } else {
        s
    }
}
pub fn almide_rt_float_parse(s: &str) -> Result<f64, String> { s.trim().parse::<f64>().map_err(|e| e.to_string()) }
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
pub fn almide_rt_float_is_nan(n: f64) -> bool { n.is_nan() }
pub fn almide_rt_float_is_infinite(n: f64) -> bool { n.is_infinite() }
pub fn almide_rt_float_to_bits(f: f64) -> i64 { f.to_bits() as i64 }

// ── Sized conversions (for `@intrinsic` migration of `float.to_*`) ──

pub fn almide_rt_float_to_int8(n: f64) -> i8 { n as i8 }
pub fn almide_rt_float_to_int16(n: f64) -> i16 { n as i16 }
pub fn almide_rt_float_to_int32(n: f64) -> i32 { n as i32 }
pub fn almide_rt_float_to_int64(n: f64) -> i64 { n as i64 }
pub fn almide_rt_float_to_uint8(n: f64) -> u8 { n as u8 }
pub fn almide_rt_float_to_uint16(n: f64) -> u16 { n as u16 }
pub fn almide_rt_float_to_uint32(n: f64) -> u32 { n as u32 }
pub fn almide_rt_float_to_uint64(n: f64) -> u64 { n as u64 }
pub fn almide_rt_float_to_float32(n: f64) -> f32 { n as f32 }
pub fn almide_rt_float_to_float64(n: f64) -> f64 { n }
