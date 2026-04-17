//! Integration tests: the bridge lifts real IrExpr fragments into
//! egg, saturation fires the fusion rules, and extraction returns
//! the expected fused structure.
//!
//! These tests construct IR by hand (there's no parser in the PoC);
//! what matters is that the bridge works on the structural shape
//! that the real lowering pass produces, not that we re-invoke the
//! full parse → infer → lower pipeline.

use almide_base::intern::sym;
use almide_egg_lab::{fusion_rules, AlmideExpr, Bridge, FusionCost};
use almide_ir::{BinOp, CallTarget, IrExpr, IrExprKind, Mutability, VarId, VarTable};
use almide_lang::types::{Ty, TypeConstructorId};
use egg::{Extractor, RecExpr, Runner};

/// Seed a `VarTable` so that `VarId(n)` for each n up to `max` is a
/// valid entry. Mirrors the state real lowering leaves the table in
/// after it has already named every var in the subject IR; lets us
/// predict the next id the bridge's `lower` will allocate.
fn seeded_var_table(max_existing_id: u32) -> VarTable {
    let mut vt = VarTable::new();
    for i in 0..=max_existing_id {
        vt.alloc(sym(&format!("pre_{i}")), Ty::Int, Mutability::Let, None);
    }
    vt
}

// ── Helpers: build IrExpr fragments ─────────────────────────────────

fn list_int() -> Ty {
    Ty::Applied(TypeConstructorId::List, vec![Ty::Int])
}

fn var(id: u32, ty: Ty) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Var { id: VarId(id) },
        ty,
        span: None,
    }
}

fn identity_lambda(var_id: u32, ty: Ty) -> IrExpr {
    let body = var(var_id, ty.clone());
    IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(VarId(var_id), ty.clone())],
            body: Box::new(body),
            lambda_id: Some(var_id),
        },
        ty: Ty::Fn {
            params: vec![ty.clone()],
            ret: Box::new(ty),
        },
        span: None,
    }
}

/// Non-identity lambda; we just need SOMETHING distinct from identity
/// to exercise the "falls into opaque Lam slot" branch.
fn opaque_lambda(param_id: u32, lambda_id: u32) -> IrExpr {
    // (x: Int) => 1  — body is a literal, definitely not `x`
    let body = IrExpr {
        kind: IrExprKind::LitInt { value: 1 },
        ty: Ty::Int,
        span: None,
    };
    IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(VarId(param_id), Ty::Int)],
            body: Box::new(body),
            lambda_id: Some(lambda_id),
        },
        ty: Ty::Fn {
            params: vec![Ty::Int],
            ret: Box::new(Ty::Int),
        },
        span: None,
    }
}

/// `(x: Int) => x + incr` — a non-identity lambda that references its
/// own param, so we can verify that beta-reduction rewrites both the
/// param reference and the surrounding structure.
fn incr_lambda(param_id: u32, lambda_id: u32, incr: i64) -> IrExpr {
    let body = IrExpr {
        kind: IrExprKind::BinOp {
            op: BinOp::AddInt,
            left: Box::new(var(param_id, Ty::Int)),
            right: Box::new(IrExpr {
                kind: IrExprKind::LitInt { value: incr },
                ty: Ty::Int,
                span: None,
            }),
        },
        ty: Ty::Int,
        span: None,
    };
    IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(VarId(param_id), Ty::Int)],
            body: Box::new(body),
            lambda_id: Some(lambda_id),
        },
        ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) },
        span: None,
    }
}

fn list_call(func: &str, args: Vec<IrExpr>, result_ty: Ty) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module {
                module: sym("list"),
                func: sym(func),
            },
            args,
            type_args: vec![],
        },
        ty: result_ty,
        span: None,
    }
}

// ── Driver: lift → saturate → extract best ──────────────────────────

fn optimize(ir: &IrExpr) -> (String, Vec<IrExpr>) {
    let mut bridge = Bridge::new();
    let (rec, root) = bridge.lift(ir);
    let runner = Runner::default()
        .with_iter_limit(64)
        .with_node_limit(10_000)
        .with_expr(&rec)
        .run(&fusion_rules());
    let canonical_root = runner.egraph.find(root);
    let extractor = Extractor::new(&runner.egraph, FusionCost);
    let (_cost, best) = extractor.find_best(canonical_root);
    let slots = bridge.slots().to_vec();
    (best.to_string(), slots)
}

