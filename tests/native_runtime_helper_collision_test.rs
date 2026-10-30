//! #4345: a user item spelled like a runtime helper builds natively.
//!
//! The native runtime was spliced flat into the user's crate, so every helper
//! it defined sat at the crate root beside the user's items: `pub fn mk` in
//! matrix.rs made a user `fn mk` rustc E0428 / E0308 in any program that
//! reached `matrix`, after a green `almide check`. The runtime modules now
//! live in one private module and the crate root gets back only the names in
//! the runtime's namespace (`almide_ir::is_runtime_item_name`;
//! `almide_codegen::runtime_namespace` holds the guard). A user fn spelled in
//! that namespace (`almide_abort`) is escaped like an `almide_rt_` one.
//!
//! The rlib fast path hides a collision (`use almide_rt::*` is shadowed
//! legally by a local item), so the native leg forces the inline build with
//! `ALMIDE_NO_RTLIB=1`, the path the report's failure came through. Every
//! program also runs on wasm and must print the same bytes.

use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// `almide run [--target wasm] prog.almd` on `source`; (success, stdout, stderr).
fn run(tag: &str, source: &str, wasm: bool) -> (bool, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join(format!("{tag}.almd"));
    std::fs::write(&src, source).expect("source");
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(&src).current_dir(dir.path()).env_remove("ALMIDE_COMPONENT_P3");
    if wasm {
        cmd.args(["--target", "wasm"]);
    } else {
        cmd.env("ALMIDE_NO_RTLIB", "1");
    }
    let out = cmd.output().expect("almide");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr).to_string())
}

fn assert_builds_and_agrees(tag: &str, source: &str, expected: &str) {
    let (ok, stdout, stderr) = run(tag, source, false);
    for code in ["E0428", "E0255", "E0308", "E0574", "E0423"] {
        assert!(!stderr.contains(code), "[{tag}] a user item collided with the runtime again ({code}):\n{stderr}");
    }
    assert!(ok && stdout == expected, "[{tag}] native (inline):\nstdout={stdout}\nstderr={stderr}");
    let (ok, wasm_stdout, stderr) = run(tag, source, true);
    assert!(ok, "[{tag}] wasm:\nstdout={wasm_stdout}\nstderr={stderr}");
    assert_eq!(stdout, wasm_stdout, "[{tag}] native and wasm disagree");
}

/// The report's program: a user `fn mk` beside a matrix product.
#[test]
fn user_fn_named_mk_builds_beside_matrix() {
    assert_builds_and_agrees(
        "mk",
        "fn mk(n: Int) -> Matrix =\n  matrix.from_lists(list.range(0, n) |> list.map((i) => list.range(0, n) |> list.map((j) => float.from(i + j))))\n\neffect fn main() -> Unit = {\n  let c = mk(3) * mk(3)\n  println(\"${matrix.to_lists(c)}\")\n}\n",
        "[[5.0, 8.0, 11.0], [8.0, 14.0, 20.0], [11.0, 20.0, 29.0]]\n",
    );
}

/// The same class across the other modules' helpers (hash.rs `sha256`, the
/// http client core's `request`, json.rs `json_quote`, base64.rs
/// `decode_str`, regex.rs `rx_compile`) and the prelude's `almide_abort`.
#[test]
fn user_fns_named_after_runtime_helpers_build() {
    assert_builds_and_agrees(
        "helpers",
        concat!(
            "import hash\nimport base64\nimport json\nimport regex\n\n",
            "fn sha256(s: String) -> String = \"user:${s}\"\n",
            "fn request(n: Int) -> Int = n + 1\n",
            "fn json_quote(s: String) -> String = \"<${s}>\"\n",
            "fn decode_str(s: String) -> String = s + \"!\"\n",
            "fn rx_compile(n: Int) -> Int = n * 2\n",
            "fn almide_abort(n: Int) -> Int = n - 1\n\n",
            "effect fn main() -> Unit = {\n",
            "  let d = hash.sha256_hex(bytes.from_string(\"abc\"))\n",
            "  let e = base64.encode(bytes.from_string(\"hi\"))\n",
            "  let v = json.parse(\"{\\\"a\\\":1}\")!\n",
            "  let m = regex.is_match(regex.new(\"a+\"), \"caab\")\n",
            "  println(\"${sha256(\"x\")} ${request(1)} ${json_quote(\"q\")} ${decode_str(\"z\")} ${rx_compile(3)} ${almide_abort(5)}\")\n",
            "  println(\"${d} ${e} ${json.stringify(v)} ${m}\")\n",
            "}\n",
        ),
        "user:x 2 <q> z! 6 4\nba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad aGk= {\"a\":1} true\n",
    );
}

/// A user type and fn named after the fs runtime's shared-core items.
#[test]
fn user_items_named_after_fs_core_items_build() {
    assert_builds_and_agrees(
        "fscore",
        "import fs\n\ntype FsBeneathError = { n: Int }\n\nfn io_err(n: Int) -> Int = n * 3\n\neffect fn main() -> Unit = {\n  let x = FsBeneathError { n: 7 }\n  println(\"${x.n} ${io_err(2)} ${fs.exists(\"/nonexistent_almd_4345_zzz\")}\")\n}\n",
        "7 6 false\n",
    );
}
