//! Call Resolution pass: verify-and-rewrite for every `CallTarget::Module`.
//!
//! Phase 1a (v0.14.5): user-module call verification.
//! Phase 1b (v0.14.7-phase3.5, this revision): IR rewrite + stdlib coverage.
//! Phase 1c (v0.14.7-phase3.2): `emit_stub_call*` panic on reach.
//!
//! ## What this pass does
//!
//! Walks every `CallTarget::Module { module, func }` and either:
//! - **Verifies**: a TOML-backed stdlib fn or a user-module fn exists; if
//!   neither matches, emits a postcondition violation (compile-time ICE
//!   under `ALMIDE_CHECK_IR=1`).
//! - **Rewrites**: a bundled-Almide stdlib fn (`(m, f)` not in TOML, but
//!   present as `IrFunction` inside `program.modules[m]`) → rewrite the
//!   call target to `CallTarget::Named { name: "almide_rt_<m>_<f>" }`,
//!   matching the codegen registration name. After this, every backend's
//!   stdlib dispatcher (Rust `pass_stdlib_lowering`, WASM `emit_call`) only
//!   has to handle TOML-backed modules; bundled fns flow through the same
//!   user-fn call path as any other top-level Named target.
//!
//! ## Why this rewrite, not `CallTarget::Resolved`
//!
//! A new `Resolved` variant would carry strictly more info but would
//! ripple through every IR walker, the WASM emitter's pattern matches,
//! and the optimizer. Reusing `Named` keeps the IR shape stable; the only
//! cost is that `Named` now carries two flavors (top-level user fn + the
//! `almide_rt_<m>_<f>` mangled name for bundled fns), which the
//! generator already handles since v0.14.6 (see the WASM `func_map`
//! registration in `emit_wasm/mod.rs`).
//!
//! ## Postcondition
//!
//! After this pass, every `CallTarget::Module { m, f }` is either
//! TOML-resolvable or it's a violation. Bundled-Almide fns no longer
//! appear as `Module` targets — they're `Named`.

use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut, walk_stmt_mut};
use almide_base::intern::sym;
use super::pass::{NanoPass, PassResult, Postcondition, Target};

#[derive(Debug)]
pub struct ResolveCallsPass;

impl NanoPass for ResolveCallsPass {
    fn name(&self) -> &str { "ResolveCalls" }

    fn targets(&self) -> Option<Vec<Target>> {
        // Applies to all targets. Both Rust and WASM benefit from
        // catching unresolved symbols at compile time AND from the
        // bundled → Named rewrite (eliminates per-target fallback patches).
        None
    }

    fn postconditions(&self) -> Vec<Postcondition> {
        vec![Postcondition::Custom(verify_all_calls_resolved)]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let symbols = SymbolTable::build(&program);

        struct Rewriter<'a> { symbols: &'a SymbolTable }
        impl<'a> IrMutVisitor for Rewriter<'a> {
            fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
                walk_expr_mut(self, expr);
                if let IrExprKind::Call { target, .. } = &mut expr.kind {
                    if let CallTarget::Module { module, func } = target {
                        let m = module.as_str();
                        let f = func.as_str();
                        // bundled-Almide stdlib fn (in IR module, no TOML entry)
                        // → rewrite to Named with the codegen-registered
                        // mangled name. Leaves TOML-backed stdlib calls as
                        // Module so the per-target dispatcher can apply
                        // arg decoration / inline emit.
                        //
                        // User-package modules (external deps loaded from
                        // almide.toml) are NOT rewritten here: they carry a
                        // `versioned_name` that the walker uses as the emit
                        // prefix (`almide_rt_<pkg>_v<major>_<fn>`), and the
                        // later `StdlibLoweringPass::rewrite_module_names`
                        // (Rust) / direct Module dispatch (WASM) handle the
                        // versioned lookup. Rewriting to a non-versioned
                        // `almide_rt_<pkg>_<fn>` here would break the link.
                        let is_stdlib = almide_lang::stdlib_info::is_any_stdlib(m);
                        let bundled_has = self.symbols.module_has_fn(m, f);
                        // `@inline_rust` / `@wasm_intrinsic` bundled fns stay
                        // as `CallTarget::Module` — the per-target lowering
                        // pass intercepts them and emits a template. If we
                        // rewrote them here, pass_stdlib_lowering would never
                        // see the Module target and the template dispatch
                        // would silently fall back to a Named call referring
                        // to a symbol that nobody emits.
                        let has_override = self.symbols.has_codegen_override(m, f);
                        if is_stdlib && bundled_has && !has_override {
                            let mangled = format!(
                                "almide_rt_{}_{}",
                                m.replace('.', "_"),
                                f.replace('.', "_"),
                            );
                            *target = CallTarget::Named { name: sym(&mangled) };
                        }
                    }
                }
            }
            fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
                walk_stmt_mut(self, stmt);
            }
        }
        let mut rw = Rewriter { symbols: &symbols };
        for func in &mut program.functions {
            rw.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
        for mi in 0..program.modules.len() {
            for fi in 0..program.modules[mi].functions.len() {
                let mut body = std::mem::replace(
                    &mut program.modules[mi].functions[fi].body,
                    IrExpr { kind: IrExprKind::Unit, ty: almide_lang::types::Ty::Unit, span: None },
                );
                rw.visit_expr_mut(&mut body);
                program.modules[mi].functions[fi].body = body;
            }
            for ti in 0..program.modules[mi].top_lets.len() {
                let mut val = std::mem::replace(
                    &mut program.modules[mi].top_lets[ti].value,
                    IrExpr { kind: IrExprKind::Unit, ty: almide_lang::types::Ty::Unit, span: None },
                );
                rw.visit_expr_mut(&mut val);
                program.modules[mi].top_lets[ti].value = val;
            }
        }
        PassResult { program, changed: true }
    }
}

