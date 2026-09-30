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
    "pid-kernel.almd",
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
    ("md2html.almd", "reads stdin"),
    (
        "minesweeper.almd",
        "stdin game with time-seeded random mines: a whole game cannot \
         byte-compare; its mine-free scripted prefix does, in \
         minesweeper_builds_on_wasm_and_its_scripted_prefix_matches_native",
    ),
    ("todo-api.almd", "network (HTTP server)"),
    ("typed-api-client.almd", "network (HTTP)"),
];

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// The wasm leg shells out to the `wasmtime` CLI (src/cli/commands.rs). CI
/// installs it on the Linux jobs — where this gate ENFORCES — but not on the
/// windows/macos matrix builds, and a byte-parity check cannot run without the
/// wasm executor. Skip LOUDLY there rather than fail on a missing tool: the
/// cross-OS wasm story is owned by the dedicated host-arch determinism job.
fn wasmtime_available() -> bool {
    Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
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
    if !wasmtime_available() {
        eprintln!(
            "SKIP examples_parity: no `wasmtime` on PATH — the wasm leg cannot run. \
             The parity gate enforces on the Linux CI jobs, which install it."
        );
        return;
    }
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

/// A LEDGERED example still has to COMPILE. The ledger reason exempts a file
/// from byte-comparison — stdin, network, an LLM API — never from existing as
/// a valid program. Nothing checked that before, and the shop window rotted
/// exactly there: md2html, todo-api and dotenv-check shipped uncompilable for
/// four months behind a green gate (#922 — their import lines were dropped by
/// the very commit titled "fix broken examples").
#[test]
fn every_ledgered_example_still_checks() {
    let mut broken = Vec::new();
    for (f, _) in LEDGERED {
        let out = Command::new(almide())
            .args(["check", &format!("examples/{f}")])
            .current_dir(repo_root())
            .output()
            .expect("run almide check");
        if !out.status.success() {
            broken.push(format!(
                "{f}:\n{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    assert!(
        broken.is_empty(),
        "LEDGERED examples that no longer compile — the ledger exempts a file \
         from byte-comparison, not from being a valid program:\n{}",
        broken.join("\n")
    );
}

/// The LLM examples (`examples/llm/`) are a real subproject importing the REAL
/// `almai` package, pinned by `examples/llm/almide.lock` to an exact commit —
/// so this checks them against the actual API instead of a vendored stub that
/// would drift (#922; they had been uncompilable as flat examples because
/// `import almai` had nothing to resolve against). A cold cache fetches the
/// pinned commit once; CI's network reaches github.com by construction (the
/// checkout itself does).
#[test]
fn the_llm_examples_check_against_the_real_almai() {
    let dir = repo_root().join("examples/llm");
    let mut checked = 0;
    let mut broken = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("examples/llm/") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "almd") {
            continue;
        }
        let out = Command::new(almide())
            .args(["check", path.file_name().unwrap().to_str().unwrap()])
            .current_dir(&dir)
            .output()
            .expect("run almide check");
        checked += 1;
        if !out.status.success() {
            broken.push(format!(
                "{}:\n{}{}",
                path.display(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    assert!(checked >= 3, "the llm subproject holds the three llm examples, found {checked}");
    assert!(
        broken.is_empty(),
        "llm examples that no longer check against the pinned almai:\n{}",
        broken.join("\n")
    );
}

/// Minesweeper fell between the two wasm legs for months: the structural leg
/// walled its `guard … else continue` in a value-returning fn (#3113) and the
/// incumbent refused its main loop, so `build --target wasm` said E082. The
/// board is placed by `random` on the first reveal, so a whole game never
/// byte-compares; everything before the first reveal does — flags, unflags,
/// rejected input — and so does the EOF loop after the script (`io.read_line`
/// answers "" and the game keeps asking). Both builds run the same script and
/// the first `PREFIX` bytes of stdout must be identical. The programs never
/// exit on their own, so each is killed once its prefix is read.
#[test]
fn minesweeper_builds_on_wasm_and_its_scripted_prefix_matches_native() {
    use std::io::{Read, Write};
    const SCRIPT: &str = "f 1,1\nf 2,2\nf 1,1\nnonsense\n9,9\nf 8,0\n1,x\n";
    const PREFIX: usize = 8000;
    if !wasmtime_available() {
        eprintln!("SKIP minesweeper wasm prefix: no `wasmtime` on PATH");
        return;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("minesweeper_prefix");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let native = dir.join("minesweeper");
    let wasm = dir.join("minesweeper.wasm");
    for (out, wasm_target) in [(&native, false), (&wasm, true)] {
        let mut args = vec!["build", "examples/minesweeper.almd", "-o", out.to_str().unwrap()];
        if wasm_target {
            args.extend(["--target", "wasm"]);
        }
        let o = Command::new(almide()).args(&args).current_dir(repo_root()).output().expect("almide build");
        assert!(
            o.status.success(),
            "minesweeper.almd no longer builds (wasm: {wasm_target}):\n{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
    }
    let prefix = |mut cmd: Command| -> Vec<u8> {
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn minesweeper");
        child.stdin.take().unwrap().write_all(SCRIPT.as_bytes()).expect("write script");
        let mut buf = vec![0u8; PREFIX];
        child.stdout.take().unwrap().read_exact(&mut buf).expect("read the prefix");
        let _ = child.kill();
        let _ = child.wait();
        buf
    };
    let n = prefix(Command::new(&native));
    let mut w = Command::new("wasmtime");
    w.arg(&wasm);
    let w = prefix(w);
    assert!(
        String::from_utf8_lossy(&n).contains("Invalid input"),
        "the script no longer reaches the rejected-input path; re-read the prefix"
    );
    if n != w {
        let at = n.iter().zip(&w).position(|(a, b)| a != b).unwrap_or(PREFIX);
        panic!(
            "minesweeper: native and wasm stdout diverge at byte {at} of the scripted prefix\n\
             native: {:?}\nwasm:   {:?}",
            String::from_utf8_lossy(&n[at.saturating_sub(80)..(at + 80).min(PREFIX)]),
            String::from_utf8_lossy(&w[at.saturating_sub(80)..(at + 80).min(PREFIX)])
        );
    }
}
