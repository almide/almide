//! #2186 step 2: the ONE use-kind walk (`almide_codegen::use_kind`) and the
//! borrow policy `BorrowInsertion` states over it. Hand-built IR pins (a)
//! the position every node puts its children in, (b) the counts and write
//! sets the clone and capture passes read, and (c) the fixed point: a param
//! is borrowed when every occurrence keeps it, owned when one moves it,
//! `&mut` when a callee slot is, and the iteration converges by monotone
//! ascent — a mutual-recursion group and a cross-module mirror both settle
//! on the borrowed answer without ever reading a pessimistic first round.

use almide::codegen::pass::{BorrowInsertionPass, NanoPass, Target};
use almide::codegen::use_kind::{written_vars, Chain, Ctor, ExplicitBorrows, Site, SlotMode, SlotOracle, UseSites};
use almide::ir::*;
use almide::types::{Ty, TypeConstructorId};
use almide_base::intern::{sym, Sym};

fn e(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn list_ty() -> Ty {
    Ty::Applied(TypeConstructorId::List, vec![Ty::Int])
}

fn record_ty() -> Ty {
    Ty::Named(sym("Tok"), vec![])
}

fn var(id: VarId, ty: Ty) -> IrExpr {
    e(IrExprKind::Var { id }, ty)
}

fn borrow(inner: IrExpr, mutable: bool) -> IrExpr {
    let ty = inner.ty.clone();
    e(IrExprKind::Borrow { expr: Box::new(inner), as_str: false, mutable }, ty)
}

fn member(object: IrExpr, field: &str, ty: Ty) -> IrExpr {
    e(IrExprKind::Member { object: Box::new(object), field: sym(field) }, ty)
}

fn call(name: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
    e(IrExprKind::Call { target: CallTarget::Named { name: sym(name) }, args, type_args: vec![] }, ty)
}

fn lit(n: i64) -> IrExpr {
    e(IrExprKind::LitInt { value: n }, Ty::Int)
}

fn bind(var: VarId, value: IrExpr) -> IrStmt {
    let ty = value.ty.clone();
    IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span: None }
}

fn block(stmts: Vec<IrStmt>, tail: IrExpr) -> IrExpr {
    let ty = tail.ty.clone();
    e(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, ty)
}

