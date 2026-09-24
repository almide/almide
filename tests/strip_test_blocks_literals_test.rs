//! #2511: the `#[cfg(test)]` stripper counts braces in CODE only, and refuses to delete
//! to end of file when it loses a block's end.
//!
//! `runtime/rs/src/*.rs` and `crates/almide-kernel/src/*.rs` are consumed as source TEXT
//! — embedded, stripped of their test blocks, and concatenated into the crate handed to
//! rustc. The stripper used to count `{` / `}` per line with no notion of a string
//! literal, so `regex.rs`'s `r"a\{x"` and `r"a{2"` (+4 depth) meant the block's end was
//! never reached and it ran to EOF. That was harmless only because the test module was
//! the file's last item; real code after such a block would have been deleted from every
//! emitted crate in silence. These assert on the stripper's OUTPUT TEXT, because a
//! downstream build is exactly the witness that did not notice.
//!
//! The stripper is ONE copy (`crates/almide-codegen/src/strip_test_blocks.rs`), shared
//! with `buildscript/runtime_registry.rs` by `#[path]`, so these pin both consumers.

use almide::codegen::strip_test_blocks::{strip_test_blocks, UnterminatedTestBlock};

fn strip(src: &str) -> String {
    strip_test_blocks(src, "t.rs").expect("stripper lost a block end")
}

/// The real shapes out of `runtime/rs/src/regex.rs`: raw-string literals holding
/// unbalanced braces, followed by a real function that must survive.
#[test]
fn unbalanced_braces_in_string_literals_do_not_eat_the_rest_of_the_file() {
    let src = r##"pub fn before() -> i32 { 1 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brace_escapes() {
        assert!(almide_regex_is_match(r"a\{x", "a{x"));
        assert!(almide_regex_full_match(r"a{2", "a{2"));
        assert_eq!(almide_regex_find(r"a{2,3}", "caaaab"), Some("aaa".to_string()));
    }
}

pub fn survivor(n: i32) -> i32 { n + 1 }
// ---- End Runtime ----
"##;
    let out = strip(src);
    assert!(out.contains("pub fn before()"), "text before the block was deleted:\n{out}");
    assert!(out.contains("pub fn survivor(n: i32) -> i32 { n + 1 }"), "the function after an unbalanced-literal test block was deleted:\n{out}");
    assert!(out.contains("// ---- End Runtime ----"), "the trailing comment was deleted:\n{out}");
    assert!(!out.contains("brace_escapes"), "the test block itself was not stripped:\n{out}");
    assert!(!out.contains("mod tests"), "the test block itself was not stripped:\n{out}");
}

/// A `}` inside a `//` comment used to close the block early, which leaves the module's
/// own `}` behind as a stray brace in the emitted crate.
#[test]
fn a_closing_brace_in_a_line_comment_does_not_close_the_block() {
    let src = r##"#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        // the module closes with a } like this one
        assert!(true);
    }
}

pub fn survivor() -> i32 { 7 }
"##;
    let out = strip(src);
    assert!(out.contains("pub fn survivor() -> i32 { 7 }"), "{out}");
    assert!(!out.contains("assert!(true)"), "{out}");
    assert!(
        !out.lines().any(|l| l.trim() == "}"),
        "a stray closing brace was left behind by an early block end:\n{out}"
    );
}

/// `\"` must not end the string, or the `{` after it is counted as code and the block
/// over-runs.
#[test]
fn an_escaped_quote_does_not_end_the_string_before_a_brace() {
    let src = r##"#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        let s = "he said \" { and never closed it";
        let t = "a backslash at the end \\";
        assert_eq!(s.len() + t.len() > 0, true);
    }
}

pub fn survivor() -> i32 { 9 }
"##;
    let out = strip(src);
    assert!(out.contains("pub fn survivor() -> i32 { 9 }"), "an escaped quote ended the string early:\n{out}");
    assert!(!out.contains("he said"), "{out}");
}

/// Char literals (including a `{` one and a unicode escape that SPELLS a brace), raw
/// strings at hash depth, block comments, and lifetimes — none of which are code braces,
/// and a lifetime must not be read as an unterminated char literal.
#[test]
fn braces_in_chars_raw_strings_and_block_comments_are_not_code() {
    let src = r####"#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        let open = '{';
        let esc = '\u{7b}';
        let quoted = '\'';
        let raw = r#"a { b "quoted" c"#;
        let deep = r###"still { open "## here"###;
        /* a block comment with { and a nested /* { */ inside */
        fn lifetimes<'a>(x: &'a str) -> &'a str { x }
        assert_eq!(lifetimes(raw).len() + deep.len() + open as usize + esc as usize + quoted as usize > 0, true);
    }
}

