//! Extract `@rewrite(from = "...", to = "...")` attributes from
//! `stdlib/matrix.almd` and emit a `FusionRule` registry into
//! `src/generated/fusion_rules.rs`.
//!
//! The pattern DSL parsed here is a subset of the egg/MLIR pattern
//! shape: `matrix.<func>(children...)` for calls and `?name` for
//! captures. Future Stage 1 of the MLIR + egg arc will translate the
//! same declarations into `egg::Rewrite<AlmideExpr, ()>` LHS/RHS —
//! this build step is the pre-Stage-1 skeleton.
//!
//! ## Why build-time extraction (not parser integration)
//!
//! The Almide parser lives in another crate, so using it from this
//! build.rs would introduce a circular dependency. Every `@rewrite`
//! body is a tiny string literal with a fixed shape; a hand-rolled
//! tokenizer + recursive-descent parser stays under 250 lines and keeps
//! the build fast.
//!
//! ## Output
//!
//! Generated `fusion_rules.rs` contains a single `pub fn
//! generated_fusion_rules() -> Vec<FusionRule>` that the codegen crate
//! re-exports from `pass_matrix_fusion_rules::fusion_rules()`.

use std::path::Path;

/// The shape we recognise in stdlib: every rewrite looks like a
/// `?name` capture or a `matrix.<func>(child, child, ...)` call.
#[derive(Debug)]
enum Pat {
    Call { func: String, children: Vec<Pat> },
    Capture(String),
}

/// One extracted `@rewrite(from = "...", to = "...")` with an
/// associated rule name (derived from the adjacent `fn` declaration).
struct RewriteDecl {
    /// Rule name for diagnostics. Taken from the fn name.
    name: String,
    /// The LHS pattern tree.
    from: Pat,
    /// The RHS pattern tree. In the current scope it is always a
    /// single `matrix.<func>(?cap, ?cap, ...)` call — one new runtime
    /// intrinsic, captures in some order. Enforcement is at emit time.
    to: Pat,
}

