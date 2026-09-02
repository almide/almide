//! NATIVE-leg regression pins for #1821: a user type named after a runtime
//! item must not collide with it in the generated Rust.
//!
//! Every runtime module is spliced flat into the user's Rust module, and the
//! emitter spelled the runtime's `Value` enum, the `HttpRequest` /
//! `HttpResponse` / `JsonPath` aliases and the `Endian` / `FileStat` /
//! `ProcessStatus` twins by their Almide names — so `type Value = { n: Int }`
//! beside anything that pulled the owning module in was E0428 / E0574 /
//! E0560 at rustc after a green `almide check` (ALS-T6). The runtime spells
//! them under reserved `Almide*` names now and the walker
//! (`walker/runtime_owned.rs`) renders every reference with the reserved
//! spelling, leaving the bare one to the user's declaration.
//!
//! The rlib fast path hides a collision (`use almide_rt::*` is shadowed
//! legally by a local item), so every pin forces the inline build with
//! `ALMIDE_NO_RTLIB=1`, the path the report's failures came through. The
//! wasm leg has no runtime prelude to collide with, so this pins NATIVE only,
//! per the tests/native_mut_param_pins_test.rs doctrine. Each program pulls
//! the module whose runtime source defines the twin (json → value.rs,
//! http → http.rs, bytes → bytes.rs, fs → fs.rs, process → process.rs) and
//! uses only functions that do not hand back the shadowed type, since the
//! checker resolves a user `Value` and the runtime's `Value` to ONE name —
//! that coexistence is a checker-side gap, not a spelling collision.

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

