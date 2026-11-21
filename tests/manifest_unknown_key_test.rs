//! A key `almide.toml` writes outside its section's declaration is refused
//! (#3382 made it a warning; dialect epoch 17 makes it an error). It used to
//! be dropped, so `brnach = "main"` or `tags = "v0.1.0"` built from the
//! default ref instead of the one written, and `[permission]` left every
//! capability granted. The refusal names the line, the key, the keys that
//! section declares, and the key one edit away.
//!
//! The declared-key lists live in `src/project.rs` beside the readers and are
//! bound to their sections by `MANIFEST_SCHEMA`. The
//! `every_accepted_key_changes_the_parse` matrix pins them to the readers:
//! a listed key the reader does not read fails it, and a key the reader
//! reads without listing it trips the reader's `debug_assert!`.

use std::path::{Path, PathBuf};
use std::process::Command;

use almide::diagnostic::Diagnostic;
use almide::project::{
    check_manifest, manifest_warnings, parse_toml, SectionAt, DEPENDENCY_KEYS, MANIFEST_SCHEMA, PACKAGE_KEYS, PACKAGE_METADATA_KEYS, PERMISSION_KEYS,
    TARGET_PLATFORM_KEYS, TOP_LEVEL_KEYS, TRUST_KEYS,
};

#[path = "support/novel.rs"]
mod novel;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-issue3382-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn warnings(text: &str) -> Vec<Diagnostic> {
    manifest_warnings(Path::new("almide.toml"), text)
}

/// `(line, message, hint)` of each warning.
fn shown(text: &str) -> Vec<(usize, String, String)> {
    warnings(text).into_iter().map(|d| (d.line.expect("a line"), d.message, d.hint)).collect()
}

/// `(line, message, hint)` of each refusal `check_manifest` gives, in order.
fn refused(text: &str) -> Vec<(usize, String, String)> {
    let dir = scratch(&novel::ident("refused"));
    let path = dir.join("almide.toml");
    std::fs::write(&path, text).expect("write manifest");
    let err = check_manifest(&path, text).expect_err("the manifest was accepted");
    let _ = std::fs::remove_dir_all(&dir);
    let prefix = format!("{}:", path.display());
    err.split("\nerror: ")
        .map(|one| {
            let rest = one.strip_prefix(&prefix).unwrap_or_else(|| panic!("not located: {one}"));
            let (line, rest) = rest.split_once(": ").expect("line");
            let (message, hint) = rest.split_once("\n  hint: ").expect("hint");
            (line.parse().expect("line number"), message.to_string(), hint.to_string())
        })
        .collect()
}

fn parsed(tag: &str, text: &str) -> String {
    let dir = scratch(tag);
    let path = dir.join("almide.toml");
    std::fs::write(&path, text).expect("write manifest");
    let p = parse_toml(&path).unwrap_or_else(|e| panic!("{tag}: {e}"));
    let _ = std::fs::remove_dir_all(&dir);
    // `root` and each `declared_at` name the scratch directory, which differs
    // per tag; only what the keys say is compared.
    format!("{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}", p.package, p.dependencies, p.permissions, p.proc_allow, p.fs_allow, p.native_grants, p.trust_owners, p.media_grants, p.native_deps)
        .replace(&dir.display().to_string(), "<dir>")
}

#[test]
fn a_correct_manifest_is_accepted_without_a_warning() {
    let text = "[package]\nname = \"app\"\nversion = \"0.2.0\"\nalmide = \">=0.60\"\n\
                edition = \"2026\"\ndescription = \"what `almide init` and most manifests write\"\n\
                license = \"MIT\"\nrepository = \"https://example.com/app\"\n\n\
                [dependencies]\n\
                a = { git = \"https://example.com/a\", tag = \"v1.0.0\", version = \"1.0.0\" }\n\
                b = { git = \"https://example.com/b\", branch = \"main\", subdir = \"pkgs/b\" }\n\
                c = { path = \"../c\" }\n\
                d.git = \"https://example.com/d\"\n\n\
                [dependencies.e]\ngit = \"https://example.com/e\"\ntag = \"v0.1.0\"\n\n\
                [native-deps]\nanything-goes = \"1\"\n\n\
                [target.'cfg(unix)'.native-deps]\nlibc = \"0.2\"\n\n\
                [permissions]\nallow = [\"IO\"]\nproc = [\"git\"]\n";
    assert!(warnings(text).is_empty(), "{:#?}", shown(text));
    let dir = scratch("correct");
    let path = dir.join("almide.toml");
    std::fs::write(&path, text).expect("write manifest");
    let verdict = check_manifest(&path, text);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(verdict.is_ok(), "{verdict:?}");
}

