// zlib — compression/decompression runtime for Almide.
// Uses the flate2 crate for zlib, deflate, and gzip support.

use flate2::Compression;
use flate2::read::{ZlibDecoder, ZlibEncoder, DeflateDecoder, DeflateEncoder, GzDecoder, GzEncoder};
use std::io::Read;

// ── Zlib (with header) ──

pub fn almide_rt_zlib_compress(data: &[u8]) -> Result<Vec<u8>, String> {
    almide_rt_zlib_compress_level(data, 6)
}

pub fn almide_rt_zlib_compress_level(data: &[u8], level: i64) -> Result<Vec<u8>, String> {
    let lvl = Compression::new(level.clamp(0, 9) as u32);
    let mut encoder = ZlibEncoder::new(data, lvl);
    let mut out = Vec::new();
    encoder.read_to_end(&mut out)
        .map_err(|e| format!("zlib.compress: {}", e))?;
    Ok(out)
}

pub fn almide_rt_zlib_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = ZlibDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)
        .map_err(|e| format!("zlib.decompress: {}", e))?;
    Ok(out)
}

// ── Deflate (raw, no header — used by Minecraft) ──

pub fn almide_rt_zlib_deflate(data: &[u8]) -> Result<Vec<u8>, String> {
    almide_rt_zlib_deflate_level(data, 6)
}

pub fn almide_rt_zlib_deflate_level(data: &[u8], level: i64) -> Result<Vec<u8>, String> {
    let lvl = Compression::new(level.clamp(0, 9) as u32);
    let mut encoder = DeflateEncoder::new(data, lvl);
    let mut out = Vec::new();
    encoder.read_to_end(&mut out)
        .map_err(|e| format!("zlib.deflate: {}", e))?;
    Ok(out)
}

pub fn almide_rt_zlib_inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = DeflateDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)
        .map_err(|e| format!("zlib.inflate: {}", e))?;
    Ok(out)
}

// ── Gzip ──

pub fn almide_rt_zlib_gzip(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = GzEncoder::new(data, Compression::default());
    let mut out = Vec::new();
    encoder.read_to_end(&mut out)
        .map_err(|e| format!("zlib.gzip: {}", e))?;
    Ok(out)
}

/// Empty input is an error on every leg, with THIS text (#2478): a gzip file
/// has at least one member. flate2's decoder happens to say the same, but the
/// self-hosted decoder (stdlib/zlib_inflate.almd, code 5) pins the bytes, so the
/// native answer is spelled here rather than borrowed from the library's Display.
pub const ALMIDE_GUNZIP_EMPTY_MSG: &str = "zlib.gunzip: unexpected end of file";
pub fn almide_rt_zlib_gunzip(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.is_empty() { return Err(ALMIDE_GUNZIP_EMPTY_MSG.to_string()); }
    let mut decoder = GzDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)
        .map_err(|e| format!("zlib.gunzip: {}", e))?;
    Ok(out)
}
