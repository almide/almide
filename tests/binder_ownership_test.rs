//! #2186 step 3: a local is bound in exactly ONE function. The optimizer's
//! branch-lift helpers used to reuse the enclosing fn's VarIds as their
//! params, so every VarId-keyed ownership annotation had to be second-guessed
//! with a "fn-local truth" gate wherever the helper was rendered. The helpers
//! now take fresh ids (`optimize/branch_lift.rs`), and `verify_ir` refuses
//! a program that binds one id in two functions — so the next producer to
//! share ids fails loudly instead of miscompiling quietly.

use std::process::Command;
use almide::ir::*;
use almide::types::Ty;
use almide_base::intern::sym;

fn e(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn param(v: VarId, ty: Ty) -> IrParam {
    IrParam { var: v, ty, name: sym("p"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }
}

fn func(name: &str, params: Vec<IrParam>, body: IrExpr) -> IrFunction {
    IrFunction {
        name: sym(name),
        params,
        ret_ty: body.ty.clone(),
        body,
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Public,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![],
        module_origin: None,
    }
}

#[test]
fn a_var_id_bound_in_two_functions_is_refused() {
    let mut vt = VarTable::new();
    let shared = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
    let own = vt.alloc(sym("y"), Ty::Int, Mutability::Let, None);
    let read = |v: VarId| e(IrExprKind::Var { id: v }, Ty::Int);
    // `f(x) = x` and `g(x) = x` — the same VarId as both params.
    let bad = IrProgram {
        functions: vec![func("f", vec![param(shared, Ty::Int)], read(shared)), func("g", vec![param(shared, Ty::Int)], read(shared))],
        var_table: vt.clone(),
        ..Default::default()
    };
    let errors = almide::ir::verify_program(&bad);
    assert!(errors.iter().any(|e| e.message.contains("VarId(0) is bound in two functions: 'f' and 'g'")), "{errors:?}");
    // A binder inside the body counts too: `h() = { let x = 1; x }` reuses f's param id.
    let bind = IrStmt { kind: IrStmtKind::Bind { var: shared, mutability: Mutability::Let, ty: Ty::Int, value: e(IrExprKind::LitInt { value: 1 }, Ty::Int) }, span: None };
    let body = e(IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(read(shared))) }, Ty::Int);
    let bad = IrProgram {
        functions: vec![func("f", vec![param(shared, Ty::Int)], read(shared)), func("h", vec![], body)],
        var_table: vt.clone(),
        ..Default::default()
    };
    let errors = almide::ir::verify_program(&bad);
    assert!(errors.iter().any(|e| e.message.contains("bound in two functions: 'f' and 'h'")), "{errors:?}");
    // Distinct ids: clean.
    let good = IrProgram {
        functions: vec![func("f", vec![param(shared, Ty::Int)], read(shared)), func("g", vec![param(own, Ty::Int)], read(own))],
        var_table: vt,
        ..Default::default()
    };
    assert!(almide::ir::verify_program(&good).is_empty());
}

#[test]
fn a_module_table_is_its_own_namespace_before_unification() {
    // Pre-unify, a module fn's VarId(0) indexes the MODULE's table: not the
    // root's VarId(0).
    let mut root = VarTable::new();
    let r = root.alloc(sym("x"), Ty::Int, Mutability::Let, None);
    let mut mt = VarTable::new();
    let m = mt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
    assert_eq!(r, m, "the two tables spell the same number");
    let read = |v: VarId| e(IrExprKind::Var { id: v }, Ty::Int);
    let module = IrModule {
        name: sym("m"), versioned_name: None, type_decls: vec![], functions: vec![func("g", vec![param(m, Ty::Int)], read(m))],
        top_lets: vec![], var_table: mt, exports: vec![], imports: vec![],
    };
    let program = IrProgram { functions: vec![func("f", vec![param(r, Ty::Int)], read(r))], modules: vec![module], var_table: root, ..Default::default() };
    assert!(almide::ir::verify_program(&program).is_empty(), "{:?}", almide::ir::verify_program(&program));
}

/// End to end: a lifted branch helper's params are fresh, so the helper's
/// own last-use analysis moves them (no clone per call) and the enclosing
/// loop binder moves into the call — the shape the shared ids used to
/// force into `.clone()` on both sides.
#[test]
fn a_lifted_branch_helper_owns_fresh_params() {
    let dir = std::env::temp_dir().join(format!("almide-binder-ownership-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("lift.almd");
    std::fs::write(&src, r#"type R = { task: String, ok: Bool }
fn run(t: String) -> Result[R, String] = if t == "x" then ok({ task: t, ok: true }) else err(t)
fn main() -> Unit = {
  var acc: List[R] = []
  for t in ["x", "y"] {
    let next: List[R] = match run(t) {
      ok(r) => acc + [r],
      err(_) => acc + [{ task: t, ok: false }],
    }
    acc = next
  }
  println(int.to_string(list.len(acc)))
}
"#).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&src).output().expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
    let emitted = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&src).arg("--target").arg("rust").output().expect("emit");
    let rust = String::from_utf8_lossy(&emitted.stdout);
    let helper: Vec<&str> = rust.lines().filter(|l| l.contains("branch_lift_synth_")).collect();
    assert!(!helper.is_empty(), "the in-loop heap branch is lifted:\n{rust}");
    assert!(helper.iter().any(|l| l.starts_with("pub fn branch_lift_synth_0(acc: Vec<R>, t: String)")), "the helper's params are plain owned values, not `mut` echoes of the loop's state:\n{}", helper.join("\n"));
    assert!(helper.iter().any(|l| l.contains("branch_lift_synth_0(acc.clone(), t)")), "the loop binder moves into the call — its last use in the iteration:\n{}", helper.join("\n"));
    let _ = std::fs::remove_dir_all(&dir);
}
