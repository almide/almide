// int extern — Rust native implementations
// Tested by: cargo test in stdlib/

pub fn almide_rt_int_to_string(n: i64) -> String {
    n.to_string()
}

pub fn almide_rt_int_from_string(s: String) -> Result<i64, String> {
    s.trim().parse::<i64>().map_err(|e| format!("invalid integer: {}", e))
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
