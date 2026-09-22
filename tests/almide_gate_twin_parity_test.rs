//! #2130 step 1 / #2163: the Almide gate twins answer what their `.sh` originals answer.
//!
//! Unit 0.46 ported seven gates to `tools/almide-gates/` with byte-identity
//! against the original as the stated acceptance check — and the check was
//! never built. CI runs only the `.sh` side; the twins are exercised by
//! `almide test tools/almide-gates/` and nothing ever compared the two. The
//! tool's own comment names what that costs:
//!
//! > the freshness gates compare the bash against the committed docs, never the
//! > port against either, so a reader that folded two contracts into one
//! > rendered a different index while every gate stayed green
//!
//! This is step 1 of the order that issue calls non-negotiable — build the
//! diff, watch it stay green, and only THEN promote. Nothing is promoted here
//! and no `.sh` is deleted.
//!
//! **It found drift on its first run.** `stamp`'s FATAL path (the PATH binary
//! disagreeing with the workspace build) had diverged twice: the bash had
//! learned to name the cause — a `cargo test` relinking `target/release` after
//! the last `make install` — while the port still carried the original
//! one-liner, and the bash RETURNS there while the port went on to print the
//! toolchain lines and the closing rule. Either would have shipped different
//! evidence text the day the twin was promoted.
//!
//! **And again when the last two joined (#2163).** `fuzz-track-record` still
//! printed the four-column, pre-sharding table: no `shards` column, no `Night
//! verdict` job, and `full-budget streak:` where the bash says `verdict
//! streak:` — it would have scored every sharded night as TRUNCATED.
//! `output-parity` had no `--update` ratchet, no "no baseline" path, no tool
//! checks, and did not strip `warning[…]` blocks from the oracle's stderr
//! (#1123), so a trap fixture that also prints an unused-import warning read
//! as XFAIL where the bash reads match. And the stamp's `coqc:` line was
//! empty where every caller prints `n/a`: the callers run it under
//! `set -o pipefail`, this test had not, and the twin matched the test.
//!
//! The two late twins needed an input design before they could be compared
//! honestly, and that is what `tests/gate-twin-parity/` holds:
//!
//! * `fuzz-track-record/` — a `gh` that answers from RECORDED responses (six
//!   real nights of 2026-09-16..21, two synthetic ones for the shapes that
//!   window lacked: a verdict job that never concluded, and a pre-sharding
//!   night). Both sides go through it, so the comparison neither touches the
//!   network nor changes as nights are added.
//! * `output-parity/spec/` — nine purpose-built fixtures, one per verdict row
//!   the gate can produce without a miscompile: match, the trap-row match,
//!   the #1123 warning-block match, the unbracketed-warning XFAIL, a wall, a
//!   v0fail, and the two skip shapes. MISMATCH and RUNERR need a real v1 bug
//!   to exist and are covered by the pure verdict-rule tests in
//!   `tools/almide-gates/src/parity.almd`, not here. The corpus and baseline
//!   are selected through `OUTPUT_PARITY_SPEC` / `OUTPUT_PARITY_BASELINE`,
//!   which both sides honour; the defaults are the gate.
//!
//! The invocations, for the reader (and for the ledger's `wired_by` check):
//!
//! ```text
//! bash scripts/check-contracts.sh
//! almide run tools/almide-gates/src/main.almd -- check-contracts docs/contracts
//! bash -c 'set -o pipefail; source proofs/lib/stamp.sh; stamp_toolchain <root>'
//! almide run tools/almide-gates/src/main.almd -- stamp .
//! PATH=tests/gate-twin-parity/fuzz-track-record:$PATH bash scripts/fuzz-track-record.sh 8
//! PATH=tests/gate-twin-parity/fuzz-track-record:$PATH almide run tools/almide-gates/src/main.almd -- fuzz-track-record . 8
//! OUTPUT_PARITY_SPEC=… OUTPUT_PARITY_BASELINE=… bash proofs/output-parity.sh [--update]
//! OUTPUT_PARITY_SPEC=… OUTPUT_PARITY_BASELINE=… almide run tools/almide-gates/src/main.almd -- output-parity . <oracle> <render_program> [--update]
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

struct Answer {
    code: Option<i32>,
    text: String,
}

fn run(program: &str, args: &[&str]) -> Answer {
    run_env(program, args, &[])
}

