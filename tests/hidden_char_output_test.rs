//! #4819: a hidden character (C-418's table: bidi controls, zero-width and
//! Tags-block characters, raw controls) never reaches compiler output raw.
//! A parse error quoting the character it stopped at, and a test name written
//! with a `\u{202E}` escape (so the file compiles), carried it raw into
//! `almide check` and `almide test` (human and `--json`).

use std::process::Command;

const RLO: char = '\u{202E}';

fn run(dir: &std::path::Path, args: &[&str]) -> String {
    let o = Command::new(env!("CARGO_BIN_EXE_almide")).current_dir(dir).args(args).output().expect("spawn almide");
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn a_parse_error_shows_the_character_it_stopped_at_escaped() {
    let dir = tempfile::tempdir().unwrap();
    for (k, src) in [format!("fn main() -> Unit = {{\n  let x = {RLO}1\n}}\n"), format!("fn main() -> Unit = {{\n  let {RLO} = 1\n}}\n")]
        .iter()
        .enumerate()
    {
        let file = format!("p{k}.almd");
        std::fs::write(dir.path().join(&file), src).unwrap();
        for args in [vec!["check", file.as_str()], vec!["check", file.as_str(), "--json"]] {
            let out = run(dir.path(), &args);
            assert!(out.contains("202E") || out.contains("202e"), "the character is not named:\n{out}");
            assert!(!out.contains(RLO), "`almide {}` printed a raw U+202E:\n{out}", args.join(" "));
        }
    }
}

#[test]
fn a_test_name_with_a_hidden_character_is_reported_escaped() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("t_test.almd"), "test \"ok\\u{202E}evil\" {\n  assert(false)\n}\n").unwrap();
    for args in [vec!["test", "t_test.almd"], vec!["test", "t_test.almd", "--json"]] {
        let out = run(dir.path(), &args);
        assert!(out.contains("evil"), "the failing test is not reported:\n{out}");
        assert!(!out.contains(RLO), "`almide {}` printed a raw U+202E:\n{out}", args.join(" "));
    }
}
