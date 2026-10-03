//! `[permissions].allow` refuses a name that is not a capability (#3247), and reads every TOML array spelling (#3253).
//!
//! The names were matched against `IO|Net|Env|Time|Rand|Fan` in two copies and
//! anything else became nothing: `allow = ["Log"]` was accepted in silence, and
//! `allow = ["Net", "Fil"]` refused a file read for `IO` without saying `Fil`
//! is not a category. `--profile critical --allow` refused unknown names all
//! along. Both now give the same message with a did-you-mean, the manifest one
//! on the manifest's own line. The diagnostics fixture
//! `tests/diagnostics/permissions-unknown-capability/` is the broken/fixed
//! pair; this file holds what a fixture cannot say: every command refuses,
//! and every valid name still passes.

use std::path::{Path, PathBuf};
use std::process::Command;

const READS_A_FILE: &str =
    "import fs\n\neffect fn main() -> Unit = {\n  let text = fs.read_text(\"notes.txt\")!\n  println(text)\n}\n";

/// A scratch project holding `allow = [<allow>]` and a file-reading main.
fn project(tag: &str, allow: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-issue3247-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("almide.toml"),
        format!("[package]\nname = \"permfx\"\nversion = \"0.1.0\"\n\n[permissions]\nallow = [{allow}]\n"),
    )
    .expect("write manifest");
    std::fs::write(dir.join("main.almd"), READS_A_FILE).expect("write program");
    dir
}

fn almide(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn almide");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
}

