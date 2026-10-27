// zlib — compression/decompression runtime for Almide.
// Encodes with the flate2 crate; decodes with miniz_oxide's core inflater (the
// engine flate2 itself runs) over a non-wrapping output buffer, so a distance
// reaching before the start of a stream is refused rather than read as the
// empty window's zeros (#4268).

use std::io::Read as _;

// ── Zlib (with header) ──

pub fn almide_rt_zlib_compress(data: &[u8]) -> Result<Vec<u8>, AlmideString> {
    almide_rt_zlib_compress_level(data, 6)
}

pub fn almide_rt_zlib_compress_level(data: &[u8], level: i64) -> Result<Vec<u8>, AlmideString> {
    let __almide_r: Result<Vec<u8>, String> = (|| {
        let lvl = flate2::Compression::new(level.clamp(0, 9) as u32);
        let mut encoder = flate2::read::ZlibEncoder::new(data, lvl);
        let mut out = Vec::new();
        encoder.read_to_end(&mut out)
            .map_err(|e| format!("zlib.compress: {}", e))?;
        Ok(out)
    })();
    __almide_r.map_err(Into::into)
}

pub fn almide_rt_zlib_decompress(data: &[u8]) -> Result<Vec<u8>, AlmideString> {
    zlib_decode(data, ZLIB_DEFAULT_MAX).map_err(|e| zlib_decode_err("zlib.decompress", e))
}

/// `decompress` with the output bound `max_len` in place of the default 256 MiB
/// (R-zlib-04); `max_len <= 0` aborts.
pub fn almide_rt_zlib_decompress_max(data: &[u8], max_len: i64) -> Result<Vec<u8>, AlmideString> {
    let max = zlib_checked_max("decompress_max", max_len);
    zlib_decode(data, max).map_err(|e| zlib_decode_err("zlib.decompress", e))
}

// ── Deflate (raw, no header — used by Minecraft) ──

pub fn almide_rt_zlib_deflate(data: &[u8]) -> Result<Vec<u8>, AlmideString> {
    almide_rt_zlib_deflate_level(data, 6)
}

pub fn almide_rt_zlib_deflate_level(data: &[u8], level: i64) -> Result<Vec<u8>, AlmideString> {
    let __almide_r: Result<Vec<u8>, String> = (|| {
        let lvl = flate2::Compression::new(level.clamp(0, 9) as u32);
        let mut encoder = flate2::read::DeflateEncoder::new(data, lvl);
        let mut out = Vec::new();
        encoder.read_to_end(&mut out)
            .map_err(|e| format!("zlib.deflate: {}", e))?;
        Ok(out)
    })();
    __almide_r.map_err(Into::into)
}

pub fn almide_rt_zlib_inflate(data: &[u8]) -> Result<Vec<u8>, AlmideString> {
    zlib_inflate_capped(data, ZLIB_DEFAULT_MAX)
}

/// `inflate` with the output bound `max_len` (R-zlib-04); `max_len <= 0` aborts.
pub fn almide_rt_zlib_inflate_max(data: &[u8], max_len: i64) -> Result<Vec<u8>, AlmideString> {
    zlib_inflate_capped(data, zlib_checked_max("inflate_max", max_len))
}

/// A raw DEFLATE stream is the whole input: a byte after its final block is
/// `trailing data after stream`, as a byte after a gzip member is refused (#4268).
fn zlib_inflate_capped(data: &[u8], max: u64) -> Result<Vec<u8>, AlmideString> {
    let mut out = Vec::new();
    match zlib_raw_inflate(data, 0, &mut out, max) {
        Ok(after) if after < data.len() => Err(zlib_decode_err("zlib.inflate", ZLIB_TRAILING.into())),
        Ok(_) => Ok(out),
        Err(e) => Err(zlib_decode_err("zlib.inflate", e)),
    }
}

// ── Gzip ──

pub fn almide_rt_zlib_gzip(data: &[u8]) -> Result<Vec<u8>, AlmideString> {
    let __almide_r: Result<Vec<u8>, String> = (|| {
        let mut encoder = flate2::read::GzEncoder::new(data, flate2::Compression::default());
        let mut out = Vec::new();
        encoder.read_to_end(&mut out)
            .map_err(|e| format!("zlib.gzip: {}", e))?;
        Ok(out)
    })();
    __almide_r.map_err(Into::into)
}

/// Empty input is `unexpected end of file` on every leg (#2478): a gzip file
/// has at least one member.
pub fn almide_rt_zlib_gunzip(data: &[u8]) -> Result<Vec<u8>, AlmideString> {
    gzip_decode(data, ZLIB_DEFAULT_MAX).map_err(|e| zlib_decode_err("zlib.gunzip", e))
}

