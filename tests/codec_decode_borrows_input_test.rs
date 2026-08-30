//! A derived `T.decode` borrows its `Value` input (#1679, first slice).
//!
//! `decode` reads its input through the `value.*` intrinsics — every one of
//! which already takes `&Value` — and never needs to own it, yet the derived
//! fn was emitted as `fn T_decode(_v: Value)`: every call site handed it a
//! full copy of the object (8 fields, a nested record and a list, on the
//! #1673 workload) to read once. Derives were excluded from borrow inference
//! wholesale; now their `Value` params take part, and the list/option codec
//! drivers pick the runtime twin whose `Fn` bound matches the by-reference
//! per-element decoder they are handed.
//!
//! Emit-shape tests, in the mold of `tco_accumulator_move_test.rs`. Skips
//! cleanly when the `almide` binary is unavailable.

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

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn emitted(source: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almide-decode-borrow-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    let output = Command::new(almide_bin())
        .args([src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("failed to spawn almide");
    let rust = String::from_utf8_lossy(&output.stdout).to_string();
    std::fs::remove_dir_all(&dir).ok();
    assert!(output.status.success(), "--target rust emit failed:\n{}", String::from_utf8_lossy(&output.stderr));
    rust
}

const SRC: &str = "type Address: Codec = { city: String }\n\
    type User: Codec = { name: String, tags: List[String], address: Address, homes: List[Address], alt: Address? }\n\
    fn main() -> Unit = {\n\
      let v = value.object([(\"name\", value.str(\"a\")), (\"tags\", value.array([])), (\"address\", value.object([(\"city\", value.str(\"t\"))])), (\"homes\", value.array([]))])\n\
      match User.decode(v) { ok(u) => println(u.name), err(e) => println(e) }\n\
    }\n";

#[test]
fn derived_decode_takes_its_input_by_reference() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(SRC, "sig");
    assert!(rust.contains("pub fn User_decode(_v: &Value)"), "derived decode must borrow its Value input:\n{}", grep(&rust, "_decode("));
    assert!(rust.contains("pub fn Address_decode(_v: &Value)"), "nested record decode must borrow too:\n{}", grep(&rust, "_decode("));
    assert!(rust.contains("User_decode(&v)"), "the call site hands over a borrow, not a copy:\n{}", grep(&rust, "User_decode("));
    assert!(rust.contains("Address_decode(almide_rt_value_field_ref(_v, \"address\")?)"), "a nested field decodes a BORROW into the object — no copy of the field:\n{}", grep(&rust, "Address_decode("));
}

#[test]
fn every_field_read_is_a_borrowed_lookup() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(SRC, "field-ref");
    let decode = rust.lines().skip_while(|l| !l.starts_with("pub fn User_decode(")).take_while(|l| !l.starts_with('}')).collect::<Vec<_>>().join("\n");
    assert!(decode.contains("almide_rt_value_as_string(almide_rt_value_field_ref(_v, \"name\")?)"), "a primitive field is read straight off the borrowed lookup:\n{decode}");
    assert!(decode.contains("almide_rt___decode_list_string(almide_rt_value_field_ref(_v, \"tags\")?)"), "a List[String] field hands the primitive list decoder a borrow:\n{decode}");
    assert!(!decode.contains("almide_rt_value_field(_v"), "an owned field lookup survived in the decode body — that is a copy nobody needs:\n{decode}");
}

#[test]
fn list_and_option_drivers_take_the_by_reference_twin() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(SRC, "drivers");
    assert!(rust.contains("almide_rt_value_decode_list_ref(almide_rt_value_field_ref(_v, \"homes\")?, Address_decode)"),
        "a List[Record] field must route to the `&Value` list driver with the by-ref element decoder:\n{}", grep(&rust, "decode_list"));
    assert!(!rust.contains("almide_rt_value_decode_list(&"), "the by-value list driver must never receive a borrow:\n{}", grep(&rust, "decode_list"));
    assert!(rust.contains("__decode_option_Address(_v: &Value, _key: String)"), "the derived option worker borrows its Value too:\n{}", grep(&rust, "__decode_option_Address"));
}

/// The lines of `rust` mentioning `needle` — a failure names the shapes that
/// were emitted instead of dumping the whole program.
fn grep(rust: &str, needle: &str) -> String {
    rust.lines().filter(|l| l.contains(needle)).collect::<Vec<_>>().join("\n")
}