#[test]
fn an_unknown_name_is_refused_on_its_manifest_line_by_every_command() {
    let dir = project("unknown", "\"Net\", \"Fil\"");
    for args in [&["check", "main.almd"][..], &["check", "--effects", "main.almd"][..], &["build", "main.almd"][..], &["run", "main.almd"][..]] {
        let (ok, stderr) = almide(&dir, args);
        assert!(!ok, "`almide {}` accepted `Fil`:\n{stderr}", args.join(" "));
        assert!(
            stderr.contains("almide.toml:6: unknown capability `Fil` in [permissions].allow")
                && stderr.contains("grantable capabilities are IO, Net, Env, Time, Rand, Fan")
                && stderr.contains("hint: did you mean"),
            "`almide {}` should name the line, the name and the vocabulary:\n{stderr}",
            args.join(" ")
        );
        assert!(
            !stderr.contains("capability violation"),
            "`almide {}` judged the program under a manifest it should have refused:\n{stderr}",
            args.join(" ")
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The doc comment on `Project::permissions` used `"Log"` as its example, and
/// the effect-system spec listed a `Log` category; neither exists (#3249).
#[test]
fn log_is_not_a_capability() {
    let dir = project("log", "\"IO\", \"Log\"");
    let (ok, stderr) = almide(&dir, &["check", "main.almd"]);
    assert!(!ok, "`Log` was accepted:\n{stderr}");
    assert!(stderr.contains("unknown capability `Log` in [permissions].allow"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The negative control: every name of the vocabulary passes, and the
/// program is then judged against it as before.
#[test]
fn every_valid_name_still_passes() {
    let dir = project("valid", "\"IO\", \"Net\", \"Env\", \"Time\", \"Rand\", \"Fan\"");
    let (ok, stderr) = almide(&dir, &["check", "main.almd"]);
    assert!(ok, "the full vocabulary was refused:\n{stderr}");
    let _ = std::fs::remove_dir_all(&dir);

    // A valid manifest that does not grant IO still refuses the read — the
    // enforcement itself is unchanged.
    let dir = project("valid-narrow", "\"Net\"");
    let (ok, stderr) = almide(&dir, &["check", "main.almd"]);
    assert!(!ok && stderr.contains("IO is not in [permissions].allow"), "{stderr}");
    assert!(!stderr.contains("unknown capability"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);

    // `proc` (#2589) lists commands, not capabilities: the name gate leaves
    // it alone.
    let dir = project("valid-proc", "\"IO\"]\nproc = [\"git\", \"my-tool\"");
    let (ok, stderr) = almide(&dir, &["check", "main.almd"]);
    assert!(ok, "a proc list was judged as capability names:\n{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The critical-profile path gives the same message, its own vocabulary,
/// and the suggestion.
#[test]
fn critical_allow_suggests_the_nearest_capability() {
    let dir = std::env::temp_dir().join(format!("almide-issue3247-critical-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("clean.almd"), "fn main() -> Unit = println(\"hi\")\n").expect("write");
    let (ok, stderr) = almide(&dir, &["check", "clean.almd", "--profile", "critical", "--allow", "Rnd"]);
    assert!(!ok, "{stderr}");
    assert!(
        stderr.contains("unknown capability `Rnd` in --allow — grantable capabilities are IO, Net, Env, Time, Rand, Process")
            && stderr.contains("hint: did you mean `Rand`?"),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── #3253: every TOML array spelling reads as TOML says ──────────────────
//
// The manifest was read one line at a time, so `allow = [` with its names on
// the lines below read as `allow = []` — "every capability" — and none of the
// names was judged. It is the `toml` crate now.

/// A scratch project whose `[permissions]` table is `permissions` verbatim,
/// with a main that reads a file (`IO`) and runs `git` (`Env`, `proc`).
fn raw_project(tag: &str, permissions: &str, main: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-issue3253-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("almide.toml"),
        format!("[package]\nname = \"permfx\"\nversion = \"0.1.0\"\n\n[permissions]\n{permissions}\n"),
    )
    .expect("write manifest");
    std::fs::write(dir.join("main.almd"), main).expect("write program");
    dir
}

/// A multi-line grant, with a trailing comma and comments between the items,
/// is the grant it spells: `Net` only, so the file read is refused for `IO`.
#[test]
fn a_multi_line_allow_is_the_narrow_grant_it_spells() {
    let allow = "allow = [\n  # network only\n  \"Net\",  # the API client\n]";
    let dir = raw_project("narrow", allow, READS_A_FILE);
    let (ok, stderr) = almide(&dir, &["check", "main.almd"]);
    assert!(!ok, "a multi-line `Net` grant admitted a file read:\n{stderr}");
    assert!(stderr.contains("IO is not in [permissions].allow"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);

    // The same grant widened to IO on its own line admits it.
    let dir = raw_project("narrow-io", "allow = [\n  \"Net\",\n  \"IO\",\n]", READS_A_FILE);
    let (ok, stderr) = almide(&dir, &["check", "main.almd"]);
    assert!(ok, "a multi-line grant naming IO refused the read:\n{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// An unknown name inside a multi-line array is refused on its own line.
#[test]
fn an_unknown_name_in_a_multi_line_allow_is_refused_on_its_line() {
    // Lines: 6 `allow = [`, 7 `"IO",`, 8 `"Bogus",`.
    let dir = raw_project("unknown", "allow = [\n  \"IO\",\n  \"Bogus\",\n]", READS_A_FILE);
    for args in [&["check", "main.almd"][..], &["run", "main.almd"][..]] {
        let (ok, stderr) = almide(&dir, args);
        assert!(!ok, "`almide {}` accepted `Bogus`:\n{stderr}", args.join(" "));
        assert!(
            stderr.contains("almide.toml:8: unknown capability `Bogus` in [permissions].allow"),
            "`almide {}` should name line 8:\n{stderr}",
            args.join(" ")
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A multi-line `proc` list is the list it spells: `git` runs past the
/// static gate, `make` is refused by name.
#[test]
fn a_multi_line_proc_list_is_the_list_it_spells() {
    let permissions = "allow = [\"Env\"]\nproc = [\n  \"git\",  # version control\n  \"cargo\",\n]";
    let listed = "import process\n\neffect fn main() -> Unit = {\n  let out = process.exec(\"git\", [\"--version\"])!\n  println(out)\n}\n";
    let dir = raw_project("proc-ok", permissions, listed);
    let (ok, stderr) = almide(&dir, &["check", "main.almd"]);
    assert!(ok, "`git` is on the multi-line proc list:\n{stderr}");
    let _ = std::fs::remove_dir_all(&dir);

    let unlisted = listed.replace("\"git\"", "\"make\"");
    let dir = raw_project("proc-bad", permissions, &unlisted);
    let (ok, stderr) = almide(&dir, &["check", "main.almd"]);
    assert!(!ok, "`make` is not on the multi-line proc list:\n{stderr}");
    assert!(stderr.contains("`make` is not in [permissions] proc"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A manifest that is not TOML is refused where it breaks, on every
/// command, instead of reading as "no project" (no dependencies, no
/// permissions) through the readers that ignore a parse error.
#[test]
fn a_manifest_that_is_not_toml_is_refused_on_its_line() {
    let dir = raw_project("not-toml", "allow = [\n  \"IO\"\n  \"Net\",\n]", READS_A_FILE);
    let (ok, stderr) = almide(&dir, &["check", "main.almd"]);
    assert!(!ok, "a broken array was accepted:\n{stderr}");
    assert!(stderr.contains("almide.toml:8:") && stderr.contains("almide.toml is read as TOML"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}
