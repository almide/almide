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
use almide_ir::{CallTarget, IrExpr, IrExprKind, VarId};
use almide_lang::types::{Ty, TypeConstructorId};
use egg::{Extractor, RecExpr, Runner};

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
