//! NATIVE-leg regression pins for #1829: five check-passes/build-fails walls
//! (ALS-T6) the #1821 probe sweep found beside the runtime-name family.
//!
//! 1. The prelude's `RcCow` / `SharedMut` structs are emitted into every
//!    program; a user `type RcCow = …` was E0428 + E0107. They carry the
//!    reserved `AlmideRcCow` / `AlmideSharedMut` spellings now.
//! 2. A top-level `let` emits as an upper-cased const, and every runtime
//!    `const` / `static` was reachable that way (`let key_slots` vs
//!    value.rs's `KEY_SLOTS`, `let t` vs libm_p2.rs's `T`, `var rng_state`
//!    vs random.rs's thread-local). The runtime's own take the `ALMIDE_`
//!    prefix now.
//! 3. `used_stdlib_modules` is call-driven, so a program that only NAMED
//!    `Endian` never spliced bytes.rs (E0425 / E0433). A reference to a
//!    runtime-owned type pulls the module that defines it.
//! 4. Under the auto-import the bundled `Endian` decl never reaches the
//!    program, so `${e}` fell to `Display`, which the runtime enum has not
//!    (E0277). A runtime-owned twin is repr-capable regardless of the decl.
//! 5. `"${m.size} ${m.is_file} ${kind(m)}"` rendered a `format!` that
//!    borrows `m.size` and moves `m` in the same argument list (E0505); the
//!    clone pass now applies the call guard's rule to the interpolation.
//!
//! Every pin forces the inline build (`ALMIDE_NO_RTLIB=1`, the path the
//! report's failures came through; the rlib path hides 1 and 2 because a
//! local item shadows a glob import legally) and pulls a stdlib module the
//! v1 native renderer walls on, so the v0 emitter under test is the one
//! that runs. Items 3–5 are cross-target promises; item 5's shape also runs
//! on `--target wasm` here and must print the same bytes (the auto-import
//! `Endian` shapes of 3 and 4 wall on the wasm leg on either side of this
//! change — the wasm-side gap is #1839 — so those pin NATIVE only).

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

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Build + run `source`; returns (success, stdout, stderr). `wasm` selects
/// `--target wasm`, otherwise the inline native path.
fn run(tag: &str, source: &str, wasm: bool) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("almd_1829_{tag}_{}_{}", if wasm { "wasm" } else { "native" }, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join(format!("{tag}.almd"));
    std::fs::write(&src, source).unwrap();
    let mut cmd = Command::new(almide_bin());
    cmd.args(["run", src.to_str().unwrap()]).current_dir(&dir);
    if wasm {
        cmd.args(["--target", "wasm"]);
    } else {
        cmd.env("ALMIDE_NO_RTLIB", "1");
    }
    let out = cmd.output().expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    std::fs::remove_dir_all(&dir).ok();
    (out.status.success(), stdout, stderr)
}

fn assert_native_builds(tag: &str, source: &str, expected_stdout: &str) {
    let (ok, stdout, stderr) = run(tag, source, false);
    for code in ["E0107", "E0277", "E0425", "E0428", "E0433", "E0505"] {
        assert!(!stderr.contains(code), "`{tag}` hit the native wall again ({code}):\n{stderr}");
    }
    assert!(
        ok && stdout == expected_stdout,
        "`{tag}` must build and run on the inline native path:\nstdout={stdout}\nstderr={stderr}"
    );
}

/// Item 1: a user type named after a prelude struct.
#[test]
fn user_type_named_rc_cow_or_shared_mut_builds() {
    assert_native_builds(
        "RcCow",
        "import json\ntype RcCow = { n: Int }\neffect fn main() -> Unit = {\n  let x = RcCow { n: 7 }\n  let v = json.parse(\"{\\\"a\\\":1}\")!\n  println(\"${x.n} ${json.stringify(v)}\")\n}\n",
        "7 {\"a\":1}\n",
    );
    assert_native_builds(
        "SharedMut",
        "import json\ntype SharedMut = { n: Int }\neffect fn main() -> Unit = {\n  let x = SharedMut { n: 7 }\n  let v = json.parse(\"{\\\"a\\\":1}\")!\n  println(\"${x.n} ${json.stringify(v)}\")\n}\n",
        "7 {\"a\":1}\n",
    );
}

