//! #4820: two inputs skipped the hidden-character refusals — an AST `.json`
//! entry ran with a raw U+202E in a string, and a dependency's `almide.toml`
//! holding one was read (its error swallowed) while the root's is refused.

use std::process::Command;

const RLO: char = '\u{202E}';

fn almide(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(dir)
        .env("HOME", dir.join("home"))
        .args(args)
        .output()
        .expect("spawn almide")
}

fn text(o: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn an_ast_entry_with_a_hidden_character_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("p.almd"), "effect fn main() -> Unit = println(\"plain\")\n").unwrap();
    let o = almide(dir.path(), &["p.almd", "--emit-ast"]);
    assert!(o.status.success(), "{}", text(&o));
    let ast = String::from_utf8(o.stdout).unwrap();
    assert!(ast.contains("plain"), "{ast}");
    std::fs::write(dir.path().join("ok.json"), &ast).unwrap();
    let o = almide(dir.path(), &["run", "ok.json"]);
    assert_eq!(String::from_utf8_lossy(&o.stdout), "plain\n", "the unchanged AST entry no longer runs: {}", text(&o));
    std::fs::write(dir.path().join("bad.json"), ast.replace("plain", &format!("pl{RLO}ain"))).unwrap();
    let o = almide(dir.path(), &["run", "bad.json"]);
    assert!(!o.status.success(), "an AST entry with a raw U+202E ran:\n{}", text(&o));
    assert!(!text(&o).contains(RLO), "the refusal printed the raw character:\n{}", text(&o));
}

#[test]
fn a_dependency_manifest_with_a_hidden_character_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::create_dir_all(d.join("dep/src")).unwrap();
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("dep/almide.toml"), format!("[package]\nname = \"dep\"\nversion = \"0.1.0\"\ndescription = \"a{RLO}b\"\n")).unwrap();
    std::fs::write(d.join("dep/src/mod.almd"), "fn hello() -> Int = 7\n").unwrap();
    std::fs::write(d.join("almide.toml"), "[package]\nname = \"p\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = { path = \"dep\" }\n").unwrap();
    std::fs::write(d.join("src/main.almd"), "import dep\neffect fn main() -> Unit = println(int.to_string(dep.hello()))\n").unwrap();
    let o = almide(d, &["check", "src/main.almd"]);
    assert!(!o.status.success(), "a dependency manifest with a raw U+202E was read:\n{}", text(&o));
    assert!(text(&o).contains("dependency `dep`"), "{}", text(&o));
    assert!(!text(&o).contains(RLO), "{}", text(&o));
}
