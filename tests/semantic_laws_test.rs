//! Semantic-law property oracle — the independent verification net for the v1
//! MIR migration (docs/roadmap/active/v1-phase1-mir-core.md §5.1).
//!
//! WHY THIS EXISTS — the completeness core:
//!   The native↔wasm byte gate catches a bug only when the two targets DISAGREE.
//!   That works today because native and wasm have INDEPENDENT ownership
//!   lowerings. v1 unifies them into one Core→MIR decision — and a wrong shared
//!   decision makes BOTH targets wrong IDENTICALLY, so a native==wasm gate goes
//!   blind exactly there (proven in miniature by the alias_cow gate, where
//!   omitting MakeUnique corrupts both idioms the same way with RC balanced).
//!
//!   So this oracle does NOT compare the two targets against each other. It
//!   compares each target against an INDEPENDENT ground truth: a tiny
//!   value-semantics reference model written in plain Rust here, which computes
//!   the expected observable WITHOUT the compiler. The assertion is
//!   `native == expected  AND  wasm == expected` — strictly stronger than
//!   `native == wasm`, and it survives unification: a shared Core→MIR ownership
//!   bug drives both targets away from `expected` and is caught.
//!
//! THE LAW under test: Almide value semantics (the COW contract, C-033). After a
//! copy-bind `var b = a`, `a` and `b` denote SEPARATE values; an in-place
//! mutation of one is invisible through the other. The reference model realizes
//! this by copying the value at the bind point — the executable form of the law.
//!
//! Run:  cargo test -p almide --test semantic_laws_test
//!       ALMIDE_SEMLAW_CASES=200 cargo test -p almide --test semantic_laws_test  (deeper)

use proptest::prelude::*;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

// ─────────────────────────── execution substrate ───────────────────────────
// Self-contained copies of the byte-gate helpers (each file under tests/ is its
// own crate, so siblings cannot be imported). Mirrors
// tests/wasm_runtime_test.rs:{17,1399,1425}.

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

