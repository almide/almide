//! #4531: the repo's own `.almd` read alike under the pre-16 and the
//! dialect-16 operator grouping.
//!
//! Dialect 16 moved `??` from a postfix to an infix level below the
//! arithmetic, the ranges and `|>`. Every tracked `.almd` file was migrated
//! with `almide fix`'s rewrite (`almide::regroup`), so every `??` in it reads
//! the same under both grammars; this test holds the corpus there. A file the
//! current grammar parses with a `??` the two grammars read apart is a site
//! E120 would refuse, and a fixture that would silently mean something else
//! under the new grammar.
//!
//! `ALMIDE_MIGRATE_PRECEDENCE_WRITE=1` applies the migration to every such
//! file instead of failing — how the corpus was migrated.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every git-tracked `.almd` file.
fn tracked_almd() -> Vec<PathBuf> {
    let out = Command::new("git")
        .args(["ls-files", "-z", "--", "*.almd"])
        .current_dir(repo_root())
        .output()
        .expect("git ls-files runs");
    assert!(out.status.success(), "git ls-files failed");
    let files: Vec<PathBuf> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| repo_root().join(String::from_utf8_lossy(s).as_ref()))
        .collect();
    // A vacuous corpus would pass anything (#976).
    assert!(files.len() > 1000, "only {} tracked .almd files found", files.len());
    files
}

fn regrouped_sites(source: &str) -> Option<usize> {
    let mut parser = almide::parser::Parser::new(almide::lexer::Lexer::tokenize(source));
    let program = parser.parse().ok()?;
    if !parser.errors.is_empty() {
        return None;
    }
    Some(program.regrouped.len())
}

fn rel(p: &Path) -> String {
    p.strip_prefix(repo_root()).unwrap_or(p).display().to_string()
}

#[test]
fn every_tracked_almd_reads_alike_under_both_groupings() {
    let write = std::env::var("ALMIDE_MIGRATE_PRECEDENCE_WRITE").is_ok_and(|v| v == "1");
    let mut moved: Vec<String> = Vec::new();
    let mut rewritten_files = 0usize;
    let mut rewritten_sites = 0usize;
    let mut refused: Vec<String> = Vec::new();
    for file in tracked_almd() {
        let Ok(source) = std::fs::read_to_string(&file) else { continue };
        let Some(n) = regrouped_sites(&source) else { continue };
        if n == 0 {
            continue;
        }
        if !write {
            moved.push(format!("{} ({n} site(s))", rel(&file)));
            continue;
        }
        match almide::regroup::migrate_precedence(&source) {
            almide::regroup::Migration::Rewritten { source: out, sites, .. } => {
                std::fs::write(&file, out).expect("write the migrated file");
                rewritten_files += 1;
                rewritten_sites += sites;
                println!("migrated {} ({sites} site(s))", rel(&file));
            }
            almide::regroup::Migration::Refused { reason, sites } => {
                refused.push(format!("{} ({sites} site(s)): {reason}", rel(&file)));
            }
            almide::regroup::Migration::Unchanged => {}
        }
    }
    if write {
        println!("migrated {rewritten_sites} site(s) in {rewritten_files} file(s)");
        assert!(refused.is_empty(), "the migration refused:\n{}", refused.join("\n"));
        return;
    }
    assert!(
        moved.is_empty(),
        "{} file(s) hold a `??` that reads differently since dialect 16 (E120) — run `almide fix` on them:\n{}",
        moved.len(),
        moved.join("\n")
    );
}

/// The old-meaning guard (#4531): a program written for the pre-16 grouping
/// does not compile silently under the new one. `x ?? 0 - 1` meant
/// `(x ?? 0) - 1`: `3` for `some(4)`, `-1` for `none`. Read as `x ?? (0 - 1)`
/// it type-checks too and answers `4` for `some(4)`. The checker refuses it
/// with E120; the migrated text checks clean and prints the old answers on
/// both targets.
#[test]
fn an_old_reading_is_refused_until_its_parentheses_say_which() {
    let old = "fn dec(x: Option[Int]) -> Int = x ?? 0 - 1\n\neffect fn main() -> Unit = {\n  println(int.to_string(dec(some(4))))\n  println(int.to_string(dec(none)))\n}\n";
    let (ok, codes) = check(old, "old");
    assert!(!ok && codes.iter().any(|c| c == "E120"), "`x ?? 0 - 1` checked without E120: {codes:?}");

    let almide::regroup::Migration::Rewritten { source: migrated, sites, .. } = almide::regroup::migrate_precedence(old)
    else {
        panic!("the migration did not rewrite `x ?? 0 - 1`");
    };
    assert_eq!(sites, 1);
    assert!(migrated.contains("(x ?? 0) - 1"), "{migrated}");
    let (ok, codes) = check(&migrated, "migrated");
    assert!(ok, "the migrated program does not check clean: {codes:?}");
    assert_eq!(run(&migrated, "migrated", false), "3\n-1", "native");
    assert_eq!(run(&migrated, "migrated", true), "3\n-1", "wasm");

    // Written for the new grammar (stamped), the same text is the new reading.
    let stamped = format!("@dialect(16)\n\n{old}");
    let (ok, codes) = check(&stamped, "stamped");
    assert!(ok, "a dialect-16 file must take the new reading without E120: {codes:?}");
    assert_eq!(run(&stamped, "stamped", false), "4\n-1", "native, new reading");
}

fn probe(source: &str, name: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("almide_regroup_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("prog.almd");
    std::fs::write(&file, source).expect("write probe");
    (dir, file)
}

/// `almide check`: whether it passed, and the error codes it printed.
fn check(source: &str, name: &str) -> (bool, Vec<String>) {
    let (dir, file) = probe(source, &format!("check_{name}"));
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(&dir)
        .args(["check", file.to_str().unwrap()])
        .output()
        .expect("almide check runs");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let codes = text
        .match_indices("error[")
        .filter_map(|(i, _)| text[i + 6..].split(']').next().map(str::to_string))
        .collect();
    (out.status.success(), codes)
}

/// `almide run` on one target; its trimmed stdout.
fn run(source: &str, name: &str, wasm: bool) -> String {
    let leg = if wasm { "wasm" } else { "native" };
    let (dir, file) = probe(source, &format!("run_{name}_{leg}"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.current_dir(&dir).args(["run", file.to_str().unwrap()]);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let out = cmd.output().expect("almide run");
    assert!(out.status.success(), "{name}: {leg} run failed:\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
