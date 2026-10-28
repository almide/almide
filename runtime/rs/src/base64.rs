// base64 — RFC 4648 Base64 encoding/decoding (standard + URL-safe alphabets).
//
// This is the native oracle for `base64.{encode,decode,encode_url,decode_url}`.
// The self-hosted WASM leg (`stdlib/base64_encode.almd`) must byte-match these
// outputs, including the exact error strings. (v0's leg was
// `emit_wasm/rt_encoding.rs`, retired in c71eff7b.)
//
// Standard alphabet (RFC 4648 §4):  A-Z a-z 0-9 + /  with '=' padding.
// URL-safe alphabet (RFC 4648 §5):  A-Z a-z 0-9 - _  with '=' padding.
// Both are derived from the same 0..61 prefix; only the 62/63 symbols differ.

/// Shared 0..61 prefix of both Base64 alphabets: A-Z (0..25), a-z (26..51),
/// 0-9 (52..61). The two final symbols (62, 63) are alphabet-specific and
/// appended below, so the relationship between the standard and URL-safe
/// tables is explicit rather than two opaque 64-byte literals.
const ALMIDE_ALPHABET_PREFIX: &[u8; 62] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Standard alphabet (RFC 4648 §4): prefix + '+' (62) + '/' (63).
const ALMIDE_STD_ALPHABET: [u8; 64] = build_alphabet(b'+', b'/');
/// URL-safe alphabet (RFC 4648 §5): prefix + '-' (62) + '_' (63).
const ALMIDE_URL_ALPHABET: [u8; 64] = build_alphabet(b'-', b'_');

const ALMIDE_PAD: u8 = b'=';

/// Build a full 64-entry alphabet from the shared prefix plus the two
/// alphabet-specific trailing symbols (`s62`, `s63`).
const fn build_alphabet(s62: u8, s63: u8) -> [u8; 64] {
    let mut out = [0u8; 64];
    let mut i = 0;
    while i < 62 {
        out[i] = ALMIDE_ALPHABET_PREFIX[i];
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
            out.push(ALMIDE_PAD);
            out.push(ALMIDE_PAD);
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(alphabet[((n >> 18) & 0x3f) as usize]);
            out.push(alphabet[((n >> 12) & 0x3f) as usize]);
            out.push(alphabet[((n >> 6) & 0x3f) as usize]);
            out.push(ALMIDE_PAD);
        }
        _ => {}
    }
    // All alphabet bytes are ASCII, so this is always valid UTF-8.
    out.into_iter().map(char::from).collect()
}

/// Map a single Base64 character to its 0..63 value under ONE alphabet: `s62`
/// / `s63` are that alphabet's two final symbols (`+` `/` or `-` `_`), and the
/// other alphabet's pair is an invalid character (#4119). Returns `None` for
/// any byte outside the alphabet, `=` included.
fn decode_char(c: u8, s62: u8, s63: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        _ if c == s62 => Some(62),
        _ if c == s63 => Some(63),
        _ => None,
    }
}

// Fixed, position-free error strings keep the WASM mirror cheap: constant
// character / padding / trailing-bit messages, and a length message that only
// needs an int-to-string of the input length.
const ALMIDE_ERR_CHAR: &str = "invalid base64 character";
const ALMIDE_ERR_PAD: &str = "invalid base64 padding";
const ALMIDE_ERR_BITS: &str = "invalid base64 trailing bits";

/// CANONICAL decode (#4119): exactly the padded form `encode_with` emits, so
/// one byte string has exactly one accepted spelling. Checked in this order,
/// the first failure wins:
///   1. the length is a multiple of 4      → `invalid base64 length: <len>`
///   2. at most two trailing `=`           → `invalid base64 padding`
///   3. every other byte is in THIS alphabet (a `=` before the trailing run,
///      or the other alphabet's `+/` / `-_`) → `invalid base64 character`
///   4. the bits the padding drops are zero → `invalid base64 trailing bits`
///
/// Before this the decoder stripped any number of `=`, accepted no padding,
/// read both alphabets in both functions, and ignored the dropped bits, so
/// `QQ==`, `QR==`, `QQ`, `QQ========` all decoded to `A` (RFC 4648 §12's
/// malleability). Go's `Encoding.Strict()` refuses the same set.
fn decode_str(s: &str, s62: u8, s63: u8) -> Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(format!("invalid base64 length: {}", s.len()));
    }
    let pad = bytes.iter().rev().take_while(|&&b| b == ALMIDE_PAD).count();
    if pad > 2 {
        return Err(ALMIDE_ERR_PAD.to_string());
    }
    let body = &bytes[..bytes.len() - pad];
    let vals = body
        .iter()
        .map(|&c| decode_char(c, s62, s63))
        .collect::<Option<Vec<u8>>>()
        .ok_or_else(|| ALMIDE_ERR_CHAR.to_string())?;
    // One pad drops the low 2 bits of the last symbol, two pads the low 4.
    let dropped = match pad { 1 => 0x03, 2 => 0x0f, _ => 0 };
    if vals.last().is_some_and(|&v| v & dropped != 0) {
        return Err(ALMIDE_ERR_BITS.to_string());
    }

    let mut out = Vec::with_capacity(vals.len() / 4 * 3 + 2);
    let mut chunks = vals.chunks_exact(4);
    for q in &mut chunks {
        out.push((q[0] << 2) | (q[1] >> 4));
        out.push((q[1] << 4) | (q[2] >> 2));
        out.push((q[2] << 6) | q[3]);
    }
    let rem = chunks.remainder();
    if rem.len() >= 2 {
        out.push((rem[0] << 2) | (rem[1] >> 4));
    }
    if rem.len() == 3 {
        out.push((rem[1] << 4) | (rem[2] >> 2));
    }
    Ok(out)
}

pub fn almide_rt_base64_encode(b: &Vec<u8>) -> AlmideString { let __almide_out: String = encode_with(b, &ALMIDE_STD_ALPHABET); __almide_out.into() }

pub fn almide_rt_base64_encode_url(b: &Vec<u8>) -> AlmideString { let __almide_out: String = encode_with(b, &ALMIDE_URL_ALPHABET); __almide_out.into() }

pub fn almide_rt_base64_decode(s: &str) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { decode_str(s, b'+', b'/') })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_base64_decode_url(s: &str) -> Result<Vec<u8>, AlmideString> { let __almide_r: Result<Vec<u8>, String> = (|| { decode_str(s, b'-', b'_') })(); __almide_r.map_err(Into::into) }