fn run_env(program: &str, args: &[&str], env: &[(&str, String)]) -> Answer {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(repo_root());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawn {program}: {e}"));
    Answer {
        code: out.status.code(),
        text: format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

/// The twin, through the tool's subcommand dispatch.
fn twin(args: &[&str]) -> Answer {
    twin_env(args, &[])
}

fn twin_env(args: &[&str], env: &[(&str, String)]) -> Answer {
    let main = repo_root().join("tools/almide-gates/src/main.almd");
    let mut argv = vec!["run", main.to_str().expect("path"), "--"];
    argv.extend_from_slice(args);
    run_env(almide(), &argv, env)
}

/// The first line that differs, named — a whole-output dump says only that
/// something moved, which is what makes drift tedious enough to ignore.
fn first_divergence(original: &str, port: &str) -> Option<String> {
    let (a, b): (Vec<_>, Vec<_>) = (original.lines().collect(), port.lines().collect());
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(""),
            b.get(i).copied().unwrap_or(""),
        );
        if x != y {
            return Some(format!("line {}\n  original: {x}\n  port:     {y}", i + 1));
        }
    }
    None
}

/// The stamp's `tree:` line counts dirty files at the moment each side runs,
/// and the sides run one after the other while the rest of this suite runs
/// beside them — a transient file from a neighbouring test moved the count
/// 8 -> 9 between the two output-parity invocations on this test's first run.
/// The count is a measurement of the environment, not of the gate, and the
/// stamp test above compares it directly; here it is masked so the parity
/// comparison is about the sweep.
fn mask_dirty_count(a: Answer) -> Answer {
    let text = a
        .text
        .lines()
        .map(|l| {
            if l.starts_with("  tree:     ") && l.ends_with(" dirty file(s)") {
                let mut parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() >= 4 {
                    parts[2] = "<n>";
                    return format!("  tree:     {}", parts[1..].join(" "));
                }
            }
            l.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Answer { code: a.code, text }
}

fn assert_same(label: &str, original: Answer, port: Answer) {
    assert_eq!(
        original.code, port.code,
        "{label}: the port exited {:?} where the original exited {:?}\n--- original ---\n{}\n--- port ---\n{}",
        port.code, original.code, original.text, port.text
    );
    if let Some(d) = first_divergence(&original.text, &port.text) {
        panic!("{label}: the port's output diverged from the original\n{d}");
    }
}

#[test]
fn the_contract_ledger_twin_answers_what_the_shell_gate_answers() {
    assert_same(
        "check-contracts",
        run("bash", &["scripts/check-contracts.sh"]),
        twin(&["check-contracts", "docs/contracts"]),
    );
}

/// `stamp` is sourced, not executed, so the original is invoked the way its
/// callers do. Both paths matter and which one runs depends on the machine:
/// the FATAL path when the PATH binary is stale, the toolchain listing when it
/// is not. The comparison covers whichever the machine is in — and the drift it
/// caught was on the FATAL path, which is the one a developer meets.
#[test]
fn the_toolchain_stamp_twin_answers_what_the_shell_gate_answers() {
    let root = repo_root();
    let root = root.to_str().expect("path");
    assert_same(
        "stamp",
        // Under `set -o pipefail`, as every caller (proofs/*.sh) runs it: the `coqc` line is
        // a pipeline whose `|| echo n/a` only fires with pipefail on, so without it this
        // test compared the twin against an invocation no caller makes (#2163).
        run(
            "bash",
            &[
                "-c",
                &format!("set -o pipefail; source proofs/lib/stamp.sh; stamp_toolchain '{root}'"),
            ],
        ),
        twin(&["stamp", "."]),
    );
}

/// PATH with the recorded-`gh` shim first, so both sides read the same answers.
fn recorded_gh_path() -> String {
    let shim = repo_root().join("tests/gate-twin-parity/fuzz-track-record");
    let rest = std::env::var("PATH").unwrap_or_default();
    format!("{}:{rest}", shim.display())
}

/// The nightly track record, scored over eight recorded nights. The window was
/// chosen so that every row shape the bash can print is in it: an in-flight run
/// (printed, not scored), a sharded night at 89% of its planned fuzz-minutes
/// (a streak night with findings), one at 64% (PARTIAL: a green verdict that is
/// not a streak night), a verdict job that never concluded (NO VERDICT, which
/// ends BOTH streaks), and a legacy night scored from its campaign step. The
/// assertions on the original's text are what keep this honest: a shim serving
/// nothing would make both sides print an empty table and agree.
#[test]
fn the_fuzz_track_record_twin_answers_what_the_shell_gate_answers() {
    // `FUZZ_NIGHT_RECOVER=0` scopes the comparison to SCORING. The bash also
    // recovers a reclaimed shard's minutes from its job log before it scores
    // the night (#2513), and the twin does not: the fold rounds the delivered
    // minutes to one decimal after EVERY shard, so a faithful port turns on
    // Almide's float formatting agreeing with printf's, which is a promise
    // neither side makes yet. Declared here rather than left to the fixtures —
    // all eight recorded nights happen to be pre-#2390 lines with no `missing=`
    // list, so the recovery would no-op today and the drift would be invisible
    // until the window was re-recorded.
    let env = [
        ("PATH", recorded_gh_path()),
        ("FUZZ_NIGHT_RECOVER", "0".to_string()),
    ];
    let original = run_env("bash", &["scripts/fuzz-track-record.sh", "8"], &env);
    for needle in [
        "IN PROGRESS (not scored)",
        "7/8 89%       FINDINGS (verdict delivered, red on findings)",
        "5/8 64%       PARTIAL 64% of planned fuzz-minutes — GREEN; below the 75% line, not a streak night",
        "?             NO VERDICT (verdict job: cancelled)",
        "1/1           FINDINGS (full budget, red on findings)",
        "verdict streak:     1/14  (#924 closes at 14: nights at >= 75% of planned fuzz-minutes)",
        "green streak:       0/2",
    ] {
        assert!(
            original.text.contains(needle),
            "fuzz-track-record: the recorded window no longer produces {needle:?} — the fixture design lost a row shape\n{}",
            original.text
        );
    }
    assert_same(
        "fuzz-track-record (8 nights)",
        original,
        twin_env(&["fuzz-track-record", ".", "8"], &env),
    );
    // `per_page=N` is honoured by the recording, so a shorter window is a
    // different table — three rows, none of them the 2026-09-18 night — and
    // not the same answer truncated.
    let short = run_env("bash", &["scripts/fuzz-track-record.sh", "3"], &env);
    assert!(
        short.text.contains("verdict streak:     1/14") && !short.text.contains("35320397098"),
        "{}",
        short.text
    );
    assert_same(
        "fuzz-track-record (3 nights)",
        short,
        twin_env(&["fuzz-track-record", ".", "3"], &env),
    );
}

fn on_path(tool: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {tool}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The oracle both sides run fixtures through: the release binary CI hands the
/// suite (`ALMIDE_BIN`), else the test build. Either is fine — it only has to
/// be the SAME binary on both sides.
fn oracle() -> String {
    std::env::var("ALMIDE_BIN")
        .ok()
        .filter(|p| Path::new(p).is_file())
        .unwrap_or_else(|| almide().to_string())
}

/// The v1 render leg. `ALMIDE_RENDER` when the caller built one (the WAT prelude
/// audit's spelling); else built once here — the bash builds it too, and that
/// second build is a no-op after this one.
fn render_program() -> PathBuf {
    if let Some(p) = std::env::var_os("ALMIDE_RENDER")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return p;
    }
    let st = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args([
            "build",
            "-q",
            "-p",
            "almide-mir",
            "--example",
            "render_program",
        ])
        .current_dir(repo_root())
        .status()
        .expect("spawn cargo");
    assert!(st.success(), "cargo build --example render_program failed");
    repo_root().join("target/debug/examples/render_program")
}

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("almide-gate-twin-{}-{name}", std::process::id()))
}

