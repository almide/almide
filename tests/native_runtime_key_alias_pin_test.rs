//! NATIVE-leg regression pin for #1813: a user type named `Key` must not
//! collide with the runtime's object-key alias in the generated Rust.
//!
//! `runtime/rs/src/value.rs` spelled its `Cow<'static, str>` key alias `Key`,
//! and every runtime module is spliced flat into the user's module, so a
//! program declaring `type Key = { id: Int }` alongside anything that pulls
//! the `value` module in (json here) was E0428 at rustc — check green, build
//! red (ALS-T6). The alias is `AlmideKey` now. The rlib fast path hid the
//! collision (`use almide_rt::*` is shadowed legally by a local struct), so
//! this pin forces the inline build with `ALMIDE_NO_RTLIB=1`, the path the
//! report's failure came through. The wasm leg has no runtime prelude to
//! collide with, so this pins NATIVE only, per the
//! tests/native_mut_param_pins_test.rs doctrine.

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
fn user_type_named_key_builds_on_the_inline_native_path() {
    let dir = std::env::temp_dir().join(format!("almd_1813_pin_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("key.almd");
    std::fs::write(
        &src,
        "import json\ntype Key = { id: Int }\neffect fn main() -> Unit = {\n  let k = Key { id: 1 }\n  let v = json.parse(\"{\\\"a\\\":1}\")!\n  println(\"${k.id} ${json.stringify(v)}\")\n}\n",
    )
    .unwrap();
    let out = Command::new(almide_bin())
        .args(["run", src.to_str().unwrap()])
        .env("ALMIDE_NO_RTLIB", "1")
        .current_dir(&dir)
        .output()
        .expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    std::fs::remove_dir_all(&dir).ok();
    assert!(!stderr.contains("E0428"), "the user's `Key` collided with the runtime alias again:\n{stderr}");
    assert!(
        out.status.success() && stdout == "1 {\"a\":1}\n",
        "a user type named Key must build and run on the inline native path:\nstdout={stdout}\nstderr={stderr}"
    );
}
