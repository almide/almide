//! The WASI acceptance gate (#1588): every non-host-variant manifest
//! fixture, emitted → `to_wasi` → executed by the STOCK `wasmtime` CLI
//! (no bespoke host anywhere in the loop), must reproduce the
//! manifest's normalized stdout hash and exit code.
//!
//! This is the burn-up's judgment re-run on a third-party runtime — the
//! tool-independence witness (aviation O7): the artifact's behavior no
//! longer depends on our host implementation.
//!
//! Subset by PRINCIPLE, not list: fixtures importing fs/env/process use
//! host ops the WASI build refuses (the defined refusal path), so they
//! sit outside the gate the same way they sit outside the alloc ledger.

use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

fn normalized_hash(stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(stdout);
    let no_nul: String = text.chars().filter(|c| *c != '\0').collect();
    let trimmed = no_nul.trim_end_matches('\n');
    let text = if trimmed.is_empty() { String::new() } else { format!("{trimmed}\n") };
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn wasmtime_bin() -> String {
    for cand in ["wasmtime", "/opt/homebrew/bin/wasmtime"] {
        if Command::new(cand).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false) {
            return cand.to_string();
        }
    }
    panic!("the WASI gate needs the wasmtime CLI on PATH (CI: the release-shape job installs it)");
}

#[cfg_attr(debug_assertions, ignore = "stock-runtime sweep is release-only (CI: release-shape job)")]
#[test]
fn corpus_reproduces_on_stock_wasmtime() {
    let root = workspace_root();
    let wasmtime = wasmtime_bin();
    let manifest = std::fs::read_to_string(
        root.join("crates/almide-spine/tests/golden/spec-run-manifest.txt"),
    )
    .expect("run manifest");
    let dir = std::env::temp_dir().join(format!("almide-wasi-gate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("gate dir");

    let mut swept = 0usize;
    let mut skipped = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for line in manifest.lines() {
        let mut it = line.splitn(3, '\t');
        let want_hash = it.next().expect("manifest row");
        let want_exit: i32 = it.next().expect("manifest row").parse().expect("exit");
        let rel = it.next().expect("manifest row");
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("fixture readable");
        let host_variant = text
            .lines()
            .any(|l| matches!(l.trim(), "import fs" | "import env" | "import process"));
        if host_variant {
            skipped += 1;
            continue;
        }
        let ir = almide_spine::s5::lower_to_ir(rel, &text).expect("front");
        // Structurally-refused fixtures (the CLI reroutes them to the
        // incumbent; the alloc ledger's `!` row asserts the refusal
        // stays a refusal) have no structural module to reproduce.
        let Ok(bytes) = almide_wasm::emit_program(&ir) else {
            skipped += 1;
            continue;
        };
        let wasi = almide_wasm_run::wasi::to_wasi(&bytes).expect("to_wasi");
        let path = dir.join("m.wasm");
        std::fs::write(&path, &wasi).expect("write module");
        let out = Command::new(&wasmtime)
            .arg("run")
            .arg(&path)
            .stdin(Stdio::null())
            .output()
            .expect("wasmtime runs");
        let got_exit = out.status.code().unwrap_or(-1);
        let got_hash = normalized_hash(&out.stdout);
        if got_hash != want_hash || got_exit != want_exit {
            failures.push(format!(
                "{rel}: exit {got_exit} (want {want_exit}), hash {}",
                if got_hash == want_hash { "ok" } else { "DIFFERS" }
            ));
        }
        swept += 1;
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        failures.is_empty(),
        "{} of {swept} WASI runs diverge from the manifest on stock wasmtime:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        swept >= 500,
        "sweep looks broken: only {swept} fixtures ran ({skipped} host-variant outside)"
    );
}
