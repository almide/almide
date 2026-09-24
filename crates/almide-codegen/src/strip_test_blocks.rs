//! The `#[cfg(test)]` stripper — ONE copy, shared by the two places that need it (#2511).
//!
//! `runtime/rs/src/*.rs` and `crates/almide-kernel/src/*.rs` are consumed as SOURCE TEXT,
//! not as crates: the build script embeds the runtime modules into
//! `generated/rust_runtime.rs` and inlines the kernel under `mod almide_kernel`, and emit
//! concatenates the modules a program needs into one flat module handed to rustc. The
//! runtime is compiled with `--test` in specs, so every `#[cfg(test)]` block has to be
//! deleted from that text first.
//!
//! The deleter used to count `{` / `}` per line with no idea what a string literal is, so
//! `runtime/rs/src/regex.rs`'s tests — which contain `r"a\{x"` and `r"a{2"`, +4 depth —
//! never brought the depth back to zero and the stripper ran to END OF FILE. Nothing was
//! lost only because that test module happened to be the file's last item; any real code
//! after such a block would have vanished from every emitted crate, silently, and the
//! emitted text is not read by anyone. Both copies of the stripper had the defect and
//! neither was tested, so this module is the single copy and the tests pin it
//! (`tests/strip_test_blocks_literals_test.rs`).
//!
//! Two properties hold here:
//!
//! 1. Braces are counted only in CODE — never inside a string, raw string (any hash
//!    depth), char literal, line comment or block comment, and an escape inside a normal
//!    string does not end it. Literal state carries ACROSS lines, because raw strings,
//!    normal strings and block comments all may span them.
//! 2. Losing a block's end is an ERROR, not a delete-to-EOF. "I could not strip it" is a
//!    safe failure; "I stripped everything after it" is not.
//!
//! It is `#[path]`-included by `buildscript/runtime_registry.rs` as well, so it must stay
//! free of `crate::` paths and of anything outside `std`.

#![allow(dead_code)]

use std::fmt;

/// A `#[cfg(test)]` block whose end was never found: the stripper refuses to
/// guess, and refuses to delete the rest of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnterminatedTestBlock {
    /// The file the text came from, as a label for the message.
    pub file: String,
    /// 1-based line the unterminated block opened on.
    pub line: usize,
}

impl fmt::Display for UnterminatedTestBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: unterminated `#[cfg(test)]` block — its closing brace was never found, \
             so the test-block stripper refuses to delete from here to end of file (#2511). \
             Check the braces in this block, including the ones inside string literals.",
            self.file, self.line
        )
    }
}

impl std::error::Error for UnterminatedTestBlock {}

/// Where the scanner is, carried across lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scan {
    Code,
    /// `//` to end of line.
    LineComment,
    /// `/* */`, which nests in Rust; the count is the open depth.
    BlockComment(u32),
    /// `"..."`, where `\` escapes the next char (so `\"` does not end it).
    Str,
    /// `r"..."` / `r#"..."#` at `usize` hashes; no escapes, closed by `"` + that many `#`.
    RawStr(usize),
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Consume a `'`-led token at `b[i]` and return the index just past it.
///
/// Rust spells a char literal and a lifetime with the same quote, so the two are told
/// apart the way the lexer does: `'\...'` is an escaped char literal (scan to the closing
/// quote — `'\u{7b}'` carries a BRACE that must not be counted), `'x'` is a plain one, and
/// anything else (`'a`, `'static`, `'_`) is a lifetime or a loop label, which consumes
/// only the quote itself and must NOT open a literal.
fn skip_char_or_lifetime(b: &[char], i: usize) -> usize {
    if i + 1 < b.len() && b[i + 1] == '\\' {
        let mut j = i + 1;
        while j < b.len() {
            if b[j] == '\\' {
                j += 2;
                continue;
            }
            if b[j] == '\'' {
                return j + 1;
            }
            j += 1;
        }
        return b.len();
    }
    if i + 2 < b.len() && b[i + 2] == '\'' {
        return i + 3;
    }
    i + 1
}

