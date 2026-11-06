//! The README's "at a glance" program is `examples/order-total.almd` without its tests.
//! The three READMEs must show exactly that code and the output it prints, so the first
//! program a reader sees can't rot.

use std::path::Path;
use std::process::Command;

const EXAMPLE: &str = "examples/order-total.almd";
const OUTPUT: &str = "2 orders, total 760";

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn example_code() -> String {
    let src = std::fs::read_to_string(root().join(EXAMPLE)).expect("read the example");
    src.split("\ntest \"").next().unwrap().trim_end().to_string()
}

/// The first fenced block after `heading`, with the language tag `lang`.
fn fence_after(text: &str, heading: &str, lang: &str) -> String {
    let start = text.find(heading).unwrap_or_else(|| panic!("no `{heading}` section"));
    let rest = &text[start..];
    let open = format!("```{lang}\n");
    let body = &rest[rest.find(&open).unwrap_or_else(|| panic!("no ```{lang} fence under `{heading}`")) + open.len()..];
    body[..body.find("\n```").expect("unclosed fence")].to_string()
}

#[test]
fn every_readme_shows_the_example_and_its_output() {
    let code = example_code();
    for (file, heading) in [
        ("README.md", "## Almide at a glance"),
        ("README.ja.md", "## Almide をひと目で"),
        ("README.zh-CN.md", "## 一眼看懂 Almide"),
    ] {
        let text = std::fs::read_to_string(root().join(file)).expect("read the README");
        assert_eq!(fence_after(&text, heading, "almd"), code, "{file}: the code differs from {EXAMPLE}");
        assert_eq!(fence_after(&text, heading, "text"), OUTPUT, "{file}: the printed output differs");
    }
}

#[test]
fn the_example_prints_the_output_the_readmes_show() {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", EXAMPLE, "--target", "wasm"])
        .current_dir(root())
        .output()
        .expect("run almide");
    assert!(out.status.success(), "almide run failed: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), OUTPUT);
}
