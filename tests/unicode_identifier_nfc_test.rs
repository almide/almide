//! #1456, ALS-E17: an identifier is stored in NFC, so the precomposed `é`
//! (U+00E9) and `e` + U+0301 name ONE binding, on both targets — and `fmt`
//! prints the NFC spelling. Kept out of `spec/lang`, whose files are held to
//! `fmt --check`: an NFD spelling there would be rewritten by the formatter
//! before it could ever be tested.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const NFC: &str = "caf\u{e9}";
const NFD: &str = "cafe\u{301}";

fn program() -> String {
    format!(
        "effect fn main() -> Unit = {{\n  let {NFC} = 5\n  let \u{304b}\u{3099} = 2\n  println(int.to_string({NFD} + \u{304c}))\n}}\n"
    )
}

fn run(args: &[&str]) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("nfc.almd");
    std::fs::write(&file, program()).expect("write fixture");
    let out = Command::new(almide()).arg("run").arg(&file).args(args).output().expect("run almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

#[test]
fn canonically_equivalent_spellings_are_one_binding_on_native() {
    let (ok, text) = run(&[]);
    assert!(ok && text.trim() == "7", "native: NFD and NFC spellings must resolve to one binding, got:\n{text}");
}

#[test]
fn canonically_equivalent_spellings_are_one_binding_on_wasm() {
    let (ok, text) = run(&["--target", "wasm"]);
    assert!(ok && text.trim() == "7", "wasm: NFD and NFC spellings must resolve to one binding, got:\n{text}");
}

#[test]
fn fmt_prints_identifiers_in_nfc() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("nfc.almd");
    std::fs::write(&file, program()).expect("write fixture");
    let out = Command::new(almide()).arg("fmt").arg(&file).output().expect("run almide fmt");
    assert!(out.status.success(), "fmt failed: {}", String::from_utf8_lossy(&out.stderr));
    let formatted = std::fs::read_to_string(&file).expect("read back");
    assert!(!formatted.contains(NFD), "fmt kept the NFD spelling:\n{formatted}");
    assert!(!formatted.contains("\u{304b}\u{3099}"), "fmt kept the NFD kana:\n{formatted}");
    assert_eq!(formatted.matches(NFC).count(), 2, "both spellings print as NFC:\n{formatted}");
}
