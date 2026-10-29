//! `[trust] media` (ADR-0032 D7): a dependency that owns a script-capable
//! media type builds only when the application grants that package that
//! type; a grant for a type the package does not declare, and two owners of
//! one type, are refused too. Every refusal is E118.
//!
//! No network, as in `dependency_trust_test.rs`: each "host" is a directory
//! of bare repositories that git's `url.<base>.insteadOf` maps
//! `https://github.com/` onto, so the manifests name the real source URLs a
//! grant is keyed by.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Hosts {
    root: PathBuf,
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

/// A package whose `Mark` type is a `Body` served as `media`, owning `owns`.
fn body_pkg(media: &str, owns: &str) -> String {
    format!(
        "type Mark: Body = {{ text: String }}\n\n@media_owns(\"{owns}\")\nfn Mark.__self_media_type(self) -> String = \"{media}\"\n\n\
         fn Mark.__self_to_text(self) -> String = self.text\n\nfn mark(s: String) -> Mark = Mark {{ text: s }}\n"
    )
}

impl Hosts {
    fn new(tag: &str) -> Hosts {
        let root = std::env::temp_dir().join(format!("almide-media-grant-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let h = Hosts { root };
        h.repo("almide", "svg", "svg", &body_pkg("image/svg+xml", "image/svg+xml"), "");
        h.repo("almide", "svgtoo", "svgtoo", &body_pkg("image/svg+xml", "image/svg+xml"), "");
        h.repo("almide", "csvbody", "csvbody", &body_pkg("text/csv", "text/csv"), "");
        // A chart library that pulls in a dependency which quietly claims SVG.
        h.repo(
            "alice",
            "charts",
            "charts",
            "import sneaky\n\nfn label() -> String = \"chart\"\n",
            "\n[dependencies]\nsneaky = { git = \"https://github.com/bob/sneaky\", tag = \"v0.1.0\" }\n",
        );
        h.repo("bob", "sneaky", "sneaky", &body_pkg("image/svg+xml", "image/svg+xml"), "");
        h
    }

    fn repo(&self, owner: &str, repo: &str, pkg: &str, src: &str, extra: &str) {
        let work = self.root.join("work").join(owner).join(repo);
        write(work.join("almide.toml"), &format!("[package]\nname = \"{pkg}\"\nversion = \"0.1.0\"\n{extra}"));
        write(work.join("src/mod.almd"), src);
        git(&work, &["init", "-q"]);
        git(&work, &["add", "-A"]);
        git(&work, &["commit", "-qm", "init"]);
        git(&work, &["tag", "v0.1.0"]);
        let bare = self.root.join("hosts").join(owner);
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["clone", "-q", "--bare", work.to_str().unwrap(), repo]);
    }

    /// A project `name` with `manifest`, whose main prints the Content-Type
    /// of a response built from `pkg.mark("x")`.
    fn project(&self, name: &str, manifest: &str, pkg: &str) -> PathBuf {
        let dir = self.root.join(name);
        write(dir.join("almide.toml"), &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n{manifest}"));
        write(
            dir.join("src/main.almd"),
            &format!(
                "import http\nimport {pkg}\n\neffect fn main() -> Unit = println(http.header(http.response(200, {pkg}.mark(\"x\")), \"Content-Type\") ?? \"none\")\n"
            ),
        );
        dir
    }

    fn almide(&self, dir: &Path, args: &[&str]) -> Output {
        let base = format!("file://{}/", self.root.join("hosts").display());
        Command::new(env!("CARGO_BIN_EXE_almide"))
            .current_dir(dir)
            .env("HOME", self.root.join("home"))
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", format!("url.{base}.insteadOf"))
            .env("GIT_CONFIG_VALUE_0", "https://github.com/")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("ALMIDE_RUN_PROJECT_DIR", self.root.join(".run"))
            .args(args)
            .output()
            .expect("spawn almide")
    }
}

impl Drop for Hosts {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

const RUN: &[&str] = &["run", "src/main.almd", "--target", "wasm"];
const SVG_DEP: &str = "\n[dependencies]\nsvg = { git = \"https://github.com/almide/svg\", tag = \"v0.1.0\" }\n";
const SVG_GRANT: &str = "\n[trust]\nmedia = { \"almide/svg\" = [\"image/svg+xml\"] }\n";

#[test]
fn a_granted_owner_builds_and_serves_its_type() {
    let h = Hosts::new("granted");
    let dir = h.project("app", &format!("{SVG_DEP}{SVG_GRANT}"), "svg");
    let o = h.almide(&dir, RUN);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), "image/svg+xml\n");
    let o = h.almide(&dir, &["deps"]);
    let listing = stdout(&o);
    assert!(listing.contains("media owners:"), "{listing}");
    assert!(listing.contains("html (stdlib, granted implicitly): text/html, application/xhtml+xml"), "{listing}");
    assert!(listing.contains("svg (almide/svg): image/svg+xml (granted at"), "{listing}");
}

#[test]
fn the_grant_removed_refuses_the_build_naming_the_line_to_add() {
    let h = Hosts::new("ungranted");
    let dir = h.project("app", SVG_DEP, "svg");
    for args in [RUN, &["build", "src/main.almd", "--target", "wasm", "-o", "out.wasm"], &["check", "src/main.almd"]] {
        let o = h.almide(&dir, args);
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(1), "`almide {}`:\n{err}", args.join(" "));
        for needle in [
            "error[E118]: dependency `svg` (almide/svg) owns `image/svg+xml`, which runs script in a browser",
            "does not grant it",
            "media = { \"almide/svg\" = [\"image/svg+xml\"] }",
        ] {
            assert!(err.contains(needle), "missing `{needle}`:\n{err}");
        }
    }
}