/// Item 2: a top-level `let` / `var` whose upper-cased name a runtime
/// `const`, `static` or thread-local carried (value.rs, libm_p2.rs,
/// random.rs).
#[test]
fn top_let_named_after_a_runtime_const_builds() {
    assert_native_builds(
        "key_slots",
        "import json\nlet key_slots = 1\neffect fn main() -> Unit = {\n  let v = json.parse(\"{\\\"a\\\":1}\")!\n  println(\"${key_slots} ${json.stringify(v)}\")\n}\n",
        "1 {\"a\":1}\n",
    );
    assert_native_builds(
        "t_rng_state",
        "import random\nlet t = 3\nvar rng_state = 1\neffect fn main() -> Unit = {\n  rng_state = rng_state + t\n  println(\"${math.sin(0.0)} ${t} ${rng_state} ${random.int(1, 1)}\")\n}\n",
        "0 3 4 1\n",
    );
}

/// Item 3: naming `Endian` (an annotation, a ctor, a pattern) without a
/// single `bytes.*` call.
#[test]
fn naming_endian_without_a_bytes_call_splices_bytes_rs() {
    assert_native_builds(
        "endian_eq",
        "effect fn main() -> Unit = {\n  let e: Endian = BigEndian\n  println(\"${e == LittleEndian}\")\n}\n",
        "false\n",
    );
    assert_native_builds(
        "endian_match",
        "effect fn main() -> Unit = {\n  let e: Endian = BigEndian\n  let s = match e { LittleEndian => \"le\", BigEndian => \"be\" }\n  println(s)\n  let f: Endian = LittleEndian\n  let t = match f { LittleEndian => \"le\", BigEndian => \"be\" }\n  println(t)\n}\n",
        "be\nle\n",
    );
}

/// Item 4: `${e}` on an `Endian` value with `bytes` auto-imported — the
/// literal form the explicit-import program already printed (`BigEndian`).
#[test]
fn endian_interpolation_under_the_auto_import_has_a_repr_route() {
    assert_native_builds(
        "endian_repr",
        "effect fn main() -> Unit = {\n  let e = BigEndian\n  println(\"${e}\")\n}\n",
        "BigEndian\n",
    );
    assert_native_builds(
        "endian_repr_beside_a_call",
        "effect fn main() -> Unit = {\n  let e: Endian = BigEndian\n  println(\"${e} ${bytes.len(bytes.new(0))}\")\n}\n",
        "BigEndian 0\n",
    );
}

const INTERP_MOVE_SOURCE: &str = "type Meta = { size: Int, name: String, is_dir: Bool }\nfn kind(m: Meta) -> String = match m { Meta { is_dir, .. } => if is_dir then \"dir\" else \"file\" }\neffect fn main() -> Unit = {\n  let m = Meta { size: 3, name: \"a.txt\", is_dir: false }\n  println(\"${m.size} ${m.name} ${kind(m)}\")\n  let d = Meta { size: 0, name: \"d\", is_dir: true }\n  println(\"${d} ${kind(d)}\")\n}\n";
const INTERP_MOVE_STDOUT: &str = "3 a.txt file\nMeta { size: 0, name: \"d\", is_dir: true } dir\n";

/// Item 5: a by-value use inside an interpolation whose sibling part borrows
/// the same binding — the reported `FileStat` shape and a user record.
#[test]
fn moving_a_value_inside_a_borrowing_interpolation_clones() {
    assert_native_builds("interp_move", INTERP_MOVE_SOURCE, INTERP_MOVE_STDOUT);
    assert_native_builds(
        "interp_move_file_stat",
        "import fs\nfn kind(m: FileStat) -> String = match m { FileStat { is_dir, .. } => if is_dir then \"dir\" else \"file\" }\neffect fn main() -> Unit = {\n  let p = \"/tmp/almd_1829_interp_move_pin.txt\"\n  fs.write(p, \"abc\")!\n  let m = fs.stat(p)!\n  println(\"${m.size} ${m.is_file} ${kind(m)}\")\n}\n",
        "3 true file\n",
    );
}

/// Item 5 on the wasm leg: the program the wasm leg always accepted prints
/// the same bytes natively now.
#[test]
fn moving_a_value_inside_a_borrowing_interpolation_matches_wasm() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let (ok, stdout, stderr) = run("interp_move", INTERP_MOVE_SOURCE, true);
    assert!(ok, "the wasm leg must accept the interpolation shape:\n{stderr}");
    assert_eq!(stdout, INTERP_MOVE_STDOUT, "wasm output drifted from the native pin");
}
