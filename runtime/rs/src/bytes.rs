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

/// Create Bytes from a raw pointer + length (unsafe: caller must ensure validity).
pub fn almide_rt_bytes_from_raw_ptr(ptr: *mut u8, len: i64) -> Vec<u8> {
    if ptr.is_null() || len <= 0 { return Vec::new(); }
    unsafe { std::slice::from_raw_parts(ptr, len as usize).to_vec() }
}

/// Copy Bytes content to a raw pointer. Returns number of bytes written.
pub fn almide_rt_bytes_copy_to_ptr(b: &Vec<u8>, ptr: *mut u8, cap: i64) -> i64 {
    if ptr.is_null() { return 0; }
    let n = b.len().min(cap as usize);
    unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), ptr, n); }
    n as i64
}

// ── In-place little-endian writes & data pointer ──

pub fn almide_rt_bytes_set_f32_le(b: &mut Vec<u8>, pos: i64, val: f64) {
    let p = pos as usize;
    let bytes = (val as f32).to_le_bytes();
    if p + 4 <= b.len() { b[p..p+4].copy_from_slice(&bytes); }
}
pub fn almide_rt_bytes_set_u16_le(b: &mut Vec<u8>, pos: i64, val: i64) {
    let p = pos as usize;
    let bytes = (val as u16).to_le_bytes();
    if p + 2 <= b.len() { b[p..p+2].copy_from_slice(&bytes); }
}
pub fn almide_rt_bytes_set_u8(b: &mut Vec<u8>, pos: i64, val: i64) {
    let p = pos as usize;
    if p < b.len() { b[p] = (val as u8) & 0xFF; }
}
pub fn almide_rt_bytes_set_u32_le(b: &mut Vec<u8>, pos: i64, val: i64) {
    let p = pos as usize;
    let bytes = (val as u32).to_le_bytes();
    if p + 4 <= b.len() { b[p..p+4].copy_from_slice(&bytes); }
}
pub fn almide_rt_bytes_set_i32_le(b: &mut Vec<u8>, pos: i64, val: i64) {
    let p = pos as usize;
    let bytes = (val as i32).to_le_bytes();
    if p + 4 <= b.len() { b[p..p+4].copy_from_slice(&bytes); }
}
pub fn almide_rt_bytes_set_i64_le(b: &mut Vec<u8>, pos: i64, val: i64) {
    let p = pos as usize;
    let bytes = val.to_le_bytes();
    if p + 8 <= b.len() { b[p..p+8].copy_from_slice(&bytes); }
}
pub fn almide_rt_bytes_set_f64_le(b: &mut Vec<u8>, pos: i64, val: f64) {
    let p = pos as usize;
    let bytes = val.to_le_bytes();
    if p + 8 <= b.len() { b[p..p+8].copy_from_slice(&bytes); }
}

// ── Cursor family ──

pub fn almide_rt_bytes_skip(b: &Vec<u8>, pos: i64, n: i64) -> i64 {
    let np = (pos + n) as i64;
    if np > b.len() as i64 { b.len() as i64 } else { np }
}

pub fn almide_rt_bytes_eof(b: &Vec<u8>, pos: i64) -> bool {
    (pos as usize) >= b.len()
}

// Each cursor read returns (next_pos, Option<T>). On EOF the position is
// unchanged so the caller can detect the end without losing track.

macro_rules! cursor_read_int {
    ($name:ident, $width:expr, $convert:expr) => {
        pub fn $name(b: &Vec<u8>, pos: i64) -> (i64, Option<i64>) {
            let p = pos as usize;
            if p + $width > b.len() {
                return (pos, None);
            }
            let bytes = &b[p..p + $width];
            (pos + $width, Some($convert(bytes)))
        }
    };
}

macro_rules! cursor_read_float {
    ($name:ident, $width:expr, $convert:expr) => {
        pub fn $name(b: &Vec<u8>, pos: i64) -> (i64, Option<f64>) {
            let p = pos as usize;
            if p + $width > b.len() {
                return (pos, None);
            }
            let bytes = &b[p..p + $width];
            (pos + $width, Some($convert(bytes)))
        }
    };
}