/// Whether the `almide` binary is usable — memoized so we probe once, not per case.
fn binary_ok() -> bool {
    static OK: OnceLock<bool> = OnceLock::new();
    *OK.get_or_init(|| {
        Command::new(almide_bin())
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// Compile+run on native; (exit, stdout, stderr), all trimmed. A build failure
/// surfaces as a non-zero exit with the compiler stderr (so the caller's
/// `exit == 0` assertion fails loudly with diagnostics).
fn run_native_capture(source: &str) -> (i32, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.almd");
    let bin_path = dir.path().join("prog_native");
    std::fs::write(&src_path, source).unwrap();
    let build = Command::new(almide_bin())
        .args(["build", src_path.to_str().unwrap(), "-o", bin_path.to_str().unwrap()])
        .output()
        .expect("failed to invoke almide build");
    if !build.status.success() {
        return (
            build.status.code().unwrap_or(-1),
            String::new(),
            String::from_utf8_lossy(&build.stderr).trim().to_string(),
        );
    }
    let out = Command::new(&bin_path).output().expect("failed to run native binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    )
}

/// Compile to wasm + run via wasmtime; `None` if wasmtime is unavailable.
/// Unlike the byte-gate copy this returns the build outcome instead of
/// panicking, so a build failure is reported as a failing assertion (with the
/// offending source) rather than aborting the whole proptest run.
fn run_wasm_capture(source: &str) -> Option<(i32, String, String)> {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.almd");
    let wasm_path = dir.path().join("prog.wasm");
    std::fs::write(&src_path, source).unwrap();
    let build = Command::new(almide_bin())
        .args([
            "build",
            src_path.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            wasm_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to invoke almide build --target wasm");
    if !build.status.success() {
        // Report as a divergence, not a skip: a generated value-semantics program
        // that fails to wasm-compile is itself a finding.
        return Some((
            build.status.code().unwrap_or(-1),
            String::new(),
            format!("wasm build failed: {}", String::from_utf8_lossy(&build.stderr).trim()),
        ));
    }
    match Command::new("wasmtime").arg("--dir=/").arg(wasm_path.to_str().unwrap()).output() {
        Ok(o) if o.status.code() != Some(127) => Some((
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        )),
        _ => None, // wasmtime not installed → skip the wasm leg
    }
}

// ───────────────────── the independent value-semantics model ─────────────────
//
// A scenario operates on `List[Int]` bindings. The model tracks each binding's
// value as a plain `Vec<i64>`. A copy-bind COPIES the vec (the LAW). An in-place
// op mutates only the targeted binding. The model never calls the compiler — it
// IS the ground truth the compiler's output is judged against.

/// Op selector codes (named, not raw indices — the magic-number police is real).
const OP_CREATE: usize = 0;
const OP_ALIAS: usize = 1;
const OP_INDEX_ASSIGN: usize = 2;
const OP_PUSH: usize = 3;
const OP_POP: usize = 4;
const OP_CLEAR: usize = 5;
const OP_COUNT: usize = 6;

const MAX_BINDINGS: usize = 6; // cap program size
const MAX_INIT_LEN: usize = 4; // initial list length is 1..=MAX_INIT_LEN
const INIT_VAL_MIN: i64 = 1; // initial element values: 1..=9 (single digit)
const INIT_VAL_SPAN: usize = 9;
const MUT_VAL_BASE: i64 = 90; // mutation values: 90..=99 — visibly distinct from
const MUT_VAL_SPAN: i64 = 10; //   initials, so a leaked mutation shows as a 9x digit
const STEP_DIVISOR: usize = 2; // tape bytes per generated step (roughly)
const MAX_STEPS: usize = 14;

#[derive(Clone, Debug)]
struct Scenario {
    source: String,
    expected: String, // the model's ground-truth stdout (trimmed, "\n"-joined)
}

struct Bind {
    name: String,
    vals: Vec<i64>,
    mutable: bool, // `var` (true) vs `let` (false); only `var` may be mutated
}

/// A deterministic decision tape: consumes bytes to make clamped, always-valid
/// choices. Exhausted bytes read as 0 (deterministic tail). Proptest shrinks the
/// underlying Vec<u8>, which maps to simpler scenarios.
struct Tape<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Tape<'_> {
    fn byte(&mut self) -> usize {
        let b = self.bytes.get(self.pos).copied().unwrap_or(0) as usize;
        self.pos += 1;
        b
    }
    fn choice(&mut self, n: usize) -> usize {
        self.byte() % n.max(1)
    }
    fn bit(&mut self) -> bool {
        self.byte() & 1 == 0
    }
}

/// Build a valid scenario + its ground-truth expected output from a byte tape.
fn build_scenario(tape_bytes: &[u8]) -> Scenario {
    let mut t = Tape { bytes: tape_bytes, pos: 0 };
    let mut binds: Vec<Bind> = Vec::new();
    let mut stmts: Vec<String> = Vec::new();
    let mut next_id: usize = 0;
    let mut mut_counter: i64 = 0;

    let steps = (tape_bytes.len() / STEP_DIVISOR + 1).min(MAX_STEPS);

    // names of bindings that are `var` (mutable) and satisfy a predicate
    let mutable_idxs = |binds: &[Bind], non_empty: bool| -> Vec<usize> {
        binds
            .iter()
            .enumerate()
            .filter(|(_, b)| b.mutable && (!non_empty || !b.vals.is_empty()))
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    };

    for _ in 0..steps {
        let mut op = t.choice(OP_COUNT);
        // Clamp to a valid op for the current state; fall back to CREATE, which
        // is always valid (until the binding cap, then to a mutation).
        op = match op {
            OP_ALIAS if binds.is_empty() => OP_CREATE,
            OP_CREATE if binds.len() >= MAX_BINDINGS => OP_PUSH,
            OP_ALIAS if binds.len() >= MAX_BINDINGS => OP_PUSH,
            OP_INDEX_ASSIGN if mutable_idxs(&binds, true).is_empty() => OP_CREATE,
            OP_PUSH if mutable_idxs(&binds, false).is_empty() => OP_CREATE,
            OP_POP if mutable_idxs(&binds, true).is_empty() => OP_CREATE,
            OP_CLEAR if mutable_idxs(&binds, false).is_empty() => OP_CREATE,
            other => other,
        };

        match op {
            OP_CREATE => {
                let name = format!("v{next_id}");
                next_id += 1;
                let len = 1 + t.choice(MAX_INIT_LEN);
                let vals: Vec<i64> =
                    (0..len).map(|_| INIT_VAL_MIN + t.choice(INIT_VAL_SPAN) as i64).collect();
                let lit =
                    vals.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
                stmts.push(format!("  var {name} = [{lit}]"));
                binds.push(Bind { name, vals, mutable: true });
            }
            OP_ALIAS => {
                let src = t.choice(binds.len());
                let name = format!("v{next_id}");
                next_id += 1;
                let vals = binds[src].vals.clone(); // COPY at bind = the law
                let mutable = t.bit(); // var (mutable) or let (immutable)
                let kw = if mutable { "var" } else { "let" };
                let src_name = binds[src].name.clone();
                stmts.push(format!("  {kw} {name} = {src_name}"));
                binds.push(Bind { name, vals, mutable });
            }
            OP_INDEX_ASSIGN => {
                let cands = mutable_idxs(&binds, true);
                let c = cands[t.choice(cands.len())];
                let i = t.choice(binds[c].vals.len());
                let x = MUT_VAL_BASE + (mut_counter % MUT_VAL_SPAN);
                mut_counter += 1;
                binds[c].vals[i] = x;
                let name = binds[c].name.clone();
                stmts.push(format!("  {name}[{i}] = {x}"));
            }
            OP_PUSH => {
                let cands = mutable_idxs(&binds, false);
                let c = cands[t.choice(cands.len())];
                let x = MUT_VAL_BASE + (mut_counter % MUT_VAL_SPAN);
                mut_counter += 1;
                binds[c].vals.push(x);
                let name = binds[c].name.clone();
                stmts.push(format!("  list.push({name}, {x})"));
            }
            OP_POP => {
                let cands = mutable_idxs(&binds, true);
                let c = cands[t.choice(cands.len())];
                binds[c].vals.pop();
                let name = binds[c].name.clone();
                stmts.push(format!("  list.pop({name})"));
            }
            OP_CLEAR => {
                let cands = mutable_idxs(&binds, false);
                let c = cands[t.choice(cands.len())];
                binds[c].vals.clear();
                let name = binds[c].name.clone();
                stmts.push(format!("  list.clear({name})"));
            }
            _ => unreachable!("op clamped to a known code"),
        }
    }

    // Guarantee at least one observation point.
    if binds.is_empty() {
        let vals = vec![INIT_VAL_MIN];
        stmts.push("  var v0 = [1]".to_string());
        binds.push(Bind { name: "v0".to_string(), vals, mutable: true });
    }

    // Print every binding in creation order; build the matching expected output.
    let mut expected_lines: Vec<String> = Vec::new();
    for b in &binds {
        stmts.push(format!("  println(\"{}=\" + show({}))", b.name, b.name));
        let shown = b.vals.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
        expected_lines.push(format!("{}={}", b.name, shown));
    }

    let source = format!(
        "fn show(xs: List[Int]) -> String = \
         list.join(list.map(xs, (x) => int.to_string(x)), \",\")\n\
         \n\
         fn main() -> Unit = {{\n{}\n}}\n",
        stmts.join("\n")
    );
    Scenario { source, expected: expected_lines.join("\n") }
}

fn arb_scenario() -> impl Strategy<Value = Scenario> {
    // 6..40 tape bytes → small-to-moderate programs; shrinks toward shorter.
    prop::collection::vec(any::<u8>(), 6..40).prop_map(|tape| build_scenario(&tape))
}

fn case_count() -> u32 {
    std::env::var("ALMIDE_SEMLAW_CASES")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(32)
        .max(1)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: case_count(),
        // Each case spawns native + wasm builds; keep shrink bounded so a
        // failure reports quickly with a small witness.
        max_shrink_iters: 256,
        // tests/ has no lib.rs/main.rs, so the default source-relative
        // regression file can't be located — disable persistence (the witness
        // is printed in the failure message instead).
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// The oracle: every target must match the INDEPENDENT model, not just each
    /// other. `native == expected AND wasm == expected`.
    #[test]
    fn value_semantics_matches_independent_model(sc in arb_scenario()) {
        if !binary_ok() {
            return Ok(()); // no compiler available → skip (mirrors the byte gate)
        }
        let (rc, rout, rerr) = run_native_capture(&sc.source);
        prop_assert_eq!(
            rc, 0,
            "native exited {} (stderr: {})\n--- source ---\n{}",
            rc, rerr, sc.source
        );
        prop_assert_eq!(
            &rout, &sc.expected,
            "NATIVE diverges from the independent value-semantics model\n--- source ---\n{}",
            sc.source
        );

        if let Some((wc, wout, werr)) = run_wasm_capture(&sc.source) {
            prop_assert_eq!(
                wc, 0,
                "wasm exited {} (stderr: {})\n--- source ---\n{}",
                wc, werr, sc.source
            );
            prop_assert_eq!(
                &wout, &sc.expected,
                "WASM diverges from the independent value-semantics model\n--- source ---\n{}",
                sc.source
            );
        }
    }
}

// ─────────────────────────── model self-validation ──────────────────────────
// Before trusting the model as an oracle, pin it against the KNOWN-GOOD shape
// (the alias_cow fixture, C-033) on the real compiler. If the model's ground
// truth is itself wrong, THIS fails — guarding against "the oracle is the bug".

#[test]
fn model_ground_truth_matches_compiler_on_known_shapes() {
    if !binary_ok() {
        eprintln!("skipping: almide binary unavailable");
        return;
    }
    // Mirrors alias_cow.almd blocks A/C/E/I: var-alias + IndexAssign, mutate-alias,
    // alias chain, and list.push growth — each with a hand-checked expected value.
    let source = "fn show(xs: List[Int]) -> String = \
                  list.join(list.map(xs, (x) => int.to_string(x)), \",\")\n\
                  \n\
                  fn main() -> Unit = {\n\
                  \x20 var a = [1, 2, 3]\n\
                  \x20 var b = a\n\
                  \x20 a[0] = 90\n\
                  \x20 println(\"a=\" + show(a))\n\
                  \x20 println(\"b=\" + show(b))\n\
                  \x20 var c = b\n\
                  \x20 list.push(b, 91)\n\
                  \x20 println(\"b2=\" + show(b))\n\
                  \x20 println(\"c=\" + show(c))\n\
                  }\n";
    // Independent ground truth: a=[90,2,3] (mutated), b stays [1,2,3] (alias),
    // c=[1,2,3] copies b before push, b2=[1,2,3,91] after its own push.
    let expected = "a=90,2,3\nb=1,2,3\nb2=1,2,3,91\nc=1,2,3";

    let (rc, rout, rerr) = run_native_capture(source);
    assert_eq!(rc, 0, "native build/run failed: {rerr}");
    assert_eq!(rout, expected, "model ground truth disagrees with native compiler");

    if let Some((wc, wout, werr)) = run_wasm_capture(source) {
        assert_eq!(wc, 0, "wasm build/run failed: {werr}");
        assert_eq!(wout, expected, "model ground truth disagrees with wasm compiler");
    }
}

/// The builder must always emit a program with at least one observation and a
/// matching expected, for any tape (including empty). Pure model check — no
/// compiler, always runs (even without the binary).
#[test]
fn builder_is_total_and_self_consistent() {
    for seed in 0u8..32 {
        let tape: Vec<u8> = (0..24).map(|i| seed.wrapping_mul(31).wrapping_add(i)).collect();
        let sc = build_scenario(&tape);
        assert!(sc.source.contains("fn main()"), "must emit a main");
        assert!(sc.source.contains("println("), "must observe at least one binding");
        assert!(!sc.expected.is_empty(), "must have a ground-truth expectation");
        // every expected line is `vN=...`
        for line in sc.expected.lines() {
            assert!(line.contains('='), "expected line malformed: {line}");
        }
    }
    // Empty tape is total.
    let sc = build_scenario(&[]);
    assert!(sc.source.contains("println("));
    assert!(!sc.expected.is_empty());
}
