//! The exit-code guard allocates its slot in the table that owns the body it
//! rewrites (#2374).
//!
//! `guard_exit_codes` rewrites `process.exit(e)` into a block that binds `e`
//! to a fresh `__exit_code_N` slot. It walked the entry program's functions
//! and every module's functions with ONE `VarTable` — the program's — but
//! each `IrModule` owns its own. So a `process.exit` in a module got a slot
//! id minted against a table it does not belong to:
//!
//!   * id past the module's table  → `VarId(262) out of bounds (table size:
//!     92)`, which `VarTable::use_count` reaches first as an unchecked index
//!     panic (exit 101, no diagnostic);
//!   * id inside the module's table → no panic at all. The bind lands on a
//!     row that already belongs to another function's variable, which the IR
//!     verifier reports as `VarId(0) is bound in two functions`. That is the
//!     quiet half, and it is the one a ten-line program hits.
//!
//! Both halves need the same three things: the call in a NON-ENTRY module, a
//! second function beside it (so the low ids are taken), and an argument that
//! is not an integer literal already in range (a literal is left alone).
//!
//! Reduced by o6lvl4-ab from ctxgate 0.11.0.

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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn scratch(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue2374-{}", name));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    std::fs::write(
        root.join("almide.toml"),
        "[package]\nname = \"exitslot\"\nversion = \"0.1.0\"\n",
    )
    .expect("write toml");
    for (file, body) in files {
        std::fs::write(root.join("src").join(file), body).expect("write module");
    }
    root
}

/// A module with a second function beside the one that exits — the shape that
/// makes the borrowed id land ON an existing row instead of past the end.
const RUNNER: &str = concat!(
    "import process\n",
    "type Node = { kind: Int }\n",
    "fn kind_of(n: Node) -> Int = n.kind\n",
    "effect fn run(code: Int) -> Unit = {\n",
    "  println(int.to_string(kind_of(Node { kind: code })))\n",
    "  process.exit(code)\n",
    "}\n",
);

fn build(dir: &Path, out: &str) -> (bool, String) {
    let output = Command::new(almide_bin())
        .args(["build", "src/main.almd", "-o", out])
        .current_dir(dir)
        .output()
        .expect("failed to spawn almide");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), combined)
}

#[test]
fn a_module_level_exit_does_not_borrow_the_programs_var_table() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "module-exit",
        &[
            ("runner.almd", RUNNER),
            ("main.almd", "import self.runner\neffect fn main() -> Unit = runner.run(0)!\n"),
        ],
    );
    let out_bin = dir.join("exitslot_bin");
    let (ok, out) = build(&dir, out_bin.to_str().unwrap());
    assert!(ok, "a `process.exit(<parameter>)` in a module did not build:\n{out}");
    assert!(
        !out.contains("bound in two functions") && !out.contains("out of bounds"),
        "the guard slot still came from another table:\n{out}"
    );
}

/// The rule the rewrite exists to enforce still holds from a module, on both
/// sides of the range (C-350) — the fix must move the slot, not the semantics.
#[test]
fn the_range_rule_still_holds_from_a_module() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "module-exit-range",
        &[
            ("runner.almd", RUNNER),
            (
                "main.almd",
                concat!(
                    "import env\n",
                    "import self.runner\n",
                    "effect fn main() -> Unit =\n",
                    "  runner.run(int.parse(list.get(env.args(), 0) ?? \"0\") ?? 0)!\n",
                ),
            ),
        ],
    );
    let out_bin = dir.join("exitslot_range");
    let (ok, out) = build(&dir, out_bin.to_str().unwrap());
    assert!(ok, "build failed:\n{out}");

    for (arg, want_code, want_msg) in
        [("0", 0, false), ("3", 3, false), ("125", 125, false), ("126", 1, true), ("200", 1, true)]
    {
        let run = Command::new(&out_bin).arg(arg).output().expect("failed to run");
        let code = run.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&run.stderr).to_string();
        assert_eq!(code, want_code, "exit({arg}) from a module gave {code}\nstderr: {stderr}");
        assert_eq!(
            stderr.contains("exit code must be in 0..=125"),
            want_msg,
            "exit({arg}) stderr was: {stderr}"
        );

        // C-350 is a CROSS-TARGET promise, and the slot this fix moves is in
        // the IR both legs consume — so the wasm leg is where a table mix-up
        // would show up differently, not the same way twice.
        let wasm = Command::new(almide_bin())
            .args(["run", "src/main.almd", "--target", "wasm", "--", arg])
            .current_dir(&dir)
            .output()
            .expect("failed to run on the wasm leg");
        let wasm_code = wasm.status.code().unwrap_or(-1);
        let wasm_stderr = String::from_utf8_lossy(&wasm.stderr).to_string();
        assert_eq!(
            wasm_code, want_code,
            "exit({arg}) gave {code} natively and {wasm_code} on wasm\nwasm stderr: {wasm_stderr}"
        );
        assert_eq!(
            wasm_stderr.contains("exit code must be in 0..=125"),
            want_msg,
            "exit({arg}) wasm stderr was: {wasm_stderr}"
        );
    }
}

/// A literal already in range is left alone, so the assert desugar's
/// `process.exit(1)` tail keeps rendering as it did — pinned here because the
/// fix touches the pass that makes that decision.
#[test]
fn an_in_range_literal_is_still_left_alone() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "module-exit-literal",
        &[
            (
                "runner.almd",
                concat!(
                    "import process\n",
                    "type Node = { kind: Int }\n",
                    "fn kind_of(n: Node) -> Int = n.kind\n",
                    "effect fn run(code: Int) -> Unit = {\n",
                    "  println(int.to_string(kind_of(Node { kind: code })))\n",
                    "  process.exit(7)\n",
                    "}\n",
                ),
            ),
            ("main.almd", "import self.runner\neffect fn main() -> Unit = runner.run(0)!\n"),
        ],
    );
    let out_bin = dir.join("exitslot_lit");
    let (ok, out) = build(&dir, out_bin.to_str().unwrap());
    assert!(ok, "build failed:\n{out}");
    let run = Command::new(&out_bin).output().expect("failed to run");
    assert_eq!(run.status.code(), Some(7), "a literal exit from a module changed");
}
