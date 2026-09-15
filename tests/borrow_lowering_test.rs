//! #2186 step 3: the walker renders ownership, it no longer decides it. The
//! decisions it used to take per site now live in `BorrowLoweringPass`
//! (`crates/almide-codegen/src/pass_borrow_lowering.rs`) and reach the
//! renderer as IR, and `VarStoragePass` publishes the `AlmideRcCow` verdict
//! the walker used to compute in its program setup. Hand-built IR pins each
//! rewrite the lowering pass makes and each refusal, the storage verdict, and
//! — end to end — the shape the old walker mis-rendered: a by-reference
//! `Map` param compared against a value was `&T == T` (rustc E0308 behind a
//! green check) and now derefs.

use std::process::Command;
use almide::codegen::pass::{NanoPass, Target};
use almide::codegen::pass_borrow_lowering::BorrowLoweringPass;
use almide::codegen::pass_var_storage::VarStoragePass;
use almide::ir::annotations::VarStorage;
use almide::ir::top_let_storage::{GlobalInfo, TopLetStorage};
use almide::ir::*;
use almide::types::{Ty, TypeConstructorId};
use almide_base::intern::sym;

fn e(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn list_ty() -> Ty {
    Ty::Applied(TypeConstructorId::List, vec![Ty::Int])
}

fn var(id: VarId, ty: Ty) -> IrExpr {
    e(IrExprKind::Var { id }, ty)
}

fn borrow(inner: IrExpr, as_str: bool, mutable: bool) -> IrExpr {
    let ty = inner.ty.clone();
    e(IrExprKind::Borrow { expr: Box::new(inner), as_str, mutable }, ty)
}

fn clone(inner: IrExpr) -> IrExpr {
    let ty = inner.ty.clone();
    e(IrExprKind::Clone { expr: Box::new(inner) }, ty)
}

fn lit(n: i64) -> IrExpr {
    e(IrExprKind::LitInt { value: n }, Ty::Int)
}

fn runtime(symbol: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
    e(IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty)
}

fn param(vt: &mut VarTable, name: &str, ty: Ty, borrow: ParamBorrow) -> IrParam {
    let v = vt.alloc(sym(name), ty.clone(), Mutability::Let, None);
    IrParam { var: v, ty, name: sym(name), borrow, is_mut: false, open_record: None, default: None, attrs: vec![] }
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

/// Lower one fn body under the pass; returns the rewritten body.
fn lower(vt: VarTable, params: Vec<IrParam>, body: IrExpr, ann: impl FnOnce(&mut almide::ir::annotations::CodegenAnnotations)) -> IrExpr {
    let mut program = IrProgram { functions: vec![func("f", params, body)], var_table: vt, ..Default::default() };
    ann(&mut program.codegen_annotations);
    let out = BorrowLoweringPass.run(program, Target::Rust).program;
    out.functions.into_iter().next().expect("f").body
}

fn method_of(e: &IrExpr) -> Option<&str> {
    match &e.kind {
        IrExprKind::Call { target: CallTarget::Method { method, .. }, args, .. } if args.is_empty() => Some(method.as_str()),
        _ => None,
    }
}

// ── the borrow shapes ───────────────────────────────────────────────

#[test]
fn a_borrow_of_a_by_reference_param_is_the_param_itself() {
    let mut vt = VarTable::new();
    let p = param(&mut vt, "xs", list_ty(), ParamBorrow::RefSlice);
    let m = param(&mut vt, "m", list_ty(), ParamBorrow::RefMut);
    let (px, pm) = (p.var, m.var);
    let body = e(IrExprKind::Tuple { elements: vec![
        borrow(var(px, list_ty()), false, false),
        borrow(var(pm, list_ty()), false, true),
        // `&*xs` of a `&[T]` param keeps its deref — only the plain borrow drops.
        borrow(var(px, list_ty()), true, false),
    ] }, Ty::Unit);
    let out = lower(vt, vec![p, m], body, |_| {});
    let IrExprKind::Tuple { elements } = &out.kind else { panic!("tuple") };
    assert!(matches!(elements[0].kind, IrExprKind::Var { id } if id == px), "`&xs` on a `&[T]` param is `xs`");
    assert!(matches!(elements[1].kind, IrExprKind::Var { id } if id == pm), "`&mut m` on a `&mut T` param is `m`");
    assert!(matches!(elements[2].kind, IrExprKind::Borrow { as_str: true, .. }), "`&*xs` keeps its deref");
}

#[test]
fn a_clone_of_a_str_param_is_to_string_and_a_map_key_owns_itself() {
    let mut vt = VarTable::new();
    let s = param(&mut vt, "s", Ty::String, ParamBorrow::RefStr);
    let ps = s.var;
    let map_ty = Ty::Applied(TypeConstructorId::Map, vec![Ty::String, Ty::Int]);
    let m = vt.alloc(sym("m"), map_ty.clone(), Mutability::Let, None);
    let body = e(IrExprKind::Tuple { elements: vec![
        clone(var(ps, Ty::String)),
        e(IrExprKind::MapAccess { object: Box::new(var(m, map_ty)), key: Box::new(var(ps, Ty::String)) }, Ty::Int),
    ] }, Ty::Unit);
    let out = lower(vt, vec![s], body, |_| {});
    let IrExprKind::Tuple { elements } = &out.kind else { panic!("tuple") };
    assert_eq!(method_of(&elements[0]), Some("to_string"), "`s.clone()` on a `&str` param is `s.to_string()`");
    let IrExprKind::MapAccess { key, .. } = &elements[1].kind else { panic!("map access") };
    assert_eq!(method_of(key), Some("to_string"), "a `&str` param as a map key owns itself (#1874)");
}

#[test]
fn a_stored_by_reference_param_owns_by_type_and_a_carried_borrow_stays() {
    let mut vt = VarTable::new();
    let xs = param(&mut vt, "xs", list_ty(), ParamBorrow::RefSlice);
    let s = param(&mut vt, "s", Ty::String, ParamBorrow::RefStr);
    let t = param(&mut vt, "t", Ty::Named(sym("Tok"), vec![]), ParamBorrow::Ref);
    let b = param(&mut vt, "b", Ty::Bytes, ParamBorrow::Ref);
    let (pxs, ps, pt, pb) = (xs.var, s.var, t.var, b.var);
    let a = vt.alloc(sym("a"), list_ty(), Mutability::Let, None);
    let c = vt.alloc(sym("c"), Ty::String, Mutability::Let, None);
    let d = vt.alloc(sym("d"), Ty::Named(sym("Tok"), vec![]), Mutability::Let, None);
    let tmp = vt.alloc(sym("__tco_tmp_b"), Ty::Bytes, Mutability::Let, None);
    let bind = |v: VarId, value: IrExpr| IrStmt { kind: IrStmtKind::Bind { var: v, mutability: Mutability::Let, ty: value.ty.clone(), value }, span: None };
    let body = e(IrExprKind::Block { stmts: vec![
        bind(a, var(pxs, list_ty())),
        bind(c, clone(var(ps, Ty::String))),
        bind(d, var(pt, Ty::Named(sym("Tok"), vec![]))),
        // The TCO rotation binds the BORROW into a `_`-typed temp on purpose.
        bind(tmp, borrow(var(pb, Ty::Bytes), false, false)),
    ], expr: None }, Ty::Unit);
    let out = lower(vt, vec![xs, s, t, b], body, |_| {});
    let IrExprKind::Block { stmts, .. } = &out.kind else { panic!("block") };
    let value = |i: usize| match &stmts[i].kind { IrStmtKind::Bind { value, .. } => value, _ => panic!("bind") };
    assert_eq!(method_of(value(0)), Some("to_vec"), "a `&[T]` param owns into its let by `.to_vec()`");
    assert_eq!(method_of(value(1)), Some("to_string"), "a `&str` param owns by `.to_string()` even when already Clone-wrapped");
    assert!(matches!(value(2).kind, IrExprKind::Clone { .. }), "a `&T` record param owns by `.clone()`");
    assert!(matches!(value(3).kind, IrExprKind::Var { id } if id == pb), "a carried borrow lowers to the bare param, never to an owning read");
}

#[test]
fn a_spread_over_a_borrowed_or_lazy_base_clones_it_first() {
    let mut vt = VarTable::new();
    let rec = Ty::Named(sym("St"), vec![]);
    let st = param(&mut vt, "st", rec.clone(), ParamBorrow::RefMut);
    let g = vt.alloc(sym("g"), rec.clone(), Mutability::Let, None);
    let local = vt.alloc(sym("l"), rec.clone(), Mutability::Let, None);
    let spread = |base: IrExpr| e(IrExprKind::SpreadRecord { base: Box::new(base), fields: vec![(sym("n"), lit(1))] }, rec.clone());
    let body = e(IrExprKind::Tuple { elements: vec![spread(var(st.var, rec.clone())), spread(var(g, rec.clone())), spread(var(local, rec.clone()))] }, Ty::Unit);
    let out = lower(vt, vec![st], body, |ann| {
        ann.globals.insert(g, GlobalInfo { storage: TopLetStorage::Lazy { eager_force: false }, static_name: "G".into(), decl: g });
    });
    let IrExprKind::Tuple { elements } = &out.kind else { panic!("tuple") };
    let base = |i: usize| match &elements[i].kind { IrExprKind::SpreadRecord { base, .. } => &base.kind, _ => panic!("spread") };
    assert!(matches!(base(0), IrExprKind::Clone { .. }), "a `&mut` param spreads its clone (#2037)");
    assert!(matches!(base(1), IrExprKind::Clone { .. }), "a lazy global spreads its clone");
    assert!(matches!(base(2), IrExprKind::Var { .. }), "an owned local spreads itself");
}

#[test]
fn an_indexed_borrow_reaches_into_a_local_list_but_not_into_a_snapshot() {
    let mut vt = VarTable::new();
    let xs = vt.alloc(sym("xs"), list_ty(), Mutability::Let, None);
    let g = vt.alloc(sym("g"), list_ty(), Mutability::Let, None);
    let cell = vt.alloc(sym("cell"), list_ty(), Mutability::Var, None);
    let i = vt.alloc(sym("i"), Ty::Int, Mutability::Let, None);
    let index = |root: VarId, idx: IrExpr| e(IrExprKind::IndexAccess { object: Box::new(var(root, list_ty())), index: Box::new(idx) }, Ty::Int);
    let body = e(IrExprKind::Tuple { elements: vec![
        borrow(index(xs, var(i, Ty::Int)), false, false),
        borrow(index(xs, lit(0)), true, false),
        borrow(index(g, lit(0)), false, false),
        borrow(index(cell, lit(0)), false, false),
        borrow(index(xs, runtime("almide_rt_int_abs", vec![lit(1)], Ty::Int)), false, false),
    ] }, Ty::Unit);
    let out = lower(vt, vec![], body, |ann| {
        ann.globals.insert(g, GlobalInfo { storage: TopLetStorage::RcRefCell, static_name: "G".into(), decl: g });
        ann.shared_mut_vars.insert(cell);
    });
    let IrExprKind::Tuple { elements } = &out.kind else { panic!("tuple") };
    let is_ref = |i: usize| matches!(&elements[i].kind, IrExprKind::RuntimeCall { symbol, .. } if symbol.as_str() == "almide_index_ref!");
    assert!(is_ref(0), "`&xs[i]` borrows into the local list");
    assert!(is_ref(1), "`&*xs[0]` (a `&str` slot) borrows into it too — the `&String` coerces");
    assert!(!is_ref(2), "a global's indexed read is a snapshot: no reference into it (#14e667fdc)");
    assert!(!is_ref(3), "a captured cell's indexed read is a snapshot too");
    assert!(!is_ref(4), "a computed index keeps the copying read");
}

#[test]
fn a_borrowed_field_lookup_takes_the_ref_twin_and_a_propagating_rvalue_derefs() {
    let mut vt = VarTable::new();
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let v = param(&mut vt, "_v", value_ty.clone(), ParamBorrow::Ref);
    let pv = v.var;
    let key = e(IrExprKind::LitStr { value: "k".into() }, Ty::String);
    let lookup = e(IrExprKind::Try { expr: Box::new(runtime("almide_rt_value_field_at", vec![var(pv, value_ty.clone()), key, lit(2)], value_ty.clone())) }, value_ty.clone());
    let cond = e(IrExprKind::LitBool { value: true }, Ty::Bool);
    let rvalue = e(IrExprKind::If { cond: Box::new(cond), then: Box::new(var(pv, Ty::Bytes)), else_: Box::new(var(pv, Ty::Bytes)) }, Ty::Bytes);
    let body = e(IrExprKind::Tuple { elements: vec![borrow(lookup, false, false), borrow(rvalue, false, false)] }, Ty::Unit);
    let out = lower(vt, vec![v], body, |_| {});
    let IrExprKind::Tuple { elements } = &out.kind else { panic!("tuple") };
    let IrExprKind::Try { expr } = &elements[0].kind else { panic!("the borrow is gone: the twin borrows into the object") };
    assert!(matches!(&expr.kind, IrExprKind::RuntimeCall { symbol, .. } if symbol.as_str() == "almide_rt_value_field_ref_at"));
    let IrExprKind::Borrow { expr, .. } = &elements[1].kind else { panic!("borrow") };
    assert!(matches!(expr.kind, IrExprKind::Deref { .. }), "a Bytes `if` under a borrow derefs explicitly (#1210)");
}

#[test]
fn an_equality_against_a_by_reference_param_derefs_that_side_only() {
    let mut vt = VarTable::new();
    let m = param(&mut vt, "m", list_ty(), ParamBorrow::Ref);
    let q = param(&mut vt, "q", list_ty(), ParamBorrow::Ref);
    let (pm, pq) = (m.var, q.var);
    let owned = e(IrExprKind::List { elements: vec![lit(1)] }, list_ty());
    let eq = |l: IrExpr, r: IrExpr| e(IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(l), right: Box::new(r) }, Ty::Bool);
    let body = e(IrExprKind::Tuple { elements: vec![
        eq(owned.clone(), var(pm, list_ty())),
        eq(var(pm, list_ty()), var(pq, list_ty())),
        eq(owned, e(IrExprKind::List { elements: vec![lit(2)] }, list_ty())),
    ] }, Ty::Unit);
    let out = lower(vt, vec![m, q], body, |_| {});
    let IrExprKind::Tuple { elements } = &out.kind else { panic!("tuple") };
    let sides = |i: usize| match &elements[i].kind { IrExprKind::BinOp { left, right, .. } => (&left.kind, &right.kind), _ => panic!("binop") };
    let (l, r) = sides(0);
    assert!(matches!(l, IrExprKind::List { .. }) && matches!(r, IrExprKind::Deref { .. }), "value == &T param: the param side derefs");
    let (l, r) = sides(1);
    assert!(matches!(l, IrExprKind::Var { .. }) && matches!(r, IrExprKind::Var { .. }), "&T == &T: nothing to do");
    let (l, r) = sides(2);
    assert!(matches!(l, IrExprKind::List { .. }) && matches!(r, IrExprKind::List { .. }), "value == value: nothing to do");
}

#[test]
fn a_borrowed_loop_binder_as_str_becomes_a_method_call() {
    let mut vt = VarTable::new();
    let c = vt.alloc(sym("c"), Ty::String, Mutability::Let, None);
    let d = vt.alloc(sym("d"), Ty::String, Mutability::Let, None);
    let body = e(IrExprKind::Tuple { elements: vec![borrow(var(c, Ty::String), true, false), borrow(var(d, Ty::String), true, false)] }, Ty::Unit);
    let out = lower(vt, vec![], body, |ann| { ann.borrowed_loop_vars.insert(c); });
    let IrExprKind::Tuple { elements } = &out.kind else { panic!("tuple") };
    assert_eq!(method_of(&elements[0]), Some("as_str"), "a `&String` binder reaches `&str` through `as_str` (#2188)");
    assert!(matches!(elements[1].kind, IrExprKind::Borrow { as_str: true, .. }), "an owned String keeps `&*`");
}

#[test]
fn the_pass_publishes_every_params_final_mode() {
    let mut vt = VarTable::new();
    let a = param(&mut vt, "a", list_ty(), ParamBorrow::RefMut);
    let b = param(&mut vt, "b", Ty::Int, ParamBorrow::Own);
    let (pa, pb) = (a.var, b.var);
    let program = IrProgram { functions: vec![func("f", vec![a, b], e(IrExprKind::Unit, Ty::Unit))], var_table: vt, ..Default::default() };
    let out = BorrowLoweringPass.run(program, Target::Rust).program;
    assert_eq!(out.codegen_annotations.param_borrows.get(&pa), Some(&ParamBorrow::RefMut));
    assert_eq!(out.codegen_annotations.param_borrows.get(&pb), Some(&ParamBorrow::Own));
}

// ── the storage verdict ─────────────────────────────────────────────

#[test]
fn a_captured_non_copy_var_is_an_rc_cow_and_nothing_else_is() {
    let mut vt = VarTable::new();
    let captured = vt.alloc(sym("captured"), list_ty(), Mutability::Var, None);
    let plain = vt.alloc(sym("plain"), list_ty(), Mutability::Var, None);
    let cell = vt.alloc(sym("cell"), list_ty(), Mutability::Var, None);
    let n = vt.alloc(sym("n"), Ty::Int, Mutability::Var, None);
    let x = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
    let bind = |v: VarId, value: IrExpr| IrStmt { kind: IrStmtKind::Bind { var: v, mutability: Mutability::Var, ty: value.ty.clone(), value }, span: None };
    let empty = || e(IrExprKind::List { elements: vec![] }, list_ty());
    let lambda = e(IrExprKind::Lambda { params: vec![(x, Ty::Int)], body: Box::new(e(IrExprKind::Tuple { elements: vec![
        var(captured, list_ty()), var(cell, list_ty()), var(n, Ty::Int),
    ] }, Ty::Unit)), lambda_id: None }, Ty::Unit);
    let body = e(IrExprKind::Block { stmts: vec![bind(captured, empty()), bind(plain, empty()), bind(cell, empty()), bind(n, lit(0))], expr: Some(Box::new(lambda)) }, Ty::Unit);
    let mut program = IrProgram { functions: vec![func("f", vec![], body)], var_table: vt, ..Default::default() };
    program.codegen_annotations.shared_mut_vars.insert(cell);
    let out = VarStoragePass.run(program, Target::Rust).program;
    let storage = &out.codegen_annotations;
    assert_eq!(storage.get_var_storage(&captured), VarStorage::RcCow, "a closure-captured non-Copy var is copy-on-write");
    assert_eq!(storage.get_var_storage(&plain), VarStorage::Local, "an uncaptured var is a plain `let mut`");
    assert_eq!(storage.get_var_storage(&cell), VarStorage::Local, "a shared cell is driven by the cell path, never RcCow");
    assert_eq!(storage.get_var_storage(&n), VarStorage::Local, "a Copy var is never RcCow");
}

// ── end to end ──────────────────────────────────────────────────────

/// A by-reference `Map` param compared against a value: the walker used to
/// render `almide_eq!(lit, m)` with `m: &AlmideMap` — E0308 behind a green
/// check. The lowering pass derefs the reference side and the program builds.
#[test]
fn a_borrowed_map_param_compares_against_a_value() {
    let dir = std::env::temp_dir().join(format!("almide-borrow-lowering-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("eq.almd");
    std::fs::write(&src, "fn same(m: Map[String, Int]) -> Bool = [\"a\": 1] == m\nfn main() -> Unit = println(if same([\"a\": 1]) then \"yes\" else \"no\")\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&src).output().expect("almide run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "the program must build and run:\n{stderr}");
    assert_eq!(stdout.trim(), "yes", "{stdout}\n{stderr}");
    let emitted = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&src).arg("--target").arg("rust").output().expect("emit");
    let rust = String::from_utf8_lossy(&emitted.stdout);
    assert!(rust.contains("almide_eq!(AlmideMap::from([(\"a\".to_string(), 1i64)]), (*m))"), "the reference side derefs:\n{}", rust.lines().filter(|l| l.contains("almide_eq!")).collect::<Vec<_>>().join("\n"));
    let _ = std::fs::remove_dir_all(&dir);
}