/// `gunzip` with the output bound `max_len` (R-zlib-04); `max_len <= 0` aborts.
pub fn almide_rt_zlib_gunzip_max(data: &[u8], max_len: i64) -> Result<Vec<u8>, AlmideString> {
    let max = zlib_checked_max("gunzip_max", max_len);
    gzip_decode(data, max).map_err(|e| zlib_decode_err("zlib.gunzip", e))
}

// ── Decoding: one canonical failure set on every leg (C-331) ──
//
// The containers are parsed here, mirroring the self-hosted decoder the wasm
// legs run (stdlib/zlib_inflate.almd) step for step, and flate2 only inflates
// the raw DEFLATE stream; its errors are classified (truncated vs malformed),
// never displayed — flate2/miniz wording is not a stable API.
// tests/zlib_decode_error_parity_test.rs pins both legs to these texts.
const ZLIB_EOF: &str = "unexpected end of file";
const ZLIB_BAD_GZIP_HEADER: &str = "invalid gzip header";
const ZLIB_BAD_HEADER_CRC: &str = "header crc mismatch";
const ZLIB_BAD_ZLIB_HEADER: &str = "invalid zlib header";
const ZLIB_PRESET_DICT: &str = "preset dictionary unsupported";
const ZLIB_INCOMPLETE: &str = "incomplete deflate stream";
const ZLIB_CORRUPT: &str = "corrupt deflate stream";
const ZLIB_CRC: &str = "crc mismatch";
const ZLIB_SIZE: &str = "size mismatch";
const ZLIB_ADLER: &str = "adler32 mismatch";
const ZLIB_TRAILING: &str = "trailing data after stream";

// ── The output bound (R-L0-47, R-zlib-04, #4020) ──
//
// A decoder of untrusted input stops where its output would pass `max` bytes
// and returns `zlib: output exceeds <max> bytes` — the whole text, without the
// function's prefix, on every leg (stdlib/zlib_inflate.almd spells the same).
// The inflater is stopped within one 32 KiB window past the bound however
// small the input, so a few KB of compression bomb stop near 256 MiB instead
// of exhausting memory. Across gzip members the bound is on the total.
const ZLIB_DEFAULT_MAX: u64 = 268_435_456;

/// The failure of a bounded decode: one of the canonical texts above, or the
/// output bound that was passed.
enum ZlibDecodeError {
    Text(&'static str),
    Exceeds(u64),
}

impl From<&'static str> for ZlibDecodeError {
    fn from(e: &'static str) -> Self { ZlibDecodeError::Text(e) }
}

fn zlib_decode_err(func: &str, e: ZlibDecodeError) -> AlmideString {
    match e {
        ZlibDecodeError::Text(t) => format!("{}: {}", func, t).into(),
        ZlibDecodeError::Exceeds(max) => format!("zlib: output exceeds {} bytes", max).into(),
    }
}

/// A caller's bound: positive, or the call aborts the way the self-hosted
/// body's `panic` does (R-L0-33: a count the caller can check before calling).
fn zlib_checked_max(func: &str, max_len: i64) -> u64 {
    if max_len <= 0 { almide_panic!("zlib.{}: max_len must be positive, got {}", func, max_len); }
    max_len as u64
}

/// Inflate the raw DEFLATE stream at `data[start..]` onto `out`; Ok is the
/// position of the first byte after it. A stream that needs bytes past the
/// input is incomplete, one the inflater rejects is corrupt, one whose output
/// would take `out` past `max` bytes exceeds the bound. Each stream decodes
/// into its own non-wrapping buffer: a distance reaching before the stream's
/// first output byte is corrupt (RFC 1951 §3.2.5, zlib's "invalid distance too
/// far back"), whether or not an earlier gzip member came before it (#4268).
///
/// The bound is judged in stream order, as the self-hosted decoder judges it
/// symbol by symbol: the inflater is never handed room past `max` + 1 bytes,
/// so a failure it meets before the bound is that failure, and output that
/// fills the room exceeds the bound. `out` holds at most `max` + 1 bytes.
fn zlib_raw_inflate(data: &[u8], start: usize, out: &mut Vec<u8>, max: u64) -> Result<usize, ZlibDecodeError> {
    let input = data.get(start..).unwrap_or(&[]);
    let room = max.saturating_sub(out.len() as u64).saturating_add(1);
    let mut buf = vec![0u8; room.min(64 * 1024) as usize];
    let mut inf = miniz_oxide::inflate::core::DecompressorOxide::new();
    let (mut in_pos, mut out_pos) = (0usize, 0usize);
    loop {
        let (status, n_in, n_out) = miniz_oxide::inflate::core::decompress(
            &mut inf,
            &input[in_pos..],
            &mut buf,
            out_pos,
            miniz_oxide::inflate::core::inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        );
        in_pos += n_in;
        out_pos += n_out;
        if out.len() as u64 + out_pos as u64 > max {
            return Err(ZlibDecodeError::Exceeds(max));
        }
        match status {
            miniz_oxide::inflate::TINFLStatus::Done => {
                out.extend_from_slice(&buf[..out_pos]);
                return Ok(start + in_pos);
            }
            miniz_oxide::inflate::TINFLStatus::HasMoreOutput => {
                let grown = (buf.len() as u64).saturating_mul(2).min(room) as usize;
                buf.resize(grown, 0);
            }
            miniz_oxide::inflate::TINFLStatus::NeedsMoreInput | miniz_oxide::inflate::TINFLStatus::FailedCannotMakeProgress => {
                return Err(ZlibDecodeError::Text(ZLIB_INCOMPLETE));
            }
            _ => return Err(ZlibDecodeError::Text(ZLIB_CORRUPT)),
        }
    }
}

fn zlib_crc32(bytes: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    crc.sum()
}

fn zlib_u32_le(data: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]])
}