pub fn survivor() -> i32 { 11 }
"####;
    let out = strip(src);
    assert!(out.contains("pub fn survivor() -> i32 { 11 }"), "{out}");
    assert!(!out.contains("lifetimes"), "the test block was not stripped:\n{out}");
}

/// The safe failure: losing a block's end is reported, naming the file and the line the
/// block opened on. It is never a delete-to-EOF.
#[test]
fn an_unterminated_block_is_an_error_not_a_deletion() {
    let src = "pub fn before() {}\n\n#[cfg(test)]\nmod tests {\n    fn t() {\n        assert!(true);\n    }\n";
    let err = strip_test_blocks(src, "runtime/rs/src/example.rs")
        .expect_err("an unterminated test block must not be stripped silently");
    assert_eq!(
        err,
        UnterminatedTestBlock { file: "runtime/rs/src/example.rs".to_string(), line: 3 },
        "the error must name the file and the line the block opened on"
    );
    let msg = err.to_string();
    assert!(msg.contains("runtime/rs/src/example.rs:3"), "{msg}");
    assert!(msg.contains("unterminated"), "{msg}");
}

/// A block that never closes because its literals are unbalanced is the SAME safe
/// failure, not an over-run.
#[test]
fn an_unbalanced_literal_that_hides_the_end_is_reported_too() {
    let src = "#[cfg(test)]\nmod tests {\n    fn t() { let s = \"unterminated ...\n";
    let err = strip_test_blocks(src, "t.rs").expect_err("must not run to EOF");
    assert_eq!(err.line, 1);
}

/// The live tree, not a sample: every source these two consumers strip must strip
/// CLEANLY, and every file whose last item is a trailing comment must still have it
/// afterwards. That second clause is the defect stated as a property — `regex.rs`'s
/// `// ---- End Regex Runtime ----` was being eaten, and it is the file's last line only
/// by luck. The shapes themselves are pinned above from literal text, so this stays
/// honest when a source is edited; what it adds is that nothing in the tree over-runs
/// today.
#[test]
fn every_embedded_runtime_and_kernel_source_strips_cleanly() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut scanned = 0usize;
    let mut tails = 0usize;
    for dir in ["runtime/rs/src", "crates/almide-kernel/src", "crates/almide-rt-core/src"] {
        let d = root.join(dir);
        let mut paths: Vec<_> = std::fs::read_dir(&d)
            .unwrap_or_else(|e| panic!("{}: {e}", d.display()))
            .map(|e| e.expect("dir entry").path())
            .filter(|p| p.extension().is_some_and(|x| x == "rs"))
            .collect();
        paths.sort();
        for path in paths {
            let src = std::fs::read_to_string(&path).expect("source");
            let label = path.display().to_string();
            let out = strip_test_blocks(&src, &label)
                .unwrap_or_else(|e| panic!("a source in the tree over-runs: {e}"));
            scanned += 1;
            // A trailing comment is the cheapest thing to lose to an over-run and the
            // hardest to notice, so it is asserted where the tree has one.
            if let Some(last) = src.lines().rev().find(|l| !l.trim().is_empty()) {
                // Column 0 only: an indented comment could be the last line INSIDE a
                // test block, where deleting it is the right answer.
                if last.starts_with("//") && !last.starts_with("///") {
                    tails += 1;
                    assert!(out.contains(last), "{label}: the trailing comment `{last}` was stripped away");
                }
            }
        }
    }
    // #976 blind-scan floor: a scan that reaches nothing reports clean forever.
    assert!(scanned >= 40, "only {scanned} sources scanned — the sweep went blind");
    assert!(tails >= 3, "only {tails} trailing-comment file(s) found — the tail clause went blind");
}

/// Emitting text and stripping nothing are different answers: a file with no test block
/// comes back whole.
#[test]
fn a_file_without_a_test_block_is_unchanged() {
    let src = "pub fn a() -> i32 { 1 }\npub fn b() -> i32 { 2 }\n";
    assert_eq!(strip(src), src);
}
