//! A module's top-lets that differ only in case are two slots when read or
//! written from another module (#3316).
//!
//! A cross-module read `m.buf` lowers to a synthetic use-site Var that the
//! global machinery resolves to its declaration by (module origin, name).
//! The use-site Var carried the name UPPER-cased and the key folded case on
//! both sides, so `var buf` and `let BUF` in one module shared a key: the
//! structural wasm leg (and the interpreter, which resolves through the same
//! table) read and wrote whichever was declared last. The key is now the
//! exact source spelling.
//!
//! Native refuses this program today with E0428 (the static-name collision
//! of #3305). This test pins the wasm output exactly. Native must print the
//! same output, or refuse with that one known error until #3305 lands. Any
//! other native outcome fails the test.

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

fn scratch(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue3316-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    std::fs::write(root.join("almide.toml"), "[package]\nname = \"twins\"\nversion = \"0.1.0\"\n")
        .expect("write toml");
    for (file, body) in files {
        std::fs::write(root.join("src").join(file), body).expect("write module");
    }
    root
}

fn run(root: &Path, wasm: bool) -> std::process::Output {
    let mut cmd = Command::new(almide_bin());
    cmd.current_dir(root).arg("run").arg("src/main.almd");
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    cmd.output().expect("spawn almide")
}

fn wasm_runtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok()
}

const MODULE: &str = "var buf = 0\nlet BUF = 100\npub fn touch(k: Int) -> Unit = { buf = k + 1 }\n";

/// Each case: the entry module, and the output both legs must print.
const CASES: &[(&str, &str, &str)] = &[
    (
        // The module writes its own var; the entry reads both twins.
        "module_writes",
        "import self.m\neffect fn main() -> Unit = {\n  m.touch(3)\n  println(int.to_string(m.buf))\n  println(int.to_string(m.BUF))\n}\n",
        "4\n100\n",
    ),
    (
        // The entry writes the var through the module path.
        "entry_writes",
        "import self.m\nfn poke(k: Int) -> Unit = { m.buf = k + 1 }\neffect fn main() -> Unit = {\n  poke(3)\n  println(int.to_string(m.buf))\n  println(int.to_string(m.BUF))\n}\n",
        "4\n100\n",
    ),
    (
        // The let is declared FIRST: a folded key resolved to the last one
        // declared, so the order of the twins must not matter either.
        "let_first",
        "import self.n\neffect fn main() -> Unit = {\n  n.touch(3)\n  println(int.to_string(n.buf))\n  println(int.to_string(n.BUF))\n}\n",
        "4\n100\n",
    ),
];

const MODULE_LET_FIRST: &str = "let BUF = 100\nvar buf = 0\npub fn touch(k: Int) -> Unit = { buf = k + 1 }\n";

#[test]
fn case_twin_top_lets_are_two_slots_across_modules() {
    if !wasm_runtime_available() {
        if std::env::var("CI").is_ok() {
            panic!("wasmtime is required under CI");
        }
        eprintln!("skip: wasmtime not on PATH");
        return;
    }
    for &(name, entry, want) in CASES {
        let root = scratch(name, &[("main.almd", entry), ("m.almd", MODULE), ("n.almd", MODULE_LET_FIRST)]);
        let wasm = run(&root, true);
        assert!(
            wasm.status.success(),
            "{name}: wasm leg failed: {}",
            String::from_utf8_lossy(&wasm.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&wasm.stdout), want, "{name}: wasm leg");
        let native = run(&root, false);
        if native.status.success() {
            assert_eq!(String::from_utf8_lossy(&native.stdout), want, "{name}: native leg");
        } else {
            let err = String::from_utf8_lossy(&native.stderr);
            assert!(
                err.contains("E0428"),
                "{name}: native may only refuse with the #3305 static-name collision, got: {err}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