fn param(vt: &mut VarTable, name: &str, ty: Ty) -> IrParam {
    let v = vt.alloc(sym(name), ty.clone(), Mutability::Let, None);
    IrParam { var: v, ty, name: sym(name), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }
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

fn record_decl(name: &str) -> IrTypeDecl {
    let field = |name: &str, ty: Ty| IrFieldDecl { name: sym(name), ty, default: None, alias: None, attrs: vec![] };
    IrTypeDecl {
        name: sym(name),
        kind: IrTypeDeclKind::Record { fields: vec![field("text", Ty::String), field("n", Ty::Int)] },
        deriving: None,
        generics: None,
        visibility: IrVisibility::Public,
        doc: None,
        blank_lines_before: 0,
    }
}

/// A slot oracle that borrows the slots of one named callee and consumes
/// everything else — enough to see the `Arg` modes flow through.
struct Borrows(&'static str);

impl SlotOracle for Borrows {
    fn call_slot(&self, target: &CallTarget, _: usize, _: &IrExpr) -> SlotMode {
        match target {
            CallTarget::Named { name } if name.as_str() == self.0 => SlotMode::Borrow,
            _ => SlotMode::Consume,
        }
    }
    fn runtime_slot(&self, _: Sym, _: usize, _: &IrExpr) -> SlotMode { SlotMode::Consume }
}

fn sites(uses: &UseSites, v: VarId) -> Vec<Site> {
    uses.of(v).map(|u| u.site).collect()
}

// ── (a) positions ──────────────────────────────────────────────────

#[test]
fn every_constructor_operand_and_the_tail_are_consuming_positions() {
    let mut vt = VarTable::new();
    let xs = vt.alloc(sym("xs"), list_ty(), Mutability::Let, None);
    let s = vt.alloc(sym("s"), Ty::String, Mutability::Let, None);
    // { let t = [xs]; ok(s) }  with a `${s}` interpolation and a concat on the way
    let interp = e(IrExprKind::StringInterp { parts: vec![IrStringPart::Expr { expr: var(s, Ty::String) }] }, Ty::String);
    let concat = e(IrExprKind::BinOp { op: BinOp::ConcatStr, left: Box::new(var(s, Ty::String)), right: Box::new(interp) }, Ty::String);
    let t = vt.alloc(sym("t"), list_ty(), Mutability::Let, None);
    let body = block(
        vec![bind(t, e(IrExprKind::List { elements: vec![var(xs, list_ty())] }, list_ty())), bind(t, concat)],
        e(IrExprKind::ResultOk { expr: Box::new(var(s, Ty::String)) }, Ty::String),
    );
    let uses = UseSites::of_expr(&body, Site::Result, &ExplicitBorrows);
    assert_eq!(sites(&uses, xs), vec![Site::Construct(Ctor::List)]);
    assert_eq!(sites(&uses, s), vec![Site::Concat, Site::Construct(Ctor::Interp), Site::Construct(Ctor::Ok)]);
    // A bare tail IS a result position.
    let tail = UseSites::of_expr(&var(s, Ty::String), Site::Result, &ExplicitBorrows);
    assert_eq!(sites(&tail, s), vec![Site::Result]);
}

#[test]
fn projections_root_a_chain_that_remembers_where_the_whole_chain_sits() {
    let mut vt = VarTable::new();
    let t = vt.alloc(sym("t"), record_ty(), Mutability::Let, None);
    // Tok { text: t.text }  — one-level heap projection into a record literal
    let rec = e(IrExprKind::Record { name: Some(sym("Tok")), fields: vec![(sym("text"), member(var(t, record_ty()), "text", Ty::String))] }, record_ty());
    let uses = UseSites::of_expr(&rec, Site::Result, &ExplicitBorrows);
    let u = uses.of(t).next().expect("one occurrence");
    assert_eq!(u.site, Site::Member);
    assert_eq!(u.chain, Some(Chain { top: Site::Construct(Ctor::Record), len: 1, heap: true }));
    // (t.n) + 1 — a scalar projection inside an operand: heap=false, the chain top is the operand
    let sum = e(IrExprKind::BinOp { op: BinOp::AddInt, left: Box::new(member(var(t, record_ty()), "n", Ty::Int)), right: Box::new(lit(1)) }, Ty::Int);
    let uses = UseSites::of_expr(&sum, Site::Result, &ExplicitBorrows);
    let u = uses.of(t).next().expect("one occurrence");
    assert_eq!(u.chain, Some(Chain { top: Site::Operand, len: 1, heap: false }));
    // &mut (t.text) — the chain top is the mutable borrow, and `in_mut` holds
    let mb = borrow(member(var(t, record_ty()), "text", Ty::String), true);
    let uses = UseSites::of_expr(&mb, Site::Operand, &ExplicitBorrows);
    let u = uses.of(t).next().expect("one occurrence");
    assert_eq!(u.chain.map(|c| c.top), Some(Site::Borrow { mutable: true }));
    assert!(u.in_mut && u.is_write(true) && !u.is_write(false));
}

#[test]
fn call_slots_come_from_the_oracle_and_closures_raise_the_depth() {
    let mut vt = VarTable::new();
    let xs = vt.alloc(sym("xs"), list_ty(), Mutability::Let, None);
    let i = vt.alloc(sym("i"), Ty::Int, Mutability::Let, None);
    let lambda = e(IrExprKind::Lambda { params: vec![(i, Ty::Int)], body: Box::new(call("g", vec![var(xs, list_ty())], Ty::Int)), lambda_id: None }, Ty::Int);
    let body = block(vec![], call("f", vec![var(xs, list_ty()), lambda], Ty::Int));
    let uses = UseSites::of_expr(&body, Site::Result, &Borrows("f"));
    let all: Vec<_> = uses.of(xs).collect();
    assert_eq!(all[0].site, Site::Arg(SlotMode::Borrow));
    assert_eq!(all[0].depth, 0);
    assert_eq!(all[1].site, Site::Arg(SlotMode::Consume));
    assert_eq!(all[1].depth, 1);
    let explicit = UseSites::of_expr(&body, Site::Result, &ExplicitBorrows);
    assert!(explicit.of(xs).all(|u| u.site == Site::Arg(SlotMode::Consume)));
}

#[test]
fn iterator_chains_flag_every_occurrence_under_them() {
    let mut vt = VarTable::new();
    let xs = vt.alloc(sym("xs"), list_ty(), Mutability::Let, None);
    let acc = vt.alloc(sym("acc"), Ty::Int, Mutability::Let, None);
    let x = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
    let fold = e(IrExprKind::Lambda { params: vec![(acc, Ty::Int), (x, Ty::Int)], body: Box::new(var(acc, Ty::Int)), lambda_id: None }, Ty::Int);
    let chain = e(IrExprKind::IterChain {
        source: Box::new(var(xs, list_ty())),
        consume: false,
        steps: vec![],
        collector: IterCollector::Fold { init: Box::new(lit(0)), lambda: Box::new(fold) },
    }, Ty::Int);
    let uses = UseSites::of_expr(&chain, Site::Result, &ExplicitBorrows);
    let src = uses.of(xs).next().expect("source");
    assert_eq!(src.site, Site::Iterable { consumed: false });
    assert!(src.in_chain);
    // A chain lambda is a scope that runs per element, not a closure: its
    // occurrences stay at depth 0 and are loop occurrences.
    assert!(uses.of(acc).all(|u| u.in_chain && u.depth == 0 && u.in_loop));
}

// ── (b) counts and writes ──────────────────────────────────────────

#[test]
fn counts_every_node_and_in_place_target_but_not_a_reassignment() {
    let mut vt = VarTable::new();
    let xs = vt.alloc(sym("xs"), list_ty(), Mutability::Var, None);
    let ys = vt.alloc(sym("ys"), list_ty(), Mutability::Var, None);
    let body = block(vec![
        IrStmt { kind: IrStmtKind::IndexAssign { target: xs, index: lit(0), value: lit(1) }, span: None },
        IrStmt { kind: IrStmtKind::Assign { var: ys, value: var(xs, list_ty()) }, span: None },
    ], var(xs, list_ty()));
    let uses = UseSites::of_expr(&body, Site::Result, &ExplicitBorrows);
    let counts = uses.counts();
    assert_eq!(counts.get(&xs), Some(&3), "target + assigned + tail");
    assert_eq!(counts.get(&ys), None, "a reassignment names its target without reading it");
    assert_eq!(uses.written(), [xs, ys].into_iter().collect());
    assert_eq!(written_vars(&body), [xs, ys].into_iter().collect());
    assert!(uses.occurs(xs) && !uses.occurs(ys));
}

// ── (c) the borrow policy at the fixed point ───────────────────────

fn run(program: IrProgram) -> IrProgram {
    BorrowInsertionPass.run(program, Target::Rust).program
}

fn borrows_of(program: &IrProgram, name: &str) -> Vec<ParamBorrow> {
    program.functions.iter().find(|f| f.name.as_str() == name).expect("fn")
        .params.iter().map(|p| p.borrow).collect()
}

#[test]
fn a_param_only_projected_and_borrowed_stays_borrowed_and_a_returned_one_is_owned() {
    let mut vt = VarTable::new();
    let t = param(&mut vt, "t", record_ty());
    let u = param(&mut vt, "u", record_ty());
    let reads = func("reads", vec![t.clone()], member(var(t.var, record_ty()), "n", Ty::Int));
    let returns = func("returns", vec![u.clone()], var(u.var, record_ty()));
    let program = IrProgram { functions: vec![reads, returns], type_decls: vec![record_decl("Tok")], var_table: vt, ..Default::default() };
    let out = run(program);
    assert_eq!(borrows_of(&out, "reads"), vec![ParamBorrow::Ref]);
    assert_eq!(borrows_of(&out, "returns"), vec![ParamBorrow::Own]);
}

#[test]
fn a_heap_field_moved_into_a_record_literal_owns_its_root() {
    let mut vt = VarTable::new();
    let t = param(&mut vt, "t", record_ty());
    let rebuild = e(IrExprKind::Record { name: Some(sym("Tok")), fields: vec![
        (sym("text"), member(var(t.var, record_ty()), "text", Ty::String)),
        (sym("n"), lit(0)),
    ] }, record_ty());
    let f = func("rebuild", vec![t], rebuild);
    let program = IrProgram { functions: vec![f], type_decls: vec![record_decl("Tok")], var_table: vt, ..Default::default() };
    assert_eq!(borrows_of(&run(program), "rebuild"), vec![ParamBorrow::Own]);
}

#[test]
fn ownership_flows_through_callees_and_a_mutual_recursion_settles_borrowed() {
    // leaf(xs) reads; mid(xs) = leaf(xs); top(xs) = mid(xs) — all borrowed.
    // ping(xs) = pong(xs); pong(xs) = if .. then 0 else ping(xs) — the
    // #2040 group: the optimistic first round keeps both borrowed.
    // sink(xs) = [xs]; via(xs) = sink(xs) — via must own what sink moves.
    let mut vt = VarTable::new();
    let mk = |vt: &mut VarTable, name: &str, body: &dyn Fn(VarId) -> IrExpr| {
        let p = param(vt, "xs", list_ty());
        let b = body(p.var);
        func(name, vec![p], b)
    };
    let len = |v: VarId| e(IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_len"), args: vec![var(v, list_ty())] }, Ty::Int);
    let leaf = mk(&mut vt, "leaf", &len);
    let mid = mk(&mut vt, "mid", &|v| call("leaf", vec![var(v, list_ty())], Ty::Int));
    let top = mk(&mut vt, "top", &|v| call("mid", vec![var(v, list_ty())], Ty::Int));
    let ping = mk(&mut vt, "ping", &|v| call("pong", vec![var(v, list_ty())], Ty::Int));
    let pong = mk(&mut vt, "pong", &|v| e(IrExprKind::If {
        cond: Box::new(e(IrExprKind::LitBool { value: true }, Ty::Bool)),
        then: Box::new(lit(0)),
        else_: Box::new(call("ping", vec![var(v, list_ty())], Ty::Int)),
    }, Ty::Int));
    let sink = mk(&mut vt, "sink", &|v| e(IrExprKind::List { elements: vec![var(v, list_ty())] }, list_ty()));
    let via = mk(&mut vt, "via", &|v| call("sink", vec![var(v, list_ty())], list_ty()));
    let program = IrProgram { functions: vec![leaf, mid, top, ping, pong, sink, via], var_table: vt, ..Default::default() };
    let out = run(program);
    for name in ["leaf", "mid", "top", "ping", "pong"] {
        assert_eq!(borrows_of(&out, name), vec![ParamBorrow::RefSlice], "{name}");
    }
    assert_eq!(borrows_of(&out, "sink"), vec![ParamBorrow::Own]);
    assert_eq!(borrows_of(&out, "via"), vec![ParamBorrow::Own]);
}

#[test]
fn a_param_handed_to_a_mutating_slot_is_a_mutable_borrow() {
    // push(xs) = list.push(xs, 1) — the seeded `almide_rt_list_push` takes
    // its list `&mut`, so the forwarding param does too.
    let mut vt = VarTable::new();
    let p = param(&mut vt, "xs", list_ty());
    let body = e(IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_push"), args: vec![var(p.var, list_ty()), lit(1)] }, Ty::Unit);
    let f = func("push", vec![p], body);
    let program = IrProgram { functions: vec![f], var_table: vt, ..Default::default() };
    assert_eq!(borrows_of(&run(program), "push"), vec![ParamBorrow::RefMut]);
}

#[test]
fn a_cross_module_mirror_is_read_optimistically_so_the_ascent_never_descends() {
    // Root `use(t)` calls the bare convention key `Tok.show` of module `m`'s
    // `Tok.show(t)`, which only reads its param. The root round runs before
    // the module's mirrors exist; the key must count as pending (borrowed)
    // there, not as unknown (owned) — a descent Own → Ref on the next round
    // is the ICE the monotone check raises.
    let mut vt = VarTable::new();
    let t = param(&mut vt, "t", record_ty());
    let use_fn = func("use", vec![t.clone()], call("Tok.show", vec![var(t.var, record_ty())], Ty::Int));
    let mt = param(&mut vt, "t", record_ty());
    let show = func("Tok.show", vec![mt.clone()], member(var(mt.var, record_ty()), "n", Ty::Int));
    let module = IrModule {
        name: sym("m"), versioned_name: None, type_decls: vec![record_decl("Tok")], functions: vec![show],
        top_lets: vec![], var_table: VarTable::new(), exports: vec![], imports: vec![],
    };
    let program = IrProgram { functions: vec![use_fn], modules: vec![module], var_table: vt, ..Default::default() };
    let out = run(program);
    assert_eq!(borrows_of(&out, "use"), vec![ParamBorrow::Ref]);
    assert_eq!(out.modules[0].functions[0].params[0].borrow, ParamBorrow::Ref);
}
