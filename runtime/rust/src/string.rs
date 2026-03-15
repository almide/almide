// string extern — Rust native implementations

pub fn almide_rt_string_len(s: impl AsRef<str>) -> i64 { s.as_ref().chars().count() as i64 }
pub fn almide_rt_string_to_upper(s: impl AsRef<str>) -> String { s.as_ref().to_uppercase() }
pub fn almide_rt_string_to_lower(s: impl AsRef<str>) -> String { s.as_ref().to_lowercase() }
pub fn almide_rt_string_trim(s: impl AsRef<str>) -> String { s.as_ref().trim().to_string() }
pub fn almide_rt_string_contains(s: impl AsRef<str>, sub: impl AsRef<str>) -> bool { s.as_ref().contains(sub.as_ref()) }
pub fn almide_rt_string_starts_with(s: impl AsRef<str>, prefix: impl AsRef<str>) -> bool { s.as_ref().starts_with(prefix.as_ref()) }
pub fn almide_rt_string_ends_with(s: impl AsRef<str>, suffix: impl AsRef<str>) -> bool { s.as_ref().ends_with(suffix.as_ref()) }
pub fn almide_rt_string_split(s: impl AsRef<str>, sep: impl AsRef<str>) -> Vec<String> { s.as_ref().split(sep.as_ref()).map(|x| x.to_string()).collect() }
pub fn almide_rt_string_replace(s: impl AsRef<str>, from: impl AsRef<str>, to: impl AsRef<str>) -> String { s.as_ref().replace(from.as_ref(), to.as_ref()) }
pub fn almide_rt_string_join(parts: impl AsRef<Vec<String>>, sep: impl AsRef<str>) -> String { parts.as_ref().join(sep.as_ref()) }

pub fn almide_rt_string_slice(s: impl AsRef<str>, start: i64, end: i64) -> String {
    let chars: Vec<char> = s.as_ref().chars().collect();
    let start = start.max(0) as usize;
    let end = end.min(chars.len() as i64) as usize;
    if start >= end { return String::new(); }
    chars[start..end].iter().collect()
}

pub fn almide_rt_string_trim_start(s: impl AsRef<str>) -> String { s.as_ref().trim_start().to_string() }
pub fn almide_rt_string_trim_end(s: impl AsRef<str>) -> String { s.as_ref().trim_end().to_string() }
pub fn almide_rt_string_repeat(s: impl AsRef<str>, n: i64) -> String { s.as_ref().repeat(n as usize) }
pub fn almide_rt_string_reverse(s: impl AsRef<str>) -> String { s.as_ref().chars().rev().collect() }

pub fn almide_rt_string_chars(s: impl AsRef<str>) -> Vec<String> {
    s.as_ref().chars().map(|c| c.to_string()).collect()
}

pub fn almide_rt_string_char_at(s: impl AsRef<str>, i: i64) -> Option<String> {
    s.as_ref().chars().nth(i as usize).map(|c| c.to_string())
}

pub fn almide_rt_string_char_count(s: impl AsRef<str>) -> i64 { s.as_ref().chars().count() as i64 }

pub fn almide_rt_string_index_of(s: impl AsRef<str>, sub: impl AsRef<str>) -> Option<i64> {
    let s = s.as_ref();
    s.find(sub.as_ref()).map(|i| s[..i].chars().count() as i64)
}

pub fn almide_rt_string_last_index_of(s: impl AsRef<str>, sub: impl AsRef<str>) -> Option<i64> {
    let s = s.as_ref();
    s.rfind(sub.as_ref()).map(|i| s[..i].chars().count() as i64)
}

pub fn almide_rt_string_count(s: impl AsRef<str>, sub: impl AsRef<str>) -> i64 { s.as_ref().matches(sub.as_ref()).count() as i64 }

pub fn almide_rt_string_lines(s: impl AsRef<str>) -> Vec<String> {
    s.as_ref().lines().map(|l| l.to_string()).collect()
}

pub fn almide_rt_string_pad_left(s: impl AsRef<str>, width: i64, pad: impl AsRef<str>) -> String {
    let s = s.as_ref();
    let w = width as usize;
    let len = s.chars().count();
    if len >= w { return s.to_string(); }
    let pad_ch = pad.as_ref().chars().next().unwrap_or(' ');
    let padding: String = std::iter::repeat(pad_ch).take(w - len).collect();
    format!("{}{}", padding, s)
}

