// hex — lowercase / uppercase hex encoding/decoding.

const LOWER: &[u8; 16] = b"0123456789abcdef";
const UPPER: &[u8; 16] = b"0123456789ABCDEF";

fn hex_encode_with(b: &[u8], alphabet: &[u8; 16]) -> String {
    let mut out = String::with_capacity(b.len() * 2);
    for &byte in b {
        out.push(alphabet[(byte >> 4) as usize] as char);
        out.push(alphabet[(byte & 0x0F) as usize] as char);
    }
    out
}

fn hex_from_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

pub fn almide_rt_hex_encode(b: &Vec<u8>) -> String {
    hex_encode_with(b, LOWER)
}

pub fn almide_rt_hex_encode_upper(b: &Vec<u8>) -> String {
    hex_encode_with(b, UPPER)
}

pub fn almide_rt_hex_decode(s: &str) -> Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(format!("hex string has odd length: {}", bytes.len()));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_from_nibble(bytes[i]).ok_or_else(|| format!("invalid hex char at {}", i))?;
        let lo = hex_from_nibble(bytes[i + 1]).ok_or_else(|| format!("invalid hex char at {}", i + 1))?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}
