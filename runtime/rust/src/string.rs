// string extern — Rust native implementations

pub fn almide_rt_string_len(s: String) -> i64 {
    s.chars().count() as i64
}

pub fn almide_rt_string_to_upper(s: String) -> String {
    s.to_uppercase()
}

pub fn almide_rt_string_to_lower(s: String) -> String {
    s.to_lowercase()
}

pub fn almide_rt_string_trim(s: String) -> String {
    s.trim().to_string()
}

pub fn almide_rt_string_contains(s: String, sub: String) -> bool {
    s.contains(&sub)
}

pub fn almide_rt_string_starts_with(s: String, prefix: String) -> bool {
    s.starts_with(&prefix)
}

pub fn almide_rt_string_ends_with(s: String, suffix: String) -> bool {
    s.ends_with(&suffix)
}

pub fn almide_rt_string_split(s: String, sep: String) -> Vec<String> {
    s.split(&sep).map(|x| x.to_string()).collect()
}

pub fn almide_rt_string_replace(s: String, from: String, to: String) -> String {
    s.replace(&from, &to)
}

pub fn almide_rt_string_slice(s: String, start: i64, end: i64) -> String {
    let chars: Vec<char> = s.chars().collect();
    let start = start.max(0) as usize;
    let end = end.min(chars.len() as i64) as usize;
    if start >= end { return String::new(); }
    chars[start..end].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_upper() {
        assert_eq!(almide_rt_string_to_upper("hello".into()), "HELLO");
    }

    #[test]
    fn test_len() {
        assert_eq!(almide_rt_string_len("hello".into()), 5);
        assert_eq!(almide_rt_string_len("".into()), 0);
    }

    #[test]
    fn test_contains() {
        assert!(almide_rt_string_contains("hello world".into(), "world".into()));
        assert!(!almide_rt_string_contains("hello".into(), "xyz".into()));
    }
}
