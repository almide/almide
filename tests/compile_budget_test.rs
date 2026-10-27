//! The compile-cost gate (ADR-0030 D7, #4136, security table row 27).
//!
//! One generator per known blowup shape. For each, the gate asserts two
//! things:
//!
//! 1. the run ends in a defined verdict — success, or the diagnostic of the
//!    budget it ran into — within a hang guard;
//! 2. the work it counted (`--budget-report`'s `work=`, nodes built for
//!    types) grows at most linearly with the source over the generator's
//!    range: doubling the input may at most `LINEAR_SLACK`-times the work per
//!    unit of input.
//!
//! The negative control lifts the type-size budget (`--budget
//! type_size=<u64::MAX>`, the operator's own flag) and runs the #4114
//! generator through the SAME predicate: it must fail, because each line
//! then doubles the work. If it passed, the linearity assertion would not be
//! reading the budget.
//!
//! D3 is pinned too: `--profile critical` refuses `--budget`, an unknown
//! budget name is an error, and the budget table `docs/specs/cli.md` embeds
//! equals `almide budgets --md`.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The hang guard: no generator below needs more than a second or two. A run
/// past this is a failure, not a slow pass.
const GUARD: Duration = Duration::from_secs(60);

/// Doubling the input may multiply the per-unit work by at most this.
const LINEAR_SLACK: f64 = 3.0;

const U64_MAX: &str = "18446744073709551615";

struct Run {
    ok: bool,
    stderr: String,
    /// `budget type_size … work=N`.
    type_work: u64,
    /// `budget exhaustiveness_steps … work=N`.
    match_work: u64,
    /// `built type_lists=N`: type element lists allocated (memory).
    lists_built: u64,
}

impl Run {
    fn work(&self, budget: &str) -> u64 {
        match budget {
            "type_size" => self.type_work,
            "exhaustiveness_steps" => self.match_work,
            "type_lists" => self.lists_built,
            other => panic!("no work column for {other}"),
        }
    }
}

fn work_of(stderr: &str, budget: &str) -> u64 {
    stderr
        .lines()
        .find(|l| l.starts_with(&format!("budget {budget} ")))
        .and_then(|l| l.split_whitespace().find_map(|f| f.strip_prefix("work=")))
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("no `budget {budget}` report line:\n{stderr}"))
}

