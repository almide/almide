//! E015 (reimpl lint) must name the *closest* stdlib fn, and must name
//! the same one on every run.
//!
//! The lint's candidate scan walks `module_fn_names`, which projects a
//! `HashMap`'s keys. Taking the first candidate that cleared the
//! name-distance and signature gates therefore made the suggestion a
//! function of hash iteration order: `fn atan(x: Float) -> Float` was
//! reported against `math.atan` on some runs and `math.tan` on others,
//! and `fn decompress(...)` was told to delegate to `zlib.compress` —
//! advice that silently inverts the operation if followed.
//!
//! Two invariants, both checked here because they fail independently:
//!
//! 1. **Best match.** When several stdlib names clear the gates, the
//!    smallest edit distance wins, so an exact name match always beats a
//!    near-miss.
//! 2. **Stability.** Repeated runs over the same source name the same
//!    fn. A diagnostic that changes between runs is unusable as a
//!    modification target — the metric this compiler optimises for.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// Run `almide check` on `source` and return the combined output.
fn check(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("reimpl.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Extract the `module.fn` the E015 warning points at.
fn suggested_fn(output: &str) -> Option<String> {
    let line = output
        .lines()
        .find(|l| l.contains("has the same signature as stdlib"))?;
    let start = line.find('`')? + 1;
    let rest = &line[start..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

/// Each case is a user fn whose name is an exact stdlib name but which
/// also sits within edit distance 2 of a *different* stdlib name, so the
/// first-match scan could pick either. The expectation is the exact one.
const CASES: &[(&str, &str)] = &[
    // `atan` is distance 1 from `tan`.
    (
        "fn atan(x: Float) -> Float = x\neffect fn main() -> Unit = {\n  println(float.to_string(atan(1.0)))\n}",
        "math.atan",
    ),
    // `decompress` is distance 2 from `compress` — and delegating to
    // `compress` would be actively wrong.
    (
        "fn decompress(data: Bytes) -> Result[Bytes, String] = ok(data)\neffect fn main() -> Unit = {\n  println(\"ok\")\n}",
        "zlib.decompress",
    ),
    // `window` is distance 1 from `windows`, which returns the same
    // shape, so only the distance ranking separates them.
    (
        "fn window(xs: List[Int], size: Int) -> List[List[Int]] = [xs]\neffect fn main() -> Unit = {\n  println(\"ok\")\n}",
        "list.window",
    ),
];

#[test]
fn e015_names_the_closest_stdlib_fn() {
    let dir = std::env::temp_dir().join("almide-e015-best-match");
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    for (source, expected) in CASES {
        let output = check(&dir, source);
        let got = suggested_fn(&output).unwrap_or_else(|| {
            panic!("no E015 warning for:\n{source}\n--- output ---\n{output}")
        });
        assert_eq!(
            &got, expected,
            "E015 named the wrong stdlib fn for:\n{source}"
        );
    }
}

#[test]
fn e015_suggestion_is_stable_across_runs() {
    let dir = std::env::temp_dir().join("almide-e015-stable");
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    // The bug reproduced roughly one run in three, so a handful of
    // repeats is enough to catch a reintroduction without making the
    // test slow.
    for (source, _) in CASES {
        let first = check(&dir, source);
        for run in 1..6 {
            let again = check(&dir, source);
            assert_eq!(
                suggested_fn(&first),
                suggested_fn(&again),
                "E015 suggestion changed on run {run} for:\n{source}"
            );
        }
    }
}
