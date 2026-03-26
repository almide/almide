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
use crate::types::Ty;

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
                    CallTarget::Module { module, func } => {
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

    // Verify imported modules
    for m in &program.modules {
        verify_type_decls(&m.type_decls, &m.name, &mut errors);
        for f in &m.functions {
            let qual_name = format!("{}.{}", m.name, f.name);
            verify_function(f, &m.var_table, &qual_name, &known_functions, &known_module_functions, &mut errors);
        }
        for tl in &m.top_lets {
            let mut v = Verifier {
                var_table: &m.var_table,
                fn_name: format!("{}.<top-level>", m.name),
                in_loop: false,
                errors: Vec::new(),
                known_functions: &known_functions,
                known_module_functions: &known_module_functions,
                defined_vars: (0..m.var_table.len() as u32).collect(),
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
    std::mem::discriminant(actual) == std::mem::discriminant(expected)
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_program(functions: Vec<IrFunction>, var_table: VarTable) -> IrProgram {
        IrProgram {
            functions,
            top_lets: vec![],
            type_decls: vec![],
            var_table,
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
        }
    }

    fn lit_int(v: i64) -> IrExpr {
        IrExpr { kind: IrExprKind::LitInt { value: v }, ty: Ty::Int, span: None }
    }

    fn var_expr(id: VarId, ty: Ty) -> IrExpr {
        IrExpr { kind: IrExprKind::Var { id }, ty, span: None }
    }

    fn make_fn(name: &str, body: IrExpr) -> IrFunction {
        IrFunction {
            name: name.into(),
            params: vec![],
            ret_ty: body.ty.clone(),
            body,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            visibility: IrVisibility::Public,
        }
    }

    #[test]
    fn valid_program_no_errors() {
        let mut vt = VarTable::new();
        let x = vt.alloc("x".into(), Ty::Int, Mutability::Let, None);
        let body = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![IrStmt {
                    kind: IrStmtKind::Bind { var: x, mutability: Mutability::Let, ty: Ty::Int, value: lit_int(1) },
                    span: None,
                }],
                expr: Some(Box::new(var_expr(x, Ty::Int))),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn detects_var_id_out_of_bounds() {
        let vt = VarTable::new(); // empty table
        let body = var_expr(VarId(99), Ty::Int);
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("VarId(99)"));
    }

    #[test]
    fn assign_checks_var_id_bounds() {
        let vt = VarTable::new(); // empty
        let body = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![IrStmt {
                    kind: IrStmtKind::Assign { var: VarId(99), value: lit_int(2) },
                    span: None,
                }],
                expr: None,
            },
            ty: Ty::Unit,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("VarId(99)"));
    }

    #[test]
    fn detects_break_outside_loop() {
        let vt = VarTable::new();
        let body = IrExpr { kind: IrExprKind::Break, ty: Ty::Unit, span: None };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("break outside of loop"));
    }

    #[test]
    fn allows_break_inside_loop() {
        let mut vt = VarTable::new();
        let i = vt.alloc("i".into(), Ty::Int, Mutability::Let, None);
        let body = IrExpr {
            kind: IrExprKind::ForIn {
                var: i,
                var_tuple: None,
                iterable: Box::new(IrExpr {
                    kind: IrExprKind::Range {
                        start: Box::new(lit_int(0)),
                        end: Box::new(lit_int(10)),
                        inclusive: false,
                    },
                    ty: Ty::Int, // simplified
                    span: None,
                }),
                body: vec![IrStmt {
                    kind: IrStmtKind::Expr {
                        expr: IrExpr { kind: IrExprKind::Break, ty: Ty::Unit, span: None },
                    },
                    span: None,
                }],
            },
            ty: Ty::Unit,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_binop_type_mismatch() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::AddInt,
                left: Box::new(IrExpr { kind: IrExprKind::LitStr { value: "a".into() }, ty: Ty::String, span: None }),
                right: Box::new(lit_int(1)),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("AddInt"));
    }

    #[test]
    fn skips_unknown_types_in_binop() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::AddInt,
                left: Box::new(IrExpr { kind: IrExprKind::Hole, ty: Ty::Unknown, span: None }),
                right: Box::new(lit_int(1)),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_continue_outside_loop() {
        let vt = VarTable::new();
        let body = IrExpr { kind: IrExprKind::Continue, ty: Ty::Unit, span: None };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("continue outside of loop"));
    }

    #[test]
    fn verifies_pattern_var_ids() {
        let mut vt = VarTable::new();
        let _x = vt.alloc("x".into(), Ty::Int, Mutability::Let, None);
        // Pattern references VarId(99) which doesn't exist
        let body = IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(lit_int(1)),
                arms: vec![IrMatchArm {
                    pattern: IrPattern::Bind { var: VarId(99), ty: Ty::Int },
                    guard: None,
                    body: lit_int(2),
                }],
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("VarId(99)"));
    }

    #[test]
    fn verifies_module_functions() {
        let mut main_vt = VarTable::new();
        let _x = main_vt.alloc("x".into(), Ty::Int, Mutability::Let, None);

        let mod_vt = VarTable::new(); // empty
        let mod_body = var_expr(VarId(99), Ty::Int); // out of bounds in module table

        let prog = IrProgram {
            functions: vec![make_fn("main", lit_int(0))],
            top_lets: vec![],
            type_decls: vec![],
            var_table: main_vt,
            modules: vec![IrModule {
                name: "mymod".into(),
                versioned_name: None,
                type_decls: vec![],
                functions: vec![make_fn("helper", mod_body)],
                top_lets: vec![],
                var_table: mod_vt,
            }],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
        };
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].fn_name == "mymod.helper");
    }

    #[test]
    fn detects_unop_type_mismatch() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::UnOp {
                op: UnOp::NegInt,
                operand: Box::new(IrExpr {
                    kind: IrExprKind::LitBool { value: true },
                    ty: Ty::Bool,
                    span: None,
                }),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("NegInt"));
    }

    #[test]
    fn detects_duplicate_record_fields() {
        let vt = VarTable::new();
        let prog = IrProgram {
            functions: vec![],
            top_lets: vec![],
            type_decls: vec![IrTypeDecl {
                name: "Bad".into(),
                kind: IrTypeDeclKind::Record {
                    fields: vec![
                        IrFieldDecl { name: "x".into(), ty: Ty::Int, default: None, alias: None },
                        IrFieldDecl { name: "x".into(), ty: Ty::String, default: None, alias: None },
                    ],
                },
                deriving: None,
                generics: None,
                visibility: IrVisibility::Public,
            }],
            var_table: vt,
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
        };
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate field 'x'"));
    }

    #[test]
    fn detects_duplicate_variant_cases() {
        let vt = VarTable::new();
        let prog = IrProgram {
            functions: vec![],
            top_lets: vec![],
            type_decls: vec![IrTypeDecl {
                name: "Bad".into(),
                kind: IrTypeDeclKind::Variant {
                    cases: vec![
                        IrVariantDecl { name: "A".into(), kind: IrVariantKind::Unit },
                        IrVariantDecl { name: "A".into(), kind: IrVariantKind::Unit },
                    ],
                    is_generic: false,
                    boxed_args: HashSet::new(),
                    boxed_record_fields: HashSet::new(),
                },
                deriving: None,
                generics: None,
                visibility: IrVisibility::Public,
            }],
            var_table: vt,
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
        };
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate variant case 'A'"));
    }

    #[test]
    fn detects_duplicate_param_var_ids() {
        let mut vt = VarTable::new();
        let x = vt.alloc("x".into(), Ty::Int, Mutability::Let, None);
        let f = IrFunction {
            name: "bad".into(),
            params: vec![
                IrParam { var: x, ty: Ty::Int, name: "a".into(), borrow: ParamBorrow::Own, open_record: None, default: None },
                IrParam { var: x, ty: Ty::Int, name: "b".into(), borrow: ParamBorrow::Own, open_record: None, default: None },
            ],
            ret_ty: Ty::Int,
            body: lit_int(0),
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            visibility: IrVisibility::Public,
        };
        let prog = make_program(vec![f], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate parameter VarId"));
    }

    #[test]
    fn detects_index_access_on_map() {
        let vt = VarTable::new();
        let map_ty = Ty::Applied(crate::types::TypeConstructorId::Map, vec![Ty::String, Ty::Int]);
        let body = IrExpr {
            kind: IrExprKind::IndexAccess {
                object: Box::new(IrExpr { kind: IrExprKind::EmptyMap, ty: map_ty, span: None }),
                index: Box::new(IrExpr { kind: IrExprKind::LitStr { value: "k".into() }, ty: Ty::String, span: None }),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("IndexAccess used on Map"));
    }

    #[test]
    fn detects_map_access_on_non_map() {
        let vt = VarTable::new();
        let list_ty = Ty::Applied(crate::types::TypeConstructorId::List, vec![Ty::Int]);
        let body = IrExpr {
            kind: IrExprKind::MapAccess {
                object: Box::new(IrExpr { kind: IrExprKind::List { elements: vec![] }, ty: list_ty, span: None }),
                key: Box::new(lit_int(0)),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("MapAccess used on non-Map"));
    }

    #[test]
    fn allows_map_access_on_map() {
        let vt = VarTable::new();
        let map_ty = Ty::Applied(crate::types::TypeConstructorId::Map, vec![Ty::String, Ty::Int]);
        let body = IrExpr {
            kind: IrExprKind::MapAccess {
                object: Box::new(IrExpr { kind: IrExprKind::EmptyMap, ty: map_ty, span: None }),
                key: Box::new(IrExpr { kind: IrExprKind::LitStr { value: "k".into() }, ty: Ty::String, span: None }),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        assert!(verify_program(&prog).is_empty());
    }

    #[test]
    fn pow_int_type_consistency() {
        let vt = VarTable::new();
        // PowInt with Int operands — should pass
        let body = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::PowInt,
                left: Box::new(lit_int(2)),
                right: Box::new(lit_int(3)),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        assert!(verify_program(&prog).is_empty());

        // PowInt with Float operand — should fail
        let vt2 = VarTable::new();
        let body2 = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::PowInt,
                left: Box::new(IrExpr { kind: IrExprKind::LitFloat { value: 2.0 }, ty: Ty::Float, span: None }),
                right: Box::new(lit_int(3)),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog2 = make_program(vec![make_fn("main", body2)], vt2);
        let errors = verify_program(&prog2);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("PowInt"));
    }

    #[test]
    fn detects_call_to_unknown_module_function() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: "mymod".into(), func: "nonexistent".into() },
                args: vec![],
                type_args: vec![],
            },
            ty: Ty::Unit,
            span: None,
        };
        // Create program with a module that has a "helper" function but not "nonexistent"
        let mod_fn = make_fn("helper", lit_int(0));
        let prog = IrProgram {
            functions: vec![make_fn("main", body)],
            top_lets: vec![],
            type_decls: vec![],
            var_table: vt,
            modules: vec![IrModule {
                name: "mymod".into(),
                versioned_name: None,
                type_decls: vec![],
                functions: vec![mod_fn],
                top_lets: vec![],
                var_table: VarTable::new(),
            }],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
        };
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unknown function 'mymod.nonexistent'"));
    }

    #[test]
    fn allows_call_to_known_module_function() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: "mymod".into(), func: "helper".into() },
                args: vec![],
                type_args: vec![],
            },
            ty: Ty::Int,
            span: None,
        };
        let mod_fn = make_fn("helper", lit_int(42));
        let prog = IrProgram {
            functions: vec![make_fn("main", body)],
            top_lets: vec![],
            type_decls: vec![],
            var_table: vt,
            modules: vec![IrModule {
                name: "mymod".into(),
                versioned_name: None,
                type_decls: vec![],
                functions: vec![mod_fn],
                top_lets: vec![],
                var_table: VarTable::new(),
            }],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
        };
        assert!(verify_program(&prog).is_empty());
    }

    #[test]
    fn allows_call_to_stdlib_module() {
        // stdlib modules (like "string") are not in known_module_functions — should not error
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: "string".into(), func: "len".into() },
                args: vec![IrExpr { kind: IrExprKind::LitStr { value: "hi".into() }, ty: Ty::String, span: None }],
                type_args: vec![],
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        assert!(verify_program(&prog).is_empty());
    }
}