fn scratch(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-compile-budget-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// `almide check <src> --budget-report [extra]`, killed at the guard.
fn check(name: &str, text: &str, extra: &[&str]) -> Run {
    check_env(name, text, extra, None)
}

fn check_env(name: &str, text: &str, extra: &[&str], env: Option<(&str, &str)>) -> Run {
    let path = scratch(name, text);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    if let Some((k, v)) = env {
        cmd.env(k, v);
    }
    let mut child = cmd
        .arg("check")
        .arg(&path)
        .arg("--budget-report")
        .args(extra)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn almide");
    // Drain stderr on its own thread: a run with many diagnostics fills the
    // pipe, and a child blocked on a full pipe would read as a hang.
    let mut pipe = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = pipe.read_to_string(&mut text);
        text
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().expect("wait") {
            break s;
        }
        if start.elapsed() > GUARD {
            let _ = child.kill();
            let _ = child.wait();
            panic!("`almide check {name}` ran past the {GUARD:?} hang guard: the compile cost is not bounded");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stderr = reader.join().expect("stderr reader");
    let type_work = work_of(&stderr, "type_size");
    let match_work = work_of(&stderr, "exhaustiveness_steps");
    let lists_built = stderr
        .lines()
        .find_map(|l| l.strip_prefix("built type_lists="))
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or_else(|| panic!("no `built type_lists` report line:\n{stderr}"));
    Run { ok: status.success(), stderr, type_work, match_work, lists_built }
}

/// #4114: `let t(i) = (t(i-1), t(i-1))`.
fn doubling(n: usize) -> String {
    let mut s = String::from("effect fn main() -> Unit = {\n  let t0 = 1\n");
    for i in 1..=n {
        s.push_str(&format!("  let t{i} = (t{}, t{})\n", i - 1, i - 1));
    }
    s.push_str("  println(\"ok\")\n}\n");
    s
}

/// The same doubling through a generic (ADR-0030's measured second row).
fn generic_doubling(n: usize) -> String {
    let mut s = String::from("fn dup[T](x: T) -> (T, T) = (x, x)\n\neffect fn main() -> Unit = {\n  let t0 = 1\n");
    for i in 1..=n {
        s.push_str(&format!("  let t{i} = dup(t{})\n", i - 1));
    }
    s.push_str("  println(\"ok\")\n}\n");
    s
}

/// A wide tuple match, for exhaustiveness.
fn wide_match(width: usize) -> String {
    let elems: Vec<String> = (0..width).map(|i| format!("x{i}")).collect();
    let falses: Vec<&str> = (0..width).map(|_| "false").collect();
    let trues: Vec<&str> = (0..width).map(|_| "true").collect();
    format!(
        "fn f({}) -> Int = match ({}) {{\n  ({}) => 0,\n  ({}) => 1,\n  _ => 2,\n}}\n\neffect fn main() -> Unit = println(\"${{f({})}}\")\n",
        elems.iter().map(|e| format!("{e}: Bool")).collect::<Vec<_>>().join(", "),
        elems.join(", "),
        falses.join(", "),
        trues.join(", "),
        falses.join(", "),
    )
}

/// A tuple of `Bool`s whose rows name both constructors in every column:
/// the witness search branches twice per column (Maranget's worst case).
fn bool_matrix(width: usize) -> String {
    let params: Vec<String> = (0..width).map(|i| format!("x{i}: Bool")).collect();
    let subject: Vec<String> = (0..width).map(|i| format!("x{i}")).collect();
    let mut arms = String::new();
    for (k, val) in ["true", "false"].iter().enumerate() {
        for i in 0..width {
            let row: Vec<&str> = (0..width).map(|j| if j == i { *val } else { "_" }).collect();
            arms.push_str(&format!("  ({}) => {},\n", row.join(", "), k * width + i));
        }
    }
    let args: Vec<&str> = (0..width).map(|_| "true").collect();
    format!(
        "fn f({}) -> Int = match ({}) {{\n{arms}}}\n\neffect fn main() -> Unit = println(\"${{f({})}}\")\n",
        params.join(", "),
        subject.join(", "),
        args.join(", "),
    )
}

/// A long interpolation chain.
fn interpolation(parts: usize) -> String {
    let body: String = (0..parts).map(|i| format!("${{a}}-{i}")).collect();
    format!("effect fn main() -> Unit = {{\n  let a = 1\n  println(\"{body}\")\n}}\n")
}

/// Nested parentheses.
fn nesting(depth: usize) -> String {
    format!("effect fn main() -> Unit = println(\"${{{}1{}}}\")\n", "(".repeat(depth), ")".repeat(depth))
}

/// The gate's predicate over one generator: every size ends in `verdict`
/// (checked by the caller's closure) and the counted work is linear.
fn assert_linear(shape: &str, budget: &str, sizes: &[usize], make: impl Fn(usize) -> String, extra: &[&str], verdict: impl Fn(usize, &Run)) {
    assert_linear_env(shape, budget, sizes, make, extra, None, verdict)
}

fn assert_linear_env(
    shape: &str,
    budget: &str,
    sizes: &[usize],
    make: impl Fn(usize) -> String,
    extra: &[&str],
    env: Option<(&str, &str)>,
    verdict: impl Fn(usize, &Run),
) {
    let mut prev: Option<(usize, u64)> = None;
    for &n in sizes {
        let run = check_env(&format!("{shape}-{n}.almd"), &make(n), extra, env);
        verdict(n, &run);
        if let Some((pn, pw)) = prev {
            let per_unit = run.work(budget).max(1) as f64 / n as f64;
            let prev_per_unit = pw.max(1) as f64 / pn as f64;
            assert!(
                per_unit <= LINEAR_SLACK * prev_per_unit,
                "{shape}: counted {budget} work is not linear in the source — {pw} at size {pn}, {} at size {n} ({:.1}× per unit)",
                run.work(budget),
                per_unit / prev_per_unit,
            );
        }
        prev = Some((n, run.work(budget)));
    }
}

fn ends_in_e111(shape: &'static str) -> impl Fn(usize, &Run) {
    move |n, run| {
        assert!(!run.ok, "{shape} at {n} checked clean past the type-size budget");
        assert!(run.stderr.contains("error[E111]"), "{shape} at {n}: expected E111:\n{}", run.stderr);
    }
}

fn ends_clean(shape: &'static str) -> impl Fn(usize, &Run) {
    move |n, run| assert!(run.ok, "{shape} at {n} failed:\n{}", run.stderr)
}

#[test]
fn the_doubling_chain_ends_in_e111_with_linear_work() {
    assert_linear("doubling", "type_size", &[16, 32, 64, 128], doubling, &[], ends_in_e111("doubling"));
}

#[test]
fn the_generic_doubling_ends_in_e111_with_linear_work() {
    assert_linear("generic-doubling", "type_size", &[16, 32, 64, 128], generic_doubling, &[], ends_in_e111("generic doubling"));
}

#[test]
fn a_wide_match_checks_with_linear_work() {
    assert_linear("wide-match", "exhaustiveness_steps", &[6, 12, 24], wide_match, &[], ends_clean("wide match"));
}

#[test]
fn the_bool_matrix_ends_in_e113_with_linear_work() {
    assert_linear("bool-matrix", "exhaustiveness_steps", &[24, 48, 96], bool_matrix, &[], |n, run| {
        assert!(!run.ok, "bool matrix at {n} checked clean past the exhaustiveness budget");
        assert!(run.stderr.contains("error[E113]"), "bool matrix at {n}: expected E113:\n{}", run.stderr);
    });
    // Small, the same shape is judged in full (its later rows are E014,
    // unreachable — a verdict, not the budget).
    let small = check("bool-matrix-4.almd", &bool_matrix(4), &[]);
    assert!(!small.stderr.contains("E113"), "a 4-wide bool matrix ran into the budget:\n{}", small.stderr);
}

#[test]
fn a_long_interpolation_checks_with_linear_work() {
    assert_linear("interpolation", "type_size", &[500, 1000, 2000, 4000], interpolation, &[], ends_clean("interpolation"));
}

#[test]
fn deep_nesting_ends_at_the_nesting_budget() {
    let run = check("nesting-600.almd", &nesting(600), &[]);
    assert!(!run.ok, "600 levels of nesting checked clean");
    assert!(run.stderr.contains("expression nesting too deep (max 500)"), "expected the nesting budget:\n{}", run.stderr);
    let deep = check("nesting-50000.almd", &nesting(50_000), &[]);
    assert!(deep.stderr.contains("expression nesting too deep (max 500)"), "expected the nesting budget at 50,000:\n{}", deep.stderr);
}

/// D3: the operator raises a budget on the command line, and the verdict
/// moves with it — the same source that ends at the default passes.
#[test]
fn the_command_line_raises_a_budget() {
    let run = check("nesting-raised.almd", &nesting(600), &["--budget", "nesting=700"]);
    assert!(run.ok, "`--budget nesting=700` did not admit 600 levels:\n{}", run.stderr);
    let small = check("doubling-small.almd", &doubling(10), &["--budget", "type_size=64"]);
    assert!(small.stderr.contains("error[E111]"), "`--budget type_size=64` did not lower the budget:\n{}", small.stderr);
    assert!(small.stderr.contains("max 64"), "the message names the budget in force:\n{}", small.stderr);
}

/// D3: `--profile critical` refuses `--budget`, and a budget the table does
/// not name is an error, not a silent no-op.
#[test]
fn critical_refuses_a_budget_and_an_unknown_one_is_an_error() {
    let path = scratch("plain.almd", "effect fn main() -> Unit = println(\"ok\")\n");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["check"]).arg(&path).args(["--profile", "critical", "--allow", "IO", "--budget", "type_size=100000"])
        .output().expect("spawn almide");
    assert!(!out.status.success(), "--profile critical accepted --budget");
    assert!(String::from_utf8_lossy(&out.stderr).contains("--profile critical refuses --budget"));
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["check"]).arg(&path).args(["--budget", "no_such_budget=3"])
        .output().expect("spawn almide");
    assert!(!out.status.success(), "an unknown budget name was accepted");
    assert!(String::from_utf8_lossy(&out.stderr).contains("no budget named `no_such_budget`"));
}

