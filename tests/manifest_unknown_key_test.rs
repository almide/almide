//! A key `almide.toml` writes and no reader reads is a warning (#3382). It
//! used to be dropped without a word, so `brnach = "main"` or `tags =
//! "v0.1.0"` built from the default ref instead of the one written. The
//! warning names the line, the key, the keys that table reads, and the key
//! one edit away; the build still succeeds and the exit code does not move.
//!
//! The accepted-key lists live in `src/project.rs` beside the readers. The
//! `every_accepted_key_changes_the_parse` matrix pins them to the readers:
//! a listed key the reader does not read fails it, and a key the reader
//! reads without listing it trips the reader's `debug_assert!`.

use std::path::{Path, PathBuf};
use std::process::Command;

use almide::diagnostic::Diagnostic;
use almide::project::{
    manifest_warnings, parse_toml, DEPENDENCY_KEYS, PACKAGE_KEYS, PACKAGE_METADATA_KEYS, PERMISSION_KEYS, TARGET_PLATFORM_KEYS, TOP_LEVEL_KEYS,
};

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

fn parsed(tag: &str, text: &str) -> String {
    let dir = scratch(tag);
    let path = dir.join("almide.toml");
    std::fs::write(&path, text).expect("write manifest");
    let p = parse_toml(&path).unwrap_or_else(|e| panic!("{tag}: {e}"));
    let _ = std::fs::remove_dir_all(&dir);
    // `root` and each `declared_at` name the scratch directory, which differs
    // per tag; only what the keys say is compared.
    format!("{:?} {:?} {:?} {:?} {:?}", p.package, p.dependencies, p.permissions, p.proc_allow, p.native_deps)
        .replace(&dir.display().to_string(), "<dir>")
}

#[test]
fn a_correct_manifest_warns_nothing() {
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
}

#[test]
fn an_unknown_dependency_key_names_its_line_and_the_near_key() {
    let text = "[package]\nname = \"app\"\n\n[dependencies]\n\
                a = { git = \"https://example.com/a\", brnach = \"main\" }\n\
                b = { git = \"https://example.com/b\", tags = \"v0.1.0\" }\n\
                c = { git = \"https://example.com/c\",\n      rev = \"abc123\" }\n";
    let w = shown(text);
    assert_eq!(w.len(), 3, "{w:#?}");
    assert_eq!(w[0].0, 5);
    assert_eq!(w[0].1, "unknown key `brnach` in dependency `a` is ignored — it changes nothing");
    assert!(w[0].2.starts_with("did you mean `branch`?"), "{}", w[0].2);
    assert!(
        w[0].2.ends_with("`git`, `tag`, `branch`, `version`, `path`, `subdir`"),
        "the hint lists the accepted keys: {}",
        w[0].2
    );
    assert_eq!(w[1].0, 6);
    assert!(w[1].2.starts_with("did you mean `tag`?"), "{}", w[1].2);
    assert_eq!(w[2].0, 8, "the line of the key, not of the entry");
    assert!(w[2].1.contains("`rev`"));
    assert!(w[2].2.starts_with("delete it, or write one of the keys"), "no key is one edit from `rev`: {}", w[2].2);
}

#[test]
fn dotted_and_table_dependency_spellings_warn_alike() {
    let text = "[package]\nname = \"app\"\n\n[dependencies]\n\
                a.git = \"https://example.com/a\"\n\
                a.tga = \"v1\"\n\n\
                [dependencies.b]\n\
                git = \"https://example.com/b\"\n\
                subdri = \"pkgs/b\"\n";
    let w = shown(text);
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
fn package_permissions_and_top_level_tables_warn() {
    let text = "[package]\nname = \"app\"\nverison = \"1.0.0\"\nauthors = [\"x\"]\n\n\
                [permissions]\nalow = [\"IO\"]\n\n\
                [dependecies]\na = { git = \"https://example.com/a\" }\n\n\
                [targets]\nwasm = true\n";
    let w = shown(text);
    assert_eq!(w.len(), 5, "{w:#?}");
    // Top-level tables first (file order), then the inside of each table.
    assert_eq!(w[0].0, 9);
    assert_eq!(w[0].1, "unknown table `dependecies` in almide.toml is ignored — it changes nothing");
    assert!(w[0].2.starts_with("did you mean `dependencies`?"), "{}", w[0].2);
    assert_eq!(w[1].0, 12);
    assert!(w[1].2.starts_with("did you mean `target`?"), "{}", w[1].2);
    assert_eq!(w[2].0, 3);
    assert!(w[2].1.contains("in [package]") && w[2].2.starts_with("did you mean `version`?"), "{w:#?}");
    assert_eq!(w[3].0, 4);
    assert!(w[3].2.starts_with("delete it"), "{}", w[3].2);
    assert_eq!(w[4].0, 7);
    assert!(w[4].1.contains("in [permissions]") && w[4].2.starts_with("did you mean `allow`?"), "{w:#?}");
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
            other => panic!("top-level table `{other}` has no case here — add one that sets it"),
        };
        assert_ne!(parsed(&format!("top-{key}"), &text), base, "[{key}] is listed but changes nothing");
    }
}

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// The CLI prints the warning once, on stderr, and still exits 0; `check
/// --json` carries it as a `warning` row on stdout.
#[test]
fn the_cli_warns_once_and_exits_zero() {
    let dir = scratch("cli");
    std::fs::write(
        dir.join("almide.toml"),
        "[package]\nname = \"app\"\nverison = \"0.1.0\"\n",
    )
    .expect("write manifest");
    std::fs::write(dir.join("main.almd"), "effect fn main() -> Unit = println(\"hi\")\n").expect("write source");

    let out = Command::new(almide_bin()).args(["check", "main.almd"]).current_dir(&dir).output().expect("spawn almide");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "a warning must not fail check: {stderr}");
    assert_eq!(stderr.matches("unknown key `verison`").count(), 1, "printed once: {stderr}");
    assert!(stderr.contains("almide.toml:3"), "names the line: {stderr}");
    assert!(stderr.contains("did you mean `version`?"), "{stderr}");

    let out = Command::new(almide_bin())
        .args(["check", "--json", "main.almd"])
        .current_dir(&dir)
        .output()
        .expect("spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "a warning must not fail check --json: {stdout}");
    let row = stdout.lines().find(|l| l.contains("verison")).unwrap_or_else(|| panic!("no JSON row: {stdout}"));
    assert!(row.contains("\"level\":\"warning\""), "{row}");
    assert!(row.contains("\"line\":3"), "{row}");

    let out = Command::new(almide_bin()).args(["run", "main.almd"]).current_dir(&dir).output().expect("spawn almide");
    assert!(out.status.success(), "a warning must not fail run: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi");
    let _ = std::fs::remove_dir_all(&dir);
}
