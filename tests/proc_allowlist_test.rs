//! `[permissions] proc` (#2589, ADR-0025): the commands `process.*` may
//! start. The compiler checks it statically — a spawning call whose command
//! is a string literal outside the list, or not a literal at all, is refused
//! on `check`, `run` and `build`, native and wasm alike — and the embedded
//! host carries the same list as its run-time bound (unit-tested in
//! crates/almide-wasm-run/src/tests/host_process_test.rs). Without the key
//! nothing changes.

use std::path::Path;
use std::process::Command;

fn project(dir: &Path, proc_line: &str, main: &str) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("almide.toml"),
        format!("[package]\nname = \"proct\"\nversion = \"0.1.0\"\n\n[permissions]\n{proc_line}\n"),
    )
    .unwrap();
    std::fs::write(dir.join("src/main.almd"), format!("import process\n\neffect fn main() -> Unit = {{\n{main}\n}}\n")).unwrap();
}

fn almide(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_almide")).args(args).current_dir(dir).output().expect("almide runs")
}

fn text(o: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn a_command_outside_the_list_is_refused_by_name_on_every_route() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), "proc = [\"sh\"]", "  let out = process.exec(\"cargo\", [\"--version\"])!\n  println(out)");
    for args in [
        &["check", "src/main.almd"][..],
        &["run", "src/main.almd"],
        &["run", "src/main.almd", "--target", "wasm"],
        &["build", "src/main.almd", "--target", "wasm", "-o", "m.wasm"],
    ] {
        let o = almide(td.path(), args);
        assert!(!o.status.success(), "{args:?} must refuse:\n{}", text(&o));
        assert!(
            text(&o).contains("process.exec(\"cargo\") (line 4): `cargo` is not in [permissions] proc"),
            "{args:?}:\n{}",
            text(&o)
        );
    }
}

#[test]
fn a_command_the_list_cannot_check_is_refused_too() {
    let td = tempfile::TempDir::new().unwrap();
    project(
        td.path(),
        "proc = [\"sh\"]",
        "  let cmd = string.trim(\" sh \")\n  let out = process.exec(cmd, [\"-c\", \"printf x\"])!\n  println(out)",
    );
    let o = almide(td.path(), &["check", "src/main.almd"]);
    assert!(!o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("the command is not a string literal, so [permissions] proc cannot check it"), "{}", text(&o));
    // A spawning fn passed as a value: its command is not visible at any call
    // site, so the build routes refuse it (they judge the linked IR, where the
    // value is the wrapper that calls it).
    project(td.path(), "proc = [\"sh\"]", "  let f = process.exec\n  let out = f(\"cargo\", [\"--version\"])!\n  println(out)");
    for args in [&["run", "src/main.almd"][..], &["run", "src/main.almd", "--target", "wasm"]] {
        let o = almide(td.path(), args);
        assert!(!o.status.success(), "{args:?}: {}", text(&o));
        assert!(text(&o).contains("the command is not a string literal, so [permissions] proc cannot check it"), "{args:?}: {}", text(&o));
    }
}

#[test]
fn a_listed_command_runs_on_both_targets_and_no_key_bounds_nothing() {
    let main = "  let out = process.exec(\"sh\", [\"-c\", \"printf listed\"])!\n  println(out)";
    for proc_line in ["proc = [\"sh\"]", "allow = [\"IO\", \"Env\"]"] {
        let td = tempfile::TempDir::new().unwrap();
        project(td.path(), proc_line, main);
        for extra in [&[][..], &["--target", "wasm"]] {
            let mut args = vec!["run", "src/main.almd"];
            args.extend_from_slice(extra);
            let o = almide(td.path(), &args);
            assert!(o.status.success(), "{proc_line} {args:?}:\n{}", text(&o));
            assert_eq!(String::from_utf8_lossy(&o.stdout), "listed\n", "{proc_line} {args:?}");
        }
    }
}