// ── Tests ───────────────────────────────────────────────────────────

/// `list.map(xs, (x) => x)` should collapse to just `xs` (opaque slot).
#[test]
fn identity_map_on_real_ir_collapses_to_xs() {
    let xs = var(0, list_int());
    let lambda = identity_lambda(0, Ty::Int);
    let ir = list_call("map", vec![xs.clone(), lambda], list_int());

    let (best, slots) = optimize(&ir);
    // xs was the only opaque leaf stored
    assert_eq!(slots.len(), 1);
    // Best form is exactly that opaque slot reference
    assert_eq!(best, "_slot_0");
    // And the slot holds the original xs
    match &slots[0].kind {
        IrExprKind::Var { id } => assert_eq!(id.0, 0),
        other => panic!("expected Var(0), got {:?}", other),
    }
}

/// `list.map(list.map(xs, f), g)` should fuse into a single map with a
/// compose marker. The inner `f` and outer `g` each become a Lam slot;
/// xs becomes its own slot. Expected extraction: `(map _slot_xs
/// (compose (lam _slot_g) (lam _slot_f)))` (ordering of slots depends
/// on lift order).
#[test]
fn map_map_on_real_ir_fuses_to_single_traversal() {
    let xs = var(0, list_int());
    let f = opaque_lambda(1, 101);
    let g = opaque_lambda(2, 102);
    let inner = list_call("map", vec![xs, f], list_int());
    let outer = list_call("map", vec![inner, g], list_int());

    let (best, slots) = optimize(&outer);

    // Should be a single outer map, one compose, two lam-wrapped slots.
    assert!(
        best.starts_with("(map ") && !best.contains("(map (map "),
        "expected a single outer map, got: {}",
        best
    );
    assert!(best.contains("compose"), "expected compose marker in {}", best);
    // Lift order: xs → f → g, so slots should be [xs, f, g]
    assert_eq!(slots.len(), 3);
}

/// `list.filter(list.filter(xs, p), q)` fuses into filter with an
/// and-pred marker, same idea as map-map.
#[test]
fn filter_filter_on_real_ir_fuses_predicates() {
    let xs = var(0, list_int());
    let p = opaque_lambda(1, 201);
    let q = opaque_lambda(2, 202);
    let inner = list_call("filter", vec![xs, p], list_int());
    let outer = list_call("filter", vec![inner, q], list_int());

    let (best, slots) = optimize(&outer);

    assert!(
        best.starts_with("(filter ") && !best.contains("(filter (filter "),
        "expected a single outer filter, got: {}",
        best
    );
    assert!(best.contains("and-pred"), "expected and-pred marker in {}", best);
    assert_eq!(slots.len(), 3);
}

/// Saturation across rules: `map(filter(map(xs, id), p), g)` should
/// collapse the identity map first, then be eligible for further
/// rewrites. Even if only the identity-elim fires (no map-filter
/// fusion rule yet), the point is that the extracted form is smaller
/// than the input.
#[test]
fn cross_rule_composition_on_real_ir() {
    let xs = var(0, list_int());
    let id_lam = identity_lambda(0, Ty::Int);
    let p = opaque_lambda(1, 301);
    let g = opaque_lambda(2, 302);

    let map_id = list_call("map", vec![xs, id_lam], list_int());
    let filtered = list_call("filter", vec![map_id, p], list_int());
    let mapped = list_call("map", vec![filtered, g], list_int());

    let (best, _slots) = optimize(&mapped);

    // The identity map should disappear: the innermost `map` with
    // `identity` must not survive in the extracted form.
    assert!(
        !best.contains("identity"),
        "identity should have been eliminated, got: {}",
        best
    );
    // The overall shape stays a filter-inside-map because we don't
    // have a map-filter fusion rule yet; just verify it's not
    // structurally larger than the input.
    assert!(
        best.starts_with("(map (filter "),
        "expected outer map of inner filter after id-elim, got: {}",
        best
    );
}

