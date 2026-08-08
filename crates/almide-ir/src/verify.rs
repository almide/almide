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

/// The call-target surface a `Verifier` validates against, computed once per
/// program and shared by every function-, module- and top-let-level verifier.
struct KnownNames {
    /// Known function names for CallTarget::Named validation
    functions: std::collections::HashSet<String>,
    /// Known module→function mappings for CallTarget::Module validation
    module_functions: std::collections::HashMap<String, std::collections::HashSet<String>>,
}

struct Verifier<'a> {
    var_table: &'a VarTable,
    fn_name: String,
    in_loop: bool,
    errors: Vec<IrVerifyError>,
    known: &'a KnownNames,
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

impl<'a> Verifier<'a> {
    /// `Break`/`Continue`: legal only inside a loop body.
    fn check_loop_context(&mut self, expr: &IrExpr) {
        if !self.in_loop {
            let kind = if matches!(expr.kind, IrExprKind::Break) { "break" } else { "continue" };
            self.err(format!("{} outside of loop", kind), expr.span);
        }
    }

    /// Walk a `ForIn`/`While` node with `in_loop` set, restoring it after —
    /// so `Break`/`Continue` anywhere in the body verifies, and one after the
    /// loop does not.
    fn walk_in_loop(&mut self, expr: &IrExpr) {
        let prev = self.in_loop;
        self.in_loop = true;
        walk_expr(self, expr);
        self.in_loop = prev;
    }

    /// `ForIn` binders: the loop var, plus each element of a destructured tuple.
    fn define_loop_vars(&mut self, var: VarId, var_tuple: Option<&Vec<VarId>>, span: Option<Span>) {
        self.check_var_id(var, span);
        self.define_var(var);
        for v in var_tuple.into_iter().flatten() {
            self.check_var_id(*v, span);
            self.define_var(*v);
        }
    }

    /// `Lambda` binders: every parameter is in scope for the body.
    fn define_lambda_params(&mut self, params: &[(VarId, Ty)], span: Option<Span>) {
        for (var, _) in params {
            self.check_var_id(*var, span);
            self.define_var(*var);
        }
    }

    /// `Call` target validation.
    ///
    /// `Named` is intentionally NOT gated: it covers stdlib functions,
    /// builtins (println, assert_eq), constructors and user functions alike,
    /// and several of those are only resolved at codegen time — gating here
    /// would be a false-positive factory. `Module` is gated only for modules
    /// present in `known.module_functions` (stdlib modules are absent and
    /// handled by codegen). `Method`/`Computed` are validated structurally,
    /// by walking their object/callee.
    fn check_call_target(&mut self, target: &CallTarget, span: Option<Span>) {
        let CallTarget::Module { module, func, .. } = target else { return };
        let absent = self
            .known
            .module_functions
            .get::<str>(module)
            .is_some_and(|funcs| !funcs.contains::<str>(func));
        if absent {
            self.err(format!("call to unknown function '{}.{}'", module, func), span);
        }
    }

    /// `IndexAccess`/`MapAccess` must agree with the object's type: list
    /// indexing on a Map (or map lookup on a non-Map) means an earlier pass
    /// picked the wrong node.
    fn check_indexing(&mut self, object: &IrExpr, want_map: bool, span: Option<Span>) {
        if is_unresolved(&object.ty) || object.ty.is_map() == want_map {
            return;
        }
        let message = if want_map {
            format!("MapAccess used on non-Map type '{}'", object.ty.display())
        } else {
            "IndexAccess used on Map type (should be MapAccess)".to_string()
        };
        self.err(message, span);
    }
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
            IrExprKind::Break | IrExprKind::Continue => self.check_loop_context(expr),

            // ── Loops own their walk: the body must see `in_loop` ──
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.define_loop_vars(*var, var_tuple.as_ref(), expr.span);
                self.walk_in_loop(expr);
                return; // already walked
            }
            IrExprKind::While { .. } => {
                self.walk_in_loop(expr);
                return; // already walked
            }

            // ── Lambda: check param VarIds, define them, before walking body ──
            IrExprKind::Lambda { params, .. } => self.define_lambda_params(params, expr.span),

            // ── Call target validation ──
            IrExprKind::Call { target, .. } => self.check_call_target(target, expr.span),

            // ── Access: type constraints ──
            IrExprKind::IndexAccess { object, .. } => self.check_indexing(object, false, expr.span),
            IrExprKind::MapAccess { object, .. } => self.check_indexing(object, true, expr.span),

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
    let known = KnownNames {
        functions: collect_known_functions(program),
        module_functions: collect_known_module_functions(program),
    };

    verify_type_decls(&program.type_decls, "", &mut errors);
    for f in &program.functions {
        verify_function(f, &program.var_table, &f.name, &known, &mut errors);
    }
    for tl in &program.top_lets {
        verify_top_let(tl, &program.var_table, "<top-level>".into(), &known, &mut errors);
    }

    // Verify imported modules. All module-scoped VarIds live in
    // `program.var_table` after `UnifyVarTablesPass` merges them, so
    // the verifier reuses the program-level table rather than the
    // module's now-empty one. Pre-unification callers still have
    // per-module tables and fall through to them below.
    for m in &program.modules {
        verify_module(m, &program.var_table, &known, &mut errors);
    }

    errors
}

