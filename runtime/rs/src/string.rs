// string extern — Rust native implementations
// TOML templates use &*{s} which dereferences String to &str

// Codepoint count, per the documented contract ("number of characters").
// The whole position API (len / index_of / last_index_of / get / slice /
// take / drop) is CODEPOINT-indexed — one unit, no byte/char mixing (#419).
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
pub fn almide_rt_string_join(parts: &[String], sep: &str) -> String { parts.join(sep) }
// Negative counts clamp to 0 (C-054 discipline; `n as usize` on a negative
// i64 reinterpreted to a huge count — a native "capacity overflow" panic
// while the wasm leg trapped on the negative alloc size).
pub fn almide_rt_string_repeat(s: &str, n: i64) -> String { s.repeat(n.max(0) as usize) }
pub fn almide_rt_string_reverse(s: &str) -> String { s.chars().rev().collect() }
pub fn almide_rt_string_chars(s: &str) -> Vec<String> { s.chars().map(|c| c.to_string()).collect() }
pub fn almide_rt_string_char_at(s: &str, i: i64) -> Option<String> {
    if i < 0 { return None; }
    s.chars().nth(i as usize).map(|c| c.to_string())
}
pub fn almide_rt_string_char_count(s: &str) -> i64 { s.chars().count() as i64 }
pub fn almide_rt_string_index_of(s: &str, sub: &str) -> Option<i64> { s.find(sub).map(|b| s[..b].chars().count() as i64) }
pub fn almide_rt_string_last_index_of(s: &str, sub: &str) -> Option<i64> { s.rfind(sub).map(|b| s[..b].chars().count() as i64) }
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
// UTF-8 lossy decode — the inverse of `to_bytes` (which emits UTF-8), matching
// Almide's `bytes.to_string_lossy` and every other language's bytes→string. (Was
// a Latin-1 `b as u8 as char` map, which broke the round-trip on all non-ASCII.)
// Each i64 is truncated to a byte (`b as u8`, preserving the prior wrap), then the
// byte sequence is decoded as UTF-8 with U+FFFD for each maximal invalid subpart.
pub fn almide_rt_string_from_bytes(bytes: &[i64]) -> String {
    let v: Vec<u8> = bytes.iter().map(|&b| b as u8).collect();
    String::from_utf8_lossy(&v).into_owned()
}
pub fn almide_rt_string_codepoint(s: &str) -> Option<i64> { s.chars().next().map(|c| c as i64) }
pub fn almide_rt_string_from_codepoint(cp: i64) -> String { char::from_u32(cp as u32).map(|c| c.to_string()).unwrap_or_default() }

pub fn almide_rt_string_slice(s: &str, start: i64, end: i64) -> String {
    // CODEPOINT indices, clamped to [0, char_count]; the `end = i64::MAX`
    // default degrades to "to the end".
    let count = s.chars().count();
    let s_idx = (start.max(0) as usize).min(count);
    let e_idx = (end.max(0) as usize).min(count);
    if s_idx >= e_idx { String::new() }
    else { s.chars().skip(s_idx).take(e_idx - s_idx).collect() }
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

pub fn almide_rt_string_first(s: &str) -> Option<String> {
    s.chars().next().map(|c| c.to_string())
}
pub fn almide_rt_string_last(s: &str) -> Option<String> {
    s.chars().last().map(|c| c.to_string())
}
pub fn almide_rt_string_take(s: &str, n: i64) -> String {
    s.chars().take(n as usize).collect()
}
pub fn almide_rt_string_take_end(s: &str, n: i64) -> String {
    let chars: Vec<char> = s.chars().collect();
    let start = if n as usize >= chars.len() { 0 } else { chars.len() - n as usize };
    chars[start..].iter().collect()
}
pub fn almide_rt_string_drop(s: &str, n: i64) -> String {
    s.chars().skip(n as usize).collect()
}
pub fn almide_rt_string_drop_end(s: &str, n: i64) -> String {
    let chars: Vec<char> = s.chars().collect();
    let end = if n as usize >= chars.len() { 0 } else { chars.len() - n as usize };
    chars[..end].iter().collect()
}

// ── Algorithmic primitives (Phase 3 stdlib expansion) ──

pub fn almide_rt_string_run_length_encode(s: &str) -> Vec<(String, i64)> {
    let mut result: Vec<(String, i64)> = Vec::new();
    for c in s.chars() {
        let cs: String = c.to_string();
        match result.last_mut() {
            Some((prev, count)) if *prev == cs => { *count += 1; }
            _ => { result.push((cs, 1)); }
        }
    }
    result
}

// ── Mutable string operations ──

pub fn almide_rt_string_push(s: &mut String, suffix: &str) { s.push_str(suffix); }
pub fn almide_rt_string_push_char(s: &mut String, ch: &str) { s.push_str(ch); }
pub fn almide_rt_string_clear(s: &mut String) { s.clear(); }
