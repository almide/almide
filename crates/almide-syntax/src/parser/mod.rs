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
mod expressions;
pub mod hints;
mod helpers;
mod patterns;
mod primary;
mod recovery;
mod statements;
mod test_expr_precedence;
mod types;

use crate::lexer::Token;
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
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0, errors: Vec::new(), file: None, next_expr_id: 0, depth: 0, failed_fn_names: std::collections::HashSet::new() }
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
