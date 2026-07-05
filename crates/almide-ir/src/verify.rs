// ── IR integrity verification (post-pass) ───────────────────────
//
// Debug-only pass that catches internal compiler errors before codegen.
// Runs after lowering + optimization, before monomorphization.
//
// Checks:
//   1. VarId bounds — every referenced VarId exists in VarTable
//   2. Mutability — only `var` variables appear in Assign/IndexAssign/FieldAssign
//   3. Loop context — Break/Continue only inside ForIn/While
//   4. Operator–type consistency — BinOp variant matches operand types

use super::*;
use super::visit::{IrVisitor, walk_expr, walk_stmt, walk_pattern};
use almide_lang::types::Ty;

/// An internal compiler error detected by IR verification.
#[derive(Debug)]
pub struct IrVerifyError {
    pub message: String,
    pub fn_name: String,
    pub span: Option<Span>,
}

impl std::fmt::Display for IrVerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IR verify: {} (in {})", self.message, self.fn_name)?;
        if let Some(s) = &self.span {
            write!(f, " at line {}", s.line)?;
        }
        Ok(())
    }
}

struct Verifier<'a> {
    var_table: &'a VarTable,
    fn_name: String,
    in_loop: bool,
    errors: Vec<IrVerifyError>,
    /// Known function names for CallTarget::Named validation
    known_functions: &'a std::collections::HashSet<String>,
    /// Known module→function mappings for CallTarget::Module validation
    known_module_functions: &'a std::collections::HashMap<String, std::collections::HashSet<String>>,
    /// VarIds that have been defined (by Bind, param, pattern, lambda, for-in)
    defined_vars: std::collections::HashSet<u32>,
}

impl<'a> Verifier<'a> {
    fn err(&mut self, message: String, span: Option<Span>) {
        self.errors.push(IrVerifyError {
            message,
            fn_name: self.fn_name.clone(),
            span,
        });
    }

    fn check_var_id(&mut self, id: VarId, span: Option<Span>) {
        if (id.0 as usize) >= self.var_table.len() {
            self.err(
                format!("VarId({}) out of bounds (table size: {})", id.0, self.var_table.len()),
                span,
            );
        }
    }

    fn define_var(&mut self, id: VarId) {
        self.defined_vars.insert(id.0);
    }

    fn check_var_defined(&mut self, id: VarId, span: Option<Span>) {
        // Skip if already out of bounds (reported by check_var_id)
        if (id.0 as usize) >= self.var_table.len() {
            return;
        }
        if !self.defined_vars.contains(&id.0) {
            self.err(
                format!("VarId({}) used but never defined (no Bind/param/pattern)", id.0),
                span,
            );
        }
    }

    // Note: mutability checking is intentionally omitted here.
    // The optimizer's `demote_unused_mut` pass may have already
    // demoted `Var` to `Let` for variables that are assigned but
    // whose assignments were eliminated by DCE. Checking mutability
    // after optimization would produce false positives.
}