// ── Verification ────────────────────────────────────────────────────

/// Check that every CallTarget::Module resolves to either a TOML stdlib fn,
/// a bundled-Almide fn (= IR module fn), or a user-module fn. Returns the
/// list of unresolved references as diagnostic strings.
fn verify_all_calls_resolved(program: &IrProgram) -> Vec<String> {
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};

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
                    let is_stdlib = almide_lang::stdlib_info::is_any_stdlib(m);
                    let resolved = if is_stdlib {
                        // Post-unification: every stdlib module lives in
                        // `stdlib/<m>.almd` as an `@inline_rust`-bundled IR
                        // module. The rewriter above moves plain-bundled fns
                        // to Named; any `Module {stdlib, f}` surviving here
                        // has `@inline_rust` and is resolved by the
                        // per-target lowering pass.
                        self.symbols.module_has_fn(m, f)
                    } else if self.symbols.has_user_module(m) {
                        self.symbols.module_has_fn(m, f)
                    } else {
                        // Unknown module — likely an alias not normalized yet.
                        // Don't complain (matches v0.14.5 Phase 1a behavior).
                        true
                    };
                    if !resolved {
                        self.violations.push(format!(
                            "[ResolveCalls] Unresolved call: {}.{} (in {})",
                            m, f,
                            self.in_fn.as_deref().unwrap_or("<unknown>"),
                        ));
                    }
                }
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            walk_stmt(self, stmt);
        }
    }

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
    /// Subset of the above: `(module, fn)` pairs whose IrFunction
    /// carries an `@inline_rust` or `@wasm_intrinsic` attribute. These
    /// are dispatch-only declarations (Stdlib Declarative Unification
    /// Stage 2+): their bodies are never emitted, and call sites must
    /// stay as `CallTarget::Module` so the per-target lowering
    /// (`StdlibLoweringPass` on Rust, `emit_int_call` etc. on WASM)
    /// can intercept them with the right semantics. Rewriting them to
    /// `CallTarget::Named { almide_rt_<m>_<f> }` would bypass the
    /// template-substitution path entirely and route calls to a symbol
    /// that `pass_stdlib_lowering` refuses to generate (because the
    /// bundled fn's body is skipped at emit time).
    codegen_override: HashSet<(String, String)>,
}

impl SymbolTable {
    fn build(program: &IrProgram) -> Self {
        let mut user_modules = std::collections::HashMap::new();
        let mut codegen_override: HashSet<(String, String)> = HashSet::new();
        for module in &program.modules {
            let name = module.name.to_string();
            let funcs: HashSet<String> = module.functions.iter()
                .filter(|f| !f.is_test)
                .map(|f| f.name.to_string())
                .collect();
            for f in &module.functions {
                if f.attrs.iter().any(|a|
                    matches!(a.name.as_str(), "inline_rust" | "wasm_intrinsic"))
                {
                    codegen_override.insert((name.clone(), f.name.to_string()));
                }
            }
            user_modules.insert(name, funcs);
        }
        Self { user_modules, codegen_override }
    }

    fn has_user_module(&self, name: &str) -> bool {
        self.user_modules.contains_key(name)
    }

    fn module_has_fn(&self, module: &str, func: &str) -> bool {
        self.user_modules.get(module)
            .map(|fs| fs.contains(func))
            .unwrap_or(false)
    }

    fn has_codegen_override(&self, module: &str, func: &str) -> bool {
        self.codegen_override.contains(&(module.to_string(), func.to_string()))
    }
}
