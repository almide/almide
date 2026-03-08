mod program;
mod expressions;
mod calls;
mod blocks;

use crate::ast::*;

pub(crate) const JSON_RUNTIME: &str = include_str!("json_runtime.txt");

pub(crate) struct Emitter {
    pub(crate) out: String,
    pub(crate) indent: usize,
    /// Track if we're inside an effect function (for ? operator)
    pub(crate) in_effect: bool,
    /// Names of effect functions in the program (auto-wrapped, no explicit Result return)
    pub(crate) effect_fns: Vec<String>,
    /// Names of all functions that return Result (for do-block auto-unwrap)
    pub(crate) result_fns: Vec<String>,
    /// Track if we're inside a do block (for auto-unwrap of Result calls)
    pub(crate) in_do_block: std::cell::Cell<bool>,
    /// Names of user-defined modules (for module call dispatch)
    pub(crate) user_modules: Vec<String>,
    /// Track if we're inside a test function
    pub(crate) in_test: bool,
}

impl Emitter {
    fn new() -> Self {
        Self { out: String::new(), indent: 0, in_effect: false, effect_fns: Vec::new(), result_fns: Vec::new(), in_do_block: std::cell::Cell::new(false), user_modules: Vec::new(), in_test: false }
    }

    pub(crate) fn emit_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    pub(crate) fn emitln(&mut self, s: &str) {
        self.emit_indent();
        self.out.push_str(s);
        self.out.push('\n');
    }
}

pub fn emit(program: &Program, modules: &[(String, Program)]) -> String {
    let mut emitter = Emitter::new();
    emitter.emit_program(program, modules);
    emitter.out
}
