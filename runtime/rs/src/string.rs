// string extern — Rust native implementations
// TOML templates use &*{s} which dereferences String to &str

pub fn almide_rt_string_len(s: &str) -> i64 { s.chars().count() as i64 }
pub fn almide_rt_string_to_upper(s: &str) -> String { s.to_uppercase() }
pub fn almide_rt_string_to_lower(s: &str) -> String { s.to_lowercase() }
pub fn almide_rt_string_trim(s: &str) -> String { s.trim().to_string() }
pub fn almide_rt_string_trim_start(s: &str) -> String { s.trim_start().to_string() }
pub fn almide_rt_string_trim_end(s: &str) -> String { s.trim_end().to_string() }
pub fn almide_rt_string_contains(s: &str, sub: &str) -> bool { s.contains(sub) }
pub fn almide_rt_string_starts_with(s: &str, prefix: &str) -> bool { s.starts_with(prefix) }
pub fn almide_rt_string_ends_with(s: &str, suffix: &str) -> bool { s.ends_with(suffix) }
pub fn almide_rt_string_split(s: &str, sep: &str) -> Vec<String> { s.split(sep).map(|x| x.to_string()).collect() }
pub fn almide_rt_string_replace(s: &str, from: &str, to: &str) -> String { s.replace(from, to) }
pub fn almide_rt_string_join(parts: &Vec<String>, sep: &str) -> String { parts.join(sep) }
pub fn almide_rt_string_repeat(s: &str, n: i64) -> String { s.repeat(n as usize) }
pub fn almide_rt_string_reverse(s: &str) -> String { s.chars().rev().collect() }
pub fn almide_rt_string_chars(s: &str) -> Vec<String> { s.chars().map(|c| c.to_string()).collect() }
pub fn almide_rt_string_char_at(s: &str, i: i64) -> Option<String> { s.chars().nth(i as usize).map(|c| c.to_string()) }
pub fn almide_rt_string_char_count(s: &str) -> i64 { s.chars().count() as i64 }
pub fn almide_rt_string_index_of(s: &str, sub: &str) -> Option<i64> { s.find(sub).map(|i| s[..i].chars().count() as i64) }
pub fn almide_rt_string_last_index_of(s: &str, sub: &str) -> Option<i64> { s.rfind(sub).map(|i| s[..i].chars().count() as i64) }
pub fn almide_rt_string_count(s: &str, sub: &str) -> i64 { s.matches(sub).count() as i64 }
pub fn almide_rt_string_lines(s: &str) -> Vec<String> { s.lines().map(|l| l.to_string()).collect() }
pub fn almide_rt_string_is_empty(s: &str) -> bool { s.is_empty() }
pub fn almide_rt_string_is_whitespace(s: &str) -> bool { s.chars().all(|c| c.is_whitespace()) }
pub fn almide_rt_string_is_alpha(s: &str) -> bool { !s.is_empty() && s.chars().all(|c| c.is_alphabetic()) }
pub fn almide_rt_string_is_digit(s: &str) -> bool { !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) }
pub fn almide_rt_string_is_alphanumeric(s: &str) -> bool { !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()) }
pub fn almide_rt_string_is_upper(s: &str) -> bool { !s.is_empty() && s.chars().any(|c| c.is_alphabetic()) && s.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) }
pub fn almide_rt_string_is_lower(s: &str) -> bool { !s.is_empty() && s.chars().any(|c| c.is_alphabetic()) && s.chars().all(|c| !c.is_alphabetic() || c.is_lowercase()) }
pub fn almide_rt_string_capitalize(s: &str) -> String { let mut c = s.chars(); match c.next() { Some(f) => format!("{}{}", f.to_uppercase(), c.collect::<String>()), None => String::new() } }
pub fn almide_rt_string_to_int(s: &str) -> Result<i64, String> { s.trim().parse::<i64>().map_err(|e| e.to_string()) }
pub fn almide_rt_string_to_float(s: &str) -> Result<f64, String> { s.trim().parse::<f64>().map_err(|e| e.to_string()) }
pub fn almide_rt_string_to_bytes(s: &str) -> Vec<i64> { s.bytes().map(|b| b as i64).collect() }
pub fn almide_rt_string_from_bytes(bytes: &Vec<i64>) -> String { bytes.iter().map(|&b| b as u8 as char).collect() }
pub fn almide_rt_string_codepoint(s: &str) -> Option<i64> { s.chars().next().map(|c| c as i64) }
pub fn almide_rt_string_from_codepoint(cp: i64) -> String { char::from_u32(cp as u32).map(|c| c.to_string()).unwrap_or_default() }

pub fn almide_rt_string_slice(s: &str, start: i64, end: Option<i64>) -> String {
    let chars: Vec<char> = s.chars().collect();
    let s = start.max(0) as usize;
    let e = end.unwrap_or(chars.len() as i64).min(chars.len() as i64) as usize;
    if s >= e { String::new() } else { chars[s..e].iter().collect() }
}

pub fn almide_rt_string_pad_left(s: &str, width: i64, pad: &str) -> String {
    let w = width as usize; let len = s.chars().count();
    if len >= w { return s.to_string(); }
    let p = pad.chars().next().unwrap_or(' ');
    format!("{}{}", std::iter::repeat(p).take(w - len).collect::<String>(), s)
}

pub fn almide_rt_string_pad_right(s: &str, width: i64, pad: &str) -> String {
    let w = width as usize; let len = s.chars().count();
    if len >= w { return s.to_string(); }
    let p = pad.chars().next().unwrap_or(' ');
    format!("{}{}", s, std::iter::repeat(p).take(w - len).collect::<String>())
}

pub fn almide_rt_string_replace_first(s: &str, from: &str, to: &str) -> String {
    if let Some(pos) = s.find(from) { format!("{}{}{}", &s[..pos], to, &s[pos + from.len()..]) } else { s.to_string() }
}

pub fn almide_rt_string_strip_prefix(s: &str, prefix: &str) -> Option<String> { s.strip_prefix(prefix).map(|r| r.to_string()) }
pub fn almide_rt_string_strip_suffix(s: &str, suffix: &str) -> Option<String> { s.strip_suffix(suffix).map(|r| r.to_string()) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_len() { assert_eq!(almide_rt_string_len("hello"), 5); }
    #[test] fn test_contains() { assert!(almide_rt_string_contains("hello world", "world")); }
}
