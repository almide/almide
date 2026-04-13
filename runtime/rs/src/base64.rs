// base64 — RFC 4648 encoding/decoding (no external crate dependency).

const STD_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64_encode_with(b: &[u8], alphabet: &[u8; 64], pad: bool) -> String {
    let mut out = String::with_capacity((b.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= b.len() {
        let n = ((b[i] as u32) << 16) | ((b[i + 1] as u32) << 8) | (b[i + 2] as u32);
        out.push(alphabet[((n >> 18) & 0x3F) as usize] as char);
        out.push(alphabet[((n >> 12) & 0x3F) as usize] as char);
        out.push(alphabet[((n >> 6) & 0x3F) as usize] as char);
        out.push(alphabet[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = b.len() - i;
    if rem == 1 {
        let n = (b[i] as u32) << 16;
        out.push(alphabet[((n >> 18) & 0x3F) as usize] as char);
        out.push(alphabet[((n >> 12) & 0x3F) as usize] as char);
        if pad {
            out.push('=');
            out.push('=');
        }
    } else if rem == 2 {
        let n = ((b[i] as u32) << 16) | ((b[i + 1] as u32) << 8);
        out.push(alphabet[((n >> 18) & 0x3F) as usize] as char);
        out.push(alphabet[((n >> 12) & 0x3F) as usize] as char);
        out.push(alphabet[((n >> 6) & 0x3F) as usize] as char);
        if pad {
            out.push('=');
        }
    }
    out
}

fn b64_decode_char(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

fn b64_decode_any(s: &str) -> Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    // Strip trailing '=' padding.
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == b'=' {
        end -= 1;
    }
    let chars = &bytes[..end];
    let mut out = Vec::with_capacity(chars.len() * 3 / 4);
    let mut i = 0;
    while i + 4 <= chars.len() {
        let a = b64_decode_char(chars[i]).ok_or_else(|| format!("invalid base64 char at {}", i))?;
        let b = b64_decode_char(chars[i + 1]).ok_or_else(|| format!("invalid base64 char at {}", i + 1))?;
        let c = b64_decode_char(chars[i + 2]).ok_or_else(|| format!("invalid base64 char at {}", i + 2))?;
        let d = b64_decode_char(chars[i + 3]).ok_or_else(|| format!("invalid base64 char at {}", i + 3))?;
        let n = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);
        out.push(((n >> 16) & 0xFF) as u8);
        out.push(((n >> 8) & 0xFF) as u8);
        out.push((n & 0xFF) as u8);
        i += 4;
    }
    let rem = chars.len() - i;
    if rem == 2 {
        let a = b64_decode_char(chars[i]).ok_or_else(|| format!("invalid base64 char at {}", i))?;
        let b = b64_decode_char(chars[i + 1]).ok_or_else(|| format!("invalid base64 char at {}", i + 1))?;
        let n = ((a as u32) << 18) | ((b as u32) << 12);
        out.push(((n >> 16) & 0xFF) as u8);
    } else if rem == 3 {
        let a = b64_decode_char(chars[i]).ok_or_else(|| format!("invalid base64 char at {}", i))?;
        let b = b64_decode_char(chars[i + 1]).ok_or_else(|| format!("invalid base64 char at {}", i + 1))?;
        let c = b64_decode_char(chars[i + 2]).ok_or_else(|| format!("invalid base64 char at {}", i + 2))?;
        let n = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6);
        out.push(((n >> 16) & 0xFF) as u8);
        out.push(((n >> 8) & 0xFF) as u8);
    } else if rem == 1 {
        return Err(format!("invalid base64 length: trailing 1 char"));
    }
    Ok(out)
}

pub fn almide_rt_base64_encode(b: &Vec<u8>) -> String {
    b64_encode_with(b, STD_ALPHABET, true)
}

pub fn almide_rt_base64_decode(s: &str) -> Result<Vec<u8>, String> {
    b64_decode_any(s)
}

pub fn almide_rt_base64_encode_url(b: &Vec<u8>) -> String {
    b64_encode_with(b, URL_ALPHABET, false)
}

pub fn almide_rt_base64_decode_url(s: &str) -> Result<Vec<u8>, String> {
    b64_decode_any(s)
}
