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
pub fn almide_rt_bytes_push(b: &mut Vec<u8>, val: i64) { b.push(val as u8); }
pub fn almide_rt_bytes_clear(b: &mut Vec<u8>) { b.clear(); }
pub fn almide_rt_bytes_from_string(s: &str) -> Vec<u8> { s.as_bytes().to_vec() }

// ── Bridge protocol: big-endian pack/unpack ──

pub fn almide_rt_bytes_write_i64_be(b: &mut Vec<u8>, val: i64) { b.extend_from_slice(&val.to_be_bytes()); }
pub fn almide_rt_bytes_write_f64_be(b: &mut Vec<u8>, val: f64) { b.extend_from_slice(&val.to_be_bytes()); }
pub fn almide_rt_bytes_write_u32_be(b: &mut Vec<u8>, val: i64) { b.extend_from_slice(&(val as u32).to_be_bytes()); }
pub fn almide_rt_bytes_write_u8(b: &mut Vec<u8>, val: i64) { b.push(val as u8); }
pub fn almide_rt_bytes_write_string_be(b: &mut Vec<u8>, s: &str) {
    let sb = s.as_bytes();
    b.extend_from_slice(&(sb.len() as u32).to_be_bytes());
    b.extend_from_slice(sb);
}
pub fn almide_rt_bytes_write_bool(b: &mut Vec<u8>, val: bool) { b.push(if val { 1 } else { 0 }); }

pub fn almide_rt_bytes_read_i64_be(b: &Vec<u8>, pos: i64) -> i64 {
    let p = pos as usize;
    if p + 8 > b.len() { return 0; }
    i64::from_be_bytes(b[p..p+8].try_into().unwrap())
}
pub fn almide_rt_bytes_read_f64_be(b: &Vec<u8>, pos: i64) -> f64 {
    let p = pos as usize;
    if p + 8 > b.len() { return 0.0; }
    f64::from_be_bytes(b[p..p+8].try_into().unwrap())
}
pub fn almide_rt_bytes_read_u32_be(b: &Vec<u8>, pos: i64) -> i64 {
    let p = pos as usize;
    if p + 4 > b.len() { return 0; }
    u32::from_be_bytes(b[p..p+4].try_into().unwrap()) as i64
}
pub fn almide_rt_bytes_read_u8(b: &Vec<u8>, pos: i64) -> i64 {
    b.get(pos as usize).map(|&x| x as i64).unwrap_or(0)
}
pub fn almide_rt_bytes_read_bool(b: &Vec<u8>, pos: i64) -> bool {
    b.get(pos as usize).map(|&x| x != 0).unwrap_or(false)
}
pub fn almide_rt_bytes_read_string_be(b: &Vec<u8>, pos: i64) -> String {
    let p = pos as usize;
    if p + 4 > b.len() { return String::new(); }
    let slen = u32::from_be_bytes(b[p..p+4].try_into().unwrap()) as usize;
    if p + 4 + slen > b.len() { return String::new(); }
    String::from_utf8_lossy(&b[p+4..p+4+slen]).into_owned()
}
pub fn almide_rt_bytes_as_ptr(b: &Vec<u8>) -> *mut u8 { b.as_ptr() as *mut u8 }
pub fn almide_rt_bytes_as_mut_ptr(b: &mut Vec<u8>) -> *mut u8 { b.as_mut_ptr() }
