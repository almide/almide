// base64 — RFC 4648 Base64 encoding/decoding (standard + URL-safe alphabets).
//
// This is the native oracle for `base64.{encode,decode,encode_url,decode_url}`.
// The WASM runtime (`emit_wasm/rt_encoding.rs`) must byte-match these outputs,
// including the exact error strings.
//
// Standard alphabet (RFC 4648 §4):  A-Z a-z 0-9 + /  with '=' padding.
// URL-safe alphabet (RFC 4648 §5):  A-Z a-z 0-9 - _  with '=' padding.
// Both are derived from the same 0..61 prefix; only the 62/63 symbols differ.

/// Shared 0..61 prefix of both Base64 alphabets: A-Z (0..25), a-z (26..51),
/// 0-9 (52..61). The two final symbols (62, 63) are alphabet-specific and
/// appended below, so the relationship between the standard and URL-safe
/// tables is explicit rather than two opaque 64-byte literals.
const ALPHABET_PREFIX: &[u8; 62] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Standard alphabet (RFC 4648 §4): prefix + '+' (62) + '/' (63).
const STD_ALPHABET: [u8; 64] = build_alphabet(b'+', b'/');
/// URL-safe alphabet (RFC 4648 §5): prefix + '-' (62) + '_' (63).
const URL_ALPHABET: [u8; 64] = build_alphabet(b'-', b'_');

const PAD: u8 = b'=';

/// Build a full 64-entry alphabet from the shared prefix plus the two
/// alphabet-specific trailing symbols (`s62`, `s63`).
const fn build_alphabet(s62: u8, s63: u8) -> [u8; 64] {
    let mut out = [0u8; 64];
    let mut i = 0;
    while i < 62 {
        out[i] = ALPHABET_PREFIX[i];
        i += 1;
    }
    out[62] = s62;
    out[63] = s63;
    out
}

fn encode_with(b: &[u8], alphabet: &[u8; 64]) -> String {
    // Output length is always a multiple of 4 (padded form): 4 chars per
    // 3 input bytes, rounding up.
    let mut out = Vec::with_capacity(b.len().div_ceil(3) * 4);
    let mut chunks = b.chunks_exact(3);
    for chunk in &mut chunks {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        out.push(alphabet[((n >> 18) & 0x3f) as usize]);
        out.push(alphabet[((n >> 12) & 0x3f) as usize]);
        out.push(alphabet[((n >> 6) & 0x3f) as usize]);
        out.push(alphabet[(n & 0x3f) as usize]);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let n = (rem[0] as u32) << 16;
            out.push(alphabet[((n >> 18) & 0x3f) as usize]);
            out.push(alphabet[((n >> 12) & 0x3f) as usize]);
            out.push(PAD);
            out.push(PAD);
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(alphabet[((n >> 18) & 0x3f) as usize]);
            out.push(alphabet[((n >> 12) & 0x3f) as usize]);
            out.push(alphabet[((n >> 6) & 0x3f) as usize]);
            out.push(PAD);
        }
        _ => {}
    }
    // All alphabet bytes are ASCII, so this is always valid UTF-8.
    String::from_utf8(out).unwrap()
}

/// Map a single Base64 character to its 0..63 value, accepting both the
/// standard (`+`/`/`) and URL-safe (`-`/`_`) symbols for 62/63 so a decoder
/// is liberal in what it accepts. Returns `None` for any other byte.
fn decode_char(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

// Two distinct, position-free error strings keep the WASM mirror cheap: a
// constant invalid-character message, and a length message that only needs an
// int-to-string of the original input length.
const ERR_CHAR: &str = "invalid base64 character";

fn decode_str(s: &str) -> Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    // Strip canonical trailing '=' padding (0, 1, or 2 chars), then decode the
    // significant chars. A '=' anywhere before the trailing run is invalid and
    // is rejected as a normal invalid character by `decode_char` below.
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == PAD {
        end -= 1;
    }
    let body = &bytes[..end];

    // After stripping padding, length % 4 == 1 is impossible for valid Base64
    // (a single trailing 6-bit symbol cannot encode any whole byte). Report the
    // ORIGINAL input length so the message is stable regardless of padding.
    if body.len() % 4 == 1 {
        return Err(format!("invalid base64 length: {}", s.len()));
    }

    let mut out = Vec::with_capacity(body.len() / 4 * 3 + 2);
    let mut chunks = body.chunks_exact(4);
    for chunk in &mut chunks {
        let a = nibble(chunk[0])?;
        let b = nibble(chunk[1])?;
        let c = nibble(chunk[2])?;
        let d = nibble(chunk[3])?;
        out.push((a << 2) | (b >> 4));
        out.push((b << 4) | (c >> 2));
        out.push((c << 6) | d);
    }
    let rem = chunks.remainder();
    match rem.len() {
        2 => {
            let a = nibble(rem[0])?;
            let b = nibble(rem[1])?;
            out.push((a << 2) | (b >> 4));
        }
        3 => {
            let a = nibble(rem[0])?;
            let b = nibble(rem[1])?;
            let c = nibble(rem[2])?;
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
        }
        _ => {}
    }
    Ok(out)
}

