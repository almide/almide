mod declarations;
mod expressions;
mod blocks;

use std::cell::RefCell;
use std::collections::HashSet;
use crate::ast::*;

pub(crate) struct TsEmitter {
    pub(crate) out: String,
    pub(crate) js_mode: bool,
    pub(crate) user_modules: Vec<String>,
    /// Tracks which stdlib modules (`__almd_*`) are referenced during codegen.
    pub(crate) used_stdlib: RefCell<HashSet<String>>,
}

impl TsEmitter {
    fn new() -> Self {
        Self {
            out: String::new(),
            js_mode: false,
            user_modules: Vec::new(),
            used_stdlib: RefCell::new(HashSet::new()),
        }
    }

    // Helpers

    pub(crate) fn needs_iife(expr: &Expr) -> bool {
        matches!(expr, Expr::Block { .. } | Expr::DoBlock { .. })
    }

    pub(crate) fn is_unit(expr: &Expr) -> bool {
        match expr {
            Expr::Unit { .. } => true,
            Expr::Ok { expr, .. } | Expr::Some { expr, .. } => matches!(expr.as_ref(), Expr::Unit { .. }),
            _ => false,
        }
    }

    pub(crate) fn sanitize(name: &str) -> String {
        crate::emit_common::sanitize(name)
    }

    pub(crate) fn map_module(&self, name: &str) -> String {
        // User modules take priority over stdlib
        if self.user_modules.contains(&name.to_string()) {
            name.to_string()
        } else if crate::stdlib::is_stdlib_module(name) {
            self.used_stdlib.borrow_mut().insert(name.to_string());
            format!("__almd_{}", name)
        } else {
            name.to_string()
        }
    }

    pub(crate) fn json_string(s: &str) -> String {
        serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s))
    }

    pub(crate) fn pascal_to_message(name: &str) -> String {
        let mut result = String::new();
        for (i, c) in name.chars().enumerate() {
            if i > 0 && c.is_uppercase() {
                result.push(' ');
                result.push(c.to_lowercase().next().unwrap_or(c));
            } else if i == 0 {
                result.push(c.to_uppercase().next().unwrap_or(c));
            } else {
                result.push(c);
            }
        }
        result
    }
}

pub fn emit_with_modules(program: &Program, modules: &[(String, Program)]) -> String {
    let mut emitter = TsEmitter::new();
    emitter.emit_program(program, modules);
    emitter.out
}

pub fn emit_js_with_modules(program: &Program, modules: &[(String, Program)]) -> String {
    let mut emitter = TsEmitter::new();
    emitter.js_mode = true;
    emitter.emit_program(program, modules);
    emitter.out
}
