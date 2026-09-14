//! #2121: `almide test --target wasm` must not report success for a file whose
//! tests never ran there.
//!
//! The test lane renders every file through the INCUMBENT brick, while
//! `build`/`run`/`check --target wasm` render through the two-leg router whose
//! default is the STRUCTURAL leg (#2179). So the lane walls on programs the
//! product compiles — and it reported those walls as benign SKIPs and exited 0.
//! In this repository's own spec corpus that was five files: `almide test spec/
//! --target wasm`, a CI gate, was green while five files' tests had not run on
//! wasm, under a reason line ("no verified wasm rendering") that named a v1
//! verdict as if it were the product's.
//!
//! The distinction this pins: a skip the AUTHOR declared and a skip a RENDERER
//! decided are not the same verdict, and the lane must say which one it made.
//! A declared `// wasm:skip` means wasm CANNOT run the file; a WALL means a leg
//! has not lowered the shape yet. The marker is not a place to park the second
//! — `tests/wasm_skip_ledger_test.rs` refuses subset debt outright (#812) — so
//! the walls get [`TEST_LANE_WALLS`] below, held shrink-only in both directions
//! by [`the_wall_register_lists_exactly_the_specs_that_wall`].
//!
//! That register lives HERE rather than in a `scripts/check-*.sh`: it is a
//! ledger, and #2128's decision is that a gate reading a ledger is written in
//! Almide, not shell — every `.sh` row in `proofs/gate-verification.toml` is
//! UNCLASSIFIED debt under a shrink-only ceiling, and adding one more would
//! have needed that ceiling raised to land a fix for a different problem.

use std::process::Command;

/// A `pub fn` matching a variant produced INSIDE it and returning a String —
/// the #2160 shape. The incumbent brick walls it ("heap-result `match` outside
/// the executable subset"); `almide build --target wasm` renders the same file
/// on the structural leg and reports `verified`.
const WALLS_THE_INCUMBENT: &str = r#"
type T = | A(String) | B

fn make(s: String) -> T = if s == "" then B else A(s)

pub fn f(s: String) -> String = match make(s) { A(x) => x, B => "b" }

test "f returns the payload" {
  assert_eq(f("x"), "x")
}
"#;

/// The control: an ordinary file whose tests DO run on the lane.
const RUNS_ON_THE_LANE: &str = r#"
fn double(n: Int) -> Int = n * 2

test "double doubles" {
  assert_eq(double(21), 42)
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Run `almide test <file> --target wasm` and return `(exit code, report)`.
fn test_on_wasm(dir: &std::path::Path, source: &str) -> (Option<i32>, String) {
    let file = dir.join("t_test.almd");
    std::fs::write(&file, source).expect("source");
    let out = Command::new(almide_bin())
        .args(["test", file.to_str().expect("path"), "--target", "wasm"])
        .current_dir(dir)
        .output()
        .expect("run");
    let report =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    (out.status.code(), report)
}

#[test]
fn a_renderer_wall_is_reported_as_a_wall_not_as_a_plain_skip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (_code, report) = test_on_wasm(dir.path(), WALLS_THE_INCUMBENT);
    assert!(
        report.contains("WALL ") && report.contains("tests did not run on wasm"),
        "a renderer wall must be reported under its own greppable verdict — the register's \
         gate reads these lines, and a wall that looked like every other skip is how five \
         files stopped running on wasm unnoticed:\n{report}"
    );
    assert!(
        !report.contains("no verified wasm rendering"),
        "the reason must name the LEG that declined, not claim the product has no wasm \
         rendering — `almide build --target wasm` renders this same file on the structural \
         leg and reports `verified`:\n{report}"
    );
    assert!(
        report.contains("#2179"),
        "and must name the issue that removes the wall:\n{report}"
    );
}

/// The spec files whose tests do NOT run on the wasm lane because a renderer
/// declined them — the other way a file can fail to reach that leg, and the one
/// `tests/wasm_skip_ledger_test.rs` is blind to by construction (a wall carries
/// no marker, so it appeared in no ledger at all).
///
/// A row here is NOT the claim a `// wasm:skip` makes. That marker says wasm
/// CANNOT run the file and its ledger refuses subset debt outright (#812, "fix
/// the wall rather than parking it here"). A row here says a LEG has not
/// lowered the shape yet, and names the issue that removes it.
///
/// Every row below is ONE cause: the test lane renders through the incumbent
/// brick alone, while `build`/`run`/`check --target wasm` render through the
/// two-leg router whose default is the structural leg. `almide build --target
/// wasm` reports `structural leg, verified` for all four. #2179 gives the lane
/// that route and empties this table.
const TEST_LANE_WALLS: &[(&str, &str)] = &[
    // The incumbent leaves `Pt.repr` / `__repr_list_rec_reprlib_Cfg` unlinked;
    // a dangling call would be invalid wasm, so it refuses honestly.
    ("spec/integration/modules/cross_module_repr_derive_test.almd", "#2179"),
    // `pub fn area_note`: a heap-result `match` outside the incumbent's
    // executable subset — the #2160 shape.
    ("spec/lang/as_pattern_test.almd", "#2179"),
    // `pub fn total`: a `match` over an untracked subject with a call- or
    // assign-carrying arm.
    ("spec/lang/list_rest_pattern_test.almd", "#2179"),
    // `zli_gunzip_members` / `zli_inflate_stream` unlinked. NOTE: the skip
    // ledger retired this file's row on 2026-09-01 saying it "runs the wasm leg
    // for real" after #1700, and docs/stdlib/zlib.md still says the file is
    // marked `// wasm:skip`. Neither was true, and the silent wall is why
    // nobody noticed.
    ("spec/stdlib/zlib_test.almd", "#2179"),
];