cursor_read_int!(almide_rt_bytes_read_u8_at, 1, |b: &[u8]| b[0] as i64);
cursor_read_int!(almide_rt_bytes_read_u16_le_at, 2, |b: &[u8]| u16::from_le_bytes([b[0], b[1]]) as i64);
cursor_read_int!(almide_rt_bytes_read_u32_le_at, 4, |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64);
cursor_read_int!(almide_rt_bytes_read_i32_le_at, 4, |b: &[u8]| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64);
cursor_read_int!(almide_rt_bytes_read_i64_le_at, 8, |b: &[u8]| i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]));
cursor_read_int!(almide_rt_bytes_read_u32_be_at, 4, |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as i64);
cursor_read_int!(almide_rt_bytes_read_i32_be_at, 4, |b: &[u8]| i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as i64);
cursor_read_int!(almide_rt_bytes_read_i64_be_at, 8, |b: &[u8]| i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]));

cursor_read_float!(almide_rt_bytes_read_f32_le_at, 4, |b: &[u8]| f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64);
cursor_read_float!(almide_rt_bytes_read_f64_le_at, 8, |b: &[u8]| f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]));
cursor_read_float!(almide_rt_bytes_read_f32_be_at, 4, |b: &[u8]| f32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f64);
cursor_read_float!(almide_rt_bytes_read_f64_be_at, 8, |b: &[u8]| f64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]));

pub fn almide_rt_bytes_take_at(b: &Vec<u8>, pos: i64, n: i64) -> (i64, Option<Vec<u8>>) {
    let p = pos as usize;
    let nn = n as usize;
    if p + nn > b.len() {
        return (pos, None);
    }
    (pos + n, Some(b[p..p + nn].to_vec()))
}

// ── Big-endian appenders ──

pub fn almide_rt_bytes_append_u16_be(b: &mut Vec<u8>, val: i64) {
    b.extend_from_slice(&((val as u16).to_be_bytes()));
}
pub fn almide_rt_bytes_append_u32_be(b: &mut Vec<u8>, val: i64) {
    b.extend_from_slice(&((val as u32).to_be_bytes()));
}
pub fn almide_rt_bytes_append_i32_be(b: &mut Vec<u8>, val: i64) {
    b.extend_from_slice(&((val as i32).to_be_bytes()));
}
pub fn almide_rt_bytes_append_i64_be(b: &mut Vec<u8>, val: i64) {
    b.extend_from_slice(&val.to_be_bytes());
}
pub fn almide_rt_bytes_append_f32_be(b: &mut Vec<u8>, val: f64) {
    b.extend_from_slice(&(val as f32).to_be_bytes());
}
pub fn almide_rt_bytes_append_f64_be(b: &mut Vec<u8>, val: f64) {
    b.extend_from_slice(&val.to_be_bytes());
}

// ── Big-endian bulk readers ──

pub fn almide_rt_bytes_read_u32_be_array(b: &Vec<u8>, pos: i64, count: i64) -> Vec<i64> {
    let mut p = pos as usize;
    let n = count as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 4 > b.len() { out.push(0); p += 4; continue; }
        out.push(u32::from_be_bytes([b[p], b[p+1], b[p+2], b[p+3]]) as i64);
        p += 4;
    }
    out
}
pub fn almide_rt_bytes_read_i32_be_array(b: &Vec<u8>, pos: i64, count: i64) -> Vec<i64> {
    let mut p = pos as usize;
    let n = count as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 4 > b.len() { out.push(0); p += 4; continue; }
        out.push(i32::from_be_bytes([b[p], b[p+1], b[p+2], b[p+3]]) as i64);
        p += 4;
    }
    out
}
pub fn almide_rt_bytes_read_i64_be_array(b: &Vec<u8>, pos: i64, count: i64) -> Vec<i64> {
    let mut p = pos as usize;
    let n = count as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 8 > b.len() { out.push(0); p += 8; continue; }
        out.push(i64::from_be_bytes([b[p], b[p+1], b[p+2], b[p+3], b[p+4], b[p+5], b[p+6], b[p+7]]));
        p += 8;
    }
    out
}
pub fn almide_rt_bytes_read_f32_be_array(b: &Vec<u8>, pos: i64, count: i64) -> Vec<f64> {
    let mut p = pos as usize;
    let n = count as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 4 > b.len() { out.push(0.0); p += 4; continue; }
        out.push(f32::from_be_bytes([b[p], b[p+1], b[p+2], b[p+3]]) as f64);
        p += 4;
    }
    out
}
pub fn almide_rt_bytes_read_f64_be_array(b: &Vec<u8>, pos: i64, count: i64) -> Vec<f64> {
    let mut p = pos as usize;
    let n = count as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 8 > b.len() { out.push(0.0); p += 8; continue; }
        out.push(f64::from_be_bytes([b[p], b[p+1], b[p+2], b[p+3], b[p+4], b[p+5], b[p+6], b[p+7]]));
        p += 8;
    }
    out
}

