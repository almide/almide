//! C-331: zlib DECODING is byte-identical across legs — the `err` text
//! included. The native leg spelled flate2's io errors (`invalid gzip
//! header`, `corrupt gzip stream does not have a matching checksum`, …) and
//! decoded only the first gzip member, while the self-hosted decoder the wasm
//! legs run (stdlib/zlib_inflate.almd) named the failure its own way and
//! decoded every member — so `zlib.gunzip("  spaced  ")` said `invalid gzip
//! header` natively and `not gzip` on wasm.
//!
//! One canonical message set, per corrupt-input class:
//!
//! | class                                         | text                            |
//! |-----------------------------------------------|---------------------------------|
//! | empty, header cut short, trailer cut short    | `unexpected end of file`        |
//! | bad magic / method / reserved flags (gzip)    | `invalid gzip header`           |
//! | FHCRC does not match (gzip)                   | `header crc mismatch`           |
//! | bad CM / CINFO / FCHECK (zlib)                | `invalid zlib header`           |
//! | FDICT set (zlib)                              | `preset dictionary unsupported` |
//! | DEFLATE stream needs bytes past the input     | `incomplete deflate stream`     |
//! | DEFLATE stream malformed                      | `corrupt deflate stream`        |
//! | CRC-32 / ISIZE / Adler-32 trailer wrong       | `crc mismatch` / `size mismatch` / `adler32 mismatch` |
//! | bytes after a zlib / raw DEFLATE stream       | `trailing data after stream` (#4268) |
//!
//! A distance reaching before the stream's first output byte is `corrupt
//! deflate stream` (RFC 1951 §3.2.5), never the empty window's zeros (#4268).
//!
//! The probe pins every named case to its text on both legs, then sweeps
//! every prefix and every single-byte xor of three valid vectors and requires
//! the two legs to print the same line for each.
use std::path::{Path, PathBuf};
use std::process::Command;

const PROBE: &str = r#"import zlib
import hash

fn show(r: Result[Bytes, String]) -> String = match r {
  ok(b) => "ok ${bytes.len(b)} ${hash.crc32(b)}",
  err(e) => "err ${e}",
}

effect fn gz(b: Bytes) -> String = {
  let r: Result[Bytes, String] = zlib.gunzip(b)
  show(r)
}

effect fn inf(b: Bytes) -> String = {
  let r: Result[Bytes, String] = zlib.inflate(b)
  show(r)
}

effect fn dz(b: Bytes) -> String = {
  let r: Result[Bytes, String] = zlib.decompress(b)
  show(r)
}

effect fn run(f: String, b: Bytes) -> String = match f {
  "gunzip" => gz(b)!,
  "inflate" => inf(b)!,
  _ => dz(b)!,
}

// every prefix and every single-byte xor (1, 128, 255) of one valid vector
effect fn sweep(f: String, v: Bytes) -> Unit = {
  for i in 0..<bytes.len(v) {
    println("${f} cut ${i}: " + run(f, bytes.slice(v, 0, i))!)
  }
  for x in [1, 128, 255] {
    for i in 0..<bytes.len(v) {
      println("${f} x${x} ${i}: " + run(f, bytes.set(v, i, int.bxor(bytes.get(v, i) ?? 0, x)))!)
    }
  }
}

