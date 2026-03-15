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

pub fn almide_rt_string_join(parts: Vec<String>, sep: String) -> String {
    parts.join(&sep)
}

pub fn almide_rt_string_slice(s: String, start: i64, end: i64) -> String {
    let chars: Vec<char> = s.chars().collect();
    let start = start.max(0) as usize;
    let end = end.min(chars.len() as i64) as usize;
    if start >= end { return String::new(); }
    chars[start..end].iter().collect()
}

pub fn almide_rt_string_trim_start(s: String) -> String { s.trim_start().to_string() }
pub fn almide_rt_string_trim_end(s: String) -> String { s.trim_end().to_string() }

pub fn almide_rt_string_repeat(s: String, n: i64) -> String { s.repeat(n as usize) }

pub fn almide_rt_string_reverse(s: String) -> String { s.chars().rev().collect() }

pub fn almide_rt_string_chars(s: String) -> Vec<String> {
    s.chars().map(|c| c.to_string()).collect()
}

pub fn almide_rt_string_char_at(s: String, i: i64) -> Option<String> {
    s.chars().nth(i as usize).map(|c| c.to_string())
}

pub fn almide_rt_string_char_count(s: String) -> i64 { s.chars().count() as i64 }

pub fn almide_rt_string_index_of(s: String, sub: String) -> Option<i64> {
    s.find(&sub).map(|i| s[..i].chars().count() as i64)
}

pub fn almide_rt_string_last_index_of(s: String, sub: String) -> Option<i64> {
    s.rfind(&sub).map(|i| s[..i].chars().count() as i64)
}

pub fn almide_rt_string_count(s: String, sub: String) -> i64 {
    s.matches(&sub).count() as i64
}

pub fn almide_rt_string_lines(s: String) -> Vec<String> {
    s.lines().map(|l| l.to_string()).collect()
}

pub fn almide_rt_string_pad_left(s: String, width: i64, pad: String) -> String {
    let w = width as usize;
    let len = s.chars().count();
    if len >= w { return s; }
    let pad_ch = pad.chars().next().unwrap_or(' ');
    let padding: String = std::iter::repeat(pad_ch).take(w - len).collect();
    format!("{}{}", padding, s)
}

pub fn almide_rt_string_pad_right(s: String, width: i64, pad: String) -> String {
    let w = width as usize;
    let len = s.chars().count();
    if len >= w { return s; }
    let pad_ch = pad.chars().next().unwrap_or(' ');
    let padding: String = std::iter::repeat(pad_ch).take(w - len).collect();
    format!("{}{}", s, padding)
}

pub fn almide_rt_string_replace_first(s: String, from: String, to: String) -> String {
    if let Some(pos) = s.find(&from) {
        format!("{}{}{}", &s[..pos], to, &s[pos + from.len()..])
    } else { s }
}

pub fn almide_rt_string_strip_prefix(s: String, prefix: String) -> Option<String> {
    s.strip_prefix(prefix.as_str()).map(|r| r.to_string())
}

pub fn almide_rt_string_strip_suffix(s: String, suffix: String) -> Option<String> {
    s.strip_suffix(suffix.as_str()).map(|r| r.to_string())
}

pub fn almide_rt_string_is_empty(s: String) -> bool { s.is_empty() }
pub fn almide_rt_string_is_whitespace(s: String) -> bool { s.chars().all(|c| c.is_whitespace()) }
pub fn almide_rt_string_is_alpha(s: String) -> bool { !s.is_empty() && s.chars().all(|c| c.is_alphabetic()) }
pub fn almide_rt_string_is_digit(s: String) -> bool { !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) }
pub fn almide_rt_string_is_alphanumeric(s: String) -> bool { !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()) }
pub fn almide_rt_string_is_upper(s: String) -> bool { !s.is_empty() && s.chars().all(|c| c.is_uppercase()) }
pub fn almide_rt_string_is_lower(s: String) -> bool { !s.is_empty() && s.chars().all(|c| c.is_lowercase()) }

pub fn almide_rt_string_capitalize(s: String) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => format!("{}{}", c.to_uppercase(), chars.collect::<String>()),
        None => String::new(),
    }
}

pub fn almide_rt_string_to_int(s: String) -> Result<i64, String> {
    s.trim().parse::<i64>().map_err(|e| e.to_string())
}

pub fn almide_rt_string_to_float(s: String) -> Result<f64, String> {
    s.trim().parse::<f64>().map_err(|e| e.to_string())
}

pub fn almide_rt_string_to_bytes(s: String) -> Vec<i64> {
    s.bytes().map(|b| b as i64).collect()
}

pub fn almide_rt_string_from_bytes(bytes: Vec<i64>) -> String {
    bytes.iter().map(|&b| b as u8 as char).collect()
}

pub fn almide_rt_string_codepoint(s: String) -> Option<i64> {
    s.chars().next().map(|c| c as i64)
}

pub fn almide_rt_string_from_codepoint(cp: i64) -> String {
    char::from_u32(cp as u32).map(|c| c.to_string()).unwrap_or_default()
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
