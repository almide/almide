//! The identifiers of generated Rust source, read token by token (#3486).
//!
//! Several build decisions ask "does the generated crate use runtime module
//! `m`?" — which modules to splice, which crates.io crates the manifest
//! declares, whether the rlib fast path applies. They used to answer it with
//! `code.contains("almide_rt_<m>_")`, and a user STRING LITERAL is emitted into
//! that text verbatim: `println("AlmideHttpRequest almide_rt_zlib_")` spliced
//! the http and zlib modules and pulled rustls/flate2 into a program that uses
//! neither. A literal is not a reference. This module lexes the source just
//! far enough to tell them apart — string, byte-string, raw-string, C-string
//! and char literals and comments are skipped whole — and yields only the
//! identifier tokens, so a decision keyed on an identifier cannot be flipped
//! by text the user wrote inside quotes.
//!
//! The identifiers that can carry a runtime spelling are then the compiler's:
//! a user fn spelled `almide_rt_…` reaches Rust escaped (#3487,
//! `almide_ir::is_reserved_fn_name`), and a user type spelled `Almide…` is
//! qualified and mangled (`is_rust_reserved_type_name`).

/// The runtime modules the rendered user code references by an identifier,
/// beyond the IR's call-driven `used_stdlib_modules`:
///
/// - a module whose `almide_rt_<module>_` symbol the code names — a few
///   operators lower to a runtime call, not a `CallTarget::Module` (float `**`
///   renders `almide_rt_math_fpow(..)` via the power_expr template);
/// - the module that defines a runtime-owned type the code names (#1829):
///   `let e: Endian = BigEndian` names bytes.rs's `AlmideEndian` without
///   calling a `bytes.*` fn (`walker::runtime_owned::modules_spelled_in`).
///
/// Identifier tokens only (#3486): a user string literal spelling
/// `almide_rt_zlib_` or `AlmideHttpRequest` is not a reference.
pub fn referenced_runtime_modules(user_code: &str) -> Vec<&'static str> {
    let reserved: Vec<&str> = identifiers(user_code)
        .filter(|id| id.starts_with("almide_rt_") || id.starts_with("Almide"))
        .collect();
    let mut out: Vec<&'static str> = crate::generated::rust_runtime::RUST_RUNTIME_MODULES
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| {
            let prefix = format!("almide_rt_{name}_");
            reserved.iter().any(|id| id.starts_with(&prefix))
        })
        .collect();
    out.extend(crate::walker::runtime_owned::modules_spelled_in(&reserved));
    out
}

/// Every identifier token of `code`, in order, outside literals and comments.
/// A raw identifier `r#name` yields `name`.
pub fn identifiers(code: &str) -> Identifiers<'_> {
    Identifiers { src: code, bytes: code.as_bytes(), pos: 0 }
}

/// Does `code` contain an identifier token that starts with `prefix`?
pub fn has_ident_with_prefix(code: &str, prefix: &str) -> bool {
    identifiers(code).any(|id| id.starts_with(prefix))
}

/// Does `code` contain the identifier token `name` exactly?
pub fn has_ident(code: &str, name: &str) -> bool {
    identifiers(code).any(|id| id == name)
}

/// Iterator over the identifier tokens of a Rust source.
pub struct Identifiers<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

fn is_ident_start(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphabetic() || b >= 0x80
}

fn is_ident_continue(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric() || b >= 0x80
}

impl<'a> Identifiers<'a> {
    fn peek(&self, off: usize) -> Option<u8> {
        self.bytes.get(self.pos + off).copied()
    }

    /// Skip a `"…"` body; `pos` is on the opening quote.
    fn skip_quoted(&mut self) {
        self.pos += 1;
        while let Some(b) = self.peek(0) {
            self.pos += 1;
            match b {
                b'\\' => self.pos += 1,
                b'"' => return,
                _ => {}
            }
        }
    }

    /// Skip a raw string; `pos` is on the first `#` or the `"`.
    fn skip_raw(&mut self) {
        let mut hashes = 0;
        while self.peek(0) == Some(b'#') {
            hashes += 1;
            self.pos += 1;
        }
        // On the opening quote.
        self.pos += 1;
        while self.pos < self.bytes.len() {
            let tail = &self.bytes[self.pos + 1..];
            if self.bytes[self.pos] == b'"' && tail.len() >= hashes && tail[..hashes].iter().all(|&b| b == b'#') {
                self.pos += 1 + hashes;
                return;
            }
            self.pos += 1;
        }
    }

    /// `pos` is on a `'`: skip a char literal, or step over a lifetime's
    /// quote (its name is then lexed as an ordinary identifier).
    fn skip_quote(&mut self) {
        match self.peek(1) {
            Some(b'\\') => {
                self.pos += 2;
                while let Some(b) = self.peek(0) {
                    self.pos += 1;
                    if b == b'\'' {
                        return;
                    }
                }
            }
            Some(_) => {
                // One char (possibly multi-byte), then a closing quote = char literal.
                let ch_len = self.src[self.pos + 1..].chars().next().map_or(1, char::len_utf8);
                if self.peek(1 + ch_len) == Some(b'\'') {
                    self.pos += 2 + ch_len;
                } else {
                    self.pos += 1;
                }
            }
            None => self.pos += 1,
        }
    }

