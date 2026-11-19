//! A package is its origin, not the name it declares for itself (#4834).
//!
//! MVS unified dependencies by `PkgId { name, major }`, where `name` is the
//! fetched package's own `[package] name`. A transitive dependency from
//! another source that declared the same name at a higher version replaced
//! the root's direct dependency with no warning, and `[permissions] native`,
//! which grants by name, then applied to the replacement. Measured before the
//! change: the case below printed `json-b (from util)`.
//!
//! Now two origins (a typed git source plus its subdir, or a canonical local
//! directory) that declare one name are refused, naming both, and a native
//! grant names the origin the root declares under that name.

#[path = "support/git_host.rs"]
mod git_host;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4834-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home")).expect("mkdir");
    root
}

fn run(dir: &Path, root: &Path) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.current_dir(dir)
        .env("HOME", root.join("home"))
        .env("ALMIDE_RUN_PROJECT_DIR", root.join(".run"))
        .args(["run", "src/main.almd", "--target", "wasm"]);
    git_host::shared().env(&mut cmd).output().expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// `name`-declaring package in `dir`, optionally depending on `deps`.
fn package(dir: &Path, name: &str, version: &str, body: &str, deps: &str) {
    write(dir.join("almide.toml"), &format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n\n[dependencies]\n{deps}"));
    write(dir.join("src/mod.almd"), body);
}

/// The issue's minimal case, with path dependencies: the app asks for
/// `codec` from `jsona`; `util` asks for `jsonfork` from `jsonb`, which ALSO
/// declares itself `codec`, at a higher version.
#[test]
fn a_transitive_package_declaring_the_roots_name_from_another_path_is_refused() {
    let root = scratch("path");
    package(&root.join("jsona"), "codec", "1.0.0", "fn who() -> String = \"json-a\"\n", "");
    package(&root.join("jsonb"), "codec", "1.5.0", "fn who() -> String = \"json-b\"\n", "");
    package(
        &root.join("util"),
        "util",
        "0.1.0",
        "fn u() -> Int = 1\n",
        &format!("jsonfork = {{ path = \"{}\", version = \"1.5.0\" }}\n", root.join("jsonb").display()),
    );
    let app = root.join("app");
    package(
        &app,
        "app",
        "0.1.0",
        "",
        &format!(
            "codec = {{ path = \"{}\", version = \"1.0.0\" }}\nutil = {{ path = \"{}\" }}\n",
            root.join("jsona").display(),
            root.join("util").display()
        ),
    );
    write(app.join("src/main.almd"), "import codec\neffect fn main() -> Unit = println(codec.who())\n");
    let o = run(&app, &root);
    let t = text(&o);
    assert!(!o.status.success(), "two origins of `codec` were unified:\n{t}");
    assert!(!t.contains("json-b"), "the transitive package replaced the root's:\n{t}");
    assert!(t.contains("two sources declare the package `codec`"), "{t}");
    for d in ["jsona", "jsonb"] {
        let canon = std::fs::canonicalize(root.join(d)).unwrap();
        assert!(t.contains(&canon.display().to_string()), "the refusal must name {d}:\n{t}");
    }
    assert!(t.contains("app → util → jsonfork"), "the refusal must name the chain:\n{t}");
    let _ = std::fs::remove_dir_all(&root);
}

/// The same with git sources, and a native grant: the app grants `codec`
/// native code for the `codec` it names; a same-named package from another
/// repository that brings an `@extern` is refused before anything builds,
/// rather than inheriting the grant.
#[test]
fn a_same_named_git_package_from_another_source_is_refused_and_not_granted() {
    let root = scratch("git");
    let host = git_host::shared();
    let a = root.join("codec-a");
    let b = root.join("codec-b");
    package(&a, "codec", "1.0.0", "fn who() -> String = \"codec-a\"\n", "");
    package(&b, "codec", "1.5.0", "@extern(rust, \"std::process\", \"abort\")\nfn boom() -> Unit = _\n\nfn who() -> String = \"codec-b\"\n", "");
    let (url_a, url_b) = (host.url_of(&a), host.url_of(&b));
    for d in [&a, &b] {
        git_host::git(d, &["init", "-q"]);
        git_host::git(d, &["add", "-A"]);
        git_host::git(d, &["commit", "-qm", "c"]);
        git_host::git(d, &["tag", "v1.0.0"]);
    }
    let util = root.join("util");
    package(&util, "util", "0.1.0", "fn u() -> Int = 1\n", &format!("fork = {{ git = \"{url_b}\", tag = \"v1.0.0\" }}\n"));
    let app = root.join("app");
    write(
        app.join("almide.toml"),
        &format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ncodec = {{ git = \"{url_a}\", tag = \"v1.0.0\" }}\nutil = {{ path = \"{}\" }}\n\n[permissions]\nnative = [\"codec\"]\n",
            util.display()
        ),
    );
    write(app.join("src/main.almd"), "import codec\neffect fn main() -> Unit = println(codec.who())\n");
    let o = run(&app, &root);
    let t = text(&o);
    assert!(!o.status.success() && !String::from_utf8_lossy(&o.stdout).contains("codec-b"), "the other source's `codec` was used:\n{t}");
    assert!(t.contains("two sources declare the package `codec`") && t.contains(&url_a) && t.contains(&url_b), "{t}");
    // Control: with only the root's own `codec`, the same app builds.
    write(app.join("almide.toml"), &format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ncodec = {{ git = \"{url_a}\", tag = \"v1.0.0\" }}\n"));
    let o = run(&app, &root);
    assert!(o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "codec-a", "{}", text(&o));
    let _ = std::fs::remove_dir_all(&root);
}
