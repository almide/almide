// zlib — compression/decompression runtime for Almide.
// Uses the flate2 crate for zlib, deflate, and gzip support.

use flate2::{Compression, Crc, Decompress, FlushDecompress, Status};
use flate2::read::{ZlibEncoder, DeflateEncoder, GzEncoder};
use std::io::Read;

// ── Zlib (with header) ──

pub fn almide_rt_zlib_compress(data: &[u8]) -> Result<Vec<u8>, AlmideString> {
    almide_rt_zlib_compress_level(data, 6)
}

pub fn almide_rt_zlib_compress_level(data: &[u8], level: i64) -> Result<Vec<u8>, AlmideString> {
    let __almide_r: Result<Vec<u8>, String> = (|| {
        let lvl = Compression::new(level.clamp(0, 9) as u32);
        let mut encoder = ZlibEncoder::new(data, lvl);
        let mut out = Vec::new();
        encoder.read_to_end(&mut out)
            .map_err(|e| format!("zlib.compress: {}", e))?;
        Ok(out)
    })();
    __almide_r.map_err(Into::into)
}

pub fn almide_rt_zlib_decompress(data: &[u8]) -> Result<Vec<u8>, AlmideString> {
    zlib_decode(data).map_err(|e| format!("zlib.decompress: {}", e).into())
}

// ── Deflate (raw, no header — used by Minecraft) ──

pub fn almide_rt_zlib_deflate(data: &[u8]) -> Result<Vec<u8>, AlmideString> {
    almide_rt_zlib_deflate_level(data, 6)
}

pub fn almide_rt_zlib_deflate_level(data: &[u8], level: i64) -> Result<Vec<u8>, AlmideString> {
    let __almide_r: Result<Vec<u8>, String> = (|| {
        let lvl = Compression::new(level.clamp(0, 9) as u32);
        let mut encoder = DeflateEncoder::new(data, lvl);
        let mut out = Vec::new();
        encoder.read_to_end(&mut out)
            .map_err(|e| format!("zlib.deflate: {}", e))?;
        Ok(out)
    })();
    __almide_r.map_err(Into::into)
}

pub fn almide_rt_zlib_inflate(data: &[u8]) -> Result<Vec<u8>, AlmideString> {
    let mut out = Vec::new();
    match zlib_raw_inflate(data, 0, &mut out) {
        Ok(_) => Ok(out),
        Err(e) => Err(format!("zlib.inflate: {}", e).into()),
    }
}

// ── Gzip ──

pub fn almide_rt_zlib_gzip(data: &[u8]) -> Result<Vec<u8>, AlmideString> {
    let __almide_r: Result<Vec<u8>, String> = (|| {
        let mut encoder = GzEncoder::new(data, Compression::default());
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
    gzip_decode(data).map_err(|e| format!("zlib.gunzip: {}", e).into())
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

/// Inflate the raw DEFLATE stream at `data[start..]` onto `out`; Ok is the
/// position of the first byte after it. A stream that needs bytes past the
/// input is incomplete, one the inflater rejects is corrupt. A fresh inflater
/// per stream: a distance reaching before `out`'s length at entry reads the
/// empty window's zeros, as the self-hosted decoder does.
fn zlib_raw_inflate(data: &[u8], start: usize, out: &mut Vec<u8>) -> Result<usize, &'static str> {
    let input = data.get(start..).unwrap_or(&[]);
    let mut inf = Decompress::new(false);
    loop {
        let (seen_in, seen_out) = (inf.total_in(), inf.total_out());
        out.reserve(32 * 1024);
        let rest = &input[inf.total_in() as usize..];
        match inf.decompress_vec(rest, out, FlushDecompress::None) {
            Err(_) => return Err(ZLIB_CORRUPT),
            Ok(Status::StreamEnd) => return Ok(start + inf.total_in() as usize),
            Ok(_) if inf.total_in() == seen_in && inf.total_out() == seen_out => return Err(ZLIB_INCOMPLETE),
            Ok(_) => {}
        }
    }
}

fn zlib_crc32(bytes: &[u8]) -> u32 {
    let mut crc = Crc::new();
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
fn gzip_decode(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    if data.is_empty() {
        return Err(ZLIB_EOF);
    }
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let start = gzip_header_end(data, pos)?;
        let member_start = out.len();
        let after = zlib_raw_inflate(data, start, &mut out)?;
        if after + 8 > data.len() {
            return Err(ZLIB_EOF);
        }
        if zlib_crc32(&out[member_start..]) != zlib_u32_le(data, after) {
            return Err(ZLIB_CRC);
        }
        if (out.len() - member_start) as u32 != zlib_u32_le(data, after + 4) {
            return Err(ZLIB_SIZE);
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

/// The zlib container: CMF FLG, the DEFLATE stream, a big-endian Adler-32.
fn zlib_decode(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    if data.len() < 2 {
        return Err(ZLIB_EOF);
    }
    let (cmf, flg) = (data[0], data[1]);
    if cmf & 15 != 8 || cmf >> 4 > 7 || (u32::from(cmf) * 256 + u32::from(flg)) % 31 != 0 {
        return Err(ZLIB_BAD_ZLIB_HEADER);
    }
    if flg & 32 != 0 {
        return Err(ZLIB_PRESET_DICT);
    }
    let mut out = Vec::new();
    let after = zlib_raw_inflate(data, 2, &mut out)?;
    if after + 4 > data.len() {
        return Err(ZLIB_EOF);
    }
    let stored = u32::from_be_bytes([data[after], data[after + 1], data[after + 2], data[after + 3]]);
    if zlib_adler32(&out) != stored {
        return Err(ZLIB_ADLER);
    }
    Ok(out)
}