#[test]
fn an_unknown_dependency_key_names_its_line_and_the_near_key() {
    let text = "[package]\nname = \"app\"\n\n[dependencies]\n\
                a = { git = \"https://example.com/a\", brnach = \"main\" }\n\
                b = { git = \"https://example.com/b\", tags = \"v0.1.0\" }\n\
                c = { git = \"https://example.com/c\",\n      rev = \"abc123\" }\n";
    let w = refused(text);
    assert_eq!(w.len(), 3, "{w:#?}");
    assert_eq!(w[0].0, 5);
    assert_eq!(w[0].1, "unknown key `brnach` in dependency `a` is refused — almide.toml accepts only the keys its sections declare");
    assert!(w[0].2.starts_with("did you mean `branch`?"), "{}", w[0].2);
    assert!(
        w[0].2.ends_with("`git`, `tag`, `branch`, `version`, `path`, `subdir`"),
        "the hint lists the declared keys: {}",
        w[0].2
    );
    assert_eq!(w[1].0, 6);
    assert!(w[1].2.starts_with("did you mean `tag`?"), "{}", w[1].2);
    assert_eq!(w[2].0, 8, "the line of the key, not of the entry");
    assert!(w[2].1.contains("`rev`"));
    assert!(w[2].2.starts_with("delete it, or write one of the keys"), "no key is one edit from `rev`: {}", w[2].2);
}

#[test]
fn dotted_and_table_dependency_spellings_are_refused_alike() {
    let text = "[package]\nname = \"app\"\n\n[dependencies]\n\
                a.git = \"https://example.com/a\"\n\
                a.tga = \"v1\"\n\n\
                [dependencies.b]\n\
                git = \"https://example.com/b\"\n\
                subdri = \"pkgs/b\"\n";
    let w = refused(text);
    assert_eq!(w.len(), 2, "{w:#?}");
    assert_eq!((w[0].0, w[0].2.starts_with("did you mean `tag`?")), (6, true), "{w:#?}");
    assert_eq!((w[1].0, w[1].2.starts_with("did you mean `subdir`?")), (10, true), "{w:#?}");
}

#[test]
fn a_dependency_that_declares_nothing_warns() {
    let text = "[package]\nname = \"app\"\n\n[dependencies]\nfoo = \"1.0\"\nbar = { tag = \"v1\" }\n";
    let w = shown(text);
    assert_eq!(w.len(), 2, "{w:#?}");
    assert_eq!(w[0].0, 5);
    assert!(w[0].1.starts_with("dependency `foo` is ignored"), "{}", w[0].1);
    assert_eq!(w[1].0, 6);
    assert!(w[1].1.starts_with("dependency `bar` is ignored — it names neither"), "{}", w[1].1);
}