impl<'a> IrVisitor for Verifier<'a> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            // ── Variables ──
            IrExprKind::Var { id } => {
                self.check_var_id(*id, expr.span);
                self.check_var_defined(*id, expr.span);
            }

            // ── Operators: check type consistency ──
            IrExprKind::BinOp { op, left, right } => {
                verify_binop_types(*op, left, right, self, expr.span);
            }
            IrExprKind::UnOp { op, operand } => {
                verify_unop_types(*op, operand, self, expr.span);
            }

            // ── Loop context ──
            IrExprKind::Break | IrExprKind::Continue => {
                if !self.in_loop {
                    let kind = if matches!(expr.kind, IrExprKind::Break) { "break" } else { "continue" };
                    self.err(format!("{} outside of loop", kind), expr.span);
                }
            }

            // ── ForIn: check var ids, define vars, then walk with in_loop=true ──
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.check_var_id(*var, expr.span);
                self.define_var(*var);
                if let Some(tuple_vars) = var_tuple {
                    for v in tuple_vars {
                        self.check_var_id(*v, expr.span);
                        self.define_var(*v);
                    }
                }
                let prev = self.in_loop;
                self.in_loop = true;
                walk_expr(self, expr);
                self.in_loop = prev;
                return; // already walked
            }

            // ── While: walk with in_loop=true ──
            IrExprKind::While { .. } => {
                let prev = self.in_loop;
                self.in_loop = true;
                walk_expr(self, expr);
                self.in_loop = prev;
                return;
            }

            // ── Lambda: check param VarIds, define them, before walking body ──
            IrExprKind::Lambda { params, .. } => {
                for (var, _) in params {
                    self.check_var_id(*var, expr.span);
                    self.define_var(*var);
                }
            }

            // ── Call target validation ──
            IrExprKind::Call { target, .. } => {
                match target {
                    CallTarget::Named { name } => {
                        // Named calls include stdlib functions, builtins (println, assert_eq),
                        // constructors, and user functions. Only validate user-defined functions
                        // — skip constructors (uppercase) and anything not in known_functions
                        // (may be stdlib/builtin resolved at codegen time).
                        // This is intentionally lenient to avoid false positives.
                        let is_constructor = name.chars().next().map_or(false, |c| c.is_uppercase());
                        if !is_constructor && self.known_functions.contains::<str>(name) {
                            // Valid: known user function — no error
                        }
                        // else: could be stdlib, builtin, or test function — skip
                    }
                    CallTarget::Module { module, func, .. } => {
                        // Only validate user modules (present in known_module_functions).
                        // Stdlib modules are not in known_module_functions and are handled by codegen.
                        if let Some(funcs) = self.known_module_functions.get::<str>(module) {
                            if !funcs.contains::<str>(func) {
                                self.err(format!("call to unknown function '{}.{}'", module, func), expr.span);
                            }
                        }
                    }
                    // Method and Computed targets are validated structurally (object/callee are walked)
                    _ => {}
                }
            }

            // ── Access: type constraints ──
            IrExprKind::IndexAccess { object, .. } => {
                if !is_unresolved(&object.ty) && object.ty.is_map() {
                    self.err("IndexAccess used on Map type (should be MapAccess)".into(), expr.span);
                }
            }
            IrExprKind::MapAccess { object, .. } => {
                if !is_unresolved(&object.ty) && !object.ty.is_map() {
                    self.err(
                        format!("MapAccess used on non-Map type '{}'", object.ty.display()),
                        expr.span,
                    );
                }
            }

            _ => {}
        }

        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        match &stmt.kind {
            IrStmtKind::Bind { var, .. } => {
                self.check_var_id(*var, stmt.span);
                self.define_var(*var);
            }
            IrStmtKind::Assign { var, .. } => {
                self.check_var_id(*var, stmt.span);
            }
            IrStmtKind::IndexAssign { target, .. } => {
                self.check_var_id(*target, stmt.span);
            }
            IrStmtKind::MapInsert { target, .. } => {
                self.check_var_id(*target, stmt.span);
            }
            IrStmtKind::FieldAssign { target, .. } => {
                self.check_var_id(*target, stmt.span);
            }
            _ => {}
        }

        walk_stmt(self, stmt);
    }

    fn visit_pattern(&mut self, pat: &IrPattern) {
        match pat {
            IrPattern::Bind { var, .. } => {
                self.check_var_id(*var, None);
                self.define_var(*var);
            }
            _ => {}
        }
        walk_pattern(self, pat);
    }
}