    /// Skip a (nestable) block comment; `pos` is on `/*`.
    fn skip_block_comment(&mut self) {
        let mut depth = 0usize;
        while self.pos < self.bytes.len() {
            if self.peek(0) == Some(b'/') && self.peek(1) == Some(b'*') {
                depth += 1;
                self.pos += 2;
            } else if self.peek(0) == Some(b'*') && self.peek(1) == Some(b'/') {
                depth -= 1;
                self.pos += 2;
                if depth == 0 {
                    return;
                }
            } else {
                self.pos += 1;
            }
        }
    }

    /// After an identifier-shaped word `w`: is it a literal prefix (`r"`,
    /// `b"`, `br#"`, `c"`, `b'`, …)? If so, skip the literal and say so.
    fn skip_prefixed_literal(&mut self, word: &str) -> bool {
        match (word, self.peek(0)) {
            ("b" | "c", Some(b'"')) => {
                self.skip_quoted();
                true
            }
            ("b", Some(b'\'')) => {
                self.skip_quote();
                true
            }
            ("r" | "br" | "cr", Some(b'"')) => {
                self.skip_raw();
                true
            }
            ("r" | "br" | "cr", Some(b'#')) => {
                let mut off = 0;
                while self.peek(off) == Some(b'#') {
                    off += 1;
                }
                if self.peek(off) == Some(b'"') {
                    self.skip_raw();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

impl<'a> Iterator for Identifiers<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        while let Some(b) = self.peek(0) {
            match b {
                b'"' => self.skip_quoted(),
                b'\'' => self.skip_quote(),
                b'/' if self.peek(1) == Some(b'/') => {
                    while let Some(c) = self.peek(0) {
                        if c == b'\n' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                b'/' if self.peek(1) == Some(b'*') => self.skip_block_comment(),
                b if is_ident_start(b) => {
                    let start = self.pos;
                    while self.peek(0).is_some_and(is_ident_continue) {
                        self.pos += 1;
                    }
                    let word = &self.src[start..self.pos];
                    if self.skip_prefixed_literal(word) {
                        continue;
                    }
                    // Raw identifier `r#name`.
                    if word == "r" && self.peek(0) == Some(b'#') && self.peek(1).is_some_and(is_ident_start) {
                        self.pos += 1;
                        let s = self.pos;
                        while self.peek(0).is_some_and(is_ident_continue) {
                            self.pos += 1;
                        }
                        return Some(&self.src[s..self.pos]);
                    }
                    return Some(word);
                }
                b if b.is_ascii_digit() => {
                    // A number, suffix included (`1u8`, `0x_ff`): not an identifier.
                    while self.peek(0).is_some_and(is_ident_continue) {
                        self.pos += 1;
                    }
                }
                _ => self.pos += 1,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(src: &str) -> Vec<&str> {
        identifiers(src).collect()
    }

    #[test]
    fn literals_and_comments_hide_their_text() {
        let src = concat!(
            "fn main() {\n",
            "    println!(\"{}\", \"almide_rt_zlib_x AlmideHttpRequest\".to_string());\n",
            "    let s = r#\"almide_rt_http_get \"quoted\" \"#;\n",
            "    let t = br##\"almide_rt_sse_x\"##;\n",
            "    let u = b\"almide_rt_matrix_x\\\"\";\n",
            "    let c = 'a'; let q = '\\''; let e = '\\u{1F600}'; let m = 'é';\n",
            "    // almide_rt_prim_budget_x\n",
            "    /* outer /* almide_rt_x */ still */\n",
            "    almide_rt_list_len(&v)\n",
            "}\n",
        );
        let all = ids(src);
        assert!(all.contains(&"almide_rt_list_len"), "{all:?}");
        for hidden in ["almide_rt_zlib_x", "AlmideHttpRequest", "almide_rt_http_get", "almide_rt_sse_x", "almide_rt_matrix_x", "almide_rt_prim_budget_x", "almide_rt_x", "quoted", "still"] {
            assert!(!all.contains(&hidden), "{hidden} leaked out of a literal or comment: {all:?}");
        }
    }

    #[test]
    fn lifetimes_raw_idents_and_numbers() {
        let all = ids("fn f<'a>(x: &'a str) -> u8 { let r#type = 1u8; 0x_ff + r#type }");
        assert_eq!(all, vec!["fn", "f", "a", "x", "a", "str", "u8", "let", "type", "type"]);
    }

    #[test]
    fn prefix_queries() {
        assert!(has_ident_with_prefix("x = almide_rt_http_get(u);", "almide_rt_http_"));
        assert!(!has_ident_with_prefix("x = \"almide_rt_http_get\";", "almide_rt_http_"));
        assert!(has_ident("let v: AlmideValue = x;", "AlmideValue"));
        assert!(!has_ident("let v: AlmideValueX = x;", "AlmideValue"));
    }
}
