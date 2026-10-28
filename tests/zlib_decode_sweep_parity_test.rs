//! C-331, widened: the zlib decoders agree on native and wasm for EVERY short
//! input, not only for mutations of valid vectors. The named-case test
//! (zlib_decode_error_parity_test.rs) pins the texts; this sweep pins the
//! precedence between them. A dynamic-Huffman block header whose HLIT/HDIST is
//! out of range is malformed as soon as the header is read — native reports
//! `corrupt deflate stream` even when the input then ends, while the
//! self-hosted decoder read the code-length codes past the end first and
//! reported `incomplete deflate stream` (`[100, 255, 255]`, GCE inspection).
//!
//! The probe decodes every 1- and 2-byte input with all three decoders, every
//! 3-byte raw input whose last byte is one of five values (the full 2^24 set was
//! swept by hand), and a seeded random set of 4..64-byte inputs (behind a valid
//! container header for gunzip/decompress), and both legs must print the same
//! token for each.
use std::path::{Path, PathBuf};
use std::process::Command;

const PROBE: &str = r#"import zlib
import hash

// One token per decode: the output length and crc for ok, the err text's
// failure class otherwise (the texts are pinned by the named-case test).
fn tok(fname: String, r: Result[Bytes, String]) -> String = match r {
  ok(o) => "[ok ${bytes.len(o)} ${hash.crc32(o)}]",
  err(e) => "[" + string.replace(e, "zlib.${fname}: ", "") + "]",
}

effect fn dec(f: Int, b: Bytes) -> String = match f {
  0 => {
    let r: Result[Bytes, String] = zlib.inflate(b)
    tok("inflate", r)
  },
  1 => {
    let r: Result[Bytes, String] = zlib.gunzip(b)
    tok("gunzip", r)
  },
  _ => {
    let r: Result[Bytes, String] = zlib.decompress(b)
    tok("decompress", r)
  },
}

fn lcg(s: Int) -> Int = (s * 1103515245 + 12345) % 2147483648

effect fn main() -> Unit = {
  // GCE inspection: a dynamic block whose HDIST is out of range, cut short
  println("named: " + dec(0, bytes.from_list([100, 255, 255]))!)
  // every 1- and 2-byte input, all three decoders
  for f in 0..<3 {
    var one = ""
    for a in 0..<256 {
      one = one + dec(f, bytes.from_list([a]))!
    }
    println("${f} len1: ${one}")
    for a in 0..<256 {
      var line = ""
      for b in 0..<256 {
        line = line + dec(f, bytes.from_list([a, b]))!
      }
      println("${f} len2 ${a}: ${line}")
    }
  }
  // every 3-byte raw input whose last byte is one of five values
  for a in 0..<256 {
    var line = ""
    for b in 0..<256 {
      for c in [0, 1, 127, 128, 255] {
        line = line + dec(0, bytes.from_list([a, b, c]))!
      }
    }
    println("0 len3 ${a}: ${line}")
  }
  // seeded random 4..64-byte inputs; the container decoders get a valid header first
  var s = 20261016
  for f in 0..<3 {
    for k in 0..<64 {
      var line = ""
      for _ in 0..<64 {
        s = lcg(s)
        let n = 4 + s % 61
        var xs: List[Int] = match f {
          1 => [31, 139, 8, 0, 0, 0, 0, 0, 0, 255],
          2 => [120, 156],
          _ => [],
        }
        for i in 0..<n {
          s = lcg(s)
          xs = xs + [int.bshr(s, 16) % 256]
        }
        line = line + dec(f, bytes.from_list(xs))!
      }
      println("${f} rand ${k}: ${line}")
    }
  }
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

fn probe_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-zlib-sweep-parity-{}", std::process::id()));
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

fn tokens(line: &str) -> (&str, Vec<&str>) {
    let (key, rest) = line.split_once(": ").unwrap_or((line, ""));
    (key, rest.split_inclusive(']').collect())
}

#[test]
fn zlib_decoders_agree_on_every_short_input_on_every_leg() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let dir = probe_dir();
    let native = run_leg(&dir, &[]);
    let wasm = run_leg(&dir, &["--target", "wasm"]);
    let _ = std::fs::remove_dir_all(&dir);
    for (label, out) in [("native", &native), ("wasm", &wasm)] {
        assert_eq!(out.first().map(String::as_str), Some("named: [corrupt deflate stream]"), "the {label} leg");
    }
    assert_eq!(native.len(), wasm.len(), "the legs printed a different number of lines");
    let mut bad = Vec::new();
    for (n, w) in native.iter().zip(&wasm).filter(|(n, w)| n != w) {
        let ((key, tn), (_, tw)) = (tokens(n), tokens(w));
        for (i, (a, b)) in tn.iter().zip(&tw).enumerate().filter(|(_, (a, b))| a != b) {
            bad.push(format!("{key} #{i}: native {a} vs wasm {b}"));
        }
        if tn.len() != tw.len() {
            bad.push(format!("{key}: {} vs {} tokens", tn.len(), tw.len()));
        }
    }
    let shown: Vec<_> = bad.iter().take(20).cloned().collect();
    assert!(bad.is_empty(), "{} decodes differ across legs (C-331), first {}:\n{}", bad.len(), shown.len(), shown.join("\n"));
}
