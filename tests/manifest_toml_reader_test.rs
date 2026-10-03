//! `almide.toml` is read by the `toml` crate (#3253), not one line at a
//! time. The line reader lost every value written over several lines: a
//! multi-line `[permissions].allow` read as empty, which grants every
//! capability. These pin the other tables the same reader served: a value
//! is what TOML says it is, however it is spelled, and the two dependency
//! tables keep the order the file writes them in.

use std::path::PathBuf;

fn parse(tag: &str, text: &str) -> Result<almide::project::Project, String> {
    let dir: PathBuf = std::env::temp_dir().join(format!("almide-issue3253-reader-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("almide.toml");
    std::fs::write(&path, text).expect("write manifest");
    let out = almide::project::parse_toml(&path);
    let _ = std::fs::remove_dir_all(&dir);
    out
}

#[test]
fn permissions_read_every_array_spelling() {
    let p = parse(
        "perm",
        "[package]\nname = \"a\"\n\n[permissions]\n\
         allow = [\n  # comment before an item\n  \"IO\",  # trailing comment\n\n  \"Net\",\n]\n\
         proc = [\n  \"git\",\n  'cargo'\n]\n",
    )
    .expect("valid manifest");
    assert_eq!(p.permissions, ["IO", "Net"]);
    assert_eq!(p.proc_allow.as_deref(), Some(&["git".to_string(), "cargo".to_string()][..]));

    let p = parse("perm-empty", "[package]\nname = \"a\"\n\n[permissions]\nproc = [\n]\n").expect("valid manifest");
    assert!(p.permissions.is_empty());
    assert_eq!(p.proc_allow.as_deref(), Some(&[][..]), "an empty proc list allows no command; it is not absent");
}

#[test]
fn package_fields_and_their_defaults() {
    let p = parse("pkg", "[package]\nname = \"app\"\nalmide = \">=0.60\"\n").expect("valid manifest");
    assert_eq!(p.package.name, "app");
    assert_eq!(p.package.version, "0.1.0", "an absent version defaults as before");
    assert_eq!(p.package.almide_min.as_deref(), Some(">=0.60"));

    // `name` in another table is not the package name.
    let p = parse("pkg-other", "[dependencies]\nname = { path = \"../name\" }\n\n[package]\nname = \"app\"\n")
        .expect("valid manifest");
    assert_eq!(p.package.name, "app");
}

#[test]
fn dependencies_keep_file_order_and_read_every_spelling() {
    let p = parse(
        "deps",
        "[package]\nname = \"app\"\n\n[dependencies]\n\
         zeta = { path = \"../zeta\" }\n\
         alpha = { git = \"https://example.com/alpha\", tag = \"v1.0.0\" }\n\
         \"quoted\" = { path = \"../quoted\" }\n\
         not_a_dep = \"1.0\"\n\n\
         [dependencies.mid]\ngit = \"https://example.com/mid\"\nbranch = \"main\"\n",
    )
    .expect("valid manifest");
    let names: Vec<&str> = p.dependencies.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["zeta", "alpha", "quoted", "mid"], "file order; a bare string is not a dependency");
    let alpha = &p.dependencies[1];
    assert_eq!(alpha.git, "https://example.com/alpha");
    assert_eq!(alpha.tag.as_deref(), Some("v1.0.0"));
    assert_eq!(p.dependencies[3].branch.as_deref(), Some("main"));
}

#[test]
fn native_deps_keep_their_spec_and_file_order() {
    let p = parse(
        "native",
        "[package]\nname = \"app\"\n\n[native-deps]\n\
         serde = { version = \"1\", features = [\n  \"derive\",\n] }\n\
         anyhow = \"1.0\"\n",
    )
    .expect("valid manifest");
    let got: Vec<(&str, &str)> = p.native_deps.iter().map(|d| (d.name.as_str(), d.spec.as_str())).collect();
    assert_eq!(got[1], ("anyhow", "1.0"));
    assert_eq!(got[0].0, "serde");
    // The table spec reaches the generated Cargo.toml as one inline table.
    let spec: toml::Value = toml::from_str::<toml::Table>(&format!("x = {}", got[0].1))
        .expect("the spec is an inline TOML table")["x"]
        .clone();
    assert_eq!(spec["version"].as_str(), Some("1"));
    assert_eq!(spec["features"].as_array().map(Vec::len), Some(1));
    assert!(!got[0].1.contains('\n'), "an inline table on one line: {}", got[0].1);
}

/// A permission list that is not an array of strings is refused on its line;
/// the line reader read `allow = "IO"` as a one-name list and anything it
/// could not split as empty, which allows everything.
#[test]
fn a_permission_list_that_is_not_an_array_of_strings_is_refused() {
    let e = parse("perm-str", "[package]\nname = \"a\"\n\n[permissions]\nallow = \"IO\"\n").err().expect("a string is not a list");
    assert!(e.contains("almide.toml:5: [permissions].allow must be an array of strings"), "{e}");
    let e = parse("perm-int", "[package]\nname = \"a\"\n\n[permissions]\nproc = [\n  \"git\",\n  3,\n]\n")
        .err()
        .expect("an integer is not a command");
    assert!(e.contains("[permissions].proc must be an array of strings"), "{e}");
}
