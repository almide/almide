mod declarations;
mod expressions;
mod blocks;

use crate::ast::*;

pub(crate) struct TsEmitter {
    pub(crate) out: String,
    pub(crate) js_mode: bool,
}

impl TsEmitter {
    fn new() -> Self {
        Self { out: String::new(), js_mode: false }
    }

    // Helpers

    pub(crate) fn needs_iife(expr: &Expr) -> bool {
        matches!(expr, Expr::Block { .. } | Expr::DoBlock { .. })
    }

    pub(crate) fn is_unit(expr: &Expr) -> bool {
        match expr {
            Expr::Unit => true,
            Expr::Ok { expr } | Expr::Some { expr } => matches!(expr.as_ref(), Expr::Unit),
            _ => false,
        }
    }

    pub(crate) fn sanitize(name: &str) -> String {
        name.replace('?', "_qm_")
    }

    pub(crate) fn map_module(name: &str) -> String {
        if crate::stdlib::is_stdlib_module(name) {
            format!("__{}", name)
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
                result.push(c.to_lowercase().next().unwrap());
            } else if i == 0 {
                result.push(c.to_uppercase().next().unwrap());
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