pub fn almide_rt_bytes_data_ptr(b: &Vec<u8>) -> i64 {
    b.as_ptr() as i64
}

// ── Little-endian reads (match stdlib/defs/bytes.toml) ──

pub fn almide_rt_bytes_read_i32_le(b: &Vec<u8>, pos: i64) -> i64 {
    let p = pos as usize;
    if p + 4 > b.len() { return 0; }
    i32::from_le_bytes(b[p..p+4].try_into().unwrap()) as i64
}
pub fn almide_rt_bytes_read_u32_le(b: &Vec<u8>, pos: i64) -> i64 {
    let p = pos as usize;
    if p + 4 > b.len() { return 0; }
    u32::from_le_bytes(b[p..p+4].try_into().unwrap()) as i64
}
pub fn almide_rt_bytes_read_u16_le(b: &Vec<u8>, pos: i64) -> i64 {
    let p = pos as usize;
    if p + 2 > b.len() { return 0; }
    u16::from_le_bytes(b[p..p+2].try_into().unwrap()) as i64
}
pub fn almide_rt_bytes_read_i64_le(b: &Vec<u8>, pos: i64) -> i64 {
    let p = pos as usize;
    if p + 8 > b.len() { return 0; }
    i64::from_le_bytes(b[p..p+8].try_into().unwrap())
}
pub fn almide_rt_bytes_read_f32_le(b: &Vec<u8>, pos: i64) -> f64 {
    let p = pos as usize;
    if p + 4 > b.len() { return 0.0; }
    f32::from_le_bytes(b[p..p+4].try_into().unwrap()) as f64
}
pub fn almide_rt_bytes_read_f64_le(b: &Vec<u8>, pos: i64) -> f64 {
    let p = pos as usize;
    if p + 8 > b.len() { return 0.0; }
    f64::from_le_bytes(b[p..p+8].try_into().unwrap())
}
// F16 → F32: reassemble the u16 bits and expand.
pub fn almide_rt_bytes_read_f16_le(b: &Vec<u8>, pos: i64) -> f64 {
    let p = pos as usize;
    if p + 2 > b.len() { return 0.0; }
    let bits = u16::from_le_bytes(b[p..p+2].try_into().unwrap());
    f16_bits_to_f64(bits) as f64
}

// Read `len` bytes from position `pos` as UTF-8.
// Invalid UTF-8 sequences are replaced (via String::from_utf8_lossy).
pub fn almide_rt_bytes_read_string_at(b: &Vec<u8>, pos: i64, len: i64) -> String {
    let p = pos as usize;
    let n = len as usize;
    if p + n > b.len() { return String::new(); }
    String::from_utf8_lossy(&b[p..p + n]).into_owned()
}

// Bulk array reads: avoid per-element Almide-side loops.

pub fn almide_rt_bytes_read_i32_le_array(b: &Vec<u8>, pos: i64, count: i64) -> Vec<i64> {
    let mut p = pos as usize;
    let n = count as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 4 > b.len() { out.push(0); p += 4; continue; }
        out.push(i32::from_le_bytes([b[p], b[p+1], b[p+2], b[p+3]]) as i64);
        p += 4;
    }
    out
}

pub fn almide_rt_bytes_read_i64_le_array(b: &Vec<u8>, pos: i64, count: i64) -> Vec<i64> {
    let mut p = pos as usize;
    let n = count as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 8 > b.len() { out.push(0); p += 8; continue; }
        out.push(i64::from_le_bytes([b[p], b[p+1], b[p+2], b[p+3], b[p+4], b[p+5], b[p+6], b[p+7]]));
        p += 8;
    }
    out
}

pub fn almide_rt_bytes_read_u32_le_array(b: &Vec<u8>, pos: i64, count: i64) -> Vec<i64> {
    let mut p = pos as usize;
    let n = count as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 4 > b.len() { out.push(0); p += 4; continue; }
        out.push(u32::from_le_bytes([b[p], b[p+1], b[p+2], b[p+3]]) as i64);
        p += 4;
    }
    out
}