#[test]
fn package_permissions_and_top_level_tables_are_refused_in_file_order() {
    let text = "[package]\nname = \"app\"\nverison = \"1.0.0\"\nauthors = [\"x\"]\n\n\
                [permissions]\nalow = [\"IO\"]\n\n\
                [dependecies]\na = { git = \"https://example.com/a\" }\n\n\
                [targets]\nwasm = true\n";
    let w = refused(text);
    assert_eq!(w.len(), 5, "{w:#?}");
    assert_eq!(w[0].0, 3);
    assert!(w[0].1.contains("in [package]") && w[0].2.starts_with("did you mean `version`?"), "{w:#?}");
    assert_eq!(w[1].0, 4);
    assert!(w[1].2.starts_with("delete it"), "{}", w[1].2);
    assert_eq!(w[2].0, 7);
    assert!(w[2].1.contains("in [permissions]") && w[2].2.starts_with("did you mean `allow`?"), "{w:#?}");
    assert_eq!(w[3].0, 9);
    assert_eq!(w[3].1, "unknown table `dependecies` in almide.toml is refused — almide.toml accepts only the tables its sections declare");
    assert!(w[3].2.starts_with("did you mean `dependencies`?"), "{}", w[3].2);
    assert_eq!(w[4].0, 12);
    assert!(w[4].2.starts_with("did you mean `target`?"), "{}", w[4].2);
}

/// Every section of the schema is judged: a fresh key (one no list can have
/// named) written in each declared section is refused, with that section's
/// keys in the hint. The control: the same manifest without the fresh key is
/// accepted.
#[test]
fn novel_a_fresh_key_in_every_declared_section_is_refused() {
    let pkg = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n";
    for section in MANIFEST_SCHEMA {
        let key = novel::ident("key");
        let (with, without) = match section.at {
            SectionAt::Root => (format!("{pkg}\n[{key}]\nx = 1\n"), pkg.to_string()),
            SectionAt::Table("package") => (format!("{pkg}{key} = \"x\"\n"), pkg.to_string()),
            SectionAt::Table("permissions") => (format!("{pkg}\n[permissions]\nallow = [\"IO\"]\n{key} = []\n"), format!("{pkg}\n[permissions]\nallow = [\"IO\"]\n")),
            SectionAt::Table("trust") => (format!("{pkg}\n[trust]\nowners = [\"github.com/alice\"]\n{key} = []\n"), format!("{pkg}\n[trust]\nowners = [\"github.com/alice\"]\n")),
            SectionAt::Table(other) => panic!("section [{other}] has no case here — add one"),
            SectionAt::DependencyEntry => (
                format!("{pkg}\n[dependencies]\nd = {{ path = \"../d\", {key} = \"x\" }}\n"),
                format!("{pkg}\n[dependencies]\nd = {{ path = \"../d\" }}\n"),
            ),
            SectionAt::TargetPlatform => (
                format!("{pkg}\n[target.'cfg(unix)'.{key}]\nlibc = \"0.2\"\n"),
                format!("{pkg}\n[target.'cfg(unix)'.native-deps]\nlibc = \"0.2\"\n"),
            ),
        };
        let w = refused(&with);
        assert_eq!(w.len(), 1, "{:?}: {w:#?}", section.at);
        assert!(w[0].1.contains(&format!("`{key}`")) && w[0].1.contains("is refused"), "{:?}: {w:#?}", section.at);
        assert!(w[0].2.contains(&format!("`{}`", section.keys[0])), "the hint lists the section's keys: {:?}: {w:#?}", section.at);
        let dir = scratch(&novel::ident("control"));
        let path = dir.join("almide.toml");
        std::fs::write(&path, &without).expect("write manifest");
        let verdict = check_manifest(&path, &without);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(verdict.is_ok(), "control {:?}: {verdict:?}", section.at);
    }
}

