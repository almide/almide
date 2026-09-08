//! A derived `T.decode` pays its #1675 error frame only when a field fails
//! (#2050).
//!
//! Every field of a derived decode carries a path frame: an `Err` gets the
//! field's key spliced in (`expected Int, received Str, at x`). The frame was
//! a per-type worker `T___erratw_<mangle>(r, "key")` that matched a CLONE of
//! the field's `Result` and took the key as an owned `String` — a payload
//! clone and a key allocation on the SUCCESS path of every field, 2x the
//! hand-written reference on the 8-field `decode` perf row. On the native
//! leg the frame is now `.map_err(|_we| almide_rt___err_at(_we, "key"
//! .to_string()))` on the field's own result: the success path is the plain
//! `?`, the key stays a `&'static str` literal until an error needs it, and
//! the workers no call site names are dropped from the program.
//!
//! Emit-shape tests, in the mold of `codec_decode_borrows_input_test.rs`.
//! Skips cleanly when the `almide` binary is unavailable.

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
    let dir = std::env::temp_dir().join(format!("almide-decode-frame-{}-{}", tag, std::process::id()));
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

/// The lines of `rust` from `pub fn <name>(` to the fn's closing brace.
fn body_of(rust: &str, name: &str) -> String {
    let head = format!("pub fn {name}(");
    rust.lines().skip_while(|l| !l.starts_with(&head)).take_while(|l| !l.starts_with('}')).collect::<Vec<_>>().join("\n")
}

const SRC: &str = "type Address: Codec = { city: String, zip: String }\n\
    type User: Codec = { id: Int, name: String, tags: List[String], address: Address, homes: List[Address] }\n\
    fn main() -> Unit = {\n\
      let v = value.object([(\"id\", value.int(1)), (\"name\", value.str(\"a\")), (\"tags\", value.array([])), (\"address\", value.object([(\"city\", value.str(\"t\")), (\"zip\", value.str(\"z\"))])), (\"homes\", value.array([]))])\n\
      match User.decode(v) { ok(u) => println(u.name), err(e) => println(e) }\n\
    }\n";

#[test]
fn every_field_frame_is_a_map_err_on_the_borrowed_lookup() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(SRC, "frame");
    let decode = body_of(&rust, "User_decode");
    for (key, read) in [
        ("id", "almide_rt_value_as_int(almide_rt_value_field_ref_at(_v, \"id\", 0)?)"),
        ("name", "almide_rt_value_as_string(almide_rt_value_field_ref_at(_v, \"name\", 1)?)"),
        ("tags", "almide_rt___decode_list_string(almide_rt_value_field_ref_at(_v, \"tags\", 2)?)"),
        ("address", "Address_decode(almide_rt_value_field_ref_at(_v, \"address\", 3)?)"),
        ("homes", "almide_rt_value_decode_list_ref(almide_rt_value_field_ref_at(_v, \"homes\", 4)?, Address_decode)"),
    ] {
        let frame = format!("({read}.map_err(|_we| almide_rt___err_at(_we, {key:?}.to_string())))?");
        assert!(decode.contains(&frame), "field `{key}` must read its borrowed lookup with the frame as a `.map_err` on it:\n{decode}");
    }
    let nested = body_of(&rust, "Address_decode");
    assert!(nested.contains("(almide_rt_value_as_string(almide_rt_value_field_ref_at(_v, \"city\", 0)?).map_err(|_we| almide_rt___err_at(_we, \"city\".to_string())))?"),
        "the nested record's own decode carries the same frame shape:\n{nested}");
}

#[test]
fn the_success_path_clones_no_payload_and_allocates_no_key() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(SRC, "success");
    for name in ["User_decode", "Address_decode"] {
        let decode = body_of(&rust, name);
        assert!(!decode.contains(".clone()"), "{name} must not clone anything on the way to its record:\n{decode}");
        // The only owned copy of a key is the one inside a `map_err` closure — the error path.
        for line in decode.lines().filter(|l| l.contains(".to_string()")) {
            assert!(line.contains(".map_err(|_we|"), "{name} allocates a key outside the error closure:\n{line}");
        }
    }
}

#[test]
fn the_frame_workers_are_not_emitted() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(SRC, "workers");
    let workers: Vec<&str> = rust.lines().filter(|l| l.contains("___erratw_")).collect();
    assert!(workers.is_empty(), "no call site names a frame worker any more, so none may be emitted:\n{}", workers.join("\n"));
}

/// The frame's bytes are #1675's: a failing field reports `, at <key>`, a
/// nested failure prepends the parent's key, and the error the wasm leg
/// (whose worker is unchanged) prints is the one native prints.
#[test]
fn the_error_text_is_unchanged() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let src = "type Address: Codec = { city: String, zip: String }\n\
        type User: Codec = { id: Int, name: String, address: Address }\n\
        fn show(v: Value) -> String = match User.decode(v) { ok(u) => u.name, err(e) => e }\n\
        fn main() -> Unit = {\n\
          println(show(value.object([(\"id\", value.str(\"x\")), (\"name\", value.str(\"a\")), (\"address\", value.object([]))])))\n\
          println(show(value.object([(\"id\", value.int(1)), (\"name\", value.str(\"a\")), (\"address\", value.object([(\"city\", value.int(3))]))])))\n\
          println(show(value.object([(\"id\", value.int(1)), (\"name\", value.str(\"a\")), (\"address\", value.object([(\"city\", value.str(\"c\"))]))])))\n\
          println(show(value.object([(\"id\", value.int(1)), (\"name\", value.str(\"a\")), (\"address\", value.object([(\"city\", value.str(\"c\")), (\"zip\", value.str(\"z\"))]))])))\n\
        }\n";
    let dir = std::env::temp_dir().join(format!("almide-decode-frame-text-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("prog.almd");
    std::fs::write(&path, src).unwrap();
    let output = Command::new(almide_bin()).args(["run", path.to_str().unwrap()]).output().expect("failed to spawn almide");
    std::fs::remove_dir_all(&dir).ok();
    assert!(output.status.success(), "run failed:\n{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "expected Int, received Str, at id\nexpected Str, received Int, at address.city\nmissing field 'zip', at address\na\n");
}