/// End of the NUL-terminated field at `p`, or past the input when it has none.
fn gzip_skip_zstr(data: &[u8], p: usize) -> usize {
    data.get(p..).and_then(|r| r.iter().position(|&b| b == 0)).map_or(data.len() + 1, |i| p + i + 1)
}

/// Where the deflate stream starts after the gzip member header at `pos`.
fn gzip_header_end(data: &[u8], pos: usize) -> Result<usize, &'static str> {
    let head = &data[pos..];
    if head.iter().zip([31u8, 139, 8]).any(|(a, b)| *a != b) {
        return Err(ZLIB_BAD_GZIP_HEADER);
    }
    if head.len() < 10 {
        return Err(ZLIB_EOF);
    }
    let flg = head[3];
    let p0 = pos + 10;
    let p1 = if flg & 4 != 0 {
        let xlen = data.get(p0).map_or(0, |&b| b as usize) | data.get(p0 + 1).map_or(0, |&b| (b as usize) << 8);
        p0 + 2 + xlen
    } else {
        p0
    };
    let p2 = if flg & 8 != 0 { gzip_skip_zstr(data, p1) } else { p1 };
    let p3 = if flg & 16 != 0 { gzip_skip_zstr(data, p2) } else { p2 };
    let end = if flg & 2 != 0 { p3 + 2 } else { p3 };
    if flg & 224 != 0 {
        return Err(ZLIB_BAD_GZIP_HEADER);
    }
    if end > data.len() {
        return Err(ZLIB_EOF);
    }
    if flg & 2 != 0 && zlib_crc32(&data[pos..p3]) & 0xffff != u32::from(u16::from_le_bytes([data[p3], data[p3 + 1]])) {
        return Err(ZLIB_BAD_HEADER_CRC);
    }
    Ok(end)
}

/// gzip members back to back, each checked against its CRC-32 and ISIZE.
fn gzip_decode(data: &[u8], max: u64) -> Result<Vec<u8>, ZlibDecodeError> {
    if data.is_empty() {
        return Err(ZLIB_EOF.into());
    }
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let start = gzip_header_end(data, pos)?;
        let member_start = out.len();
        let after = zlib_raw_inflate(data, start, &mut out, max)?;
        if after + 8 > data.len() {
            return Err(ZLIB_EOF.into());
        }
        if zlib_crc32(&out[member_start..]) != zlib_u32_le(data, after) {
            return Err(ZLIB_CRC.into());
        }
        if (out.len() - member_start) as u32 != zlib_u32_le(data, after + 4) {
            return Err(ZLIB_SIZE.into());
        }
        pos = after + 8;
    }
    Ok(out)
}

fn zlib_adler32(bytes: &[u8]) -> u32 {
    let (mut s1, mut s2) = (1u32, 0u32);
    for &b in bytes {
        s1 = (s1 + u32::from(b)) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    (s2 << 16) | s1
}

/// The zlib container: CMF FLG, the DEFLATE stream, a big-endian Adler-32, and
/// nothing after it (#4268).
fn zlib_decode(data: &[u8], max: u64) -> Result<Vec<u8>, ZlibDecodeError> {
    if data.len() < 2 {
        return Err(ZLIB_EOF.into());
    }
    let (cmf, flg) = (data[0], data[1]);
    if cmf & 15 != 8 || cmf >> 4 > 7 || (u32::from(cmf) * 256 + u32::from(flg)) % 31 != 0 {
        return Err(ZLIB_BAD_ZLIB_HEADER.into());
    }
    if flg & 32 != 0 {
        return Err(ZLIB_PRESET_DICT.into());
    }
    let mut out = Vec::new();
    let after = zlib_raw_inflate(data, 2, &mut out, max)?;
    if after + 4 > data.len() {
        return Err(ZLIB_EOF.into());
    }
    let stored = u32::from_be_bytes([data[after], data[after + 1], data[after + 2], data[after + 3]]);
    if zlib_adler32(&out) != stored {
        return Err(ZLIB_ADLER.into());
    }
    if after + 4 < data.len() {
        return Err(ZLIB_TRAILING.into());
    }
    Ok(out)
}
