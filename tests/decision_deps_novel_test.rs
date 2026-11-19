//! ADR-0035 (a dependency is a typed identity): one novel-item test per
//! decision-site row whose decision moved from source TEXT to a typed value
//! (proofs/decision-sites.toml: trust.source-owner, trust.add-trust-flag,
//! trust.install-owner, deps.lock-parse, deps.ref-into-cache-path,
//! deps.git-invocation, deps.mvs-identity, deps.symlink-walk).
//!
//! Each test builds an owner, host, scheme, ref, lock commit, package name or
//! link name no list in the compiler can hold (tests/support/novel.rs) and
//! sees it refused. None of them reaches the network: every host is under
//! `.invalid`, the proxy variables point at a closed port, and every refusal
//! asserted here is made before anything is fetched — so a control that
//! removes a check turns the test red with a fetch failure, never a fetch.

#[path = "support/novel.rs"]
mod novel;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

fn text(o: &Output) -> String {
    format!("exit {:?}\n{}{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// `almide <args>` in `dir`, with the dependency cache under `home` and no
/// way out to the network.
fn almide(dir: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("ALL_PROXY", "http://127.0.0.1:9")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .env_remove("NO_PROXY")
        .env_remove("no_proxy")
        .output()
        .expect("spawn almide")
}

/// A fresh DNS host no grant can name: `<label>.invalid`.
fn fresh_host() -> String {
    format!("{}.invalid", novel::ident("host").replace('_', "-"))
}

/// A project in `dir` depending on `deps` (TOML lines), with `extra` TOML
/// after the dependencies, whose main imports `import` (when not empty).
fn app(dir: &Path, deps: &str, extra: &str, import: &str) -> PathBuf {
    write(dir.join("almide.toml"), &format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n{deps}\n{extra}"));
    let main = match import {
        "" => "effect fn main() -> Unit = println(\"app\")\n".to_string(),
        m => format!("import {m}\n\neffect fn main() -> Unit = println({m}.hello())\n"),
    };
    write(dir.join("src/main.almd"), &main);
    dir.to_path_buf()
}

fn home(td: &tempfile::TempDir) -> PathBuf {
    let h = td.path().join("home");
    std::fs::create_dir_all(&h).unwrap();
    h
}

/// Nothing was fetched: no package tree was written to the cache.
fn fetched_nothing(home: &Path) -> bool {
    !home.join(".almide/cache/trees").exists()
}

/// trust.source-owner: trust is decided on the typed source (host and a
/// whole-segment path prefix), so an owner no grant names — on a host no
/// list holds — is refused before any fetch, a grant covers whole segments
/// only, and a string that is not a source (the old "local" class, #4810)
/// is no source at all.
#[test]
fn novel_trust_a_fresh_owner_on_a_fresh_host_is_refused_before_any_fetch() {
    let td = tempfile::TempDir::new().unwrap();
    let home = home(&td);
    let host = fresh_host();
    let (owner, repo, dep) = (novel::ident("owner"), novel::ident("repo"), novel::ident("dep"));

    // A fresh owner on a fresh host, no grant.
    let proj = app(&td.path().join("plain"), &format!("{dep} = {{ git = \"https://{host}/{owner}/{repo}\", tag = \"v0.1.0\" }}\n"), "", &dep);
    for args in [&["check", "src/main.almd"][..], &["run", "src/main.almd"]] {
        let o = almide(&proj, &home, args);
        let t = text(&o);
        assert!(!o.status.success(), "`almide {}` admitted an untrusted fresh owner:\n{t}", args.join(" "));
        assert!(
            t.contains("is fetched from an owner this project does not trust") && t.contains(&format!("owner: {host}/{owner}")),
            "`almide {}`:\n{t}",
            args.join(" ")
        );
    }

    // A grant for `<host>/<owner>` does not cover `<host>/<owner>x`: the
    // prefix is whole segments, not text.
    let proj = app(
        &td.path().join("prefix"),
        &format!("{dep} = {{ git = \"https://{host}/{owner}x/{repo}\", tag = \"v0.1.0\" }}\n"),
        &format!("[trust]\nowners = [\"{host}/{owner}\"]\n"),
        &dep,
    );
    let o = almide(&proj, &home, &["check", "src/main.almd"]);
    let t = text(&o);
    assert!(!o.status.success() && t.contains(&format!("owner: {host}/{owner}x")) && t.contains("does not trust"), "a text prefix was read as a grant:\n{t}");

    // Not a URL: the pre-ADR reader classed it local and skipped trust.
    let proj = app(&td.path().join("local"), &format!("{dep} = {{ git = \"{owner}/{repo}\", tag = \"v0.1.0\" }}\n"), "", &dep);
    let o = almide(&proj, &home, &["check", "src/main.almd"]);
    let t = text(&o);
    assert!(!o.status.success() && t.contains("it is not a URL"), "a non-URL source was admitted:\n{t}");

    assert!(fetched_nothing(&home), "a refused source was fetched");
}

/// trust.add-trust-flag: `almide add` refuses a fresh owner unless `--trust`
/// names exactly that owner, before anything is fetched or written.
#[test]
fn novel_add_a_fresh_owner_is_refused_and_a_trust_flag_must_name_it() {
    let td = tempfile::TempDir::new().unwrap();
    let home = home(&td);
    let host = fresh_host();
    let (owner, other, repo, dep) = (novel::ident("owner"), novel::ident("other"), novel::ident("repo"), novel::ident("dep"));
    let proj = app(&td.path().join("app"), "", "", "");
    let before = std::fs::read(proj.join("almide.toml")).unwrap();
    let git = format!("https://{host}/{owner}/{repo}");

    let o = almide(&proj, &home, &["add", &dep, "--git", &git, "--tag", "v0.1.0"]);
    let t = text(&o);
    assert!(!o.status.success(), "an untrusted fresh owner was added:\n{t}");
    assert!(t.contains(&format!("refusing to add `{dep}`")) && t.contains(&format!("owner: {host}/{owner}")), "{t}");

    let wrong = format!("{host}/{other}");
    let o = almide(&proj, &home, &["add", &dep, "--git", &git, "--tag", "v0.1.0", "--trust", &wrong]);
    let t = text(&o);
    assert!(!o.status.success() && t.contains("does not match the owner"), "--trust naming another fresh owner was accepted:\n{t}");

    assert_eq!(std::fs::read(proj.join("almide.toml")).unwrap(), before, "almide.toml was written");
    assert!(!proj.join("almide.lock").exists(), "almide.lock was written");
    assert!(fetched_nothing(&home), "a refused source was fetched");
}

/// trust.install-owner: `almide tool install` of a fresh remote owner —
/// as a URL or as a `host/owner/repo` spec, both read into one typed source
/// — is refused before any fetch, and `--trust` must name that owner.
#[test]
fn novel_install_a_fresh_owner_is_refused_before_any_fetch() {
    let td = tempfile::TempDir::new().unwrap();
    let home = home(&td);
    let host = fresh_host();
    let (owner, other, repo) = (novel::ident("owner"), novel::ident("other"), novel::ident("repo"));
    let bin = td.path().join("bin");
    let bin = bin.to_str().unwrap();
    for spec in [format!("https://{host}/{owner}/{repo}"), format!("{host}/{owner}/{repo}")] {
        let o = almide(td.path(), &home, &["tool", "install", &spec, "--bin-dir", bin]);
        let t = text(&o);
        assert_eq!(o.status.code(), Some(1), "`{spec}` was installed:\n{t}");
        assert!(t.contains("refusing to install") && t.contains("its owner is not trusted") && t.contains(&format!("owner: {host}/{owner}")), "{t}");
    }
    let wrong = format!("{host}/{other}");
    let spec = format!("https://{host}/{owner}/{repo}");
    let o = almide(td.path(), &home, &["tool", "install", &spec, "--bin-dir", bin, "--trust", &wrong]);
    let t = text(&o);
    assert!(!o.status.success() && t.contains("does not match the owner"), "--trust naming another fresh owner was accepted:\n{t}");
    assert!(fetched_nothing(&home), "a refused spec was fetched");
    assert!(!td.path().join("bin").join(&repo).exists(), "something was installed");
}

/// deps.lock-parse: a lock entry's commit is a typed 40/64 hex id and its
/// source a typed source, so a fresh commit string (a name, an option, a
/// path) or a fresh scheme in the lock is refused when the lock is read.
#[test]
fn novel_lock_a_fresh_commit_or_source_is_refused_at_read_time() {
    let td = tempfile::TempDir::new().unwrap();
    let home = home(&td);
    let host = fresh_host();
    let (owner, repo, dep) = (novel::ident("owner"), novel::ident("repo"), novel::ident("dep"));
    let url = format!("https://{host}/{owner}/{repo}");
    let proj = app(
        &td.path().join("app"),
        &format!("{dep} = {{ git = \"{url}\", tag = \"v0.1.0\" }}\n"),
        &format!("[trust]\nowners = [\"{host}/{owner}\"]\n"),
        &dep,
    );
    for commit in [novel::ident("commit"), format!("-{}", novel::ident("opt")), format!("../{}", novel::path_segment())] {
        write(proj.join("almide.lock"), &format!("{dep} = {{ git = \"{url}\", ref = \"v0.1.0\", commit = \"{commit}\" }}\n"));
        let o = almide(&proj, &home, &["check", "src/main.almd"]);
        let t = text(&o);
        assert!(!o.status.success(), "lock commit `{commit}` was admitted:\n{t}");
        assert!(t.contains("is not a hex commit hash") && t.contains(&commit), "lock commit `{commit}`:\n{t}");
    }
    let scheme = novel::ident("scheme");
    write(
        proj.join("almide.lock"),
        &format!("{dep} = {{ git = \"{scheme}://{host}/{owner}/{repo}\", ref = \"v0.1.0\", commit = \"{}\" }}\n", "a".repeat(40)),
    );
    let o = almide(&proj, &home, &["check", "src/main.almd"]);
    let t = text(&o);
    assert!(!o.status.success() && t.contains(&format!("lock entry '{dep}'")) && t.contains("is refused"), "a lock source with a fresh scheme was admitted:\n{t}");
    assert!(fetched_nothing(&home), "a refused lock entry was fetched");
}

/// deps.ref-into-cache-path: a `tag` / `branch` is a typed ref name, so a
/// fresh path-shaped or option-shaped value is refused on its manifest line
/// (it used to be joined into the cache path, #4838).
#[test]
fn novel_ref_a_fresh_path_shaped_tag_or_branch_is_refused_on_its_line() {
    let td = tempfile::TempDir::new().unwrap();
    let home = home(&td);
    let host = fresh_host();
    let (owner, repo, dep) = (novel::ident("owner"), novel::ident("repo"), novel::ident("dep"));
    let (a, b) = (novel::path_segment(), novel::path_segment());
    let cases = [
        ("tag", format!("/{a}/{b}")),
        ("branch", format!("../{a}")),
        ("tag", format!("{a}/../{b}")),
        ("branch", format!("-{a}")),
    ];
    for (k, (key, value)) in cases.iter().enumerate() {
        let proj = app(
            &td.path().join(format!("app{k}")),
            &format!("{dep} = {{ git = \"https://{host}/{owner}/{repo}\", {key} = \"{value}\" }}\n"),
            &format!("[trust]\nowners = [\"{host}/{owner}\"]\n"),
            &dep,
        );
        let o = almide(&proj, &home, &["check", "src/main.almd"]);
        let t = text(&o);
        assert!(!o.status.success(), "{key} = {value} was admitted:\n{t}");
        assert!(t.contains(&format!("`{key} = ")) && t.contains("is not a ref name") && t.contains(value.as_str()), "{key} = {value}:\n{t}");
    }
    assert!(fetched_nothing(&home), "a refused ref was fetched");
}

/// deps.git-invocation: the fetch contacts only a typed https / ssh source,
/// so a fresh scheme, a fresh transport helper and option-shaped hosts,
/// users and path segments with fresh names never reach a fetch.
#[test]
fn novel_source_a_fresh_scheme_helper_or_option_shape_never_reaches_a_fetch() {
    let td = tempfile::TempDir::new().unwrap();
    let home = home(&td);
    let host = fresh_host();
    let (owner, repo, dep) = (novel::ident("owner"), novel::ident("repo"), novel::ident("dep"));
    let (scheme, helper, opt, user) = (novel::ident("scheme"), novel::ident("helper"), novel::ident("opt"), novel::ident("user"));
    let cases = [
        (format!("{scheme}://{host}/{owner}/{repo}"), "only https:// and ssh:// are"),
        (format!("{helper}::{host}/{owner}/{repo}"), "is a git transport helper"),
        (format!("--{opt}=x"), "begins with `-`"),
        (format!("ssh://-{opt}/{owner}/{repo}"), "begins with `-`"),
        (format!("{user}@-{opt}:{owner}/{repo}"), "begins with `-`"),
        (format!("https://{host}/-{opt}/{repo}"), "begins with `-`"),
    ];
    for (k, (src, why)) in cases.iter().enumerate() {
        let proj = app(
            &td.path().join(format!("app{k}")),
            &format!("{dep} = {{ git = \"{src}\", tag = \"v0.1.0\" }}\n"),
            &format!("[trust]\nowners = [\"{host}/{owner}\"]\n"),
            &dep,
        );
        let o = almide(&proj, &home, &["check", "src/main.almd"]);
        let t = text(&o);
        assert!(!o.status.success(), "source `{src}` was admitted:\n{t}");
        assert!(t.contains(why) && t.contains("is refused"), "source `{src}`: expected `{why}`:\n{t}");
    }
    assert!(fetched_nothing(&home), "a refused source was fetched");
}

/// deps.mvs-identity: a package is its origin (#4834). Two origins that
/// declare one fresh package name are refused, naming both; neither is
/// selected over the other.
#[test]
fn novel_two_origins_declaring_one_fresh_name_are_refused() {
    let td = tempfile::TempDir::new().unwrap();
    let home = home(&td);
    let root = td.path();
    let (name, util, fork) = (novel::ident("pkg"), novel::ident("util"), novel::ident("fork"));
    let package = |dir: &Path, pkg: &str, version: &str, body: &str, deps: &str| {
        write(dir.join("almide.toml"), &format!("[package]\nname = \"{pkg}\"\nversion = \"{version}\"\n\n[dependencies]\n{deps}"));
        write(dir.join("src/mod.almd"), body);
    };
    package(&root.join("a"), &name, "1.0.0", "fn hello() -> String = \"from-a\"\n", "");
    package(&root.join("b"), &name, "1.5.0", "fn hello() -> String = \"from-b\"\n", "");
    package(
        &root.join("u"),
        &util,
        "0.1.0",
        "fn u() -> Int = 1\n",
        &format!("{fork} = {{ path = \"{}\", version = \"1.5.0\" }}\n", root.join("b").display()),
    );
    let proj = app(
        &root.join("app"),
        &format!(
            "{name} = {{ path = \"{}\", version = \"1.0.0\" }}\n{util} = {{ path = \"{}\" }}\n",
            root.join("a").display(),
            root.join("u").display()
        ),
        "",
        &name,
    );
    let o = almide(&proj, &home, &["check", "src/main.almd"]);
    let t = text(&o);
    assert!(!o.status.success(), "two origins of the fresh `{name}` were unified:\n{t}");
    assert!(t.contains(&format!("two sources declare the package `{name}`")), "{t}");
    for d in ["a", "b"] {
        let canon = std::fs::canonicalize(root.join(d)).unwrap();
        assert!(t.contains(&canon.display().to_string()), "the refusal must name {d}:\n{t}");
    }
}

/// deps.symlink-walk: a link with a fresh name under a dependency's `src/`
/// is never followed — neither by the sub-namespace walk (the broken module
/// behind it is never parsed) nor by an import naming it.
#[cfg(unix)]
#[test]
fn novel_a_fresh_link_under_a_dependency_src_is_never_followed() {
    let td = tempfile::TempDir::new().unwrap();
    let home = home(&td);
    let root = td.path();
    let (dep, link, module) = (novel::ident("dep"), novel::ident("link"), novel::module());
    let outside = root.join("outside");
    write(outside.join(format!("{module}.almd")), "fn broken( = \n");
    let proj = root.join("app");
    write(proj.join("dep/almide.toml"), &format!("[package]\nname = \"{dep}\"\nversion = \"0.1.0\"\n"));
    write(proj.join("dep/src/mod.almd"), "fn hello() -> String = \"hi\"\n");
    std::os::unix::fs::symlink(&outside, proj.join("dep/src").join(&link)).expect("symlink");
    app(&proj, &format!("{dep} = {{ path = \"dep\" }}\n"), "", &dep);

    let o = almide(&proj, &home, &["check", "src/main.almd"]);
    let t = text(&o);
    assert!(o.status.success(), "a link under the dependency's src/ broke the check:\n{t}");
    assert!(!t.contains(&module), "the module behind the link was loaded:\n{t}");

    write(proj.join("src/main.almd"), &format!("import {dep}.{link}.{module}\n\neffect fn main() -> Unit = println(\"x\")\n"));
    let o = almide(&proj, &home, &["check", "src/main.almd"]);
    let t = text(&o);
    assert!(!o.status.success() && t.contains(&format!("sub-module '{dep}.{link}.{module}' not found")), "an import through the link was followed:\n{t}");
}