pub fn almide_rt_bytes_read_f64_le_array(b: &Vec<u8>, pos: i64, count: i64) -> Vec<f64> {
    let mut p = pos as usize;
    let n = count as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 8 > b.len() { out.push(0.0); p += 8; continue; }
        out.push(f64::from_le_bytes([b[p], b[p+1], b[p+2], b[p+3], b[p+4], b[p+5], b[p+6], b[p+7]]));
        p += 8;
    }
    out
}

pub fn almide_rt_bytes_read_f32_le_array(b: &Vec<u8>, pos: i64, count: i64) -> Vec<f64> {
    let mut p = pos as usize;
    let n = count as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 4 > b.len() { out.push(0.0); p += 4; continue; }
        out.push(f32::from_le_bytes([b[p], b[p+1], b[p+2], b[p+3]]) as f64);
        p += 4;
    }
    out
}

pub fn almide_rt_bytes_read_f16_le_array(b: &Vec<u8>, pos: i64, count: i64) -> Vec<f64> {
    let mut p = pos as usize;
    let n = count as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 2 > b.len() { out.push(0.0); p += 2; continue; }
        let bits = u16::from_le_bytes([b[p], b[p+1]]);
        out.push(f16_bits_to_f64(bits) as f64);
        p += 2;
    }
    out
}

// Skip `count` length-prefixed entries starting at `pos`.
// Each entry = [u32 len little-endian][len bytes]. Returns next position.
// Native implementation bypasses per-iteration Almide-side Vec<u8> clones.
pub fn almide_rt_bytes_skip_length_prefixed_le(b: &Vec<u8>, pos: i64, count: i64) -> i64 {
    let mut p = pos as usize;
    let n = count as usize;
    let buf = b.as_slice();
    for _ in 0..n {
        if p + 4 > buf.len() { return p as i64; }
        let len = u32::from_le_bytes([buf[p], buf[p+1], buf[p+2], buf[p+3]]) as usize;
        p += 4 + len;
    }
    p as i64
}

// IEEE-754 half precision → f32. Hardware-free reference impl.
fn f16_bits_to_f64(bits: u16) -> f32 {
    let sign = (bits >> 15) & 1;
    let exp = ((bits >> 10) & 0x1f) as i32;
    let mant = (bits & 0x3ff) as u32;
    let sign_f = if sign == 1 { -1.0f32 } else { 1.0 };
    if exp == 0 {
        // subnormal or zero: value = sign * mant * 2^-24
        sign_f * (mant as f32) * (2.0f32).powi(-24)
    } else if exp == 31 {
        if mant == 0 { sign_f * f32::INFINITY } else { f32::NAN }
    } else {
        // normal: (1 + mant/1024) * 2^(exp - 15)
        let m = 1.0f32 + (mant as f32) / 1024.0;
        sign_f * m * (2.0f32).powi(exp - 15)
    }
}

pub fn almide_rt_bytes_read_length_prefixed_strings_le(b: &Vec<u8>, pos: i64, count: i64) -> Vec<String> {
    let mut p = pos as usize;
    let n = count as usize;
    let buf = b.as_slice();
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if p + 4 > buf.len() { break; }
        let len = u32::from_le_bytes([buf[p], buf[p+1], buf[p+2], buf[p+3]]) as usize;
        p += 4;
        if p + len > buf.len() { break; }
        out.push(String::from_utf8_lossy(&buf[p..p+len]).into_owned());
        p += len;
    }
    out
}

pub fn almide_rt_bytes_append_f64_le(b: &mut Vec<u8>, val: f64) {
    b.extend_from_slice(&val.to_le_bytes());
}

pub fn almide_rt_bytes_append_f32_le(b: &mut Vec<u8>, val: f64) {
    b.extend_from_slice(&(val as f32).to_le_bytes());
}

pub fn almide_rt_bytes_append_u8(b: &mut Vec<u8>, val: i64) {
    b.push((val as u8) & 0xFF);
}

pub fn almide_rt_bytes_append_u16_le(b: &mut Vec<u8>, val: i64) {
    b.extend_from_slice(&((val as u16).to_le_bytes()));
}

pub fn almide_rt_bytes_append_u32_le(b: &mut Vec<u8>, val: i64) {
    b.extend_from_slice(&((val as u32).to_le_bytes()));
}

pub fn almide_rt_bytes_append_i32_le(b: &mut Vec<u8>, val: i64) {
    b.extend_from_slice(&((val as i32).to_le_bytes()));
}

pub fn almide_rt_bytes_append_i64_le(b: &mut Vec<u8>, val: i64) {
    b.extend_from_slice(&val.to_le_bytes());
}