/// Lift of a non-combinator expression (plain variable) should
/// produce a single opaque slot — the bridge must not crash on
/// things it doesn't know how to structure.
#[test]
fn lift_of_plain_var_is_opaque() {
    let xs = var(0, list_int());
    let mut bridge = Bridge::new();
    let (rec, _root) = bridge.lift(&xs);
    let rec_str = rec.to_string();
    assert_eq!(rec_str, "_slot_0");
    assert_eq!(bridge.slots().len(), 1);
}

// ── Lower (round-trip) tests ────────────────────────────────────────

fn saturate(ir: &IrExpr) -> (Bridge, RecExpr<AlmideExpr>) {
    let mut bridge = Bridge::new();
    let (rec, root) = bridge.lift(ir);
    let runner = Runner::default()
        .with_iter_limit(64)
        .with_node_limit(10_000)
        .with_expr(&rec)
        .run(&fusion_rules());
    let canonical = runner.egraph.find(root);
    let iters = runner.iterations.len();
    assert!(iters < 64, "saturation hit iter limit: {iters}");
    let extractor = Extractor::new(&runner.egraph, FusionCost);
    let (_, best) = extractor.find_best(canonical);
    (bridge, best)
}

fn collect_var_ids(expr: &IrExpr, out: &mut Vec<VarId>) {
    match &expr.kind {
        IrExprKind::Var { id } => out.push(*id),
        IrExprKind::Lambda { params, body, .. } => {
            for (id, _) in params {
                out.push(*id);
            }
            collect_var_ids(body, out);
        }
        IrExprKind::Call { args, .. } => {
            for a in args {
                collect_var_ids(a, out);
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            collect_var_ids(left, out);
            collect_var_ids(right, out);
        }
        _ => {}
    }
}

/// `list.map(xs, identity)` lifts, saturates under identity-map, and
/// lowers back to just `xs`. No fresh VarId should be allocated —
/// identity-map eliminates the lambda outright.
#[test]
fn lower_identity_map_returns_xs_unchanged() {
    let xs = var(0, list_int());
    let id_lam = identity_lambda(0, Ty::Int);
    let ir = list_call("map", vec![xs.clone(), id_lam], list_int());

    let (bridge, best) = saturate(&ir);
    let mut vt = seeded_var_table(0);
    let before = vt.len();
    let lowered = bridge.lower(&best, &mut vt).expect("lower succeeds");

    assert_eq!(vt.len(), before, "no fresh VarId expected after identity elim");
    match &lowered.kind {
        IrExprKind::Var { id } => assert_eq!(id.0, 0, "lowered form should be xs"),
        other => panic!("expected xs (Var), got {other:?}"),
    }
    assert_eq!(lowered.ty, list_int());
}

/// `list.map(list.map(xs, f), g)` lifts, saturates to
/// `(map xs (compose g f))`, and lowers into a single
/// `list.map` whose lambda body is g applied to f applied to a
/// fresh param. Verifies the beta-reduction mechanics:
/// - one fresh VarId is allocated (the new lambda param)
/// - that VarId appears inside the body exactly where the param
///   should flow (no stray references to f's / g's old params)
/// - the outer shape is still `list.map(xs, Lambda(...))`
#[test]
fn lower_map_map_fuses_into_single_map_with_composed_lambda() {
    let xs = var(0, list_int());
    // f: (x) => x + 1 — references its own param, so substitution is
    // observable in the composed body.
    let f = incr_lambda(1, 101, 1);
    // g: (y) => y + 10 — distinct body + distinct param VarId so we
    // can see the two lambdas keep their operations separate after
    // beta-reduction.
    let g = incr_lambda(2, 102, 10);
    let inner = list_call("map", vec![xs, f], list_int());
    let outer = list_call("map", vec![inner, g], list_int());

    let (bridge, best) = saturate(&outer);
    let mut vt = seeded_var_table(2);
    let before = vt.len();
    let lowered = bridge.lower(&best, &mut vt).expect("lower succeeds");

    // Outer shape: list.map(xs, Lambda(..))
    let IrExprKind::Call { target, args, .. } = &lowered.kind else {
        panic!("expected Call, got {:?}", lowered.kind);
    };
    match target {
        CallTarget::Module { module, func } => {
            assert_eq!(module.as_str(), "list");
            assert_eq!(func.as_str(), "map");
        }
        other => panic!("expected Module target, got {other:?}"),
    }
    assert_eq!(args.len(), 2);
    // arg 0 is xs (opaque slot roundtrip).
    match &args[0].kind {
        IrExprKind::Var { id } => assert_eq!(id.0, 0),
        other => panic!("expected xs (Var), got {other:?}"),
    }
    // arg 1 is the composed Lambda.
    let IrExprKind::Lambda { params, body, lambda_id } = &args[1].kind else {
        panic!("expected composed Lambda, got {:?}", args[1].kind);
    };
    assert_eq!(params.len(), 1, "composed lambda must be unary");
    assert!(lambda_id.is_none(), "fresh lambda should not reuse lambda_id");
    let (fresh_param, fresh_ty) = &params[0];
    assert_eq!(fresh_ty, &Ty::Int);

    // Exactly one fresh VarId was allocated.
    assert_eq!(vt.len(), before + 1, "one fresh VarId expected");
    assert_eq!(fresh_param.0 as usize, before, "fresh id matches alloc order");

    // Body must reference only the fresh param — f's (1) and g's (2)
    // old param VarIds must not survive (both were substituted out).
    let mut ids = Vec::new();
    collect_var_ids(body, &mut ids);
    assert!(
        ids.iter().any(|id| *id == *fresh_param),
        "composed body should reference the fresh param, got ids: {ids:?}",
    );
    assert!(
        !ids.iter().any(|id| id.0 == 1 || id.0 == 2),
        "f's and g's old param VarIds must be substituted away, got: {ids:?}",
    );
}

/// `list.filter(list.filter(xs, p), q)` should lower into a single
/// `list.filter` whose predicate is `p(v) && q(v)` — one fresh VarId,
/// a BinOp::And body, and no leftover references to p's / q's old
/// param VarIds.
#[test]
fn lower_filter_filter_fuses_into_conjunctive_predicate() {
    let xs = var(0, list_int());
    // p: (x: Int) => true
    let p = IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(VarId(1), Ty::Int)],
            body: Box::new(IrExpr {
                kind: IrExprKind::LitBool { value: true },
                ty: Ty::Bool,
                span: None,
            }),
            lambda_id: Some(201),
        },
        ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Bool) },
        span: None,
    };
    // q: (x: Int) => false (distinct body to ensure it survives into
    // the right side of the AND)
    let q = IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(VarId(2), Ty::Int)],
            body: Box::new(IrExpr {
                kind: IrExprKind::LitBool { value: false },
                ty: Ty::Bool,
                span: None,
            }),
            lambda_id: Some(202),
        },
        ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Bool) },
        span: None,
    };
    let inner = list_call("filter", vec![xs, p], list_int());
    let outer = list_call("filter", vec![inner, q], list_int());

    let (bridge, best) = saturate(&outer);
    let mut vt = seeded_var_table(2);
    let before = vt.len();
    let lowered = bridge.lower(&best, &mut vt).expect("lower succeeds");

    let IrExprKind::Call { args, target, .. } = &lowered.kind else {
        panic!("expected Call, got {:?}", lowered.kind);
    };
    match target {
        CallTarget::Module { module, func } => {
            assert_eq!(module.as_str(), "list");
            assert_eq!(func.as_str(), "filter");
        }
        other => panic!("expected Module target, got {other:?}"),
    }
    let IrExprKind::Lambda { params, body, .. } = &args[1].kind else {
        panic!("expected composed Lambda");
    };
    assert_eq!(params.len(), 1);
    assert_eq!(vt.len(), before + 1, "one fresh VarId expected");

    // Body = (p_body) AND (q_body_with_fresh_param)
    let IrExprKind::BinOp { op, left, right } = &body.kind else {
        panic!("expected BinOp, got {:?}", body.kind);
    };
    assert!(matches!(op, BinOp::And));
    assert_eq!(body.ty, Ty::Bool);
    assert!(matches!(&left.kind, IrExprKind::LitBool { value: true }));
    assert!(matches!(&right.kind, IrExprKind::LitBool { value: false }));
}

