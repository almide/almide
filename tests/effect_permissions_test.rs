//! `[permissions].allow` sees the categories the stdlib actually touches
//! (#3246) and treats an `@extern` call as every category (#3245).
//!
//! Before: `random` was never `Rand`, `io` was nothing, `path` / `url` were
//! `IO` / `Net`, and a plain-fn `@extern` counted as pure — so a project that
//! denied `Rand` used randomness freely, and a foreign call reached the host
//! with no category at all.
use std::path::Path;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn project(dir: &Path, allow: &[&str], main: &str) {
    let allow = allow.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(", ");
    let toml = format!("[package]\nname = \"perm\"\nversion = \"0.1.0\"\n\n[permissions]\nallow = [{allow}]\n");
    std::fs::write(dir.join("almide.toml"), toml).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.almd"), main).unwrap();
}

/// `almide check` from the project root: `(passed, stderr+stdout)`.
fn check(allow: &[&str], main: &str, extra: &[&str]) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), allow, main);
    let out = Command::new(almide())
        .current_dir(dir.path())
        .arg("check")
        .arg("src/main.almd")
        .args(extra)
        .output()
        .expect("run almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

const RANDOM: &str = "import random\n\neffect fn main() -> Unit = {\n  let n = random.int(1, 6)!\n  println(\"${n}\")\n}\n";
const STDIN: &str = "import io\n\neffect fn main() -> Unit = println(io.read_line())\n";
const PATH_URL: &str = "import path\nimport url\n\nfn main() -> Unit = {\n  println(path.join(\"a\", \"b\"))\n  println(url.encode_component(\"a b\"))\n}\n";
const EXTERN: &str = "@extern(rust, \"host\", \"push\")\nfn push(x: Int) -> Unit\n\nfn main() -> Unit = push(1)\n";
const CALENDAR: &str = "fn main() -> Unit = println(datetime.to_iso(0))\n";

#[test]
fn random_is_rand() {
    let (ok, text) = check(&["IO"], RANDOM, &[]);
    assert!(!ok && text.contains("Rand is not in [permissions].allow"), "{text}");
    let (ok, text) = check(&["Rand"], RANDOM, &[]);
    assert!(ok, "{text}");
}

#[test]
fn the_io_module_is_io() {
    let (ok, text) = check(&["Net"], STDIN, &[]);
    assert!(!ok && text.contains("IO is not in [permissions].allow"), "{text}");
    let (ok, text) = check(&["IO"], STDIN, &[]);
    assert!(ok, "{text}");
}

#[test]
fn path_and_url_are_pure() {
    let (ok, text) = check(&["Rand"], PATH_URL, &[]);
    assert!(ok, "{text}");
}

#[test]
fn datetime_calendar_math_is_not_the_clock() {
    let (ok, text) = check(&["Rand"], CALENDAR, &[]);
    assert!(ok, "{text}");
}

/// ⊤ is every category: allowing five of six is still a violation, and the
/// missing one is named.
#[test]
fn a_plain_fn_extern_is_every_category() {
    let five = ["IO", "Net", "Env", "Time", "Rand"];
    let (ok, text) = check(&five, EXTERN, &[]);
    assert!(!ok && text.contains("Fan is not in [permissions].allow"), "{text}");
    let (ok, text) = check(&["IO", "Net", "Env", "Time", "Rand", "Fan"], EXTERN, &[]);
    assert!(ok, "{text}");
}

#[test]
fn check_effects_reports_the_extern_and_its_caller() {
    let (_, text) = check(&["IO", "Net", "Env", "Time", "Rand", "Fan"], EXTERN, &["--effects"]);
    assert!(text.contains("push  → {IO, Net, Env, Time, Rand, Fan}"), "{text}");
    assert!(text.contains("main  → {IO, Net, Env, Time, Rand, Fan}"), "{text}");
}
