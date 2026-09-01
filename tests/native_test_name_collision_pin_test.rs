//! NATIVE-leg regression pin for #1721: two test names whose ASCII skeleton
//! and non-ASCII count coincide must NOT collide in the generated Rust.
//!
//! The walker replaced every non-ASCII char with `_`, so for a non-English
//! project the mangled name carried nothing beyond the ASCII prefix and the
//! length — 2 of 11 real test names collided (E0428) on the reporter's first
//! try. The fix appends an 8-hex FNV-1a of the original spelling whenever the
//! name contains non-ASCII; ASCII names keep their exact historical spelling.
//! The wasm leg never mangles (IR names stay raw), so this pins NATIVE, per
//! the tests/native_mut_param_pins_test.rs doctrine. `// wasm:skip` forces
//! the native fallback exactly as the report's CI leg ran.

use std::path::Path;
use std::process::Command;

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

#[test]
fn same_skeleton_nonascii_test_names_build_and_pass_native() {
    let dir = std::env::temp_dir().join(format!("almd_1721_pin_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("almide.toml"), "[package]\nname = \"pin1721\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(dir.join("src/mod.almd"), "fn ident(n: Int) -> Int = n\n").unwrap();
    // The issue's exact pair: same ASCII prefix, same non-ASCII length.
    std::fs::write(
        dir.join("tests/x_test.almd"),
        "// wasm:skip\ntest \"a: 値を取る旗に値が無ければ断る\" {\n  assert_eq(1, 1)\n}\n\ntest \"a: 旗の値は位置引数に混ざらない\" {\n  assert_eq(1, 1)\n}\n",
    )
    .unwrap();
    let out = Command::new(almide_bin())
        .args(["test", "tests/x_test.almd"])
        .current_dir(&dir)
        .output()
        .expect("failed to spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    std::fs::remove_dir_all(&dir).ok();
    assert!(!text.contains("E0428"), "the mangled names collided again:\n{text}");
    assert!(
        out.status.success() && text.contains("0 failed"),
        "colliding-skeleton test names must build and pass on the native leg:\n{text}"
    );
}
