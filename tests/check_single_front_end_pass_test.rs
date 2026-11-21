//! `almide check` runs the front end once (#4187). With user modules it used
//! to check the program, then parse, resolve and check every source a second
//! time from disk to lower the whole program for its gates (E109, the
//! project permissions). The whole-program IR is now lowered in the same
//! pass, so a program split into a module lexes exactly one more source than
//! the same program in one file — before, it lexed every source twice.

use std::process::Command;

fn sources_lexed(dir: &std::path::Path, entry: &str) -> u64 {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(dir)
        .args(["check", entry, "--timings"])
        .output()
        .expect("spawn almide");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let line = stderr.lines().find(|l| l.starts_with("almide-timings ")).unwrap_or_else(|| panic!("no timings line:\n{stderr}"));
    let at = line.find("\"sources\":").expect("sources field") + "\"sources\":".len();
    line[at..].trim_end_matches('}').trim().parse().expect("a count")
}

#[test]
fn a_module_split_lexes_each_source_once() {
    let dir = std::env::temp_dir().join(format!("almide_check_single_pass_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("helper.almd"), "fn twice(n: Int) -> Int = n * 2\n").unwrap();
    std::fs::write(dir.join("split.almd"), "import helper\n\neffect fn main() -> Unit = println(\"${helper.twice(21)}\")\n").unwrap();
    std::fs::write(dir.join("whole.almd"), "fn twice(n: Int) -> Int = n * 2\n\neffect fn main() -> Unit = println(\"${twice(21)}\")\n").unwrap();
    let split = sources_lexed(&dir, "split.almd");
    let whole = sources_lexed(&dir, "whole.almd");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(split, whole + 1, "the split program lexed {split} sources, the one-file program {whole}");
}
