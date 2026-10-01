//! `var (x, y) = p` / `var { a, b } = r` — E091 (#3149).
//!
//! `var` binds one name (GRAMMAR.md: "no destructuring for var"). The pattern
//! form used to die as "Expected identifier … check the token just BEFORE",
//! a hint pointing at a syntax slip that is not there, and every name in the
//! pattern then cascaded into its own E003. This names the rule once, offers
//! both readings (one `var` per name, or `let` when nothing reassigns them),
//! and recovers by binding each name as a `var` of its component, so the rest
//! of the block checks as the author meant it.

use crate::ast::*;
use crate::intern::{sym, Sym};
use crate::lexer::{Token, TokenType};
use super::Parser;

/// One step from the destructured value to a bound name.
#[derive(Clone, Copy)]
enum Access {
    Index(usize),
    Field(Sym),
}

impl Parser {
    /// Is the token after `var` the start of a pattern rather than a name?
    pub(crate) fn at_var_destructure(&self) -> bool {
        matches!(
            self.peek_at(1).map(|t| &t.token_type),
            Some(TokenType::LParen | TokenType::LBrace)
        )
    }

    /// Parse `var <pattern> [: T] = value`, report E091 once, and return the
    /// first recovery binding; the rest wait in `pending_stmts` for the
    /// enclosing block to take.
    pub(crate) fn parse_var_destructure(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        let var_tok = self.current().clone();
        self.expect(TokenType::Var)?;
        // Positioned at the pattern, which is what the rule is about.
        let diag = self.diag_error("`var` does not destructure", "", "var pattern").with_code("E091");
        let pattern = if self.check(TokenType::LParen) {
            self.parse_destructure_tuple()?
        } else {
            self.parse_destructure_record()?
        };
        if self.check(TokenType::Colon) {
            self.advance();
            self.parse_type_expr()?;
        }
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        let last_tok = self.tokens[self.pos.saturating_sub(1)].clone();

        let mut bindings = Vec::new();
        collect_bindings(&pattern, &mut Vec::new(), &mut bindings);
        let simple_base = match &value.kind {
            ExprKind::Ident { name } => Some(*name),
            _ => None,
        };
        let diag = var_destructure_diag(diag, &pattern, &bindings, simple_base, &var_tok, &last_tok);
        self.errors.push(diag);

        let mut stmts = Vec::new();
        let base = match simple_base {
            Some(n) => n,
            None => {
                let tmp = sym(&format!("__var_destructure_{}", value.id.0));
                stmts.push(Stmt::Let { name: tmp, ty: None, value, span: Some(span) });
                tmp
            }
        };
        for (name, path) in bindings {
            let mut e = Expr::new(self.next_id(), Some(span), ExprKind::Ident { name: base });
            for step in path {
                let kind = match step {
                    Access::Index(index) => ExprKind::TupleIndex { object: Box::new(e), index },
                    Access::Field(field) => ExprKind::Member { object: Box::new(e), field },
                };
                e = Expr::new(self.next_id(), Some(span), kind);
            }
            stmts.push(Stmt::Var { name, ty: None, value: e, span: Some(span) });
        }
        if stmts.is_empty() {
            return Ok(Stmt::Error { span: Some(span) });
        }
        let first = stmts.remove(0);
        self.pending_stmts.extend(stmts);
        Ok(first)
    }

    /// `{ a, b }` after `let` / `var`: shorthand only — each name is both the
    /// field label and the local.
    pub(crate) fn parse_destructure_record(&mut self) -> Result<Pattern, String> {
        self.expect(TokenType::LBrace)?;
        let mut names = Vec::new();
        while !self.check(TokenType::RBrace) {
            names.push(self.expect_ident()?);
            if self.check(TokenType::Comma) { self.advance(); self.skip_newlines(); }
        }
        self.expect(TokenType::RBrace)?;
        let fields = names.into_iter().map(|n| FieldPattern { name: n, pattern: None }).collect();
        Ok(Pattern::RecordPattern { name: sym(""), fields, rest: false })
    }
}

/// The hint names the rule with the program's own names, and the fix-its give
/// both readings: one `var` per name (span-exact when the statement is one
/// line over a plain name; a `try:` snippet otherwise) and `var` → `let`.
fn var_destructure_diag(
    diag: crate::diagnostic::Diagnostic,
    pattern: &Pattern,
    bindings: &[(Sym, Vec<Access>)],
    simple_base: Option<Sym>,
    var_tok: &Token,
    last_tok: &Token,
) -> crate::diagnostic::Diagnostic {
    let base_text = simple_base.map(|n| n.to_string()).unwrap_or_else(|| "p".to_string());
    let var_lines: Vec<String> = bindings.iter()
        .map(|(n, path)| format!("var {} = {}", n, access_text(&base_text, path)))
        .collect();
    let mut diag = diag;
    diag.hint = format!(
        "var does not destructure; bind each name (`{}`), or use `let {} = {}` if they need not change",
        var_lines.first().map(String::as_str).unwrap_or("var x = p.0"),
        pattern_text(pattern),
        base_text,
    );
    let one_line = last_tok.line == var_tok.line && var_tok.col > 0 && last_tok.end_col > var_tok.col;
    let diag = if simple_base.is_some() && one_line && !var_lines.is_empty() {
        let indent = " ".repeat(var_tok.col - 1);
        diag.with_suggested_fix(var_tok.line, var_tok.col, last_tok.end_col, var_lines.join(&format!("\n{}", indent)))
    } else {
        let mut lines = Vec::new();
        if simple_base.is_none() {
            lines.push("let p = <value>".to_string());
        }
        lines.extend(var_lines);
        diag.with_try(lines.join("\n"))
    };
    diag.with_alternative(var_tok.line, var_tok.col, var_tok.end_col, "let")
}

fn collect_bindings(pat: &Pattern, path: &mut Vec<Access>, out: &mut Vec<(Sym, Vec<Access>)>) {
    match pat {
        Pattern::Ident { name } => out.push((*name, path.clone())),
        Pattern::Tuple { elements } => {
            for (i, e) in elements.iter().enumerate() {
                path.push(Access::Index(i));
                collect_bindings(e, path, out);
                path.pop();
            }
        }
        Pattern::RecordPattern { fields, .. } => {
            for f in fields {
                path.push(Access::Field(f.name));
                match &f.pattern {
                    Some(p) => collect_bindings(p, path, out),
                    None => out.push((f.name, path.clone())),
                }
                path.pop();
            }
        }
        _ => {}
    }
}

/// `base.0.1` lexes `.0.1` as the float `0.1`, so a projection that follows
/// a tuple index parenthesizes what precedes it: `(base.0).1`.
fn access_text(base: &str, path: &[Access]) -> String {
    let mut s = base.to_string();
    let mut after_index = false;
    for a in path {
        s = match a {
            Access::Index(i) if after_index => format!("({}).{}", s, i),
            Access::Index(i) => format!("{}.{}", s, i),
            Access::Field(f) => format!("{}.{}", s, f),
        };
        after_index = matches!(a, Access::Index(_));
    }
    s
}

fn pattern_text(pat: &Pattern) -> String {
    match pat {
        Pattern::Ident { name } => name.to_string(),
        Pattern::Tuple { elements } => {
            format!("({})", elements.iter().map(pattern_text).collect::<Vec<_>>().join(", "))
        }
        Pattern::RecordPattern { fields, .. } => {
            format!("{{ {} }}", fields.iter().map(|f| f.name.to_string()).collect::<Vec<_>>().join(", "))
        }
        _ => "_".to_string(),
    }
}
