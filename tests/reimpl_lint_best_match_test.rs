//! E015 candidates must have an exact name and compatible signature, exclude
//! host/effect surfaces, and remain deterministic. Even a candidate with the
//! same name is advisory: the body has not been proven equivalent (#2113).

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

/// Exact matches remain visible; neighboring names cannot redirect the hint.
const CASES: &[(&str, &str)] = &[
    // `atan` is distance 1 from `tan`.
    (
        "fn atan(x: Float) -> Float = x\neffect fn main() -> Unit = {\n  println(float.to_string(atan(1.0)))\n}",
        "math.atan",
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

#[test]
fn e015_does_not_recommend_host_calls_or_near_names() {
    let dir = tempfile::tempdir().unwrap();
    for source in [
        "fn optional(s: String) -> Option[String] = if s == \"\" then none else some(s)",
        "fn option(s: String) -> Option[String] = if s == \"\" then none else some(s)",
        "fn get(s: String) -> Option[String] = some(s)",
        "fn trims(s: String) -> String = s",
        "fn decompress(data: Bytes) -> Result[Bytes, String] = ok(data)",
    ] {
        let output = check(dir.path(), source);
        assert!(!output.contains("E015"), "{source}\n{output}");
    }
}

#[test]
fn e015_states_its_evidence_without_a_replacement_edit() {
    let dir = tempfile::tempdir().unwrap();
    let output = check(dir.path(), CASES[0].0);
    assert!(output.contains("equivalent behaviour is not established"), "{output}");
    assert!(!output.contains("try:"), "{output}");
}

#[test]
fn ide_outline_recognizes_bundled_args_module() {
    let output = Command::new(almide()).args(["ide", "outline", "@stdlib/args", "--json"])
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let outline: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(outline["functions"].as_array().unwrap().iter().any(|f| f["name"] == "option"));
}
