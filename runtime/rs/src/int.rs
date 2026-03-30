// int extern — Rust native implementations
// Tested by: cargo test in stdlib/

pub fn almide_rt_int_to_string(n: i64) -> String {
    n.to_string()
}

pub fn almide_rt_int_from_string(s: String) -> Result<i64, String> {
    s.trim().parse::<i64>().map_err(|e| format!("invalid integer: {}", e))
}

pub fn almide_rt_int_abs(n: i64) -> i64 { n.abs() }
pub fn almide_rt_int_min(a: i64, b: i64) -> i64 { a.min(b) }
pub fn almide_rt_int_max(a: i64, b: i64) -> i64 { a.max(b) }
pub fn almide_rt_int_clamp(n: i64, lo: i64, hi: i64) -> i64 { n.clamp(lo, hi) }
pub fn almide_rt_int_to_float(n: i64) -> f64 { n as f64 }
pub fn almide_rt_int_to_hex(n: i64) -> String { format!("{:x}", n) }
pub fn almide_rt_int_to_u(n: i64) -> i64 { n.unsigned_abs() as i64 }
pub fn almide_rt_int_parse(s: &str) -> Result<i64, String> { s.trim().parse::<i64>().map_err(|e| e.to_string()) }
pub fn almide_rt_int_parse_hex(s: &str) -> Result<i64, String> { i64::from_str_radix(s.trim().trim_start_matches("0x"), 16).map_err(|e| e.to_string()) }
pub fn almide_rt_int_band(a: i64, b: i64) -> i64 { a & b }
pub fn almide_rt_int_bor(a: i64, b: i64) -> i64 { a | b }
pub fn almide_rt_int_bxor(a: i64, b: i64) -> i64 { a ^ b }
pub fn almide_rt_int_bnot(n: i64) -> i64 { !n }
pub fn almide_rt_int_bshl(n: i64, bits: i64) -> i64 { n << bits }
pub fn almide_rt_int_bshr(n: i64, bits: i64) -> i64 { n >> bits }
pub fn almide_rt_int_rotate_left(a: i64, n: i64, bits: i64) -> i64 {
    let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
    let v = (a as u64) & mask;
    let n = (n % bits) as u32;
    ((v << n) | (v >> (bits as u32 - n))) as i64 & mask as i64
}
pub fn almide_rt_int_rotate_right(a: i64, n: i64, bits: i64) -> i64 {
    let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
    let v = (a as u64) & mask;
    let n = (n % bits) as u32;
    ((v >> n) | (v << (bits as u32 - n))) as i64 & mask as i64
}
pub fn almide_rt_int_wrap_add(a: i64, b: i64, bits: i64) -> i64 {
    let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
    ((a as u64).wrapping_add(b as u64) & mask) as i64
}
pub fn almide_rt_int_wrap_mul(a: i64, b: i64, bits: i64) -> i64 {
    let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
    ((a as u64).wrapping_mul(b as u64) & mask) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_string() {
        assert_eq!(almide_rt_int_to_string(42), "42");
        assert_eq!(almide_rt_int_to_string(-1), "-1");
        assert_eq!(almide_rt_int_to_string(0), "0");
    }

    #[test]
    fn test_from_string() {
        assert_eq!(almide_rt_int_from_string("42".into()), Ok(42));
        assert_eq!(almide_rt_int_from_string("-1".into()), Ok(-1));
        assert!(almide_rt_int_from_string("abc".into()).is_err());
        assert!(almide_rt_int_from_string("".into()).is_err());
    }
}

pub fn almide_rt_int_to_u32(n: i64) -> i64 { (n as u32) as i64 }
pub fn almide_rt_int_to_u8(n: i64) -> i64 { (n as u8) as i64 }