pub fn almide_rt_string_pad_right(s: impl AsRef<str>, width: i64, pad: impl AsRef<str>) -> String {
    let s = s.as_ref();
    let w = width as usize;
    let len = s.chars().count();
    if len >= w { return s.to_string(); }
    let pad_ch = pad.as_ref().chars().next().unwrap_or(' ');
    let padding: String = std::iter::repeat(pad_ch).take(w - len).collect();
    format!("{}{}", s, padding)
}

pub fn almide_rt_string_replace_first(s: impl AsRef<str>, from: impl AsRef<str>, to: impl AsRef<str>) -> String {
    let s = s.as_ref(); let from = from.as_ref(); let to = to.as_ref();
    if let Some(pos) = s.find(from) { format!("{}{}{}", &s[..pos], to, &s[pos + from.len()..]) } else { s.to_string() }
}

pub fn almide_rt_string_strip_prefix(s: impl AsRef<str>, prefix: impl AsRef<str>) -> Option<String> {
    s.as_ref().strip_prefix(prefix.as_ref()).map(|r| r.to_string())
}

pub fn almide_rt_string_strip_suffix(s: impl AsRef<str>, suffix: impl AsRef<str>) -> Option<String> {
    s.as_ref().strip_suffix(suffix.as_ref()).map(|r| r.to_string())
}

pub fn almide_rt_string_is_empty(s: impl AsRef<str>) -> bool { s.as_ref().is_empty() }
pub fn almide_rt_string_is_whitespace(s: impl AsRef<str>) -> bool { s.as_ref().chars().all(|c| c.is_whitespace()) }
pub fn almide_rt_string_is_alpha(s: impl AsRef<str>) -> bool { let s = s.as_ref(); !s.is_empty() && s.chars().all(|c| c.is_alphabetic()) }
pub fn almide_rt_string_is_digit(s: impl AsRef<str>) -> bool { let s = s.as_ref(); !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) }
pub fn almide_rt_string_is_alphanumeric(s: impl AsRef<str>) -> bool { let s = s.as_ref(); !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()) }
pub fn almide_rt_string_is_upper(s: impl AsRef<str>) -> bool { let s = s.as_ref(); !s.is_empty() && s.chars().all(|c| c.is_uppercase()) }
pub fn almide_rt_string_is_lower(s: impl AsRef<str>) -> bool { let s = s.as_ref(); !s.is_empty() && s.chars().all(|c| c.is_lowercase()) }

pub fn almide_rt_string_capitalize(s: impl AsRef<str>) -> String {
    let s = s.as_ref();
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => format!("{}{}", c.to_uppercase(), chars.collect::<String>()),
        None => String::new(),
    }
}

pub fn almide_rt_string_to_int(s: impl AsRef<str>) -> Result<i64, String> { s.as_ref().trim().parse::<i64>().map_err(|e| e.to_string()) }
pub fn almide_rt_string_to_float(s: impl AsRef<str>) -> Result<f64, String> { s.as_ref().trim().parse::<f64>().map_err(|e| e.to_string()) }

pub fn almide_rt_string_to_bytes(s: impl AsRef<str>) -> Vec<i64> { s.as_ref().bytes().map(|b| b as i64).collect() }

pub fn almide_rt_string_from_bytes(bytes: impl AsRef<Vec<i64>>) -> String {
    bytes.as_ref().iter().map(|&b| b as u8 as char).collect()
}

pub fn almide_rt_string_codepoint(s: impl AsRef<str>) -> Option<i64> {
    s.as_ref().chars().next().map(|c| c as i64)
}

pub fn almide_rt_string_from_codepoint(cp: i64) -> String {
    char::from_u32(cp as u32).map(|c| c.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_upper() { assert_eq!(almide_rt_string_to_upper("hello"), "HELLO"); }

    #[test]
    fn test_len() { assert_eq!(almide_rt_string_len("hello"), 5); }

    #[test]
    fn test_contains() { assert!(almide_rt_string_contains("hello world", "world")); }
}
