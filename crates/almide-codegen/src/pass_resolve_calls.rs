//! Call Resolution pass: verify that every user-module call target resolves
//! to a known IrFunction at compile time, eliminating runtime traps from
//! unresolved symbols.
//!
//! This is Phase 1 of the codegen-ideal-form roadmap. Future phases:
//! - Phase 1b: Normalize module names in CallTarget::Module (alias → canonical)
//! - Phase 1c: Remove emit_stub_call entirely
//!
//! ## What this pass does
//!
//! Walks every `CallTarget::Module { module, func }` in the program.
//! - If `module` is a stdlib/bundled module → skip (emit layer handles these).
//! - If `module` is a user module → verify `func` exists in that module's
//!   function list.
//! - If resolution fails → record an error.
//!
//! The pass does NOT modify the IR (yet). It only verifies.
//!
//! ## Postcondition
//!
//! Every `CallTarget::Module { module, func }` where `module` is a user
//! module (not stdlib) has a matching `IrFunction` in `program.modules`.

use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use super::pass::{NanoPass, PassResult, Postcondition, Target};

#[derive(Debug)]
pub struct ResolveCallsPass;

impl NanoPass for ResolveCallsPass {
    fn name(&self) -> &str { "ResolveCalls" }

    fn targets(&self) -> Option<Vec<Target>> {
        // Applies to all targets. Both Rust and WASM benefit from
        // catching unresolved symbols at compile time.
        None
    }

    fn postconditions(&self) -> Vec<Postcondition> {
        vec![Postcondition::Custom(verify_all_calls_resolved)]
    }

    fn run(&self, program: IrProgram, _target: Target) -> PassResult {
        // This pass is currently verification-only — no IR changes.
        // Future: rewrite CallTarget::Module with canonical module names.
        PassResult { program, changed: false }
    }
}

// ── Verification ────────────────────────────────────────────────────

/// Check that every user-module CallTarget resolves to a known IrFunction.
/// Returns list of unresolved references as diagnostic strings.
fn verify_all_calls_resolved(program: &IrProgram) -> Vec<String> {
    let symbols = SymbolTable::build(program);
    let mut checker = CallChecker {
        symbols: &symbols,
        in_fn: None,
        violations: Vec::new(),
    };

    for func in &program.functions {
        checker.in_fn = Some(func.name.to_string());
        checker.visit_expr(&func.body);
    }
    for (i, tl) in program.top_lets.iter().enumerate() {
        checker.in_fn = Some(format!("top_let[{}]", i));
        checker.visit_expr(&tl.value);
    }
    for module in &program.modules {
        let mod_name = module.name.to_string();
        for func in &module.functions {
            checker.in_fn = Some(format!("{}::{}", mod_name, func.name));
            checker.visit_expr(&func.body);
        }
        for (i, tl) in module.top_lets.iter().enumerate() {
            checker.in_fn = Some(format!("{}::top_let[{}]", mod_name, i));
            checker.visit_expr(&tl.value);
        }
    }

    checker.violations
}

// ── Symbol table ────────────────────────────────────────────────────

struct SymbolTable {
    /// (module_name → set of function names declared in that module)
    user_modules: std::collections::HashMap<String, HashSet<String>>,
}

impl SymbolTable {
    fn build(program: &IrProgram) -> Self {
        let mut user_modules = std::collections::HashMap::new();
        for module in &program.modules {
            let name = module.name.to_string();
            let funcs: HashSet<String> = module.functions.iter()
                .filter(|f| !f.is_test)
                .map(|f| f.name.to_string())
                .collect();
            user_modules.insert(name, funcs);
        }
        Self { user_modules }
    }

    /// Is this a known user module (i.e. we have its IR)?
    fn has_user_module(&self, name: &str) -> bool {
        self.user_modules.contains_key(name)
    }

    /// Does this user module declare this function?
    fn has_func(&self, module: &str, func: &str) -> bool {
        self.user_modules.get(module)
            .map(|fs| fs.contains(func))
            .unwrap_or(false)
    }
}

// ── Walker ──────────────────────────────────────────────────────────

struct CallChecker<'a> {
    symbols: &'a SymbolTable,
    in_fn: Option<String>,
    violations: Vec<String>,
}

impl<'a> IrVisitor for CallChecker<'a> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExprKind::Call { target, .. } = &expr.kind {
            if let CallTarget::Module { module, func } = target {
                let m = module.as_str();
                let f = func.as_str();
                // Stdlib / bundled modules are handled by the emit layer —
                // skip them. We only verify calls into user code.
                let is_stdlib_or_bundled =
                    almide_lang::stdlib_info::is_any_stdlib(m);
                if !is_stdlib_or_bundled && self.symbols.has_user_module(m) {
                    if !self.symbols.has_func(m, f) {
                        self.violations.push(format!(
                            "[ResolveCalls] Unresolved call: {}.{} (in {})",
                            m, f,
                            self.in_fn.as_deref().unwrap_or("<unknown>"),
                        ));
                    }
                }
                // If the module isn't in user_modules AND isn't stdlib, it's
                // likely an alias we haven't normalized yet (Phase 1b territory)
                // — don't complain for now, the emit layer will trap at runtime
                // and we'll address in Phase 1b.
            }
        }
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        walk_stmt(self, stmt);
    }
}