/// Verify IR integrity for the main program. Returns errors found.
/// Intended for debug builds — call after optimization, before monomorphization.
pub fn verify_program(program: &IrProgram) -> Vec<IrVerifyError> {
    let mut errors = Vec::new();

    // Build known function sets for CallTarget validation
    let mut known_functions = std::collections::HashSet::new();
    for f in &program.functions {
        known_functions.insert(f.name.to_string());
    }
    for m in &program.modules {
        for f in &m.functions {
            known_functions.insert(f.name.to_string());
        }
    }

    let mut known_module_functions: std::collections::HashMap<String, std::collections::HashSet<String>> = std::collections::HashMap::new();
    for m in &program.modules {
        // Skip bundled stdlib modules: their `module.func` calls intermix
        // bundled fns with TOML-backed runtime fns, and the latter are not in
        // `m.functions`. Codegen handles dispatch — verify must not gate on
        // an incomplete view of the module surface.
        if almide_lang::stdlib_info::is_bundled_module(m.name.as_str()) {
            continue;
        }
        let funcs: std::collections::HashSet<String> = m.functions.iter().map(|f| f.name.to_string()).collect();
        known_module_functions.insert(m.name.to_string(), funcs);
    }

    // Verify type declarations
    verify_type_decls(&program.type_decls, "", &mut errors);

    // Verify main module functions
    for f in &program.functions {
        verify_function(f, &program.var_table, &f.name, &known_functions, &known_module_functions, &mut errors);
    }
    for tl in &program.top_lets {
        let mut v = Verifier {
            var_table: &program.var_table,
            fn_name: "<top-level>".into(),
            in_loop: false,
            errors: Vec::new(),
            known_functions: &known_functions,
            known_module_functions: &known_module_functions,
            defined_vars: (0..program.var_table.len() as u32).collect(),
        };
        v.check_var_id(tl.var, None);
        v.visit_expr(&tl.value);
        errors.append(&mut v.errors);
    }

    // Verify imported modules. All module-scoped VarIds live in
    // `program.var_table` after `UnifyVarTablesPass` merges them, so
    // the verifier reuses the program-level table rather than the
    // module's now-empty one.
    let module_vt: &VarTable = if program.modules.iter().all(|m| m.var_table.entries.is_empty()) {
        &program.var_table
    } else {
        // Pre-unification callers still have per-module tables.
        // Fall through per-module below.
        &program.var_table
    };
    for m in &program.modules {
        verify_type_decls(&m.type_decls, &m.name, &mut errors);
        let vt: &VarTable = if m.var_table.entries.is_empty() { module_vt } else { &m.var_table };
        for f in &m.functions {
            let qual_name = format!("{}.{}", m.name, f.name);
            verify_function(f, vt, &qual_name, &known_functions, &known_module_functions, &mut errors);
        }
        for tl in &m.top_lets {
            let mut v = Verifier {
                var_table: vt,
                fn_name: format!("{}.<top-level>", m.name),
                in_loop: false,
                errors: Vec::new(),
                known_functions: &known_functions,
                known_module_functions: &known_module_functions,
                defined_vars: (0..vt.len() as u32).collect(),
            };
            v.check_var_id(tl.var, None);
            v.visit_expr(&tl.value);
            errors.append(&mut v.errors);
        }
    }

    errors
}

fn verify_function(
    f: &IrFunction,
    var_table: &VarTable,
    name: &str,
    known_functions: &std::collections::HashSet<String>,
    known_module_functions: &std::collections::HashMap<String, std::collections::HashSet<String>>,
    errors: &mut Vec<IrVerifyError>,
) {
    let mut v = Verifier {
        var_table,
        fn_name: name.to_string(),
        in_loop: false,
        errors: Vec::new(),
        known_functions,
        known_module_functions,
        defined_vars: std::collections::HashSet::new(),
    };

    // Pre-populate defined_vars with all VarIds in VarTable.
    // Some vars are introduced implicitly (open record fields, monomorphization)
    // without explicit Bind stmts, so we trust the VarTable as the source of truth.
    for i in 0..var_table.len() {
        v.defined_vars.insert(i as u32);
    }

    // Check parameter VarIds are valid and unique
    let mut seen_param_ids = std::collections::HashSet::new();
    for p in &f.params {
        v.check_var_id(p.var, None);
        if !seen_param_ids.insert(p.var.0) {
            v.err(format!("duplicate parameter VarId({}) for '{}'", p.var.0, p.name), None);
        }
    }

    v.visit_expr(&f.body);
    errors.append(&mut v.errors);
}

