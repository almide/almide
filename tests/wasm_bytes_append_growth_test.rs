//! #3294: on `--target wasm` every k-byte member of the bytes append family
//! (`append_*_{le,be}`, `append_u8`, `append_bool`, `append_bytes`, `push`)
//! built a fresh `len + k` block and copied the payload on every call, so a
//! buffer built by appending was quadratic and ran out of memory near 640 KB
//! (160 000 × `append_f32_le`). They now share `$bytes_push`'s amortized growth.
//!
//! Pinned: the 160k-append loops complete with the right length, and the whole
//! family's bytes — edge values, NaN, aliasing snapshot, a `mut` parameter —
//! are identical to native's.
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn run(src: &str, wasm: bool) -> (bool, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("m.almd");
    std::fs::write(&file, src).expect("write");
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(&file);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let out = cmd.output().expect("spawn almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn appending_160k_values_completes_on_wasm() {
    for (call, width) in [
        ("bytes.append_f32_le(b, float.from(i))", 4),
        ("bytes.append_i64_be(b, i)", 8),
        ("bytes.append_u32_be(b, i)", 4),
        ("bytes.append_bool(b, i % 2 == 0)", 1),
        ("bytes.append_u16_le(b, i)", 2),
    ] {
        let src = format!(
            "effect fn main() -> Unit = {{\n  var b = bytes.zeros(0)\n  for i in 0..<160000 {{ {call} }}\n  println(int.to_string(bytes.len(b)))\n}}\n"
        );
        let (ok, out, err) = run(&src, true);
        assert!(ok, "{call}: wasm run failed (quadratic growth ran out of memory before #3294):\n{err}");
        assert_eq!(out.trim(), (160000 * width).to_string(), "{call}");
    }
}

const FAMILY: &str = r#"fn fill(mut p: Bytes) -> Unit = {
  bytes.append_u32_be(p, 7)
  bytes.append_u32_be(p, bytes.len(bytes.from_string("hé")))
  bytes.append_bytes(p, bytes.from_string("hé"))
}

effect fn main() -> Unit = {
  var b = bytes.zeros(0)
  bytes.append_u16_le(b, -2)
  bytes.append_u16_be(b, 65535)
  bytes.append_i16_le(b, -300)
  bytes.append_i16_be(b, 300)
  bytes.append_u32_le(b, 4294967295)
  bytes.append_u32_be(b, 1)
  bytes.append_i32_le(b, -1)
  bytes.append_i32_be(b, -123456)
  bytes.append_i64_le(b, -9223372036854775807)
  bytes.append_i64_be(b, 81985529216486895)
  bytes.append_f32_le(b, 1.5)
  bytes.append_f32_be(b, 0.0 / 0.0)
  bytes.append_f64_le(b, -2.25)
  bytes.append_f64_be(b, 0.0 / 0.0)
  bytes.append_u8(b, 300)
  let snap = b
  bytes.push(b, 513)
  bytes.append_u32_be(b, 3735928559)
  bytes.append_i64_be(b, -5)
  bytes.append_f64_be(b, 3.0)
  bytes.append_bool(b, true)
  bytes.append_bool(b, false)
  bytes.append_u32_be(b, bytes.len(bytes.from_string("añb")))
  bytes.append_bytes(b, bytes.from_string("añb"))
  bytes.append_u16_le(b, 258)
  bytes.append_u16_be(b, 258)
  bytes.append_u32_le(b, 16909060)
  bytes.append_i32_be(b, -2)
  bytes.append_f32_le(b, 2.5)
  fill(b)
  println(int.to_string(bytes.len(snap)))
  println(int.to_string(bytes.len(b)))
  println(bytes.to_list(b) |> list.map((x) => int.to_string(x)) |> list.join(","))
  var big = bytes.zeros(3)
  for i in 0..<5000 { bytes.append_f64_le(big, float.from(i)) }
  println(int.to_string(bytes.len(big)))
  println(float.to_string(bytes.get_f64_le(big, 3 + 8 * 4999) ?? 0.0))
}
"#;

#[test]
fn the_whole_family_writes_the_bytes_native_writes() {
    let (ok, native, err) = run(FAMILY, false);
    assert!(ok, "native run failed:\n{err}");
    let (ok, wasm, err) = run(FAMILY, true);
    assert!(ok, "wasm run failed:\n{err}");
    assert_eq!(wasm, native, "wasm bytes must equal native bytes");
    // The snapshot taken before the writers kept its length: the growth
    // never writes through a shared block.
    assert!(native.starts_with("65\n123\n"), "{native}");
}
