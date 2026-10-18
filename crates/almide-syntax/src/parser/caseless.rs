//! Caseless type-side names (#1456).
//!
//! Almide splits names by case: an uppercase start is a type-side name (a
//! type, a variant case, a constructor), anything else is a value-side name.
//! Scripts without case — CJK, kana, Hangul, Thai, Arabic, Hebrew, ... —
//! have no uppercase, so the lexer leaves every caseless name on the value
//! side. A caseless name becomes type-side by DECLARATION instead: when this
//! module writes `type 住所 = …` or a variant case `| 赤`, every token spelled
//! `住所` / `赤` in the module is reclassified `TypeName`, and from there on it
//! takes exactly the paths an ASCII capital takes (record literal, constructor
//! pattern in `match`, type position, convention method head).
//!
//! The consequence mirrors ASCII: `let Red = 1` is not a binding, and neither
//! is `let 赤 = 1` in a module that declares the case `赤`.

use std::collections::HashSet;

use crate::lexer::{Token, TokenType};

pub use almide_base::names::is_caseless_name;

fn is_trivia(t: &Token) -> bool {
    matches!(t.token_type, TokenType::Newline | TokenType::Comment)
}

/// The caseless names this token stream declares type-side: the name after
/// `type` / `protocol`, and every caseless name that heads a variant case or an
/// alias target (directly after the declaration's `=` or a `|`, outside any
/// bracket).
pub fn declared_caseless_type_names(tokens: &[Token]) -> HashSet<String> {
    let mut names = HashSet::new();
    let next_sig = |from: usize| (from..tokens.len()).find(|&j| !is_trivia(&tokens[j]));
    let mut i = 0;
    while i < tokens.len() {
        let kind = tokens[i].token_type;
        if !matches!(kind, TokenType::Type | TokenType::Protocol) {
            i += 1;
            continue;
        }
        let Some(n) = next_sig(i + 1) else { break };
        if tokens[n].token_type == TokenType::Ident && is_caseless_name(&tokens[n].value) {
            names.insert(tokens[n].value.clone());
        }
        i = n + 1;
        if kind == TokenType::Protocol {
            continue;
        }
        i = scan_type_body(tokens, i, &mut names);
    }
    names
}

/// Walk one `type` declaration's body from just after its name, collecting
/// caseless case heads; returns the index where the declaration ends.
fn scan_type_body(tokens: &[Token], mut i: usize, names: &mut HashSet<String>) -> usize {
    let mut depth = 0usize;
    let mut seen_eq = false;
    // The last significant token at depth 0, so a case head is recognised and
    // a line break after `=` / `|` does not end the declaration.
    let mut prev: Option<TokenType> = None;
    while i < tokens.len() {
        let t = &tokens[i];
        match t.token_type {
            TokenType::LParen | TokenType::LBracket | TokenType::LBrace => depth += 1,
            TokenType::RParen | TokenType::RBracket | TokenType::RBrace => depth = depth.saturating_sub(1),
            TokenType::EOF => return i,
            TokenType::Newline if depth == 0 && seen_eq => {
                let continues = matches!(prev, Some(TokenType::Eq | TokenType::Pipe))
                    || tokens[i + 1..]
                        .iter()
                        .find(|t| !is_trivia(t))
                        .is_some_and(|t| t.token_type == TokenType::Pipe);
                if !continues {
                    return i;
                }
            }
            _ => {}
        }
        if is_trivia(t) {
            i += 1;
            continue;
        }
        if depth == 0 {
            if t.token_type == TokenType::Eq {
                seen_eq = true;
            } else if t.token_type == TokenType::Ident
                && seen_eq
                && matches!(prev, Some(TokenType::Eq | TokenType::Pipe))
                && is_caseless_name(&t.value)
            {
                names.insert(t.value.clone());
            }
            prev = Some(t.token_type);
        }
        i += 1;
    }
    i
}

/// The hint for a name position that met a token #1456 explains: a caseless
/// name this module declares type-side, or a compatibility character.
pub fn name_position_hint(tok: &Token) -> Option<String> {
    match tok.token_type {
        TokenType::TypeName if is_caseless_name(&tok.value) => Some(format!(
            "'{}' is declared in this module as a type or a variant case. A caseless name \
             has no capital letter to tell the two apart, so once declared type-side it \
             cannot also name a binding, parameter or function. Pick a different name.",
            tok.value
        )),
        TokenType::Unknown => tok.value.chars().next().and_then(compatibility_hint),
        _ => None,
    }
}

/// `ａ` (U+FF41) and its kin: a character whose compatibility normal form
/// (NFKC) is different text. Identifiers refuse it rather than fold it, so
/// the hint names the plain spelling to write instead.
pub fn compatibility_hint(ch: char) -> Option<String> {
    use unicode_normalization::UnicodeNormalization;
    let plain: String = std::iter::once(ch).nfkc().collect();
    let is_name_text = plain.chars().all(crate::lexer::is_ident_continue_char);
    (plain != ch.to_string() && !plain.is_empty() && is_name_text).then(|| {
        format!(
            "'{}' (U+{:04X}) is a compatibility form of '{}' (full-width, ligature, circled or \
             styled). A name takes the plain character: write '{}'.",
            ch, ch as u32, plain, plain
        )
    })
}

/// Reclassify every `Ident` token spelled as a declared caseless type-side name.
pub fn reclassify(tokens: &mut [Token], names: &HashSet<String>) {
    if names.is_empty() {
        return;
    }
    for t in tokens.iter_mut() {
        if t.token_type == TokenType::Ident && names.contains(&t.value) {
            t.token_type = TokenType::TypeName;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn declared(src: &str) -> Vec<String> {
        let mut v: Vec<String> = declared_caseless_type_names(&Lexer::tokenize(src)).into_iter().collect();
        v.sort();
        v
    }

    #[test]
    fn type_names_and_case_heads_are_collected() {
        assert_eq!(declared("type 住所 = { 都市: String }"), vec!["住所"]);
        assert_eq!(declared("type 色 = | 赤 | 緑(Int)"), vec!["緑", "色", "赤"]);
        assert_eq!(declared("type 色 =\n  | 赤\n  | 青 { 値: Int }\nfn f() -> Int = 1"), vec!["色", "赤", "青"]);
        assert_eq!(declared("type 色 = 赤 | 緑"), vec!["緑", "色", "赤"]);
        assert_eq!(declared("protocol 比較 {\n}"), vec!["比較"]);
    }

    #[test]
    fn value_side_names_are_not_collected() {
        // Field names, payload names, later declarations' bodies.
        assert_eq!(declared("type 住所 = { 都市: String }\nlet 値 = 1"), vec!["住所"]);
        assert_eq!(declared("type T = | A(名前) \nlet x = 値"), Vec::<String>::new());
        assert_eq!(declared("type T = { x: Int }\nfn f() -> Int = 値"), Vec::<String>::new());
        assert!(!is_caseless_name("_名"));
        assert!(!is_caseless_name("abc"));
        assert!(!is_caseless_name("Δ"));
    }
}
