// bytes extern — Rust native implementations
// Signatures match TOML templates: &Vec<u8> for read-only

pub fn almide_rt_bytes_len(b: &Vec<u8>) -> i64 { b.len() as i64 }
pub fn almide_rt_bytes_is_empty(b: &Vec<u8>) -> bool { b.is_empty() }
pub fn almide_rt_bytes_get(b: &Vec<u8>, i: i64) -> Option<i64> { b.get(i as usize).map(|&x| x as i64) }
pub fn almide_rt_bytes_get_or(b: &Vec<u8>, i: i64, default: i64) -> i64 { b.get(i as usize).map(|&x| x as i64).unwrap_or(default) }
pub fn almide_rt_bytes_set(mut b: Vec<u8>, i: i64, val: i64) -> Vec<u8> { if (i as usize) < b.len() { b[i as usize] = val as u8; } b }
pub fn almide_rt_bytes_slice(b: &Vec<u8>, start: i64, end: i64) -> Vec<u8> {
    let s = (start as usize).min(b.len());
    let e = (end as usize).min(b.len());
    if s >= e { Vec::new() } else { b[s..e].to_vec() }
}
pub fn almide_rt_bytes_from_list(xs: &Vec<i64>) -> Vec<u8> { xs.iter().map(|&x| x as u8).collect() }
pub fn almide_rt_bytes_to_list(b: &Vec<u8>) -> Vec<i64> { b.iter().map(|&x| x as i64).collect() }
pub fn almide_rt_bytes_concat(a: &Vec<u8>, b: &Vec<u8>) -> Vec<u8> { let mut r = a.clone(); r.extend_from_slice(b); r }
pub fn almide_rt_bytes_repeat(b: &Vec<u8>, n: i64) -> Vec<u8> { b.repeat(n.max(0) as usize) }
pub fn almide_rt_bytes_new(len: i64) -> Vec<u8> { vec![0u8; len.max(0) as usize] }