/// Every free-function name reachable in the program — the main program's own
/// functions plus every module's.
fn collect_known_functions(program: &IrProgram) -> std::collections::HashSet<String> {
    let mut known = std::collections::HashSet::new();
    for f in &program.functions {
        known.insert(f.name.to_string());
    }
    for m in &program.modules {
        for f in &m.functions {
            known.insert(f.name.to_string());
        }
    }
    known
}

/// module name → the function names a `module.func` call may legally name.
///
/// Bundled stdlib modules are skipped entirely: their `module.func` calls
/// intermix bundled fns with TOML-backed runtime fns, and the latter are not
/// in `m.functions`. Codegen handles dispatch — verify must not gate on an
/// incomplete view of the module surface.
fn collect_known_module_functions(
    program: &IrProgram,
) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    let mut known = std::collections::HashMap::new();
    for m in &program.modules {
        if almide_lang::stdlib_info::is_bundled_module(m.name.as_str()) {
            continue;
        }
        // A duplicate module NAME must not silently overwrite: two IrModules
        // sharing a name (a dependency's root and a sibling, #884) would leave
        // the map holding whichever came last, and every call into the other
        // would report unknown. Merge instead — the union is the honest
        // surface, and the gate still catches a genuinely absent name.
        known
            .entry(m.name.to_string())
            .or_insert_with(std::collections::HashSet::new)
            .extend(module_call_surface(m));
    }
    known
}

/// The names a call into `m` may legally spell: its surviving functions, the
/// generic BASE name behind every monomorphized instance, and its declared
/// exports.
///
/// MONOMORPHIZATION renames a generic module fn to its specialized instances
/// (`get` → `get__Int`, …) and the ORIGINAL disappears from `m.functions`,
/// while a call site that mono did not rewrite still names the generic (#884:
/// `ceangal.cell.get` reported unknown while the non-generic `is_dirty` in the
/// same module verified fine — and only when the module was reached through a
/// consumer's import graph, which is what made it look import-order dependent).
/// An instance is `<base>__<suffix>`, so the base is what a not-yet-rewritten
/// call spells.
///
/// The EXPORTS are the module's DECLARED surface, captured at lowering — before
/// monomorphization, which drops a generic fn that no reachable call
/// instantiated. A dependency module linked but never reached from the
/// consumer's entry still has its body verified, and its calls into a sibling's
/// generic fn then named something mono had removed. The declared surface is
/// the right question for "does this function exist".
fn module_call_surface(m: &IrModule) -> std::collections::HashSet<String> {
    let mut funcs: std::collections::HashSet<String> =
        m.functions.iter().map(|f| f.name.to_string()).collect();
    let bases: Vec<String> = funcs
        .iter()
        .filter_map(|n| n.split_once("__").map(|(b, _)| b.to_string()))
        .filter(|b| !b.is_empty())
        .collect();
    funcs.extend(bases);
    funcs.extend(m.exports.iter().filter_map(|e| match e {
        crate::IrExport::Function { name, .. } => Some(name.to_string()),
        _ => None,
    }));
    funcs
}

/// Verify one module's type decls, functions and top-level lets. `program_vt`
/// is the merged program table used when the module's own table is empty
/// (post-`UnifyVarTablesPass`).
fn verify_module(
    m: &IrModule,
    program_vt: &VarTable,
    known: &KnownNames,
    errors: &mut Vec<IrVerifyError>,
) {
    verify_type_decls(&m.type_decls, &m.name, errors);
    let vt: &VarTable = if m.var_table.entries.is_empty() { program_vt } else { &m.var_table };
    for f in &m.functions {
        let qual_name = format!("{}.{}", m.name, f.name);
        verify_function(f, vt, &qual_name, known, errors);
    }
    for tl in &m.top_lets {
        verify_top_let(tl, vt, format!("{}.<top-level>", m.name), known, errors);
    }
}

/// Verify one top-level let's binder and value. Every VarId in `var_table` is
/// pre-defined: a top-let value may reference any other top-let regardless of
/// declaration order.
fn verify_top_let(
    tl: &IrTopLet,
    var_table: &VarTable,
    fn_name: String,
    known: &KnownNames,
    errors: &mut Vec<IrVerifyError>,
) {
    let mut v = Verifier {
        var_table,
        fn_name,
        in_loop: false,
        errors: Vec::new(),
        known,
        defined_vars: (0..var_table.len() as u32).collect(),
    };
    v.check_var_id(tl.var, None);
    v.visit_expr(&tl.value);
    errors.append(&mut v.errors);
}

fn verify_function(
    f: &IrFunction,
    var_table: &VarTable,
    name: &str,
    known: &KnownNames,
    errors: &mut Vec<IrVerifyError>,
) {
    let mut v = Verifier {
        var_table,
        fn_name: name.to_string(),
        in_loop: false,
        errors: Vec::new(),
        known,
        // Pre-populate defined_vars with all VarIds in VarTable.
        // Some vars are introduced implicitly (open record fields, monomorphization)
        // without explicit Bind stmts, so we trust the VarTable as the source of truth.
        defined_vars: (0..var_table.len() as u32).collect(),
    };

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
    if matches!(op, BinOp::And | BinOp::Or)
        && (!ty_matches(lt, &Ty::Bool) || !ty_matches(rt, &Ty::Bool))
    {
        v.err(
            format!(
                "{:?} expects Bool operands, got {} and {}",
                op, lt.display(), rt.display()
            ),
            span,
        );
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

include!("verify_tests.rs");