#[test]
fn a_forged_owner_in_a_transitive_dependency_is_refused() {
    let h = Hosts::new("forged");
    let manifest = "\n[dependencies]\ncharts = { git = \"https://github.com/alice/charts\", tag = \"v0.1.0\" }\n\n\
                    [trust]\nowners = [\"github.com/alice\", \"github.com/bob\"]\n";
    let dir = h.project("app", manifest, "charts");
    // The program never touches `sneaky`; owning SVG is refused anyway.
    write(dir.join("src/main.almd"), "import charts\n\neffect fn main() -> Unit = println(charts.label())\n");
    let o = h.almide(&dir, RUN);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("error[E118]: dependency `sneaky` (required by another dependency) (bob/sneaky) owns `image/svg+xml`"),
        "{err}"
    );
}

#[test]
fn two_granted_owners_of_one_type_are_refused() {
    let h = Hosts::new("duplicate");
    let manifest = "\n[dependencies]\nsvg = { git = \"https://github.com/almide/svg\", tag = \"v0.1.0\" }\n\
                    svgtoo = { git = \"https://github.com/almide/svgtoo\", tag = \"v0.1.0\" }\n\n\
                    [trust]\nmedia = { \"almide/svg\" = [\"image/svg+xml\"], \"almide/svgtoo\" = [\"image/svg+xml\"] }\n";
    let dir = h.project("app", manifest, "svg");
    let o = h.almide(&dir, RUN);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.contains("error[E118]: dependency `svgtoo` (almide/svgtoo) owns `image/svg+xml` and dependency `svg` (almide/svg) owns `image/svg+xml`"), "{err}");
}

#[test]
fn a_grant_for_a_type_the_package_does_not_declare_is_refused() {
    let h = Hosts::new("overgrant");
    let manifest = format!("{SVG_DEP}\n[trust]\nmedia = {{ \"almide/svg\" = [\"image/svg+xml\", \"text/html\"] }}\n");
    let dir = h.project("app", &manifest, "svg");
    let o = h.almide(&dir, RUN);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.contains("grants `almide/svg` the media type `text/html`, which it does not declare"), "{err}");
}

#[test]
fn an_owner_of_a_type_that_runs_no_script_needs_no_grant() {
    let h = Hosts::new("csv");
    let dir = h.project("app", "\n[dependencies]\ncsvbody = { git = \"https://github.com/almide/csvbody\", tag = \"v0.1.0\" }\n", "csvbody");
    let o = h.almide(&dir, RUN);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), "text/csv\n");
}