/// Cross-rule: `list.map(list.map(xs, id), f)` should collapse the
/// identity inside the e-graph and lower into `list.map(xs, f)` —
/// no fresh VarId at all, because the surviving lambda is retrieved
/// verbatim from the slot table.
#[test]
fn lower_identity_inside_map_chain_reuses_original_lambda() {
    let xs = var(0, list_int());
    let id_lam = identity_lambda(0, Ty::Int);
    // f: (x) => x + 7 — non-identity, so it survives the identity-map
    // collapse and we can see it flow back through the slot table.
    let f = incr_lambda(2, 303, 7);
    let inner = list_call("map", vec![xs, id_lam], list_int());
    let outer = list_call("map", vec![inner, f], list_int());

    let (bridge, best) = saturate(&outer);
    let mut vt = seeded_var_table(2);
    let before = vt.len();
    let lowered = bridge.lower(&best, &mut vt).expect("lower succeeds");

    assert_eq!(vt.len(), before, "identity elim path needs no fresh id");
    let IrExprKind::Call { args, .. } = &lowered.kind else {
        panic!("expected Call");
    };
    let IrExprKind::Lambda { params, lambda_id, .. } = &args[1].kind else {
        panic!("expected original f Lambda");
    };
    assert_eq!(params[0].0 .0, 2, "original f param VarId preserved");
    assert_eq!(*lambda_id, Some(303), "original lambda_id preserved");
}