/// Build + run `source` on the inline native path; returns (success, stdout, stderr).
fn run_inline(tag: &str, source: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("almd_1821_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join(format!("{tag}.almd"));
    std::fs::write(&src, source).unwrap();
    let out = Command::new(almide_bin())
        .args(["run", src.to_str().unwrap()])
        .env("ALMIDE_NO_RTLIB", "1")
        .current_dir(&dir)
        .output()
        .expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    std::fs::remove_dir_all(&dir).ok();
    (out.status.success(), stdout, stderr)
}

fn assert_user_type_builds(tag: &str, source: &str, expected_stdout: &str) {
    let (ok, stdout, stderr) = run_inline(tag, source);
    for code in ["E0428", "E0574", "E0560", "E0170", "E0308"] {
        assert!(!stderr.contains(code), "user type `{tag}` collided with the runtime again ({code}):\n{stderr}");
    }
    assert!(
        ok && stdout == expected_stdout,
        "a user type named `{tag}` must build and run on the inline native path:\nstdout={stdout}\nstderr={stderr}"
    );
}

#[test]
fn user_type_named_value_builds_with_value_rs_spliced() {
    // http.rs pulls value.rs in (the runtime `Value` enum) without handing
    // a `Value` back to the program.
    assert_user_type_builds(
        "Value",
        "import http\ntype Value = { n: Int }\neffect fn main() -> Unit = {\n  let x = Value { n: 7 }\n  let r = http.response(200, \"body\")\n  println(\"${x.n} ${http.body(r)}\")\n}\n",
        "7 body\n",
    );
}

#[test]
fn user_variant_named_value_builds_with_value_rs_spliced() {
    assert_user_type_builds(
        "ValueVariant",
        "import http\ntype Value = | Small(Int) | Big\neffect fn main() -> Unit = {\n  let x = Small(3)\n  let d = match x { Small(n) => n, Big => 0 }\n  let r = http.response(200, \"body\")\n  println(\"${d} ${http.body(r)}\")\n}\n",
        "3 body\n",
    );
}

#[test]
fn user_type_named_http_request_builds_with_http_rs_spliced() {
    assert_user_type_builds(
        "HttpRequest",
        "import http\ntype HttpRequest = { n: Int }\neffect fn main() -> Unit = {\n  let x = HttpRequest { n: 7 }\n  let r = http.response(200, \"body\")\n  println(\"${x.n} ${http.body(r)}\")\n}\n",
        "7 body\n",
    );
}

#[test]
fn user_type_named_http_response_builds_with_http_rs_spliced() {
    // `http.request` hands back a String result, never an HttpResponse, so the
    // user's HttpResponse is the only one the program spells.
    assert_user_type_builds(
        "HttpResponse",
        "import http\nimport json\ntype HttpResponse = { n: Int }\neffect fn main() -> Unit = {\n  let x = HttpResponse { n: 7 }\n  let v = json.parse(\"{\\\"a\\\":1}\")!\n  println(\"${x.n} ${json.stringify(v)}\")\n}\n",
        "7 {\"a\":1}\n",
    );
}

#[test]
fn user_type_named_json_path_builds_beside_json() {
    assert_user_type_builds(
        "JsonPath",
        "import json\ntype JsonPath = { n: Int }\neffect fn main() -> Unit = {\n  let x = JsonPath { n: 7 }\n  let v = json.parse(\"{\\\"a\\\":1}\")!\n  println(\"${x.n} ${json.stringify(v)}\")\n}\n",
        "7 {\"a\":1}\n",
    );
}

#[test]
fn user_type_named_endian_builds_with_bytes_rs_spliced() {
    assert_user_type_builds(
        "Endian",
        "import bytes\ntype Endian = { n: Int }\neffect fn main() -> Unit = {\n  let x = Endian { n: 7 }\n  println(\"${x.n} ${bytes.len(bytes.new(2))}\")\n}\n",
        "7 2\n",
    );
}

#[test]
fn user_type_named_file_stat_builds_with_fs_rs_spliced() {
    assert_user_type_builds(
        "FileStat",
        "import fs\ntype FileStat = { n: Int }\neffect fn main() -> Unit = {\n  let x = FileStat { n: 7 }\n  println(\"${x.n} ${fs.exists(\"/nonexistent_almd_1821_zzz\")}\")\n}\n",
        "7 false\n",
    );
}

#[test]
fn user_type_named_process_status_builds_with_process_rs_spliced() {
    assert_user_type_builds(
        "ProcessStatus",
        "import process\ntype ProcessStatus = { n: Int }\neffect fn main() -> Unit = {\n  let x = ProcessStatus { n: 7 }\n  let s = process.exec(\"echo\", [\"hi\"])!\n  println(\"${x.n} ${string.trim(s)}\")\n}\n",
        "7 hi\n",
    );
}

/// The runtime-private items (`PathStep` in json.rs, `RxNode` in regex.rs)
/// are spliced flat just the same; they carry the reserved prefix too.
#[test]
fn user_type_named_after_a_runtime_private_item_builds() {
    assert_user_type_builds(
        "PathStep",
        "import json\ntype PathStep = { n: Int }\neffect fn main() -> Unit = {\n  let x = PathStep { n: 7 }\n  let v = json.parse(\"{\\\"a\\\":1}\")!\n  println(\"${x.n} ${json.stringify(v)}\")\n}\n",
        "7 {\"a\":1}\n",
    );
    assert_user_type_builds(
        "RxNode",
        "import regex\ntype RxNode = { n: Int }\neffect fn main() -> Unit = {\n  let x = RxNode { n: 7 }\n  println(\"${x.n} ${regex.is_match(\"a+\", \"caab\")}\")\n}\n",
        "7 true\n",
    );
}

/// With `bytes` auto-imported the bundled `Endian` decl never reaches the
/// program, and a bare `LittleEndian` PATTERN rendered as a catch-all binding
/// (E0170 at rustc). The runtime-owned ctors are registered unconditionally
/// now, so construction and patterns qualify against `AlmideEndian` alike.
#[test]
fn endian_ctors_qualify_without_an_explicit_bytes_import() {
    assert_user_type_builds(
        "EndianAuto",
        "fn pick(big: Bool) -> Endian = if big then BigEndian else LittleEndian\nfn name(e: Endian) -> String = match e { LittleEndian => \"le\", BigEndian => \"be\" }\neffect fn main() -> Unit = {\n  var t = bytes.new(0)\n  bytes.write_uint16(t, 258, LittleEndian)\n  bytes.write_uint16(t, 258, pick(true))\n  println(\"${int.from_uint16(bytes.read_uint16(t, 0, LittleEndian))} ${int.from_uint16(bytes.read_uint16(t, 2, BigEndian))} ${name(pick(false))} ${name(pick(true))}\")\n}\n",
        "258 258 le be\n",
    );
}

/// The twins' repr impls moved from the walker-emitted decl into the runtime;
/// their literal form, equality and record patterns must print exactly what
/// they printed before (and what the wasm leg prints).
#[test]
fn runtime_twins_keep_their_repr_equality_and_patterns() {
    // (`import bytes` is explicit and `bytes.len` is called: a program that
    // only NAMES `Endian` never splices bytes.rs, and under the auto-import
    // the bundled decl never reaches the program so `${e}` has no repr route
    // — both pre-existing walls on either side of this change.)
    let source = "import bytes\nimport fs\nimport process\nfn kind(m: FileStat) -> String = match m { FileStat { is_dir, .. } => if is_dir then \"dir\" else \"file\" }\neffect fn main() -> Unit = {\n  let p = \"/tmp/almd_1821_twin_repr_pin.txt\"\n  fs.write(p, \"abc\")!\n  let m = fs.stat(p)!\n  println(\"${m.size} ${m.is_file} ${m == m}\")\n  println(kind(m))\n  let s = process.exec_status(\"true\", [])!\n  println(\"${s.code} ${s} ${s == s}\")\n  let e: Endian = BigEndian\n  println(\"${e} ${e == LittleEndian} ${bytes.len(bytes.new(0))}\")\n}\n";
    let (ok, stdout, stderr) = run_inline("twins", source);
    assert!(ok, "the twin program must build on the inline native path:\n{stderr}");
    assert_eq!(
        stdout,
        "3 true true\nfile\n0 ProcessStatus { code: 0, stdout: \"\", stderr: \"\" } true\nBigEndian false 0\n",
        "twin repr / equality / pattern output drifted:\nstderr={stderr}"
    );
}
