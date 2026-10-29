//! C-390 (R-L0-47, R-zlib-04, #4020): the zlib decoders bound their output,
//! and the bound is judged in the same stream order on every leg.
//!
//! Two probes, each run on native and on wasm:
//!
//! - THE DEFAULT BOUND. A compact fixed-Huffman DEFLATE stream (a literal 0,
//!   then length-258 distance-1 matches: 13 bytes for every 8 matches, so the
//!   probe rebuilds it with `bytes.repeat`) inflates to exactly 268435456 zero
//!   bytes, and its one-byte-longer twin to 268435457. Each decoder answers ok
//!   at the bound and `zlib: output exceeds 268435456 bytes` one past it —
//!   the compression-bomb shape, about 1.7 MB of input for 256 MiB of output.
//!   (spec/wasm_cross/zlib_output_bound.almd walks the `_max` bound across a
//!   small output; this is the default's own number.)
//! - THE ORDER. Every prefix and every single-byte xor of the three vectors
//!   tests/zlib_decode_error_parity_test.rs sweeps, decoded under bounds
//!   inside and around their 133-byte output, must print the same line on both
//!   legs: where a corrupt or truncated stream meets the bound, the two legs
//!   name the same failure.
use std::path::{Path, PathBuf};
use std::process::Command;

const BOMB: &str = r#"import zlib

fn show(r: Result[Bytes, String]) -> String = match r {
  ok(b) => "ok ${bytes.len(b)} ${bytes.get(b, bytes.len(b) - 1) ?? 9}",
  err(e) => "err ${e}",
}

fn le32(n: Int) -> Bytes = bytes.from_list([n % 256, n / 256 % 256, n / 65536 % 256, n / 16777216 % 256])

fn be32(n: Int) -> Bytes = bytes.from_list([n / 16777216 % 256, n / 65536 % 256, n / 256 % 256, n % 256])

fn raw(extra: Int) -> Bytes = {
  let period = bytes.from_list([24, 5, 163, 96, 20, 140, 130, 81, 48, 10, 70, 193, 40])
  let tail = if extra == 0 then 128 else 192
  (bytes.from_list([99]) + bytes.repeat(period, 130055)) + bytes.from_list([24, 5, 163, 96, 20, 140, 130, 81, 48, 10, 70, 193, tail, 3, 0])
}

fn zl(extra: Int) -> Bytes = {
  let adler = if extra == 0 then 4026531841 else 4026597377
  (bytes.from_list([120, 1]) + raw(extra)) + be32(adler)
}

fn gz(extra: Int) -> Bytes = {
  let crc = if extra == 0 then 705592763 else 2392558580
  let head = bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255])
  ((head + raw(extra)) + le32(crc)) + le32(268435456 + extra)
}

effect fn main() -> Unit = {
  println("inflate at: " + show(zlib.inflate(raw(0))))
  println("inflate over: " + show(zlib.inflate(raw(1))))
  println("decompress at: " + show(zlib.decompress(zl(0))))
  println("decompress over: " + show(zlib.decompress(zl(1))))
  println("gunzip at: " + show(zlib.gunzip(gz(0))))
  println("gunzip over: " + show(zlib.gunzip(gz(1))))
}
"#;

const BOMB_EXPECTED: &[&str] = &[
    "inflate at: ok 268435456 0",
    "inflate over: err zlib: output exceeds 268435456 bytes",
    "decompress at: ok 268435456 0",
    "decompress over: err zlib: output exceeds 268435456 bytes",
    "gunzip at: ok 268435456 0",
    "gunzip over: err zlib: output exceeds 268435456 bytes",
];

const ORDER: &str = r#"import zlib
import hash

fn show(r: Result[Bytes, String]) -> String = match r {
  ok(b) => "ok ${bytes.len(b)} ${hash.crc32(b)}",
  err(e) => "err ${e}",
}

fn run(f: String, b: Bytes, n: Int) -> String = match f {
  "gunzip" => show(zlib.gunzip_max(b, n)),
  "inflate" => show(zlib.inflate_max(b, n)),
  _ => show(zlib.decompress_max(b, n)),
}

// every bound around the output, then every prefix and single-byte xor under
// bounds that cut the output early, late and not at all
fn sweep(f: String, v: Bytes) -> Unit = {
  for n in 1...140 {
    println("${f} bound ${n}: " + run(f, v, n))
  }
  for n in [1, 40, 132, 133] {
    for i in 0..<bytes.len(v) {
      println("${f} bound ${n} cut ${i}: " + run(f, bytes.slice(v, 0, i), n))
    }
    for x in [1, 128, 255] {
      for i in 0..<bytes.len(v) {
        println("${f} bound ${n} x${x} ${i}: " + run(f, bytes.set(v, i, int.bxor(bytes.get(v, i) ?? 0, x)), n))
      }
    }
  }
}

effect fn main() -> Unit = {
  sweep("gunzip", @GUNZIP@)
  sweep("inflate", @INFLATE@)
  sweep("decompress", @DECOMPRESS@)
}
"#;

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

fn probe_dir(tag: &str, src: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-zlib-bound-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    std::fs::write(d.join("probe.almd"), src).expect("write probe");
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

fn have_almide() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

#[test]
fn the_default_bound_is_256_mib_on_every_leg() {
    if !have_almide() {
        return;
    }
    let dir = probe_dir("bomb", BOMB);
    let native = run_leg(&dir, &[]);
    let wasm = run_leg(&dir, &["--target", "wasm"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(native, BOMB_EXPECTED, "native");
    assert_eq!(wasm, BOMB_EXPECTED, "wasm");
}

#[test]
fn the_bound_meets_corrupt_and_truncated_input_in_one_order_on_every_leg() {
    if !have_almide() {
        return;
    }
    let src = ORDER
        .replace("@GUNZIP@", GUNZIP)
        .replace("@INFLATE@", INFLATE)
        .replace("@DECOMPRESS@", DECOMPRESS);
    let dir = probe_dir("order", &src);
    let native = run_leg(&dir, &[]);
    let wasm = run_leg(&dir, &["--target", "wasm"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(native.len(), wasm.len(), "the legs printed a different number of lines");
    let bad: Vec<String> = native
        .iter()
        .zip(&wasm)
        .filter(|(n, w)| n != w)
        .map(|(n, w)| format!("native `{n}` vs wasm `{w}`"))
        .collect();
    assert!(bad.is_empty(), "{} bounded decodes differ (C-390):\n{}", bad.len(), bad.join("\n"));
    // The sweep reached every outcome class, the bound among them.
    for want in ["err zlib: output exceeds", "ok 133", "incomplete deflate stream", "corrupt deflate stream"] {
        assert!(native.iter().any(|l| l.contains(want)), "no `{want}` line: the sweep did not reach it");
    }
}

const GUNZIP: &str = "bytes.from_list([31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 219, 199, 77, 67, 133, 0, 0, 0])";
const INFLATE: &str = "bytes.from_list([165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1])";
const DECOMPRESS: &str = "bytes.from_list([120, 156, 165, 204, 139, 13, 128, 32, 12, 4, 208, 85, 110, 0, 112, 0, 183, 225, 115, 74, 147, 74, 19, 104, 76, 112, 122, 221, 193, 1, 222, 243, 70, 36, 189, 164, 18, 143, 74, 198, 164, 30, 177, 217, 244, 29, 214, 137, 202, 98, 149, 35, 128, 55, 199, 130, 242, 12, 200, 203, 25, 63, 208, 93, 74, 210, 13, 254, 187, 120, 1, 122, 109, 47, 153])";