/// Scan one line, advancing `state`, and report `(net brace delta, saw a `}` in code)`.
///
/// Both outputs count CODE braces only; the `saw a closing brace` flag is the
/// literal-aware replacement for the old `line.contains('}')`.
fn scan_line(line: &str, state: &mut Scan) -> (i32, bool) {
    let b: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    let mut delta = 0i32;
    let mut closed = false;
    while i < b.len() {
        match *state {
            Scan::LineComment => break,
            Scan::BlockComment(depth) => {
                if b[i] == '*' && i + 1 < b.len() && b[i + 1] == '/' {
                    *state = if depth <= 1 { Scan::Code } else { Scan::BlockComment(depth - 1) };
                    i += 2;
                } else if b[i] == '/' && i + 1 < b.len() && b[i + 1] == '*' {
                    *state = Scan::BlockComment(depth + 1);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            Scan::Str => {
                if b[i] == '\\' {
                    i += 2;
                } else if b[i] == '"' {
                    *state = Scan::Code;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            Scan::RawStr(hashes) => {
                if b[i] == '"' {
                    let mut k = 0usize;
                    while k < hashes && i + 1 + k < b.len() && b[i + 1 + k] == '#' {
                        k += 1;
                    }
                    if k == hashes {
                        *state = Scan::Code;
                        i += 1 + hashes;
                    } else {
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            Scan::Code => i = scan_code_char(&b, i, state, &mut delta, &mut closed),
        }
    }
    if *state == Scan::LineComment {
        *state = Scan::Code;
    }
    (delta, closed)
}

/// One step of the scanner in code position; returns the next index.
fn scan_code_char(b: &[char], i: usize, state: &mut Scan, delta: &mut i32, closed: &mut bool) -> usize {
    match b[i] {
        '{' => {
            *delta += 1;
            i + 1
        }
        '}' => {
            *delta -= 1;
            *closed = true;
            i + 1
        }
        '/' if i + 1 < b.len() && b[i + 1] == '/' => {
            *state = Scan::LineComment;
            i + 2
        }
        '/' if i + 1 < b.len() && b[i + 1] == '*' => {
            *state = Scan::BlockComment(1);
            i + 2
        }
        '"' => {
            *state = Scan::Str;
            i + 1
        }
        '\'' => skip_char_or_lifetime(b, i),
        c if is_ident_start(c) => {
            // Consume the whole identifier, so the `r` of `for` can never be read as a
            // raw-string prefix and `r#type` (a raw IDENTIFIER) is not read as one either.
            let start = i;
            let mut j = i;
            while j < b.len() && is_ident_continue(b[j]) {
                j += 1;
            }
            let word: String = b[start..j].iter().collect();
            if matches!(word.as_str(), "r" | "br" | "cr") && j < b.len() && (b[j] == '"' || b[j] == '#') {
                let mut hashes = 0usize;
                let mut k = j;
                while k < b.len() && b[k] == '#' {
                    hashes += 1;
                    k += 1;
                }
                if k < b.len() && b[k] == '"' {
                    *state = Scan::RawStr(hashes);
                    return k + 1;
                }
            }
            j
        }
        _ => i + 1,
    }
}

/// Delete every `#[cfg(test)]` / `mod tests` block from `src`, counting braces in code
/// only. `file` labels the text in the error message.
///
/// Returns `Err(UnterminatedTestBlock)` when a block's end is never reached, rather than
/// deleting the remainder of the file.
pub fn strip_test_blocks(src: &str, file: &str) -> Result<String, UnterminatedTestBlock> {
    let mut out = String::new();
    let mut depth = 0i32;
    let mut in_test_mod = false;
    let mut opened_at = 0usize;
    let mut state = Scan::Code;
    for (idx, line) in src.lines().enumerate() {
        // A `#[cfg(test)]` spelled inside a string or a block comment opens nothing.
        let code_at_line_start = state == Scan::Code;
        let trimmed = line.trim();
        if !in_test_mod
            && code_at_line_start
            && (trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("mod tests"))
        {
            in_test_mod = true;
            depth = 0;
            opened_at = idx + 1;
        }
        let (delta, closed) = scan_line(line, &mut state);
        if in_test_mod {
            depth += delta;
            if depth <= 0 && closed {
                in_test_mod = false;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if in_test_mod {
        return Err(UnterminatedTestBlock { file: file.to_string(), line: opened_at });
    }
    Ok(out)
}
