//! VarStoragePass: decide the storage of every function-local `var` binding
//! of non-Copy type on the Rust target — `let mut T` when nothing captures
//! it, `AlmideRcCow<T>` (copy-on-write) when a closure does — and publish
//! the verdict as `CodegenAnnotations::var_storage`.
//!
//! The classification used to run inside the walker's program setup
//! (`walker/program_render.rs`), which made the renderer the author of an
//! ownership decision it was meant to read (#2186 step 3). It is a pass
//! now: the walker only looks the verdict up.

use std::collections::HashSet;
use almide_ir::*;
use almide_ir::annotations::VarStorage;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct VarStoragePass;

impl NanoPass for VarStoragePass {
    fn name(&self) -> &str { "VarStorage" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    /// The shared-cell verdict must be final: a captured-and-mutated var is
    /// a cell (`shared_mut_vars`, CaptureClone) and never an `AlmideRcCow`.
    fn depends_on(&self) -> Vec<&'static str> { vec!["CaptureClone"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let exclude = storage_exclusions(&program);
        let non_copy_var_binds = non_copy_var_binds(&program);
        let captured = lambda_captured_vars(&program);
        let ann = &mut program.codegen_annotations;
        // Only vars captured by lambdas get AlmideRcCow; the rest stay a plain
        // `let mut T` (no entry). Captured mutable vars that became shared
        // cells (`Rc<Cell>` for Copy via P3, `AlmideSharedMut` for non-Copy
        // via P6) are driven by the shared-mut path, NOT AlmideRcCow —
        // AlmideRcCow's copy-on-write would lose a mutation made through the
        // closure. (Closure v2 P6.)
        let mut rc_cow: Vec<VarId> = non_copy_var_binds.into_iter()
            .filter(|v| !exclude.contains(v) && !ann.is_shared_mut(v) && captured.contains(v))
            .collect();
        rc_cow.sort_by_key(|v| v.0);
        for v in rc_cow {
            ann.var_storage.insert(v, VarStorage::RcCow);
        }
        PassResult { program, changed: false }
    }
}

/// Vars that must NEVER get `AlmideRcCow` storage: mutable top-lets (module
/// globals, handled by the `ModuleRc`/`ModuleCell` path) and every fn param
/// (borrow inference owns those).
fn storage_exclusions(program: &IrProgram) -> HashSet<VarId> {
    let mut exclude = HashSet::new();
    let top_lets = program.top_lets.iter().chain(program.modules.iter().flat_map(|m| m.top_lets.iter()));
    exclude.extend(top_lets.filter(|tl| tl.mutable).map(|tl| tl.var));
    let functions = program.functions.iter().chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    exclude.extend(functions.flat_map(|f| f.params.iter().map(|p| p.var)));
    exclude
}

/// Every `var` binding of a non-Copy type — derived from THE copy-ness
/// classifier (§4 stage 2c, #531: the projection table in
/// `almide_ir::top_let_storage`).
fn non_copy_var_binds(program: &IrProgram) -> HashSet<VarId> {
    struct Binds(HashSet<VarId>);
    impl IrVisitor for Binds {
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            if let IrStmtKind::Bind { var, mutability: Mutability::Var, ty, .. } = &stmt.kind
                && !almide_ir::top_let_storage::rccow_copyish(ty)
            {
                self.0.insert(*var);
            }
            walk_stmt(self, stmt);
        }
    }
    let mut binds = Binds(HashSet::new());
    for f in every_function(program) { binds.visit_expr(&f.body); }
    binds.0
}

/// Every var some lambda captures — via the single shared free-variable
/// analysis (`almide_ir::free_vars`), the same one the wasm closure path
/// uses: a lambda's captures are the free vars of its body relative to its
/// params, and the union over every lambda is the full captured set.
/// (Closure v2, P4: one capture analysis for both targets.)
fn lambda_captured_vars(program: &IrProgram) -> HashSet<VarId> {
    struct Captures(HashSet<VarId>);
    impl IrVisitor for Captures {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Lambda { params, body, .. } = &expr.kind {
                let bound: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
                self.0.extend(almide_ir::free_vars::free_vars(body, &bound));
            }
            walk_expr(self, expr);
        }
    }
    let mut captures = Captures(HashSet::new());
    for f in every_function(program) { captures.visit_expr(&f.body); }
    captures.0
}

fn every_function(program: &IrProgram) -> impl Iterator<Item = &IrFunction> {
    program.functions.iter().chain(program.modules.iter().flat_map(|m| m.functions.iter()))
}
