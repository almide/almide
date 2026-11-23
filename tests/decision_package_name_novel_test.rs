//! manifest.package-name: `[package].name` is held to one grammar — ASCII
//! letters, digits and `_`, not starting with a digit — by the manifest
//! reader itself (`parse_toml`), so every route that reads the manifest
//! refuses a name the file-name routes (build, compile, install) would
//! refuse. Each name here is a fresh identifier with one non-plain spelling
//! put into it, so no list of bad names can have held it. The control is the
//! fresh identifier itself, which every route accepts.
use std::path::Path;
use std::process::{Command, Output};

#[path = "support/novel.rs"]
mod novel;

fn project(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir.join("src")).expect("mkdir src");
    std::fs::write(dir.join("almide.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n")).expect("toml");
    std::fs::write(dir.join("src/main.almd"), "effect fn main() -> Unit = println(\"hi\")\n\ntest \"t\" { assert(true) }\n").expect("main");
}

/// Every route, as the argument list after the subcommand.
const ROUTES: &[&[&str]] = &[
    &["check", "src/main.almd"],
    &["run", "src/main.almd"],
    &["run", "src/main.almd", "--target", "wasm"],
    &["build", "src/main.almd", "-o", "out.bin"],
    &["test", "src/main.almd"],
    &["compile"],
];

fn almide(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide")).args(args).current_dir(dir).output().expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("exit={:?}\nstdout:\n{}\nstderr:\n{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn novel_a_fresh_non_plain_package_name_is_refused_on_every_route() {
    let base = novel::ident("pkg");
    let spellings = [
        format!("../{base}"),
        format!("{base}/x"),
        format!("{base} x"),
        format!("{base}.x"),
        format!("9{base}"),
        format!("{base}\u{e9}"),
    ];
    for name in &spellings {
        for args in ROUTES {
            let t = tempfile::tempdir().expect("scratch");
            project(t.path(), name);
            let o = almide(t.path(), args);
            let txt = text(&o);
            assert!(!o.status.success(), "{name:?} {args:?} must be refused:\n{txt}");
            assert!(txt.contains("is not a plain name"), "{name:?} {args:?}: the refusal names the grammar:\n{txt}");
        }
    }
}

#[test]
fn novel_control_the_fresh_plain_name_is_accepted_on_every_route() {
    let name = novel::ident("pkg");
    for args in ROUTES {
        let t = tempfile::tempdir().expect("scratch");
        project(t.path(), &name);
        let o = almide(t.path(), args);
        assert!(o.status.success(), "{name} {args:?}:\n{}", text(&o));
    }
}
