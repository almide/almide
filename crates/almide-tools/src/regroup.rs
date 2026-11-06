//! `almide fix`'s dialect-16 migration (#4531): keep what a file meant when
//! `??` stopped binding tighter than every binary operator.
//!
//! The file is read with the PRE-16 grammar (`parser::regroup::parse_legacy`,
//! with the ADR-0005 D6 re-association that grammar's checker applied), the
//! parentheses under which the current grammar reads the same tree are added
//! (`parser::regroup::parenthesize_for_current`), and the tree is printed. The
//! printed text is kept only if the current grammar parses it to exactly the
//! pre-16 tree (parentheses aside) with no `??` left that the two grammars
//! read apart — so the rewrite is the identity on the program's meaning by
//! construction, and anything this cannot prove is refused, never applied.
//!
//! The edit is surgical where it can be: the added parentheses are located
//! by aligning the printed tree with the source's own tokens and spliced
//! into the source text, so the rest of the file keeps its layout. When the
//! alignment fails (a file the formatter would change, a hole inside an
//! interpolated string), the whole file is printed instead. Either text is
//! held to the same proof.

use almide_lang::lexer::Lexer;
use almide_lang::parser::regroup::{grouping_value, parenthesize_for_current, parse_legacy_tokens};
use almide_lang::parser::Parser;

/// What the migration did to one source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Migration {
    /// No `??` in the file reads differently under the current grammar
    /// (or the file does not parse, or is stamped `@dialect(16)` or later).
    Unchanged,
    /// The rewritten text; `sites` is how many `??` the current grammar read
    /// differently, `parens` how many parentheses were added.
    Rewritten { source: String, sites: usize, parens: usize },
    /// A `??` reads differently but the rewrite could not be proven to keep
    /// the old reading. The file is left as it is.
    Refused { reason: String, sites: usize },
}

/// Migrate one source text (see the module docs).
pub fn migrate_precedence(source: &str) -> Migration {
    let tokens = Lexer::tokenize(source);
    let mut parser = Parser::new(tokens.clone());
    let Ok(current) = parser.parse() else { return Migration::Unchanged };
    if !parser.errors.is_empty() || current.regrouped.is_empty() {
        return Migration::Unchanged;
    }
    let sites = current.regrouped.len();
    let Some((mut legacy, fresh)) = parse_legacy_tokens(tokens) else {
        return Migration::Refused { reason: "the file does not parse under the pre-16 grammar".into(), sites };
    };
    let reference = sugar_free(grouping_value(&legacy));
    let plain = crate::fmt::format_program(&legacy);
    let parens = parenthesize_for_current(&mut legacy, fresh);
    let printed = crate::fmt::format_program(&legacy);
    if let Some(spliced) = splice_parens(source, &plain, &printed) {
        if proves(&spliced, &reference).is_ok() {
            return Migration::Rewritten { source: spliced, sites, parens };
        }
    }
    match proves(&printed, &reference) {
        Ok(()) => Migration::Rewritten { source: printed, sites, parens },
        Err(reason) => Migration::Refused { reason, sites },
    }
}

/// `grouping_value` with the type spellings the formatter canonicalizes
/// (`Option[T]` / `T?`, `T!String` / `T!`) made one, as its own verifier
/// does: a reprint may respell a type, never group an operator differently.
fn sugar_free(mut v: serde_json::Value) -> serde_json::Value {
    crate::fmt::normalize_type_sugar(&mut v);
    v
}

/// The rewrite's proof: `text` parses cleanly under the current grammar, to
/// `reference` (the pre-16 tree, parentheses aside), with no `??` the two
/// grammars read apart.
fn proves(text: &str, reference: &serde_json::Value) -> Result<(), String> {
    let mut check = Parser::new(Lexer::tokenize(text));
    let reparsed = match check.parse() {
        Ok(p) if check.errors.is_empty() => p,
        _ => return Err("the rewritten text does not parse".into()),
    };
    if &sugar_free(grouping_value(&reparsed)) != reference {
        return Err("the rewritten text does not read as the original did under the pre-16 grammar".into());
    }
    if !reparsed.regrouped.is_empty() {
        return Err(format!("{} `??` would still read differently after the rewrite", reparsed.regrouped.len()));
    }
    Ok(())
}

/// The tokens that carry the program: no newlines, comments or EOF.
fn significant(source: &str) -> Vec<almide_lang::lexer::Token> {
    use almide_lang::lexer::TokenType;
    Lexer::tokenize(source)
        .into_iter()
        .filter(|t| !matches!(t.token_type, TokenType::Newline | TokenType::Comment | TokenType::EOF))
        .collect()
}

