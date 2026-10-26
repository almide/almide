//! A cloned repository's `almide.toml` cannot drive the reader's terminal
//! (#4126, security table row 21). A key or string holding a control or
//! bidirectional-control character (the set C-418 escapes in runtime error
//! texts) is refused when the manifest is read, so no manifest text reaches a
//! diagnostic or a `Built …` line with one: before, `"\e]0;pwn\a\e[2Kfake" = 1`
//! landed verbatim in `warning: unknown key …` from `almide check` (setting the
//! terminal title and erasing the line), and a package name with escapes was
//! printed raw by `almide build`.
//!
//! Every case runs the real CLI and asserts on the bytes of both streams: no
//! byte below 0x20 other than `\n`, no DEL, no C1 or bidi control. The refusal
//! itself shows the offending text escaped.

use std::path::PathBuf;
use std::process::{Command, Output};

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn project(tag: &str, manifest: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-4126-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("almide.toml"), manifest).expect("write manifest");
    std::fs::write(
        dir.join("main.almd"),
        "effect fn main() -> Unit = println(\"hi\")\n",
    )
    .expect("write source");
    dir
}

fn run(dir: &PathBuf, args: &[&str]) -> Output {
    Command::new(almide_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn almide")
}

/// The characters that move or hide text when printed, found in `bytes`
/// (`\n` is the stream's own line break and is allowed).
fn terminal_controls(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .chars()
        .filter(|&c| c != '\n' && almide_base::err_text::almide_rt_errtext_is_escaped(c))
        .map(|c| format!("U+{:04X}", c as u32))
        .collect()
}

fn assert_clean(what: &str, out: &Output) {
    for (stream, bytes) in [("stdout", &out.stdout), ("stderr", &out.stderr)] {
        let found = terminal_controls(bytes);
        assert!(
            found.is_empty(),
            "{what}: {stream} carries {found:?} from almide.toml:\n{}",
            String::from_utf8_lossy(bytes).escape_debug()
        );
    }
}

/// The forms #4126's comment probed, raw bytes and TOML escapes alike: an
/// unknown top-level key, a `[package]` name, a dependency name, a string
/// value, and a bidi control.
const CASES: &[(&str, &str)] = &[
    (
        "unknown-key-raw",
        "[package]\nname = \"app\"\n\"\u{1b}]0;pwn\u{7}\u{1b}[2Kfake\" = 1\n",
    ),
    (
        "unknown-key-escape",
        "[package]\nname = \"app\"\n\"\\u001b]0;pwn\\u0007\\u001b[2Kfake\" = 1\n",
    ),
    (
        "package-name",
        "[package]\nname = \"app\\u001b[2K\\rBuilt ok\"\n",
    ),
    (
        "dependency-name",
        "[package]\nname = \"app\"\n\n[dependencies]\n\"d\\u001b[31m\" = { path = \"../d\" }\n",
    ),
    (
        "string-value",
        "[package]\nname = \"app\"\ndescription = \"line\\nINFO: forged\"\n",
    ),
    (
        "bidi",
        "[package]\nname = \"app\"\nlicense = \"MIT\u{202e}txt.exe\"\n",
    ),
];

#[test]
fn a_manifest_text_with_a_terminal_control_is_refused_and_never_printed_raw() {
    for (tag, manifest) in CASES {
        let dir = project(tag, manifest);
        for args in [
            &["check", "main.almd"][..],
            &["build", "main.almd", "-o", "app"][..],
        ] {
            let out = run(&dir, args);
            let what = format!("{tag}: almide {}", args.join(" "));
            assert!(!out.status.success(), "{what}: the manifest was accepted");
            let stderr = String::from_utf8_lossy(&out.stderr);
            // TOML itself refuses a raw C0 byte in a string; everything else is
            // the manifest reader's refusal.
            let toml_refused =
                tag.ends_with("-raw") && stderr.contains("almide.toml is read as TOML");
            assert!(
                toml_refused
                    || stderr.contains("holds a control or bidi character"),
                "{what}: not refused for its control character:\n{}",
                stderr.escape_debug()
            );
            assert!(
                stderr.contains("almide.toml:"),
                "{what}: the refusal names no line: {stderr}"
            );
            assert_clean(&what, &out);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The refusal shows the text with C-418's escapes, so the reader can see
/// what is there.
#[test]
fn the_refusal_shows_the_text_escaped() {
    let dir = project("shown", CASES[1].1);
    let out = run(&dir, &["check", "main.almd"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(r#"key `\u{1b}]0;pwn\u{7}\u{1b}[2Kfake`"#),
        "{}",
        stderr.escape_debug()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The control for the tests above: an ordinary manifest — non-ASCII text,
/// quotes and backslashes included — is read as before.
#[test]
fn an_ordinary_manifest_is_read_as_before() {
    let dir = project(
        "ordinary",
        "[package]\nname = \"app\"\ndescription = \"日本語 \\\"q\\\" C:\\\\dir\"\n",
    );
    let out = run(&dir, &["run", "main.almd"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi");
    assert_clean("ordinary", &out);
    let _ = std::fs::remove_dir_all(&dir);
}
