//! Every example is either BYTE-IDENTICAL across targets or says why not.
//!
//! `examples/` is the shop window: the programs a newcomer runs first, and the
//! programs that silently rot first. Minesweeper proved it — its wasm build
//! carried an empty 81-cell minefield for months (#810's census found it),
//! because nothing executed the examples on the wasm leg. This gate
//! institutionalizes the lesson, with the same discipline as the wasm skip
//! ledger: every `examples/*.almd` must appear in exactly one list —
//!
//! - [`PARITY`]: deterministic, input-free — `almide run` on BOTH targets must
//!   produce byte-identical stdout and exit code, on every `cargo test`.
//! - [`LEDGERED`]: a declared reason it cannot byte-compare today (stdin,
//!   network, filesystem, an LLM API, or a named subset issue). A row here is
//!   a fact, not a parking spot — when the blocker lands, the file moves up.
//!
//! A NEW example that lands in neither list fails the roster test, so nothing
//! ships unverified-by-default again. `*_test.almd` files are exempt: they run
//! under `almide test` on both targets already.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Deterministic, input-free examples: byte-identical native == wasm, asserted.
const PARITY: &[&str] = &[
    "balanced-parens.almd",
    "binary-search.almd",
    "fan_demo.almd",
    "lisp.almd",
    "llama_block.almd",
    "raytracer.almd",
    "roman-numeral.almd",
];

/// Why the rest cannot byte-compare today.
const LEDGERED: &[(&str, &str)] = &[
    ("almide-grep.almd", "reads files + argv"),
    ("api-client.almd", "network (HTTP)"),
    ("csv-to-json.almd", "reads stdin"),
    ("dotenv-check.almd", "reads .env files"),
    ("llm-chat.almd", "LLM API + stdin"),
    ("llm-code-review.almd", "LLM API"),
    ("llm-json-extract.almd", "LLM API"),
    ("md2html.almd", "reads stdin"),
    (
        "minesweeper.almd",
        "stdin game; wasm leg honestly walls (auto-? nested in a loop — the \
         effect-unwrap desugar's reach, see tests/nested_effect_unwrap_test.rs)",
    ),
    ("todo-api.almd", "network (HTTP server)"),
    ("typed-api-client.almd", "network (HTTP)"),
];

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(example: &Path, wasm: bool) -> (i32, String) {
    let mut args = vec!["run", example.to_str().unwrap()];
    if wasm {
        args.push("--target");
        args.push("wasm");
    }
    let out = Command::new(almide())
        .args(&args)
        .current_dir(repo_root())
        .output()
        .expect("run almide");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

#[test]
fn every_example_is_rostered() {
    let known: std::collections::HashSet<&str> = PARITY
        .iter()
        .copied()
        .chain(LEDGERED.iter().map(|(f, _)| *f))
        .collect();
    let mut unrostered = Vec::new();
    for entry in std::fs::read_dir(repo_root().join("examples")).expect("examples/") {
        let name = entry.expect("entry").file_name().to_string_lossy().to_string();
        if !name.ends_with(".almd") || name.ends_with("_test.almd") {
            continue; // *_test.almd runs under `almide test` on both targets
        }
        if !known.contains(name.as_str()) {
            unrostered.push(name);
        }
    }
    assert!(
        unrostered.is_empty(),
        "examples with no verification story: {unrostered:?}\n\
         Add each to PARITY (deterministic, input-free — the default) or to \
         LEDGERED with the reason it cannot byte-compare."
    );
    // The ledger only shrinks the honest way: a row for a file that no longer
    // exists is stale.
    for (f, _) in LEDGERED {
        assert!(
            repo_root().join("examples").join(f).exists(),
            "LEDGERED row for a file that no longer exists: {f}"
        );
    }
}

#[test]
fn parity_examples_are_byte_identical_across_targets() {
    let mut failures = Vec::new();
    for f in PARITY {
        let path = repo_root().join("examples").join(f);
        assert!(path.exists(), "PARITY row for a missing file: {f}");
        let (nc, nout) = run(&path, false);
        if nc != 0 {
            failures.push(format!("{f}: native exit {nc}"));
            continue;
        }
        let (wc, wout) = run(&path, true);
        if wc != 0 {
            failures.push(format!("{f}: wasm exit {wc} (native was clean — a wall or a crash regressed)"));
            continue;
        }
        if nout != wout {
            let seen = nout
                .lines()
                .zip(wout.lines())
                .position(|(a, b)| a != b)
                .map(|i| i + 1)
                .unwrap_or(0);
            failures.push(format!(
                "{f}: stdout diverges (first differing line {seen}) — the cross-target \
                 byte-identity this example advertises is broken"
            ));
        }
    }
    assert!(failures.is_empty(), "example parity failures:\n{}", failures.join("\n"));
}