/// The output-parity gate over `tests/gate-twin-parity/output-parity/spec/`.
///
/// Two scenarios, each run on both sides and diffed to the byte:
///
/// 1. A FORGED baseline: it names a file that can never match (the part-file
///    with no entry point) and omits two that do. The original must report the
///    phantom as a REGRESSION, the omitted pair as NEW matches, the unbracketed
///    warning fixture under XFAIL, and exit 1 — the gate's negative, and the
///    offenders are named on both sides.
/// 2. `--update` into a scratch baseline: the ratchet's write, compared as the
///    bytes written and the message printed.
///
/// Tool-gated the way the wasm suites are (#983): without `wasmtime` the two
/// SKIP branches are compared and that is all; with `ALMIDE_EXPECT_TOOLS=1` the
/// sweep must have actually run.
#[test]
fn the_output_parity_twin_answers_what_the_shell_gate_answers() {
    let corpus = "tests/gate-twin-parity/output-parity/spec";
    let oracle = oracle();
    let render = render_program();
    let render = render.to_str().expect("path").to_string();
    let have_tools = on_path("wasmtime");

    let forged = scratch("forged-baseline.txt");
    std::fs::write(
        &forged,
        format!(
            "{corpus}/hello_match.almd\n{corpus}/no_main_part.almd\n{corpus}/trap_div_zero.almd\n"
        ),
    )
    .expect("write forged baseline");
    let env_for = |baseline: &Path| {
        vec![
            ("ALMIDE_BIN", oracle.clone()),
            ("OUTPUT_PARITY_SPEC", corpus.to_string()),
            (
                "OUTPUT_PARITY_BASELINE",
                baseline.to_string_lossy().into_owned(),
            ),
        ]
    };

    let env = env_for(&forged);
    let original = run_env("bash", &["proofs/output-parity.sh"], &env);
    if have_tools {
        for needle in [
            "output-parity: match=",
            " XFAIL=1 ",
            "    x tests/gate-twin-parity/output-parity/spec/trap_with_plain_warning.almd",
            "output-parity: NEW matches not yet in baseline",
            "  + tests/gate-twin-parity/output-parity/spec/trap_with_bracket_warning.almd",
            "output-parity: REGRESSION",
            "  - tests/gate-twin-parity/output-parity/spec/no_main_part.almd",
        ] {
            assert!(
                original.text.contains(needle),
                "output-parity: the forged run no longer produces {needle:?} — the fixture design lost a row\n{}",
                original.text
            );
        }
        assert_eq!(
            original.code,
            Some(1),
            "the forged baseline must fail the gate\n{}",
            original.text
        );
    } else {
        assert!(
            std::env::var("ALMIDE_EXPECT_TOOLS").is_err(),
            "ALMIDE_EXPECT_TOOLS=1 but wasmtime is not on PATH — the output-parity comparison would cover only the SKIP branch"
        );
        assert!(
            original.text.contains("wasmtime not found"),
            "{}",
            original.text
        );
    }
    assert_same(
        "output-parity (forged baseline)",
        mask_dirty_count(original),
        mask_dirty_count(twin_env(&["output-parity", ".", &oracle, &render], &env)),
    );
    let _ = std::fs::remove_file(&forged);

    if !have_tools {
        return;
    }
    let base_a = scratch("update-original.txt");
    let base_b = scratch("update-port.txt");
    let original = run_env(
        "bash",
        &["proofs/output-parity.sh", "--update"],
        &env_for(&base_a),
    );
    assert!(
        original
            .text
            .contains("output-parity: baseline updated -> "),
        "{}",
        original.text
    );
    let port = twin_env(
        &["output-parity", ".", &oracle, &render, "--update"],
        &env_for(&base_b),
    );
    let (wrote_a, wrote_b) = (
        std::fs::read_to_string(&base_a).expect("original wrote its baseline"),
        std::fs::read_to_string(&base_b).expect("port wrote its baseline"),
    );
    let _ = std::fs::remove_file(&base_a);
    let _ = std::fs::remove_file(&base_b);
    // The message names the path it wrote, and the two paths differ by design;
    // compare the message with the path masked, and the written bytes as-is.
    let mask = |a: &Answer, p: &Path| {
        mask_dirty_count(Answer {
            code: a.code,
            text: a
                .text
                .replace(&p.to_string_lossy().into_owned(), "<baseline>"),
        })
    };
    assert_same(
        "output-parity (--update)",
        mask(&original, &base_a),
        mask(&port, &base_b),
    );
    assert!(
        wrote_a.lines().count() >= 4,
        "the ratcheted baseline should hold every matching fixture\n{wrote_a}"
    );
    if let Some(d) = first_divergence(&wrote_a, &wrote_b) {
        panic!("output-parity (--update): the written baselines differ\n{d}");
    }
}

/// The comparison must be able to SEE a difference — a diff that cannot fail is
/// the same decoration as the missing check it replaces. A forged original is
/// the cheapest honest probe: it differs from the port by construction.
#[test]
fn the_comparison_names_the_first_differing_line() {
    assert_eq!(first_divergence("a\nb\nc\n", "a\nb\nc\n"), None);
    assert_eq!(
        first_divergence("a\nb\nc\n", "a\nB\nc\n").as_deref(),
        Some("line 2\n  original: b\n  port:     B")
    );
    assert_eq!(
        first_divergence("a\n", "a\nb\n").as_deref(),
        Some("line 2\n  original: \n  port:     b")
    );
}