/// D1: the table in `docs/specs/cli.md` is `almide budgets --md`, byte for byte.
#[test]
fn the_cli_spec_embeds_the_budget_table() {
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).args(["budgets", "--md"]).output().expect("spawn almide");
    assert!(out.status.success());
    let table = String::from_utf8_lossy(&out.stdout).to_string();
    let spec = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/specs/cli.md")).unwrap();
    let begin = "<!-- almide budgets --md: begin -->\n";
    let end = "<!-- almide budgets --md: end -->";
    let a = spec.find(begin).expect("cli.md has the budgets block") + begin.len();
    let b = spec[a..].find(end).expect("the budgets block ends") + a;
    assert_eq!(&spec[a..b], table, "docs/specs/cli.md's budget table drifted from `almide budgets --md`");
}

/// The negative control: with the type-size budget lifted, the #4114
/// generator's work doubles with every line, and the gate's own predicate
/// must refuse it.
#[test]
fn lifting_the_type_budget_fails_the_gate() {
    let value = format!("type_size={U64_MAX}");
    let lifted = ["--budget", value.as_str()];
    let result = std::panic::catch_unwind(|| {
        assert_linear("doubling-lifted", "type_size", &[8, 12, 16], doubling, &lifted, |_, _| {});
    });
    assert!(result.is_err(), "the doubling chain passed the linearity gate with the type-size budget lifted — the gate does not read the budget");
}

