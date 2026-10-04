//! The platform key of a `[target.<key>.native-deps]` table (#3350), checked
//! the way Cargo checks the key of `[target.<key>.dependencies]`: either a
//! `cfg(...)` expression or a target name. The key is copied verbatim into the
//! generated Cargo.toml, so anything Cargo would refuse is refused here, on
//! the manifest line that writes it, instead of as a Cargo error about a file
//! the author never wrote.
//!
//! The grammar is Cargo's (`cargo-platform`):
//!
//! ```text
//! key   := "cfg(" expr ")" | name
//! expr  := "all(" list ")" | "any(" list ")" | "not(" expr ")"
//!        | ident | ident "=" string
//! list  := [ expr ("," expr)* [","] ]
//! ident := [A-Za-z_][A-Za-z0-9_]*
//! name  := one or more of [A-Za-z0-9_.-]
//! ```
//!
//! A string is `"` … `"` with no escapes.

/// Check a `[target.<key>]` key. `Err` says what is wrong in Cargo's terms.
pub fn validate_target_key(key: &str) -> Result<(), String> {
    if let Some(inner) = key.strip_prefix("cfg(").and_then(|k| k.strip_suffix(')')) {
        return validate_cfg_expr(inner);
    }
    if key.starts_with("cfg(") || key.starts_with("cfg ") {
        return Err("a `cfg(...)` key must end with `)`".to_string());
    }
    if key.is_empty() {
        return Err("the target name is empty".to_string());
    }
    match key.chars().find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))) {
        Some(c) => Err(format!(
            "unexpected character `{c}` in target name — a key is either `cfg(...)` or a target triple"
        )),
        None => Ok(()),
    }
}

#[derive(Debug, PartialEq)]
enum Token<'a> {
    LeftParen,
    RightParen,
    Comma,
    Equals,
    Ident(&'a str),
    Str(&'a str),
}

fn describe(t: Option<&Token>) -> String {
    match t {
        None => "the end of the expression".to_string(),
        Some(Token::LeftParen) => "`(`".to_string(),
        Some(Token::RightParen) => "`)`".to_string(),
        Some(Token::Comma) => "`,`".to_string(),
        Some(Token::Equals) => "`=`".to_string(),
        Some(Token::Ident(s)) => format!("`{s}`"),
        Some(Token::Str(s)) => format!("\"{s}\""),
    }
}

fn tokenize(s: &str) -> Result<Vec<Token<'_>>, String> {
    let mut out = Vec::new();
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {}
            '(' => out.push(Token::LeftParen),
            ')' => out.push(Token::RightParen),
            ',' => out.push(Token::Comma),
            '=' => out.push(Token::Equals),
            '"' => {
                let rest = &s[i + 1..];
                let end = rest.find('"').ok_or_else(|| format!("unterminated string `{}`", &s[i..]))?;
                out.push(Token::Str(&rest[..end]));
                for _ in 0..rest[..=end].chars().count() {
                    chars.next();
                }
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut end = i + c.len_utf8();
                while let Some(&(j, d)) = chars.peek() {
                    if !(d.is_ascii_alphanumeric() || d == '_') {
                        break;
                    }
                    end = j + d.len_utf8();
                    chars.next();
                }
                out.push(Token::Ident(&s[i..end]));
            }
            c => return Err(format!("unexpected character `{c}` in cfg expression")),
        }
    }
    Ok(out)
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token<'a>> {
        self.tokens.get(self.pos)
    }

    fn eat(&mut self, want: &Token) -> Result<(), String> {
        if self.peek() == Some(want) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected {}, found {}", describe(Some(want)), describe(self.peek())))
        }
    }

    fn expr(&mut self) -> Result<(), String> {
        let Some(Token::Ident(name)) = self.peek() else {
            return Err(format!("expected a cfg name, `all`, `any` or `not`, found {}", describe(self.peek())));
        };
        let name = *name;
        self.pos += 1;
        match (name, self.peek()) {
            ("all" | "any", Some(Token::LeftParen)) => {
                self.pos += 1;
                while self.peek() != Some(&Token::RightParen) {
                    self.expr()?;
                    if self.peek() == Some(&Token::Comma) {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                self.eat(&Token::RightParen)
            }
            ("not", Some(Token::LeftParen)) => {
                self.pos += 1;
                self.expr()?;
                self.eat(&Token::RightParen)
            }
            (_, Some(Token::Equals)) => {
                self.pos += 1;
                match self.peek() {
                    Some(Token::Str(_)) => {
                        self.pos += 1;
                        Ok(())
                    }
                    other => Err(format!("expected a string after `{name} =`, found {}", describe(other))),
                }
            }
            _ => Ok(()),
        }
    }
}

/// Check the expression inside `cfg( … )`.
fn validate_cfg_expr(inner: &str) -> Result<(), String> {
    let mut parser = Parser { tokens: tokenize(inner)?, pos: 0 };
    parser.expr()?;
    match parser.peek() {
        None => Ok(()),
        Some(t) => Err(format!("unexpected {} after the cfg expression", describe(Some(t)))),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_target_key;

    #[test]
    fn keys_cargo_accepts_are_accepted() {
        for key in [
            r#"cfg(target_os = "android")"#,
            r#"cfg(not(any(target_os = "android", target_os = "ios")))"#,
            r#"cfg(all(unix, target_pointer_width = "64",))"#,
            "cfg(windows)",
            "cfg(any())",
            "x86_64-pc-windows-gnu",
            "aarch64-linux-android",
            "thumbv7em-none-eabihf.json",
        ] {
            assert_eq!(validate_target_key(key), Ok(()), "{key}");
        }
    }

    #[test]
    fn keys_cargo_refuses_are_refused() {
        for key in [
            "",
            "cfg()",
            "cfg(",
            "cfg(target_os = )",
            r#"cfg(target_os = "android""#,
            r#"cfg(target_os = "android)"#,
            "cfg(target_os = android)",
            "cfg(not(unix, windows))",
            "cfg(unix windows)",
            "cfg(any(unix)",
            "cfg(1abc)",
            "cfg(unix) ",
            "x86_64 linux",
            "target/os",
        ] {
            assert!(validate_target_key(key).is_err(), "accepted `{key}`");
        }
    }
}