/// Decode one Base64 character to its 0..63 value, reporting `ERR_CHAR` on
/// failure (a stray `=` in the body lands here too).
fn nibble(c: u8) -> Result<u8, String> {
    decode_char(c).ok_or_else(|| ERR_CHAR.to_string())
}

pub fn almide_rt_base64_encode(b: &Vec<u8>) -> String {
    encode_with(b, &STD_ALPHABET)
}

pub fn almide_rt_base64_encode_url(b: &Vec<u8>) -> String {
    encode_with(b, &URL_ALPHABET)
}

pub fn almide_rt_base64_decode(s: &str) -> Result<Vec<u8>, String> {
    decode_str(s)
}

pub fn almide_rt_base64_decode_url(s: &str) -> Result<Vec<u8>, String> {
    decode_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 4648 §10 test vectors (standard alphabet), validating this impl
    // against a known-good third source independent of any round-trip.
    #[test]
    fn rfc4648_vectors() {
        let v: &[(&str, &str)] = &[
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (plain, encoded) in v {
            let bytes = plain.as_bytes().to_vec();
            assert_eq!(almide_rt_base64_encode(&bytes), *encoded, "encode {plain:?}");
            assert_eq!(almide_rt_base64_decode(encoded).unwrap(), bytes, "decode {encoded:?}");
        }
    }

    #[test]
    fn man_examples() {
        assert_eq!(almide_rt_base64_encode(&b"Man".to_vec()), "TWFu");
        assert_eq!(almide_rt_base64_encode(&b"Ma".to_vec()), "TWE=");
        assert_eq!(almide_rt_base64_encode(&b"M".to_vec()), "TQ==");
        assert_eq!(almide_rt_base64_decode("TWFu").unwrap(), b"Man");
    }

    #[test]
    fn url_safe_distinguishes_62_63() {
        // 0xFB,0xFF,0xFE → '+/+' region: std "+/" , url "-_".
        let bytes = vec![0xfb, 0xff, 0xbf];
        let std = almide_rt_base64_encode(&bytes);
        let url = almide_rt_base64_encode_url(&bytes);
        assert!(std.contains('+') || std.contains('/'));
        assert!(url.contains('-') || url.contains('_'));
        assert!(!url.contains('+') && !url.contains('/'));
        assert_eq!(almide_rt_base64_decode_url(&url).unwrap(), bytes);
    }

    #[test]
    fn decode_errors() {
        // length % 4 == 1 after padding strip → length error naming orig len
        assert_eq!(
            almide_rt_base64_decode("Q").unwrap_err(),
            "invalid base64 length: 1"
        );
        // invalid char → constant char error
        assert_eq!(
            almide_rt_base64_decode("Q*==").unwrap_err(),
            "invalid base64 character"
        );
        // stray '=' inside the body is treated as an invalid character
        assert_eq!(
            almide_rt_base64_decode("Q=Fu").unwrap_err(),
            "invalid base64 character"
        );
    }

    // Property: decode(encode(b)) == b for arbitrary byte lists, using a small
    // xorshift PRNG so the fuzz is deterministic and host-independent.
    #[test]
    fn roundtrip_fuzz() {
        let mut state: u64 = 0x9E3779B97F4A7C15;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20_000 {
            let len = (next() % 64) as usize;
            let b: Vec<u8> = (0..len).map(|_| (next() & 0xff) as u8).collect();
            let enc = almide_rt_base64_encode(&b);
            assert_eq!(almide_rt_base64_decode(&enc).unwrap(), b, "std roundtrip {b:?}");
            let enc_u = almide_rt_base64_encode_url(&b);
            assert_eq!(almide_rt_base64_decode_url(&enc_u).unwrap(), b, "url roundtrip {b:?}");
        }
    }
}