/// The same refusal on every command that reads the manifest: a fresh key in a
/// `[dependencies]` entry (not one of the security tables #4851 covered) stops
/// check, check --json, run, build, test, bench and emit before the program
/// runs; the control without it runs.
#[test]
fn novel_a_fresh_dependency_key_is_refused_on_every_route() {
    let key = novel::ident("depkey");
    let dir = scratch(&novel::ident("routes"));
    let dep = dir.join("d");
    std::fs::create_dir_all(dep.join("src")).expect("mkdir dep");
    std::fs::write(dep.join("almide.toml"), "[package]\nname = \"d\"\nversion = \"0.1.0\"\n").expect("write dep manifest");
    std::fs::write(dep.join("src/mod.almd"), "fn one() -> Int = 1\n").expect("write dep source");
    let app = dir.join("app");
    std::fs::create_dir_all(app.join("src")).expect("mkdir app");
    std::fs::write(app.join("src/main.almd"), "fn main() -> Unit = println(\"hi\")\n").expect("write source");
    std::fs::write(app.join("src/main_test.almd"), "test \"t\" {\n  assert(true)\n}\n").expect("write test");
    let manifest = |extra: &str| format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nd = {{ path = \"../d\"{extra} }}\n");
    let routes: [&[&str]; 7] = [
        &["check", "src/main.almd"],
        &["check", "--json", "src/main.almd"],
        &["run", "src/main.almd"],
        &["build", "src/main.almd", "-o", "out"],
        &["test", "src/main_test.almd"],
        &["bench", "src/main.almd"],
        &["emit", "src/main.almd", "--target", "rust"],
    ];
    std::fs::write(app.join("almide.toml"), manifest(&format!(", {key} = \"x\""))).expect("write manifest");
    for args in routes {
        let out = Command::new(almide_bin()).args(args).current_dir(&app).output().expect("spawn almide");
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(!out.status.success(), "`almide {}` accepted a fresh key:\n{text}", args.join(" "));
        assert!(text.contains(&format!("unknown key `{key}` in dependency `d` is refused")), "`almide {}`: {text}", args.join(" "));
        assert!(!text.contains("hi\n"), "`almide {}` ran the program:\n{text}", args.join(" "));
    }
    std::fs::write(app.join("almide.toml"), manifest("")).expect("write manifest");
    let out = Command::new(almide_bin()).args(["run", "src/main.almd"]).current_dir(&app).output().expect("spawn almide");
    assert!(out.status.success(), "control: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The drift guard: each listed key changes what `parse_toml` returns, so a
/// list that names a key the reader ignores fails here. A key the reader
/// reads without listing it panics in the reader's `debug_assert!` (this
/// test runs every reader).
#[test]
fn every_accepted_key_changes_the_parse() {
    let pkg = "[package]\nname = \"app\"\n";
    let base = parsed("base", pkg);
    for key in PACKAGE_KEYS {
        let text = match *key {
            "name" => "[package]\nname = \"other\"\n".to_string(),
            _ => format!("{pkg}{key} = \"9.9.9\"\n"),
        };
        assert_ne!(parsed(&format!("pkg-{key}"), &text), base, "[package].{key} is listed but changes nothing");
    }
    for key in PACKAGE_METADATA_KEYS {
        let text = format!("{pkg}{key} = \"x\"\n");
        assert_eq!(
            parsed(&format!("meta-{key}"), &text),
            base,
            "[package].{key} changes the parse now — move it to PACKAGE_KEYS"
        );
    }
    for key in PERMISSION_KEYS {
        let text = format!("{pkg}[permissions]\n{key} = [\"IO\"]\n");
        assert_ne!(parsed(&format!("perm-{key}"), &text), base, "[permissions].{key} is listed but changes nothing");
    }
    let git_dep = format!("{pkg}[dependencies]\nd = {{ git = \"https://example.com/d\" }}\n");
    let path_dep = format!("{pkg}[dependencies]\nd = {{ path = \"../d\" }}\n");
    for key in DEPENDENCY_KEYS {
        let (base_text, value) = match *key {
            "git" => (&path_dep, "https://example.com/d"),
            "path" => (&git_dep, "../d"),
            "subdir" => (&git_dep, "pkgs/d"),
            _ => (&git_dep, "v1"),
        };
        let with = base_text.replace(" }\n", &format!(", {key} = \"{value}\" }}\n"));
        assert_ne!(
            parsed(&format!("dep-{key}"), &with),
            parsed(&format!("dep-{key}-base"), base_text),
            "dependency key `{key}` is listed but changes nothing"
        );
    }
    for key in TRUST_KEYS {
        // `media` is a table from a package source to its media types
        // (ADR-0032 D7); `owners` a list of owners.
        let value = match *key {
            "media" => "{ \"github.com/almide/svg\" = [\"image/svg+xml\"] }",
            _ => "[\"github.com/alice\"]",
        };
        let text = format!("{pkg}[trust]\n{key} = {value}\n");
        assert_ne!(parsed(&format!("trust-{key}"), &text), base, "[trust].{key} is listed but changes nothing");
    }
    for key in TARGET_PLATFORM_KEYS {
        let text = format!("{pkg}[target.'cfg(unix)'.{key}]\nlibc = \"0.2\"\n");
        assert_ne!(parsed(&format!("target-{key}"), &text), base, "[target.*].{key} is listed but changes nothing");
    }
    for key in TOP_LEVEL_KEYS {
        let text = match *key {
            "package" => "[package]\nname = \"other\"\n".to_string(),
            "dependencies" => git_dep.clone(),
            "native-deps" => format!("{pkg}[native-deps]\nlibc = \"0.2\"\n"),
            "permissions" => format!("{pkg}[permissions]\nallow = [\"IO\"]\n"),
            "target" => format!("{pkg}[target.'cfg(unix)'.native-deps]\nlibc = \"0.2\"\n"),
            "trust" => format!("{pkg}[trust]\nowners = [\"github.com/alice\"]\n"),
            other => panic!("top-level table `{other}` has no case here — add one that sets it"),
        };
        assert_ne!(parsed(&format!("top-{key}"), &text), base, "[{key}] is listed but changes nothing");
    }
}

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// The CLI refuses a stray `[package]` key once, on stderr, naming the line
/// and the near key, and exits non-zero; the program does not run.
#[test]
fn the_cli_refuses_once_and_exits_non_zero() {
    let dir = scratch("cli");
    std::fs::write(dir.join("almide.toml"), "[package]\nname = \"app\"\nverison = \"0.1.0\"\n").expect("write manifest");
    std::fs::write(dir.join("main.almd"), "effect fn main() -> Unit = println(\"hi\")\n").expect("write source");
    for args in [vec!["check", "main.almd"], vec!["check", "--json", "main.almd"], vec!["run", "main.almd"]] {
        let out = Command::new(almide_bin()).args(&args).current_dir(&dir).output().expect("spawn almide");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "`almide {}` accepted an unknown key: {stderr}", args.join(" "));
        assert_eq!(stderr.matches("unknown key `verison`").count(), 1, "printed once: {stderr}");
        assert!(stderr.contains("almide.toml:3"), "names the line: {stderr}");
        assert!(stderr.contains("did you mean `version`?"), "{stderr}");
        assert!(!String::from_utf8_lossy(&out.stdout).contains("hi\n"), "the program ran");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// #4851: in the tables that restrict the program an unknown key, or a
/// top-level table one edit from one, is refused on every command — a
/// misspelled bound used to warn and grant every capability.
#[test]
fn a_misspelled_security_key_or_table_is_refused() {
    let pkg = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n";
    for (tag, table) in [
        ("perm-table", "[permission]\nallow = []\n"),
        ("perm-case", "[Permissions]\nallow = []\n"),
        ("perm-key", "[permissions]\nalow = []\n"),
        ("perm-new-key", "[permissions]\nnet = [\"example.com\"]\n"),
        ("trust-table", "[trusts]\nowners = []\n"),
        ("trust-key", "[trust]\nowner = [\"github.com/alice\"]\n"),
    ] {
        let dir = scratch(tag);
        std::fs::write(dir.join("almide.toml"), format!("{pkg}{table}")).expect("write manifest");
        std::fs::write(dir.join("main.almd"), "effect fn main() -> Unit = println(\"hi\")\n").expect("write source");
        for args in [vec!["check", "main.almd"], vec!["run", "main.almd", "--target", "wasm"]] {
            let out = Command::new(almide_bin()).args(&args).current_dir(&dir).output().expect("spawn almide");
            let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            assert!(!out.status.success(), "{tag}: `almide {}` accepted a misspelled bound:\n{text}", args.join(" "));
            assert!(text.contains("is refused"), "{tag}: {text}");
            assert!(!text.contains("hi\n"), "{tag}: the program ran:\n{text}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