/// Splice the parentheses `parened` has over `plain` (the same tree printed
/// without them) into `source`, which `plain` is a reprint of. `None` when
/// the source's tokens do not line up with the reprint's (commas aside), or
/// a closing parenthesis would follow a token that spans lines.
fn splice_parens(source: &str, plain: &str, parened: &str) -> Option<String> {
    use almide_lang::lexer::TokenType;
    let same = |a: &almide_lang::lexer::Token, b: &almide_lang::lexer::Token| {
        a.token_type == b.token_type && a.value == b.value && a.raw == b.raw
    };
    let s = significant(source);
    let p = significant(plain);
    let q = significant(parened);
    // `plain` index → `source` index. The reprint may respell a few tokens
    // (`Option[T]` as `T?`, a trailing comma, `!x` as `not x`): a mismatch
    // re-synchronises on the nearest pair of positions, a few tokens ahead
    // on each side, where three tokens agree again. The tokens in between
    // stay unmapped, and a parenthesis that would land on one refuses the
    // splice.
    const WINDOW: usize = 8;
    let agree = |i: usize, j: usize| (0..3).all(|k| match (s.get(i + k), p.get(j + k)) {
        (Some(a), Some(b)) => same(a, b),
        (None, None) => true,
        _ => false,
    });
    let mut p_to_s: Vec<Option<usize>> = vec![None; p.len()];
    let (mut i, mut j) = (0, 0);
    while i < s.len() && j < p.len() {
        if same(&s[i], &p[j]) {
            p_to_s[j] = Some(i);
            i += 1;
            j += 1;
            continue;
        }
        let resync = (1..=2 * WINDOW)
            .flat_map(|d| (0..=d).map(move |a| (a, d - a)))
            .filter(|&(a, b)| a <= WINDOW && b <= WINDOW)
            .find(|&(a, b)| agree(i + a, j + b));
        let (a, b) = resync?;
        i += a;
        j += b;
    }
    // Walk `parened` against `plain`: every token `plain` lacks is an added
    // parenthesis, before `plain[j]` (an opener) or after `plain[j - 1]`.
    // (line, col, text), 1-based char columns as the lexer counts them.
    let mut inserts: Vec<(usize, usize, &str)> = Vec::new();
    let mut j = 0;
    for t in &q {
        if p.get(j).is_some_and(|b| same(t, b)) {
            j += 1;
            continue;
        }
        match t.token_type {
            TokenType::LParen => {
                let at = &s[p_to_s.get(j).copied().flatten()?];
                inserts.push((at.line, at.col, "("));
            }
            TokenType::RParen => {
                let before = &s[p_to_s.get(j.checked_sub(1)?).copied().flatten()?];
                let spans_lines = before.raw.as_deref().is_some_and(|r| r.contains('\n')) || before.value.contains('\n');
                if spans_lines {
                    return None;
                }
                inserts.push((before.line, before.end_col, ")"));
            }
            _ => return None,
        }
    }
    if j != p.len() {
        return None;
    }
    let mut lines: Vec<Vec<char>> = source.split('\n').map(|l| l.chars().collect()).collect();
    // Right to left, so an insertion never shifts one still to come. At one
    // position the `(` goes in first, so the `)` inserted after it lands in
    // front of it (`a)(b`); marks of one kind at one position are alike.
    inserts.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)).then((a.2 == ")").cmp(&(b.2 == ")"))));
    for (line, col, text) in inserts {
        let row = lines.get_mut(line.checked_sub(1)?)?;
        let at = col.checked_sub(1)?;
        if at > row.len() {
            return None;
        }
        row.insert(at, text.chars().next()?);
    }
    Some(lines.into_iter().map(|l| l.into_iter().collect::<String>()).collect::<Vec<_>>().join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewritten(src: &str) -> String {
        match migrate_precedence(src) {
            Migration::Rewritten { source, .. } => source,
            other => panic!("expected a rewrite of {src:?}, got {other:?}"),
        }
    }

    fn assert_body(src: &str, expected: &str) {
        let out = rewritten(&format!("fn f(x: Int?, s: Int, m: Map[String, Int]) -> Int = {src}\n"));
        assert!(out.contains(expected), "{src} → {out:?}, wanted {expected:?}");
    }

    #[test]
    fn each_moved_pair_keeps_its_pre_16_reading() {
        assert_body("x ?? 0 - 1", "(x ?? 0) - 1");
        assert_body("map.get(m, \"k\") ?? 0 + 1", "(map.get(m, \"k\") ?? 0) + 1");
        assert_body("s + x ?? 0", "s + (x ?? 0)");
        assert_body("x ?? 1 * s", "(x ?? 1) * s");
        assert_body("s * x ?? 1", "s * (x ?? 1)");
        assert_body("x ?? 2 ^ s", "(x ?? 2) ^ s");
        assert_body("-x ?? 0", "-(x ?? 0)");
    }

    #[test]
    fn a_pipe_stage_fallback_keeps_its_reading() {
        let src = "fn f(xs: List[Int]) -> Int = xs |> list.first ?? 0 |> (n) => n + 1\n";
        let out = rewritten(src);
        assert!(out.contains("(xs |> list.first ?? 0) |> (n) => n + 1"), "{out:?}");
    }

    #[test]
    fn an_unmoved_file_is_left_alone() {
        for src in [
            "fn f(x: Int?) -> Int = x ?? 0\n",
            "fn f(xs: List[Int]) -> Int = xs |> list.first ?? 0\n",
            "fn f(x: Int?, y: Int) -> Bool = x ?? 0 == y\n",
            "fn f(x: Int?) -> Int = (x ?? 0) - 1\n",
            "@dialect(16)\n\nfn f(x: Int?) -> Int = x ?? 0 - 1\n",
        ] {
            assert_eq!(migrate_precedence(src), Migration::Unchanged, "{src}");
        }
    }

    /// The splice keeps the rest of the file as written, including the type
    /// spellings and the layout the formatter would change.
    #[test]
    fn the_rest_of_the_file_keeps_its_spelling() {
        let src = "fn dec(x: Option[Int]) -> Int =\n    x ?? 0 - 1\n\nfn keep(xs: List[Int]) -> Int = list.len(xs)\n";
        assert_eq!(
            rewritten(src),
            "fn dec(x: Option[Int]) -> Int =\n    (x ?? 0) - 1\n\nfn keep(xs: List[Int]) -> Int = list.len(xs)\n"
        );
    }

    #[test]
    fn a_hole_is_migrated_too() {
        let src = "fn f(x: Int?) -> String = \"${x ?? 0 + 1}\"\n";
        let out = rewritten(src);
        assert!(out.contains("${(x ?? 0) + 1}"), "{out:?}");
    }
}