fn verify_type_decls(decls: &[IrTypeDecl], module: &str, errors: &mut Vec<IrVerifyError>) {
    for decl in decls {
        let loc = if module.is_empty() { decl.name.to_string() } else { format!("{}.{}", module, decl.name) };
        match &decl.kind {
            IrTypeDeclKind::Record { fields } => {
                let mut seen = std::collections::HashSet::new();
                for f in fields {
                    if !seen.insert(&f.name) {
                        errors.push(IrVerifyError {
                            message: format!("duplicate field '{}' in record type", f.name),
                            fn_name: loc.clone(),
                            span: None,
                        });
                    }
                }
            }
            IrTypeDeclKind::Variant { cases, .. } => {
                let mut seen = std::collections::HashSet::new();
                for c in cases {
                    if !seen.insert(&c.name) {
                        errors.push(IrVerifyError {
                            message: format!("duplicate variant case '{}'", c.name),
                            fn_name: loc.clone(),
                            span: None,
                        });
                    }
                }
            }
            IrTypeDeclKind::Alias { .. } => {}
        }
    }
}

// ── Operator–type consistency ─────────────────────────────────────

/// Check that a BinOp variant is consistent with its operand types.
/// Only flags clear contradictions (e.g., AddInt on String operands).
fn verify_binop_types(op: BinOp, left: &IrExpr, right: &IrExpr, v: &mut Verifier, span: Option<Span>) {
    let lt = &left.ty;
    let rt = &right.ty;

    // Skip if either side is Unknown (error recovery) or TypeVar (generic)
    if is_unresolved(lt) || is_unresolved(rt) { return; }

    let expected = match op {
        BinOp::AddInt | BinOp::SubInt | BinOp::MulInt
        | BinOp::DivInt | BinOp::ModInt | BinOp::PowInt => Some(Ty::Int),
        BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat
        | BinOp::DivFloat | BinOp::PowFloat => Some(Ty::Float),
        BinOp::ConcatStr => Some(Ty::String),
        // ConcatList, Eq, Neq, comparisons, And, Or — operand types vary
        _ => None,
    };

    if let Some(expected_ty) = expected {
        if !ty_matches(lt, &expected_ty) || !ty_matches(rt, &expected_ty) {
            v.err(
                format!(
                    "{:?} expects {} operands, got {} and {}",
                    op, expected_ty.display(), lt.display(), rt.display()
                ),
                span,
            );
        }
    }

    // And/Or require Bool
    if matches!(op, BinOp::And | BinOp::Or) {
        if !ty_matches(lt, &Ty::Bool) || !ty_matches(rt, &Ty::Bool) {
            v.err(
                format!(
                    "{:?} expects Bool operands, got {} and {}",
                    op, lt.display(), rt.display()
                ),
                span,
            );
        }
    }
}

fn verify_unop_types(op: UnOp, operand: &IrExpr, v: &mut Verifier, span: Option<Span>) {
    let t = &operand.ty;
    if is_unresolved(t) { return; }

    let expected = match op {
        UnOp::NegInt => Some(Ty::Int),
        UnOp::NegFloat => Some(Ty::Float),
        UnOp::Not => Some(Ty::Bool),
    };

    if let Some(expected_ty) = expected {
        if !ty_matches(t, &expected_ty) {
            v.err(
                format!(
                    "{:?} expects {} operand, got {}",
                    op, expected_ty.display(), t.display()
                ),
                span,
            );
        }
    }
}

fn is_unresolved(ty: &Ty) -> bool {
    matches!(ty, Ty::Unknown | Ty::TypeVar(_))
}

fn ty_matches(actual: &Ty, expected: &Ty) -> bool {
    if is_unresolved(actual) { return true; }
    // Sized Numeric Types (Stage 1c): every sized integer is accepted
    // wherever `Ty::Int` is expected, and `Ty::Float32` where `Ty::Float`
    // is expected. The BinOp variants in IR (`AddInt`, `AddFloat`, ...)
    // are not width-parameterized; the actual WASM / Rust op is chosen
    // at emit time from the operand's ty.
    if expected == &Ty::Int
        && matches!(
            actual,
            Ty::Int8 | Ty::Int16 | Ty::Int32
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
        )
    {
        return true;
    }
    if expected == &Ty::Float && matches!(actual, Ty::Float32) {
        return true;
    }
    std::mem::discriminant(actual) == std::mem::discriminant(expected)
}

include!("verify_p2.rs");
