//! #1456: module names are ASCII. A caseless name is type-side by spelling
//! wherever a qualifier is read (`地理.作る` would be a convention method of a
//! type `地理`, and `type 別名 = 地理.住所` would make `地理` a type), and a
//! module name is also a FILE name, whose non-ASCII spelling the file system
//! may store decomposed (macOS) — the reason Rust refuses a non-ASCII `mod`
//! that maps to a file (E0754). So `import` / `module` path segments and an
//! `import … as` alias must be ASCII, refused at parse time with the rule
//! named, instead of being read as a type three stages later.

use std::process::Command;

fn check_project(main: &str, extra: &[(&str, &str)]) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"m\"\nversion = \"0.1.0\"\n").expect("manifest");
    std::fs::create_dir_all(dir.path().join("src")).expect("src");
    for (name, text) in extra {
        std::fs::write(dir.path().join("src").join(name), text).expect("module");
    }
    let main_path = dir.path().join("src").join("main.almd");
    std::fs::write(&main_path, main).expect("main");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("check")
        .arg(&main_path)
        .current_dir(dir.path())
        .output()
        .expect("run almide check");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

const GEO: &str = "type 住所 = { 都市: String }\n\nfn 作る(c: String) -> 住所 = 住所 { 都市: c }\n";

#[test]
fn a_non_ascii_module_path_is_refused_naming_the_rule() {
    let main = "import self.地理\n\ntype 別名 = 地理.住所\n\neffect fn main() -> Unit = {\n  let a: 別名 = 地理.作る(\"京都\")\n  println(a.都市)\n}\n";
    let (ok, text) = check_project(main, &[("地理.almd", GEO)]);
    assert!(!ok, "a non-ASCII module path must be refused, got:\n{text}");
    assert!(text.contains("module name '地理' is not ASCII"), "the diagnostic must name the segment and the rule:\n{text}");
}

#[test]
fn a_non_ascii_import_alias_is_refused_naming_the_rule() {
    let main = "import self.geo as 地理\n\neffect fn main() -> Unit = {\n  println(地理.作る(\"京都\").都市)\n}\n";
    let (ok, text) = check_project(main, &[("geo.almd", GEO)]);
    assert!(!ok, "a non-ASCII import alias must be refused, got:\n{text}");
    assert!(text.contains("module name '地理' is not ASCII"), "the diagnostic must name the alias and the rule:\n{text}");
}

#[test]
fn an_ascii_module_with_caseless_members_and_an_alias_of_its_type_checks() {
    let main = "import self.geo\n\ntype 別名 = geo.住所\n\neffect fn main() -> Unit = {\n  let a: 別名 = geo.作る(\"京都\")\n  println(a.都市)\n}\n";
    let (ok, text) = check_project(main, &[("geo.almd", GEO)]);
    assert!(ok, "an ASCII module whose members are caseless must check:\n{text}");
}
