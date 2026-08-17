//! Minimal Version Selection over a diamond (#1458).
//!
//! `docs/specs/package-system.md` documents MVS ("keep maximum requested
//! version"); the resolver implemented first-wins — whichever requirement the
//! depth-first walk met first was fetched, and the other was silently
//! dropped, so the build changed with declaration order. Pinned here with a
//! path-dep diamond: the app asks for alib 1.2.0 directly and for blib,
//! which asks for alib 1.5.0. Selection must be 1.5.0 in BOTH declaration
//! orders.

use almide::project::parse_toml;
use almide::project_fetch::fetch_all_deps;
use std::path::Path;

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

/// Lay out the diamond; `app_deps_order` supplies the app's [dependencies]
/// body so both declaration orders share one builder.
fn scratch(name: &str, app_deps: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue1458-{}", name));
    let _ = std::fs::remove_dir_all(&root);
    for (dir, ver) in [("a_old", "1.2.0"), ("a_new", "1.5.0")] {
        write(
            &root.join(dir).join("almide.toml"),
            &format!("[package]\nname = \"alib\"\nversion = \"{}\"\n", ver),
        );
        write(
            &root.join(dir).join("src").join("mod.almd"),
            &format!("fn version() -> String = \"{}\"\n", ver),
        );
    }
    write(
        &root.join("b").join("almide.toml"),
        &format!(
            "[package]\nname = \"blib\"\nversion = \"0.1.0\"\n\n[dependencies]\nalib = {{ path = \"{}\", version = \"1.5.0\" }}\n",
            root.join("a_new").display()
        ),
    );
    write(&root.join("b").join("src").join("mod.almd"), "fn go() -> Int = 1\n");
    write(&root.join("app").join("almide.toml"), app_deps);
    write(&root.join("app").join("src").join("mod.almd"), "fn run() -> Int = 0\n");
    root
}

fn selected_alib_dir(root: &Path) -> (String, std::path::PathBuf) {
    let proj = parse_toml(&root.join("app").join("almide.toml")).expect("parse app toml");
    let fetched = fetch_all_deps(&proj).expect("fetch");
    let alib: Vec<_> = fetched.iter().filter(|f| f.pkg_id.name == "alib").collect();
    assert_eq!(alib.len(), 1, "one selected alib, got {:?}", alib);
    (alib[0].version.clone(), alib[0].source_dir.clone())
}

#[test]
fn the_maximum_requested_version_wins() {
    let root = scratch(
        "fwd",
        &format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nalib = {{ path = \"{}\", version = \"1.2.0\" }}\nblib = {{ path = \"{}\" }}\n",
            std::env::temp_dir().join("almide-issue1458-fwd").join("a_old").display(),
            std::env::temp_dir().join("almide-issue1458-fwd").join("b").display()
        ),
    );
    let (version, dir) = selected_alib_dir(&root);
    assert_eq!(version, "1.5.0");
    assert!(dir.starts_with(root.join("a_new")), "selected {:?}", dir);
}

/// The other declaration order — the case first-wins got wrong: the direct
/// 1.2.0 requirement was met first and 1.5.0 was silently dropped.
#[test]
fn selection_is_declaration_order_independent() {
    let root = scratch(
        "rev",
        &format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nblib = {{ path = \"{}\" }}\nalib = {{ path = \"{}\", version = \"1.2.0\" }}\n",
            std::env::temp_dir().join("almide-issue1458-rev").join("b").display(),
            std::env::temp_dir().join("almide-issue1458-rev").join("a_old").display()
        ),
    );
    let (version, dir) = selected_alib_dir(&root);
    assert_eq!(version, "1.5.0");
    assert!(dir.starts_with(root.join("a_new")), "selected {:?}", dir);
}
