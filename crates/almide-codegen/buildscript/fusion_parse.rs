//! Shared parser for `@rewrite(from = "...", to = "...")` attributes in
//! `stdlib/matrix.almd`. Two build.rs consumers depend on this:
//!
//! - `almide-codegen/build.rs` — emits a `FusionRule` registry used by
//!   the imperative `MatrixFusionPass`.
//! - `almide-egg-lab/build.rs` — emits `egg::rewrite!` invocations for
//!   the saturation-based fusion path.
//!
//! Both emitters read the same attribute set, so the parser lives here
//! and each crate's build.rs `#[path = "..."]`-includes this file.
//! Emitters stay in their own crate.

#[derive(Debug)]
pub enum Pat {
    Call { func: String, children: Vec<Pat> },
    Capture(String),
}

pub struct RewriteDecl {
    pub name: String,
    pub from: Pat,
    pub to: Pat,
}

/// Walk the stdlib source looking for `@rewrite(` blocks, pull out
/// `from` / `to` / optional `name`, and associate each with the
/// following `fn <name>` identifier when `name` is omitted.
pub fn extract_rewrites(src: &str) -> Vec<RewriteDecl> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i + 8 <= bytes.len() {
        if &bytes[i..i + 8] == b"@rewrite" {
            let mut j = i + 8;
            while j < bytes.len() && (bytes[j] as char).is_whitespace() { j += 1; }
            if j < bytes.len() && bytes[j] == b'(' {
                let rest = &src[j + 1..];
                let (body, consumed) = read_paren_body(rest);
                let from = extract_string_arg(body, "from");
                let to = extract_string_arg(body, "to");
                let explicit_name = extract_string_arg(body, "name");
                if let (Some(f), Some(t)) = (from, to) {
                    let name = explicit_name.unwrap_or_else(|| {
                        let tail = &src[i..];
                        let fn_pos = tail.find("\nfn ").unwrap_or_else(|| {
                            panic!("@rewrite with no following `fn` and no explicit name (at byte {i})")
                        });
                        let after_fn = &tail[fn_pos + 4..];
                        after_fn
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect::<String>()
                    });
                    if name.is_empty() {
                        panic!("@rewrite at byte {i} produced empty rule name");
                    }
                    if out.iter().any(|d: &RewriteDecl| d.name == name) {
                        panic!("duplicate @rewrite rule name: {name}");
                    }
                    let from_pat = parse_pattern(&f).unwrap_or_else(|e| {
                        panic!("@rewrite {name}: from pattern: {e}\n  source: {f}")
                    });
                    let to_pat = parse_pattern(&t).unwrap_or_else(|e| {
                        panic!("@rewrite {name}: to pattern: {e}\n  source: {t}")
                    });
                    out.push(RewriteDecl { name, from: from_pat, to: to_pat });
                }
                i = j + 1 + consumed;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn read_paren_body(s: &str) -> (&str, usize) {
    let mut depth = 1;
    let mut in_str = false;
    let mut prev_escape = false;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_str {
            if prev_escape {
                prev_escape = false;
            } else if c == '\\' {
                prev_escape = true;
            } else if c == '"' {
                in_str = false;
            }
        } else {
            match c {
                '"' => in_str = true,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return (&s[..i], i + 1);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    panic!("unbalanced @rewrite paren group: {}", s);
}

fn extract_string_arg(body: &str, arg_name: &str) -> Option<String> {
    let needle = format!("{} ", arg_name);
    let eq_needle = format!("{}=", arg_name);
    let key_pos = body.find(&needle).or_else(|| body.find(&eq_needle))?;
    let after_key = &body[key_pos + arg_name.len()..];
    let after_eq = after_key.trim_start().strip_prefix('=')?.trim_start();
    let after_quote = after_eq.strip_prefix('"')?;
    let mut out = String::new();
    let mut prev_escape = false;
    for c in after_quote.chars() {
        if prev_escape {
            out.push(c);
            prev_escape = false;
        } else if c == '\\' {
            prev_escape = true;
        } else if c == '"' {
            return Some(out);
        } else {
            out.push(c);
        }
    }
    None
}

pub fn parse_pattern(src: &str) -> Result<Pat, String> {
    let mut p = PatternParser { src: src.as_bytes(), pos: 0 };
    let pat = p.parse_pattern()?;
    p.skip_ws();
    if p.pos < p.src.len() {
        return Err(format!("trailing input at byte {}: {:?}", p.pos, &src[p.pos..]));
    }
    Ok(pat)
}

struct PatternParser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> PatternParser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> { self.src.get(self.pos).copied() }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        self.skip_ws();
        match self.peek() {
            Some(ch) if ch == c => { self.pos += 1; Ok(()) }
            other => Err(format!("expected {:?} at byte {}, got {:?}", c as char, self.pos, other.map(|b| b as char))),
        }
    }

    fn parse_ident(&mut self) -> Result<String, String> {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            if c.is_ascii_alphanumeric() || c == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(format!("expected ident at byte {}", start));
        }
        Ok(std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string())
    }

    fn parse_pattern(&mut self) -> Result<Pat, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'?') => {
                self.pos += 1;
                let name = self.parse_ident()?;
                Ok(Pat::Capture(name))
            }
            Some(_) => {
                let head = self.parse_ident()?;
                if head != "matrix" {
                    return Err(format!(
                        "only `matrix.<func>` calls are supported in patterns (got `{}` at byte {})",
                        head, self.pos
                    ));
                }
                self.expect(b'.')?;
                let func = self.parse_ident()?;
                self.expect(b'(')?;
                let children = self.parse_args()?;
                self.expect(b')')?;
                Ok(Pat::Call { func, children })
            }
            None => Err("unexpected end of pattern".into()),
        }
    }

    fn parse_args(&mut self) -> Result<Vec<Pat>, String> {
        self.skip_ws();
        if self.peek() == Some(b')') {
            return Ok(vec![]);
        }
        let mut args = vec![self.parse_pattern()?];
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b',') => { self.pos += 1; args.push(self.parse_pattern()?); }
                _ => break,
            }
        }
        Ok(args)
    }
}
