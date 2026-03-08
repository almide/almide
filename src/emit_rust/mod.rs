mod program;
mod expressions;
mod calls;
mod blocks;

use crate::ast::*;

pub(crate) const JSON_RUNTIME: &str = include_str!("json_runtime.txt");
pub(crate) const HTTP_RUNTIME: &str = include_str!("http_runtime.txt");
pub(crate) const TIME_RUNTIME: &str = include_str!("time_runtime.txt");
pub(crate) const REGEX_RUNTIME: &str = include_str!("regex_runtime.txt");

pub struct EmitOptions {
    /// Skip thread wrapper around main (for WASM targets where threads are unavailable)
    pub no_thread_wrap: bool,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self { no_thread_wrap: false }
    }
}

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
    /// Skip thread wrapper around main
    pub(crate) no_thread_wrap: bool,
    /// Maps import name to versioned mod name for diamond deps
    pub(crate) module_aliases: std::collections::HashMap<String, String>,
}

impl Emitter {
    fn new(options: &EmitOptions) -> Self {
        Self { out: String::new(), indent: 0, in_effect: false, effect_fns: Vec::new(), result_fns: Vec::new(), in_do_block: std::cell::Cell::new(false), user_modules: Vec::new(), in_test: false, no_thread_wrap: options.no_thread_wrap, module_aliases: std::collections::HashMap::new() }
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

pub fn emit(program: &Program, modules: &[(String, Program, Option<crate::project::PkgId>)]) -> String {
    emit_with_options(program, modules, &EmitOptions::default())
}

pub fn emit_with_options(program: &Program, modules: &[(String, Program, Option<crate::project::PkgId>)], options: &EmitOptions) -> String {
    let mut emitter = Emitter::new(options);
    emitter.emit_program(program, modules);
    emitter.out
}
