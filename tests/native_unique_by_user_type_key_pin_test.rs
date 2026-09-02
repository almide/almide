//! NATIVE-leg regression pin for #1812: `list.unique_by` keyed by a USER type
//! (a record, a nullary variant, a payload variant) and by a Float must build
//! and dedup with the first-seen, first-occurrence discipline.
//!
//! `almide_rt_list_unique_by` required `K: Eq + Hash` for a `HashSet` seen
//! set; a record or variant derives `Clone, Debug, PartialEq` only, so the
//! program passed `almide check` and died at rustc with E0277 (ALS-T6
//! check-passes/build-fails) while the structural wasm leg printed the
//! answer. The seen set is a `PartialEq` scan now, so the two legs share one
//! key rule: `-0.0 == 0.0` collapses and NaN never matches. The cross-target
//! rows belong to C-053's fixture (spec/wasm_cross/list_unique_by_nonscalar_key.almd,
//! which lands with #1811); this file pins NATIVE, per the
//! tests/native_mut_param_pins_test.rs doctrine, without putting a
//! wasm-skipped file under spec/ (the wasm coverage ratchet counts those).

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
fn unique_by_keyed_by_record_variant_and_float_builds_and_dedups_native() {
    let dir = std::env::temp_dir().join(format!("almd_1812_pin_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("unique_by_user_key.almd");
    std::fs::write(
        &src,
        concat!(
            "type K = { a: String, n: Int }\n",
            "type Parity = | Even | Odd\n",
            "type Tag = | Lo(Int) | Hi\n",
            "fn main() -> Unit = {\n",
            "  println(\"${list.unique_by([\"ab\", \"abc\", \"ad\", \"bc\", \"b\", \"bd\"], (s) => K { a: string.take(s, 1), n: string.len(s) % 2 })}\")\n",
            "  println(\"${list.unique_by([1, 2, 3, 4, 5], (x) => if x % 2 == 0 then Even else Odd)}\")\n",
            "  println(\"${list.unique_by([1, 4, 7, 10, 12, 2], (x) => if x < 10 then Lo(x % 3) else Hi)}\")\n",
            "  println(\"${list.unique_by([1, 2, 3], (x) => if x == 1 then 0.0 else -0.0)}\")\n",
            "  println(\"${list.unique_by([1, 2, 3], (x) => 0.0 / 0.0)}\")\n",
            "}\n",
        ),
    )
    .unwrap();
    let out = Command::new(almide_bin())
        .args(["run", src.to_str().unwrap()])
        .current_dir(&dir)
        .output()
        .expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    std::fs::remove_dir_all(&dir).ok();
    assert!(!stderr.contains("E0277"), "a user-type key needs Eq + Hash again:\n{stderr}");
    let expected = "[\"ab\", \"abc\", \"bc\", \"b\"]\n[1, 2]\n[1, 10, 2]\n[1]\n[1, 2, 3]\n";
    assert!(
        out.status.success() && stdout == expected,
        "unique_by over record/variant/Float keys must build and keep first occurrences natively:\nstdout={stdout}\nstderr={stderr}"
    );
}