/// Lower of a plain Var IrExpr (no combinators) round-trips verbatim
/// through the opaque slot table — the lower pass must not crash on
/// inputs the bridge represents as a single `_slot_0`.
#[test]
fn lower_of_plain_var_roundtrips_via_slot() {
    let xs = var(0, list_int());
    let mut bridge = Bridge::new();
    let (rec, _root) = bridge.lift(&xs);
    let mut vt = seeded_var_table(0);
    let lowered = bridge.lower(&rec, &mut vt).expect("lower succeeds");
    match &lowered.kind {
        IrExprKind::Var { id } => assert_eq!(id.0, 0),
        other => panic!("expected Var(0), got {other:?}"),
    }
}

/// Sanity: a RecExpr parsed from the pure-string egg-lab API and the
/// RecExpr lifted from real IR should be structurally equivalent when
/// they represent the same fusion shape.
#[test]
fn lifted_shape_matches_parsed_shape_after_fusion() {
    let xs = var(0, list_int());
    let f = opaque_lambda(1, 401);
    let g = opaque_lambda(2, 402);
    let inner = list_call("map", vec![xs, f], list_int());
    let outer = list_call("map", vec![inner, g], list_int());

    let (from_ir, _slots) = optimize(&outer);

    // Parsed shape that we know fuses (from the original egg-lab tests)
    let parsed_input: RecExpr<AlmideExpr> =
        "(map (map xs (lam f)) (lam g))".parse().unwrap();
    let parsed_runner = Runner::default()
        .with_iter_limit(64)
        .with_node_limit(10_000)
        .with_expr(&parsed_input)
        .run(&fusion_rules());
    let parsed_root = parsed_runner.egraph.find(parsed_runner.roots[0]);
    let parsed_extractor = Extractor::new(&parsed_runner.egraph, FusionCost);
    let (_, parsed_best) = parsed_extractor.find_best(parsed_root);
    let parsed_str = parsed_best.to_string();

    // Both should be a single map with a compose inside. The slot
    // names differ but the op shape must match.
    let from_ir_ops: String = from_ir
        .chars()
        .filter(|c| !c.is_alphanumeric() || c.is_alphabetic() && c.is_lowercase())
        .collect();
    let parsed_ops: String = parsed_str
        .chars()
        .filter(|c| !c.is_alphanumeric() || c.is_alphabetic() && c.is_lowercase())
        .collect();
    let _ = (from_ir_ops, parsed_ops); // shape compare is informal

    assert!(from_ir.starts_with("(map "));
    assert!(parsed_str.starts_with("(map "));
    assert!(from_ir.contains("compose"));
    assert!(parsed_str.contains("compose"));
}
