/// Almide parser: Token stream → AST.
///
/// Input:    Vec<Token>
/// Output:   Program (decls, imports)
/// Owns:     syntax validation, operator precedence, ExprId assignment, depth limiting
/// Does NOT: type checking, name resolution, semantic validation

mod collections;
mod compounds;
mod declarations;
mod diagnostics;
mod entry;
mod fn_decls;
mod expressions;
mod fan;
pub mod hints;
mod helpers;
mod patterns;
mod primary;
mod recovery;
mod statements;
mod test_attributes;
mod test_expr_precedence;
mod test_multiline_tuple;
mod types;

use crate::lexer::{Token, TokenType};
use crate::diagnostic::Diagnostic;
use crate::ast::ExprId;

const MAX_DEPTH: usize = 500;

pub struct Parser {
    pub(crate) tokens: Vec<Token>,
    pub(crate) pos: usize,
    pub errors: Vec<Diagnostic>,
    pub(crate) file: Option<String>,
    pub(crate) next_expr_id: u32,
    pub(crate) depth: usize,
    /// Names of fn declarations whose body failed to parse. Downstream checker
    /// consults this set to suppress cascading "undefined function" diagnostics
    /// so LLMs see the real parse error on top instead of 3× E002 repeats.
    pub failed_fn_names: std::collections::HashSet<String>,
    /// Open `(`/`[` delimiter depth, maintained by `advance()`. Inside a
    /// delimiter the expression continues across newlines; at depth 0 a
    /// newline is a statement boundary — the `??` line-crossing guard (#1112)
    /// keys on this.
    pub(crate) delim_depth: usize,
    /// #1404: comments the token filter removed, keyed by the FILTERED index of
    /// the token that follows them, with the side they attach on. The parser
    /// walks the filtered stream, so `self.pos` is the lookup key.
    pub(crate) inline_comments: std::collections::HashMap<usize, Vec<(String, CommentSide)>>,
    /// Attachments resolved so far, moved into `Program.expr_comments` at the end.
    pub(crate) expr_comments: std::collections::HashMap<ExprId, crate::ast::ExprComments>,
    /// #1326: the comments collected from a continuation gap whose operator
    /// has been SEEN but not yet CONSUMED, keyed by the operator's filtered
    /// index. The Pratt loop that skips the gap may `break` on binding power
    /// and leave the operator to an outer loop; the outer loop is the one
    /// holding the operand the line actually ends with, so it drains this
    /// slot when it takes the operator at that same index. A stale entry (a
    /// speculative parse that backed out) never matches a later index and is
    /// simply overwritten.
    pub(crate) pending_gap: Option<(usize, Vec<GapComment>)>,
}

/// A comment collected from a continuation gap (#1326): the Newline/Comment
/// run between an operand and the infix operator, `|>`, or `.` chain link
/// that continues the expression on a later line.
#[derive(Debug, Clone)]
pub(crate) struct GapComment {
    pub text: String,
    /// A Newline precedes it within the run — it sits on a line of its own
    /// between the operand and the continuation. Otherwise it ends the line
    /// the operand is on.
    pub own_line: bool,
}

/// Which side of a node a removed comment binds to (#1404's ruling).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum CommentSide {
    /// Written before the following node — travels with it.
    Leading,
    /// Written after the preceding node — stays with it, never crosses a
    /// separator onto the next one.
    Trailing,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        let (tokens, inline_comments) = Self::drop_inline_comments(tokens);
        Parser { tokens, pos: 0, inline_comments, expr_comments: std::collections::HashMap::new(), pending_gap: None, errors: Vec::new(), file: None, next_expr_id: 0, depth: 0, failed_fn_names: std::collections::HashSet::new(), delim_depth: 0 }
    }

    /// Drop Comment tokens sitting INLINE mid-expression (`f(1 /* x */, 2)`) so the
    /// grammar never sees them — exactly the positions the old lexer-level skip made
    /// legal (#1318). Kept: own-line comments (preceded by a Newline or file start,
    /// or by another kept comment) and end-of-line comments (followed by Newline/EOF)
    /// — the two positions the comment_map machinery can collect and fmt can reprint.
    /// A dropped comment is still COUNTED by fmt's conservation verifier (it counts
    /// lexer tokens), so an inline comment makes fmt refuse loudly instead of
    /// deleting it silently.
    fn drop_inline_comments(
        tokens: Vec<Token>,
    ) -> (Vec<Token>, std::collections::HashMap<usize, Vec<(String, CommentSide)>>) {
        let mut kept: Vec<Token> = Vec::with_capacity(tokens.len());
        let mut inline: std::collections::HashMap<usize, Vec<(String, CommentSide)>> =
            std::collections::HashMap::new();
        for (i, tok) in tokens.iter().enumerate() {
            if tok.token_type == TokenType::Comment {
                let own_line = matches!(
                    kept.last().map(|t| &t.token_type),
                    None | Some(TokenType::Newline) | Some(TokenType::Comment)
                );
                let end_of_line = matches!(
                    tokens.get(i + 1).map(|t| &t.token_type),
                    None | Some(TokenType::Newline) | Some(TokenType::EOF)
                );
                if !own_line && !end_of_line {
                    // #1404: removed from the stream so the grammar never sees
                    // it, but RECORDED against the position it was written at,
                    // so fmt can put it back. The side follows the ruling: a
                    // comment whose next token CLOSES or SEPARATES something,
                    // or is an INFIX OPERATOR (`1 /* x */ + 2` — nothing an
                    // operator can be a leading comment of), or is another
                    // comment, was written after the node it follows and
                    // stays with it; otherwise it introduces the node that
                    // comes next. `-` is excluded from the operator set: it
                    // may open the next operand (`f(a, /* c */ -1)`).
                    let side = match tokens.get(i + 1).map(|t| &t.token_type) {
                        Some(TokenType::Comma)
                        | Some(TokenType::RParen)
                        | Some(TokenType::RBracket)
                        | Some(TokenType::RBrace)
                        | Some(TokenType::Comment) => CommentSide::Trailing,
                        Some(tt) if *tt != TokenType::Minus && Self::INFIX_TOKENS.contains(tt) => {
                            CommentSide::Trailing
                        }
                        _ => CommentSide::Leading,
                    };
                    inline.entry(kept.len()).or_default().push((tok.value.clone(), side));
                    continue;
                }
            }
            kept.push(tok.clone());
        }
        (kept, inline)
    }

    pub(crate) fn next_id(&mut self) -> ExprId {
        let id = ExprId(self.next_expr_id);
        self.next_expr_id += 1;
        id
    }

    pub fn expr_id_counter(&self) -> u32 { self.next_expr_id }

    pub(crate) fn enter_depth(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err("expression nesting too deep (max 500)".to_string())
        } else {
            Ok(())
        }
    }

    pub(crate) fn exit_depth(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    pub fn new_with_id_offset(tokens: Vec<Token>, id_offset: u32) -> Self {
        let mut p = Self::new(tokens);
        p.next_expr_id = id_offset;
        p
    }

    pub fn with_file(mut self, file: &str) -> Self {
        self.file = Some(file.to_string());
        self
    }
}