/// A chain of path dependencies `p0 → p1 → … → p5`; returns `p0`'s directory.
fn dependency_chain(len: usize) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-compile-budget-deps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for i in 0..len {
        let dir = root.join(format!("p{i}"));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let mut manifest = format!("[package]\nname = \"p{i}\"\nversion = \"0.1.0\"\n");
        if i + 1 < len {
            manifest.push_str(&format!("\n[dependencies]\np{} = {{ path = \"../p{}\" }}\n", i + 1, i + 1));
        }
        std::fs::write(dir.join("almide.toml"), manifest).unwrap();
        let src = if i == 0 {
            "import p1\n\neffect fn main() -> Unit = println(p1.name())\n".to_string()
        } else if i + 1 < len {
            format!("import p{n}\n\nfn name() -> String = \"p{i}+\" + p{n}.name()\n", n = i + 1)
        } else {
            format!("fn name() -> String = \"p{i}\"\n")
        };
        let file = if i == 0 { "main.almd" } else { "mod.almd" };
        std::fs::write(dir.join("src").join(file), src).unwrap();
    }
    root.join("p0")
}

fn check_in(dir: &std::path::Path, extra: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("check")
        .args(extra)
        .current_dir(dir)
        .output()
        .expect("spawn almide");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr))
}

/// The project-side budgets: dependency depth and count are refused before
/// a fetch, and a manifest over `manifest_bytes` before it is parsed. Each is
/// shown at a lowered budget against the same project that passes at the
/// default, so the refusal is the budget's.
#[test]
fn the_dependency_and_manifest_budgets_refuse_before_fetching() {
    let app = dependency_chain(6);
    let (ok, out) = check_in(&app, &[]);
    assert!(ok, "the six-package chain does not check at the default budgets:\n{out}");
    let (ok, out) = check_in(&app, &["--budget", "dependency_depth=3"]);
    assert!(!ok && out.contains("dependency chain too deep (4, max 3)"), "dependency_depth=3:\n{out}");
    let (ok, out) = check_in(&app, &["--budget", "dependency_count=2"]);
    assert!(!ok && out.contains("too many dependencies (3, max 2)"), "dependency_count=2:\n{out}");
    let (ok, out) = check_in(&app, &["--budget", "manifest_bytes=10"]);
    assert!(!ok && out.contains("is larger than 10 bytes"), "manifest_bytes=10:\n{out}");
}

/// ADR-0030 D5: types share structure. With the type-size budget lifted, the
/// #4114 chain checks clean, and the type lists it allocates — its memory —
/// grow linearly with the chain, within the hang guard (its time).
#[test]
fn the_doubling_chain_shares_its_types_with_the_budget_lifted() {
    let value = format!("type_size={U64_MAX}");
    let lifted = ["--budget", value.as_str()];
    assert_linear("doubling-shared", "type_lists", &[16, 32, 64, 128], doubling, &lifted, ends_clean("doubling, budget lifted"));
    assert_linear("generic-doubling-shared", "type_lists", &[16, 32, 64, 128], generic_doubling, &lifted, ends_clean("generic doubling, budget lifted"));
}

/// The negative control of the sharing gate: with sharing disabled
/// (`ALMIDE_CONTROL_NO_TYPE_SHARING`, every clone copies the tree) the same
/// predicate must fail, because the lists allocated double with each line.
#[test]
fn disabling_type_sharing_fails_the_sharing_gate() {
    let value = format!("type_size={U64_MAX}");
    let lifted = ["--budget", value.as_str()];
    let result = std::panic::catch_unwind(|| {
        assert_linear_env(
            "doubling-unshared",
            "type_lists",
            &[8, 12, 16],
            doubling,
            &lifted,
            Some(("ALMIDE_CONTROL_NO_TYPE_SHARING", "1")),
            |_, _| {},
        );
    });
    assert!(result.is_err(), "the doubling chain stayed linear with type sharing disabled — the gate does not read the sharing");
}