/// The register, held equal to what the lane actually reports — BOTH
/// directions. A new wall cannot join silently, and a row whose file now runs
/// must be deleted: this only shrinks.
///
/// It runs the lane over `spec/` rather than re-deriving the verdict, so there
/// is no second spelling of "does this file wall" to drift from the first. The
/// wall verdict is decided at RENDER time, so this is correct with or without
/// wasmtime on the box (without it the files that do run become Environment
/// skips, which are not walls).
#[test]
fn the_wall_register_lists_exactly_the_specs_that_wall() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(almide_bin())
        .args(["test", "spec/", "--target", "wasm"])
        .current_dir(root)
        .output()
        .expect("run the lane");
    let report =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let observed: std::collections::BTreeSet<&str> = report
        .lines()
        .filter_map(|l| l.strip_prefix("WALL "))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    let registered: std::collections::BTreeSet<&str> =
        TEST_LANE_WALLS.iter().map(|(f, _)| *f).collect();

    let new: Vec<&&str> = observed.difference(&registered).collect();
    assert!(
        new.is_empty(),
        "these files' tests did not run on wasm and are not in TEST_LANE_WALLS: {new:?}\n\
         Fix the wall, or add a row naming the issue that removes it. A `// wasm:skip` is NOT \
         the place — that says wasm CANNOT run the file, and its ledger refuses subset debt \
         (#812)."
    );
    let stale: Vec<&&str> = registered.difference(&observed).collect();
    assert!(
        stale.is_empty(),
        "TEST_LANE_WALLS lists files that now run on wasm: {stale:?}\nDelete the row — the \
         register only shrinks."
    );

    for (f, issue) in TEST_LANE_WALLS {
        assert!(root.join(f).exists(), "the register names a file that does not exist: {f}");
        assert!(!issue.is_empty(), "{f}: a row must name the issue that removes it");
        assert!(
            !std::fs::read_to_string(root.join(f))
                .unwrap_or_default()
                .lines()
                .any(|l| l.trim_start().starts_with("// wasm:skip")),
            "{f} is registered as subset debt AND carries a platform-limit marker — it must \
             be one or the other (#812)"
        );
    }
}

#[test]
fn the_same_file_with_a_declared_skip_is_a_skip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let declared = format!("// wasm:skip — pinned by tests/wasm_test_lane_wall_test.rs\n{WALLS_THE_INCUMBENT}");
    let (code, report) = test_on_wasm(dir.path(), &declared);
    assert_eq!(
        code,
        Some(0),
        "a skip the author DECLARED, with its reason in the file, stays benign:\n{report}"
    );
    assert!(
        report.contains("SKIP") && report.contains("wasm:skip"),
        "the declared skip must still be reported as a skip:\n{report}"
    );
}

/// The happy path must be untouched: classifying skips changed no verdict for a
/// file that actually runs.
#[test]
fn a_file_whose_tests_run_on_the_lane_still_passes() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, report) = test_on_wasm(dir.path(), RUNS_ON_THE_LANE);
    assert_eq!(code, Some(0), "an ordinary passing file must still pass:\n{report}");
    assert!(report.contains("1 tests passed"), "and must report its test:\n{report}");
}

/// The product lane's verdict on the SAME file, which is what makes the old
/// reason line a lie rather than merely terse.
#[test]
fn the_build_lane_renders_the_file_the_test_lane_walls() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("t_test.almd");
    std::fs::write(&file, WALLS_THE_INCUMBENT).expect("source");
    let out = Command::new(almide_bin())
        .args([
            "build",
            file.to_str().expect("path"),
            "--target",
            "wasm",
            "-o",
            dir.path().join("m.wasm").to_str().expect("path"),
        ])
        .env_remove("ALMIDE_WASM_INCUMBENT")
        .output()
        .expect("build");
    let report =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "the build lane must render this file:\n{report}");
    assert!(
        report.contains("structural leg") && report.contains("verified"),
        "and must render it on the structural leg — that is the leg the test lane never \
         asks (#2179):\n{report}"
    );
}