pub fn generate(workspace_root: &Path, out_dir: &Path) {
    let stdlib_path = workspace_root.join("stdlib/matrix.almd");
    println!("cargo:rerun-if-changed={}", stdlib_path.display());

    let src = std::fs::read_to_string(&stdlib_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", stdlib_path.display(), e));

    let decls = extract_rewrites(&src);
    let rust_src = emit_rust(&decls);

    let out_path = out_dir.join("fusion_rules.rs");
    std::fs::write(&out_path, rust_src)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", out_path.display(), e));
}

// ── Extraction from stdlib source ──────────────────────────────────

/// Walk the stdlib source character-by-character, looking for
/// `@rewrite(` blocks. Capture `from` / `to` string literal values
/// from inside the paren group, and associate each attribute with the
/// next `fn <name>` that follows. A fn may carry multiple attributes
/// between itself and the previous fn, so we accumulate pending
/// RewriteDecl components until the fn name lands them.
fn extract_rewrites(src: &str) -> Vec<RewriteDecl> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i + 8 <= bytes.len() {
        if &bytes[i..i + 8] == b"@rewrite" {
            let mut j = i + 8;
            while j < bytes.len() && (bytes[j] as char).is_whitespace() { j += 1; }
            if j < bytes.len() && bytes[j] == b'(' {
                // Balanced-paren extraction starting right after `(`.
                let rest = &src[j + 1..];
                let (body, consumed) = read_paren_body(rest);
                let from = extract_string_arg(body, "from");
                let to = extract_string_arg(body, "to");
                let explicit_name = extract_string_arg(body, "name");
                if let (Some(f), Some(t)) = (from, to) {
                    // Rule name comes from `name = "..."` if given, else
                    // from the next `\nfn ` identifier after this
                    // attribute. A single fn may carry multiple
                    // `@rewrite` attributes (e.g. mul_scaled with lhs /
                    // rhs mirrors), so we don't dedupe by fn name —
                    // dedup is by rule name instead.
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

/// Read a balanced-parenthesis body starting right after an opening
/// `(`. Returns (body_without_parens, bytes_consumed_including_close).
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

/// Inside a paren body like `from = "...", to = "..."` pull out the
/// string literal value for a named argument. Accepts whitespace and
/// newlines around the `=`.
fn extract_string_arg(body: &str, arg_name: &str) -> Option<String> {
    let needle = format!("{} ", arg_name);
    let eq_needle = format!("{}=", arg_name);
    // Locate either `arg_name =` or `arg_name=`.
    let key_pos = body.find(&needle).or_else(|| body.find(&eq_needle))?;
    let after_key = &body[key_pos + arg_name.len()..];
    // Skip whitespace then `=`.
    let after_eq = after_key.trim_start().strip_prefix('=')?.trim_start();
    // Expect a `"` string literal.
    let after_quote = after_eq.strip_prefix('"')?;
    // Read until the next unescaped `"`.
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

// ── Pattern DSL parser ─────────────────────────────────────────────

fn parse_pattern(src: &str) -> Result<Pat, String> {
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
                // Expect `matrix.<func>(args)`.
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

// ── Rust source emission ───────────────────────────────────────────

fn emit_rust(decls: &[RewriteDecl]) -> String {
    let mut out = String::new();
    out.push_str("// @generated by buildscript/fusion_rules.rs from stdlib/matrix.almd\n");
    out.push_str("// Any edits here will be overwritten. Source of truth:\n");
    out.push_str("//   the `@rewrite(from = \"...\", to = \"...\")` attributes in stdlib/matrix.almd\n");
    out.push_str("//\n");
    out.push_str("// This table is the pre-Stage-1 skeleton for the MLIR + egg arc. When\n");
    out.push_str("// `egg::Rewrite` lands, the same decls translate directly; the emitter\n");
    out.push_str("// below is the only piece that needs to change shape.\n");
    out.push_str("\n");
    out.push_str("use crate::pass_matrix_fusion_rules::{FusionRule, Match, Pattern, build_matrix_call, call, cap};\n\n");

    out.push_str("pub fn generated_fusion_rules() -> Vec<FusionRule> {\n    vec![\n");
    for decl in decls {
        out.push_str(&format!("        FusionRule {{\n            name: \"{}\",\n", decl.name));
        out.push_str("            pattern: ");
        emit_pat(&decl.from, &mut out);
        out.push_str(",\n            rewrite: ");
        emit_rewriter(&decl.to, &mut out);
        out.push_str(",\n        },\n");
    }
    out.push_str("    ]\n}\n");
    out
}

fn emit_pat(pat: &Pat, out: &mut String) {
    match pat {
        Pat::Call { func, children } => {
            out.push_str(&format!("call(\"{}\", vec![", func));
            for (i, c) in children.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                emit_pat(c, out);
            }
            out.push_str("])");
        }
        Pat::Capture(name) => {
            out.push_str(&format!("cap(\"{}\")", name));
        }
    }
}

/// A rewriter is encoded as a closure that, given the `Match`, builds
/// the output `IrExprKind` for a `matrix.<func>(capture_args...)` call.
/// Only that shape is currently supported for the RHS — generalising
/// to nested RHS patterns is the next step.
fn emit_rewriter(rhs: &Pat, out: &mut String) {
    let (func, children) = match rhs {
        Pat::Call { func, children } => (func, children),
        Pat::Capture(_) => panic!("rewrite `to` must be a call, got a bare capture"),
    };
    // Confirm every child is a `?cap` — a capture name we pull from
    // the Match. Nested calls in RHS are possible in principle (e.g.
    // algebraic rewrites), but none of the existing fusions need them.
    let caps: Vec<&str> = children
        .iter()
        .map(|c| match c {
            Pat::Capture(n) => n.as_str(),
            Pat::Call { .. } => panic!(
                "rewrite `to` may only use `?cap` as arguments (nested RHS calls are not yet supported)"
            ),
        })
        .collect();
    out.push_str("|m: &Match| build_matrix_call(\"");
    out.push_str(func);
    out.push_str("\", vec![");
    for (i, name) in caps.iter().enumerate() {
        if i > 0 { out.push_str(", "); }
        out.push_str(&format!("m.get(\"{}\")", name));
    }
    out.push_str("])");
}