effect fn main() -> Unit = {
  println("gunzip empty: " + run("gunzip", bytes.from_list([]))!)
  println("gunzip spaced: " + run("gunzip", bytes.from_list([32, 32, 115, 112, 97, 99, 101, 100, 32, 32]))!)
  println("gunzip one-ff: " + run("gunzip", bytes.from_list([255]))!)
  println("gunzip magic1: " + run("gunzip", bytes.from_list([31]))!)
  println("gunzip magic2: " + run("gunzip", bytes.from_list([31, 139]))!)
  println("gunzip hdr3: " + run("gunzip", bytes.from_list([31, 139, 8]))!)
  println("gunzip hdr9: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0]))!)
  println("gunzip hdr10: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255]))!)
  println("gunzip short-bad-method: " + run("gunzip", bytes.from_list([31, 139, 7]))!)
  println("gunzip bad-magic: " + run("gunzip", bytes.from_list([31, 138, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip bad-method: " + run("gunzip", bytes.from_list([31, 139, 7, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip reserved-flags: " + run("gunzip", bytes.from_list([31, 139, 8, 224, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip fextra-trunc: " + run("gunzip", bytes.from_list([31, 139, 8, 4, 0, 0, 0, 0, 0, 255, 50, 0, 1, 2]))!)
  println("gunzip fextra-ok: " + run("gunzip", bytes.from_list([31, 139, 8, 4, 0, 0, 0, 0, 0, 255, 2, 0, 1, 2, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip fname-unterminated: " + run("gunzip", bytes.from_list([31, 139, 8, 8, 0, 0, 0, 0, 0, 255, 110, 97, 109, 101]))!)
  println("gunzip fname-ok: " + run("gunzip", bytes.from_list([31, 139, 8, 8, 0, 0, 0, 0, 0, 255, 110, 0, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip fhcrc-ok: " + run("gunzip", bytes.from_list([31, 139, 8, 2, 0, 0, 0, 0, 0, 255, 144, 201, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip fhcrc-bad: " + run("gunzip", bytes.from_list([31, 139, 8, 2, 0, 0, 0, 0, 0, 255, 0, 0, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip fhcrc-cut: " + run("gunzip", bytes.from_list([31, 139, 8, 2, 0, 0, 0, 0, 0, 255, 0]))!)
  println("gunzip bad-crc: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 218, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip bad-isize: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 134, 0, 0, 0]))!)
  println("gunzip trailer-cut-1: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0]))!)
  println("gunzip trailer-cut-4: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67]))!)
  println("gunzip trailer-cut-7: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219]))!)
  println("gunzip trailer-cut-8: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1]))!)
  println("gunzip deflate-trunc: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243]))!)
  println("gunzip deflate-trunc-1: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165]))!)
  println("gunzip btype3: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 7, 219, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip bad-table: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 1, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip stored-nlen: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 1, 5, 0, 5, 0, 104, 101, 108, 108, 111, 219, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip far: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 3, 2, 0, 18, 217, 65, 255, 3, 0, 0, 0]))!)
  println("gunzip two-members: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0, 31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0]))!)
  println("gunzip trailing-garbage: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0, 120, 121, 122]))!)
  println("gunzip trailing-zeros: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0, 0, 0, 0, 0]))!)
  println("gunzip trailing-magic: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0, 31, 139]))!)
  println("gunzip second-far: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0, 31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 3, 2, 0, 18, 217, 65, 255, 3, 0, 0, 0]))!)
  println("gunzip second-bad-crc: " + run("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0, 31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 0, 0, 0, 0, 133, 0, 0, 0]))!)
  println("inflate empty: " + run("inflate", bytes.from_list([]))!)
  println("inflate spaced: " + run("inflate", bytes.from_list([32, 32, 115, 112, 97, 99, 101, 100, 32, 32]))!)
  println("inflate one-ff: " + run("inflate", bytes.from_list([255]))!)
  println("inflate btype3: " + run("inflate", bytes.from_list([7]))!)
  println("inflate stored-nlen: " + run("inflate", bytes.from_list([1, 5, 0, 5, 0, 104, 101, 108, 108, 111]))!)
  println("inflate stored-ok: " + run("inflate", bytes.from_list([1, 5, 0, 250, 255, 104, 101, 108, 108, 111]))!)
  println("inflate stored-trunc: " + run("inflate", bytes.from_list([1, 5, 0, 250, 255, 104, 101]))!)
  println("inflate far: " + run("inflate", bytes.from_list([3, 2, 0]))!)
  println("inflate lit286: " + run("inflate", bytes.from_list([27, 3]))!)
  println("inflate okfixed: " + run("inflate", bytes.from_list([75, 4, 2, 0]))!)
  println("inflate bad-table: " + run("inflate", bytes.from_list([165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 1, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1]))!)
  println("inflate trunc40: " + run("inflate", bytes.from_list([165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30]))!)
  println("inflate trunc1: " + run("inflate", bytes.from_list([165]))!)
  println("inflate trailing-garbage: " + run("inflate", bytes.from_list([165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 120, 121, 122]))!)
  println("inflate nonfinal-then-eof: " + run("inflate", bytes.from_list([0, 0, 0, 255, 255]))!)
  println("inflate dist30: " + run("inflate", bytes.from_list([75, 4, 62, 0]))!)
  println("decompress empty: " + run("decompress", bytes.from_list([]))!)
  println("decompress one-ff: " + run("decompress", bytes.from_list([255]))!)
  println("decompress hdr1: " + run("decompress", bytes.from_list([120]))!)
  println("decompress spaced: " + run("decompress", bytes.from_list([32, 32, 115, 112, 97, 99, 101, 100, 32, 32]))!)
  println("decompress hdr2: " + run("decompress", bytes.from_list([120, 156]))!)
  println("decompress hdr5: " + run("decompress", bytes.from_list([120, 156, 165, 204, 139]))!)
  println("decompress bad-cm: " + run("decompress", bytes.from_list([119, 156, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 122, 109, 47, 153]))!)
  println("decompress bad-fcheck: " + run("decompress", bytes.from_list([120, 157, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 122, 109, 47, 153]))!)
  println("decompress cinfo8: " + run("decompress", bytes.from_list([136, 152, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 122, 109, 47, 153]))!)
  println("decompress fdict: " + run("decompress", bytes.from_list([120, 187, 0, 0, 0, 1, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 122, 109, 47, 153]))!)
  println("decompress bad-adler: " + run("decompress", bytes.from_list([120, 156, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 122, 109, 47, 152]))!)
  println("decompress adler-cut-1: " + run("decompress", bytes.from_list([120, 156, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 122, 109, 47]))!)
  println("decompress adler-cut-2: " + run("decompress", bytes.from_list([120, 156, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 122, 109]))!)
  println("decompress adler-cut-4: " + run("decompress", bytes.from_list([120, 156, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1]))!)
  println("decompress deflate-trunc: " + run("decompress", bytes.from_list([120, 156, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198]))!)
  println("decompress btype3: " + run("decompress", bytes.from_list([120, 156, 7, 0, 0, 0, 1]))!)
  println("decompress bad-table: " + run("decompress", bytes.from_list([120, 156, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 1, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 122, 109, 47, 153]))!)
  println("decompress far: " + run("decompress", bytes.from_list([120, 156, 3, 2, 0, 0, 3, 0, 1]))!)
  println("decompress trailing-garbage: " + run("decompress", bytes.from_list([120, 156, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 122, 109, 47, 153, 120, 121, 122]))!)
  println("inflate far-after-lit: " + run("inflate", bytes.from_list([75, 4, 18, 0]))!)
  println("inflate trailing-xyz: " + run("inflate", bytes.from_list([203, 72, 205, 201, 201, 87, 200, 64, 34, 203, 243, 139, 114, 82, 0, 88, 89, 90]))!)
  println("decompress trailing-GARBAGE: " + run("decompress", bytes.from_list([120, 156, 203, 72, 205, 201, 201, 87, 200, 64, 34, 203, 243, 139, 114, 82, 0, 104, 125, 8, 197, 71, 65, 82, 66, 65, 71, 69]))!)
  println("decompress two-streams: " + run("decompress", bytes.from_list([120, 156, 203, 72, 205, 201, 201, 87, 200, 64, 39, 1, 104, 3, 8, 177, 120, 156, 43, 78, 77, 206, 207, 75, 81, 40, 46, 41, 74, 77, 204, 5, 0, 36, 49, 5, 41]))!)
  sweep("gunzip", bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0]))!
  sweep("inflate", bytes.from_list([165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1]))!
  sweep("decompress", bytes.from_list([120, 156, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 122, 109, 47, 153]))!
}
"#;

const EXPECTED: &[(&str, &str)] = &[
    ("gunzip empty", "err zlib.gunzip: unexpected end of file"),
    ("gunzip spaced", "err zlib.gunzip: invalid gzip header"),
    ("gunzip one-ff", "err zlib.gunzip: invalid gzip header"),
    ("gunzip magic1", "err zlib.gunzip: unexpected end of file"),
    ("gunzip magic2", "err zlib.gunzip: unexpected end of file"),
    ("gunzip hdr3", "err zlib.gunzip: unexpected end of file"),
    ("gunzip hdr9", "err zlib.gunzip: unexpected end of file"),
    ("gunzip hdr10", "err zlib.gunzip: incomplete deflate stream"),
    ("gunzip short-bad-method", "err zlib.gunzip: invalid gzip header"),
    ("gunzip bad-magic", "err zlib.gunzip: invalid gzip header"),
    ("gunzip bad-method", "err zlib.gunzip: invalid gzip header"),
    ("gunzip reserved-flags", "err zlib.gunzip: invalid gzip header"),
    ("gunzip fextra-trunc", "err zlib.gunzip: unexpected end of file"),
    ("gunzip fextra-ok", "ok 133 1129170907"),
    ("gunzip fname-unterminated", "err zlib.gunzip: unexpected end of file"),
    ("gunzip fname-ok", "ok 133 1129170907"),
    ("gunzip fhcrc-ok", "ok 133 1129170907"),
    ("gunzip fhcrc-bad", "err zlib.gunzip: header crc mismatch"),
    ("gunzip fhcrc-cut", "err zlib.gunzip: unexpected end of file"),
    ("gunzip bad-crc", "err zlib.gunzip: crc mismatch"),
    ("gunzip bad-isize", "err zlib.gunzip: size mismatch"),
    ("gunzip trailer-cut-1", "err zlib.gunzip: unexpected end of file"),
    ("gunzip trailer-cut-4", "err zlib.gunzip: unexpected end of file"),
    ("gunzip trailer-cut-7", "err zlib.gunzip: unexpected end of file"),
    ("gunzip trailer-cut-8", "err zlib.gunzip: unexpected end of file"),
    ("gunzip deflate-trunc", "err zlib.gunzip: incomplete deflate stream"),
    ("gunzip deflate-trunc-1", "err zlib.gunzip: incomplete deflate stream"),
    ("gunzip btype3", "err zlib.gunzip: corrupt deflate stream"),
    ("gunzip bad-table", "err zlib.gunzip: corrupt deflate stream"),
    ("gunzip stored-nlen", "err zlib.gunzip: corrupt deflate stream"),
    ("gunzip far", "err zlib.gunzip: corrupt deflate stream"),
    ("gunzip two-members", "ok 266 953613778"),
    ("gunzip trailing-garbage", "err zlib.gunzip: invalid gzip header"),
    ("gunzip trailing-zeros", "err zlib.gunzip: invalid gzip header"),
    ("gunzip trailing-magic", "err zlib.gunzip: unexpected end of file"),
    ("gunzip second-far", "err zlib.gunzip: corrupt deflate stream"),
    ("gunzip second-bad-crc", "err zlib.gunzip: crc mismatch"),
    ("inflate empty", "err zlib.inflate: incomplete deflate stream"),
    ("inflate spaced", "err zlib.inflate: corrupt deflate stream"),
    ("inflate one-ff", "err zlib.inflate: corrupt deflate stream"),
    ("inflate btype3", "err zlib.inflate: corrupt deflate stream"),
    ("inflate stored-nlen", "err zlib.inflate: corrupt deflate stream"),
    ("inflate stored-ok", "ok 5 907060870"),
    ("inflate stored-trunc", "err zlib.inflate: incomplete deflate stream"),
    ("inflate far", "err zlib.inflate: corrupt deflate stream"),
    ("inflate lit286", "err zlib.inflate: corrupt deflate stream"),
    ("inflate okfixed", "ok 4 2912478533"),
    ("inflate bad-table", "err zlib.inflate: corrupt deflate stream"),
    ("inflate trunc40", "err zlib.inflate: incomplete deflate stream"),
    ("inflate trunc1", "err zlib.inflate: incomplete deflate stream"),
    ("inflate trailing-garbage", "err zlib.inflate: trailing data after stream"),
    ("inflate nonfinal-then-eof", "err zlib.inflate: incomplete deflate stream"),
    ("inflate dist30", "err zlib.inflate: corrupt deflate stream"),
    ("decompress empty", "err zlib.decompress: unexpected end of file"),
    ("decompress one-ff", "err zlib.decompress: unexpected end of file"),
    ("decompress hdr1", "err zlib.decompress: unexpected end of file"),
    ("decompress spaced", "err zlib.decompress: invalid zlib header"),
    ("decompress hdr2", "err zlib.decompress: incomplete deflate stream"),
    ("decompress hdr5", "err zlib.decompress: incomplete deflate stream"),
    ("decompress bad-cm", "err zlib.decompress: invalid zlib header"),
    ("decompress bad-fcheck", "err zlib.decompress: invalid zlib header"),
    ("decompress cinfo8", "err zlib.decompress: invalid zlib header"),
    ("decompress fdict", "err zlib.decompress: preset dictionary unsupported"),
    ("decompress bad-adler", "err zlib.decompress: adler32 mismatch"),
    ("decompress adler-cut-1", "err zlib.decompress: unexpected end of file"),
    ("decompress adler-cut-2", "err zlib.decompress: unexpected end of file"),
    ("decompress adler-cut-4", "err zlib.decompress: unexpected end of file"),
    ("decompress deflate-trunc", "err zlib.decompress: incomplete deflate stream"),
    ("decompress btype3", "err zlib.decompress: corrupt deflate stream"),
    ("decompress bad-table", "err zlib.decompress: corrupt deflate stream"),
    ("decompress far", "err zlib.decompress: corrupt deflate stream"),
    ("decompress trailing-garbage", "err zlib.decompress: trailing data after stream"),
    // #4268: a distance before the first output byte, and bytes after the stream
    ("inflate far-after-lit", "err zlib.inflate: corrupt deflate stream"),
    ("inflate trailing-xyz", "err zlib.inflate: trailing data after stream"),
    ("decompress trailing-GARBAGE", "err zlib.decompress: trailing data after stream"),
    ("decompress two-streams", "err zlib.decompress: trailing data after stream"),
];

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn probe_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-zlib-err-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    std::fs::write(d.join("probe.almd"), PROBE).expect("write probe");
    d
}

fn run_leg(dir: &Path, args: &[&str]) -> Vec<String> {
    let o = Command::new(almide_bin())
        .arg("run")
        .arg("probe.almd")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn almide");
    assert!(o.status.success(), "leg {args:?} failed:\n{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8").lines().map(str::to_string).collect()
}

fn split(line: &str) -> (&str, &str) {
    line.split_once(": ").unwrap_or((line, ""))
}

#[test]
fn zlib_decoders_answer_one_err_text_per_corrupt_class_on_every_leg() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let dir = probe_dir();
    let native = run_leg(&dir, &[]);
    let wasm = run_leg(&dir, &["--target", "wasm"]);
    let _ = std::fs::remove_dir_all(&dir);
    let mut bad = Vec::new();
    for (i, (name, want)) in EXPECTED.iter().enumerate() {
        for (label, out) in [("native", &native), ("wasm", &wasm)] {
            let (got_name, got) = split(out.get(i).map(String::as_str).unwrap_or(""));
            if got_name != *name || got != *want {
                bad.push(format!("{label} {name}: got `{got}`, want `{want}`"));
            }
        }
    }
    assert_eq!(native.len(), wasm.len(), "the legs printed a different number of lines");
    for (n, w) in native.iter().zip(&wasm).skip(EXPECTED.len()) {
        if n != w {
            bad.push(format!("sweep diverges: native `{n}` vs wasm `{}`", split(w).1));
        }
    }
    assert!(bad.is_empty(), "{} zlib decode answers differ (C-331):\n{}", bad.len(), bad.join("\n"));
}
