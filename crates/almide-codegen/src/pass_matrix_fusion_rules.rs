//! Fusion rule registry shared by `MatrixFusionPass`.
//!
//! Each fusion (`gelu(scale(add(mul(a, b), bias), α))` → a single fused
//! runtime call, etc.) is described once here as a declarative
//! `FusionRule`: an input `Pattern` tree plus a `rewrite` function that
//! produces the fused `IrExprKind` from the captured sub-expressions.
//!
//! The pass applies each rule in two modes without any per-pattern
//! boilerplate:
//! - **Nested form**: match against the pattern tree directly.
//! - **Let-split form**: treat a k-chain pattern (depth `k`) as a
//!   sequence of `k` consecutive `let` bindings where each intermediate
//!   var is used exactly once, and match a sliding k-gram of statements.
//!
//! ## Why this shape
//!
//! Each `Pattern` node corresponds 1-to-1 to an MLIR `Op` matcher (or an
//! egg `Rewrite<L, N>` left-hand side). When the MLIR/egg arc lands, the
//! translation from this table to the target IR is mechanical — the
//! schema is the bridge, and the fusion catalogue never has to be
//! rewritten.
//!
//! ## Adding a new fusion
//!
//! 1. Declare the input tree with `call()` / `cap()`.
//! 2. Provide a `rewrite` closure that assembles the output call from
//!    captured operands.
//! 3. Append the rule to `FUSION_RULES`.
//!
//! The pass automatically handles: nested matching, let-split chain
//! scanning, single-use enforcement on intermediate variables, IR
//! construction with inherited `ty` / `span`. No k-gram matcher to
//! hand-roll, no capture-shuffle to re-derive.

use almide_base::intern::sym;
use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr};

/// Input-side pattern node. The leaf `Capture` binds any sub-expression
/// to a named slot; every other node matches a `matrix.<func>(children...)`
/// call and recursively matches the children. Non-matrix calls, non-call
/// expressions, and arity mismatches all fail the match.
#[derive(Clone)]
pub enum Pattern {
    /// `matrix.<func>(children...)` with exact arity = `children.len()`.
    Call {
        func: &'static str,
        children: Vec<Pattern>,
    },
    /// Any expression; bound to `name` in the resulting `Match`.
    Capture(&'static str),
}

/// Captured sub-expressions keyed by the pattern's `Capture` names.
pub struct Match {
    slots: Vec<(&'static str, IrExpr)>,
}

impl Match {
    pub fn get(&self, name: &'static str) -> IrExpr {
        self.slots
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, e)| e.clone())
            .unwrap_or_else(|| panic!("fusion rule referenced missing capture '{}'", name))
    }
}

/// A single declarative fusion: input pattern → output call kind.
pub struct FusionRule {
    /// Stable identifier for diagnostics / tracing.
    pub name: &'static str,
    /// Input tree. Matched top-down; each `Call` node checks module =
    /// "matrix", func, and arity before recursing into children.
    pub pattern: Pattern,
    /// Produces the fused `IrExprKind` from captured operands. The pass
    /// wraps it back into an `IrExpr` with the original `ty` and `span`
    /// preserved, so rewriters never have to worry about those.
    pub rewrite: fn(&Match) -> IrExprKind,
}

/// Helper constructors keep the rule table compact.
pub fn call(func: &'static str, children: Vec<Pattern>) -> Pattern {
    Pattern::Call { func, children }
}
pub fn cap(name: &'static str) -> Pattern {
    Pattern::Capture(name)
}

impl Pattern {
    /// Attempt a top-down match of this pattern against `expr`. Returns
    /// `Some(match)` with every `Capture` slot bound; `None` if any
    /// module-name / func-name / arity check fails.
    pub fn try_match(&self, expr: &IrExpr) -> Option<Match> {
        let mut slots = Vec::new();
        if self.match_inner(expr, &mut slots) {
            Some(Match { slots })
        } else {
            None
        }
    }

    fn match_inner(&self, expr: &IrExpr, slots: &mut Vec<(&'static str, IrExpr)>) -> bool {
        match self {
            Pattern::Capture(name) => {
                slots.push((name, expr.clone()));
                true
            }
            Pattern::Call { func, children } => {
                let (module, f, args) = match &expr.kind {
                    IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } => {
                        (module, func, args)
                    }
                    _ => return false,
                };
                if module.as_str() != "matrix" || f.as_str() != *func || args.len() != children.len() {
                    return false;
                }
                for (child_pat, arg) in children.iter().zip(args.iter()) {
                    if !child_pat.match_inner(arg, slots) {
                        return false;
                    }
                }
                true
            }
        }
    }

    /// Number of `Call` layers along the deepest chain. `Capture` leaves
    /// count as 0, a `Call` counts as 1 + max-child depth. Used to
    /// derive the let-split k-gram length automatically.
    pub fn depth(&self) -> usize {
        match self {
            Pattern::Capture(_) => 0,
            Pattern::Call { children, .. } => {
                1 + children.iter().map(|c| c.depth()).max().unwrap_or(0)
            }
        }
    }

    /// Linearize a chain-shaped pattern (every `Call` node has at most
    /// one `Call` child) into a sequence from innermost → outermost.
    /// Each step records the func, the index where the nested call sits
    /// among the arguments, and the `Capture` names for the non-call
    /// arguments. Non-chain shapes return `None`.
    ///
    /// The output drives the let-split matcher: step `i` corresponds to
    /// the `i`-th `let` binding in the window. Step 0 is the innermost
    /// call (no nested child); later steps reference the previous
    /// step's bound var at `nested_arg_idx`.
    pub fn linearize_chain(&self) -> Option<Vec<ChainStep>> {
        let mut steps = Vec::new();
        let mut cur: Option<&Pattern> = Some(self);
        while let Some(p) = cur {
            let (func, children) = match p {
                Pattern::Call { func, children } => (*func, children),
                Pattern::Capture(_) => return None, // chain can't be just a capture
            };
            let mut nested_idx: Option<usize> = None;
            let mut captures: Vec<(usize, &'static str)> = Vec::new();
            for (idx, c) in children.iter().enumerate() {
                match c {
                    Pattern::Call { .. } => {
                        if nested_idx.is_some() {
                            // More than one nested call — not a chain.
                            return None;
                        }
                        nested_idx = Some(idx);
                    }
                    Pattern::Capture(name) => captures.push((idx, *name)),
                }
            }
            steps.push(ChainStep {
                func,
                arity: children.len(),
                nested_arg_idx: nested_idx,
                captures,
            });
            cur = match nested_idx {
                Some(i) => Some(&children[i]),
                None => None,
            };
        }
        steps.reverse(); // innermost first
        Some(steps)
    }
}

/// One step of a linearized chain pattern.
pub struct ChainStep {
    pub func: &'static str,
    pub arity: usize,
    /// Index within `args` where the nested call sits. `None` means
    /// this is the innermost step (no nested call — all args are
    /// captures).
    pub nested_arg_idx: Option<usize>,
    /// Capture slots: `(arg_index, capture_name)`.
    pub captures: Vec<(usize, &'static str)>,
}

/// Walk the let-split k-gram for a chain-shaped rule. Returns a `Match`
/// with every capture populated from the window's IR, plus the VarIds
/// of the intermediate bindings (so the caller can verify single-use
/// and know which statements to collapse).
pub struct ChainLetMatch {
    pub captures: Match,
    /// VarIds of intermediate let bindings (step 0 .. step k-2). The
    /// outermost bind's VarId, mutability, and type come from the caller.
    pub intermediate_vars: Vec<VarId>,
}

pub fn match_chain_let_window(
    steps: &[ChainStep],
    window: &[IrStmt],
) -> Option<ChainLetMatch> {
    if steps.len() != window.len() || window.is_empty() {
        return None;
    }
    let mut slots: Vec<(&'static str, IrExpr)> = Vec::new();
    let mut prev_var: Option<VarId> = None;
    let mut intermediates: Vec<VarId> = Vec::new();

    for (i, step) in steps.iter().enumerate() {
        let stmt = &window[i];
        let (var_id, value) = match &stmt.kind {
            IrStmtKind::Bind { var, value, .. } => (*var, value),
            _ => return None,
        };
        let (module, f, args) = match &value.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } => {
                (module, func, args)
            }
            _ => return None,
        };
        if module.as_str() != "matrix" || f.as_str() != step.func || args.len() != step.arity {
            return None;
        }
        // Collect captures for this step.
        for (idx, name) in &step.captures {
            slots.push((name, args[*idx].clone()));
        }
        // If this step has a nested arg slot, it must reference the
        // previously bound var.
        if let Some(nested_idx) = step.nested_arg_idx {
            let prev = prev_var.expect("first step must not have nested_arg_idx");
            let nested_arg = &args[nested_idx];
            if !is_var_with_id(nested_arg, prev) {
                return None;
            }
        } else if i != 0 {
            // Non-first step with no nested_arg_idx is a schema bug.
            return None;
        }
        if i + 1 < window.len() {
            intermediates.push(var_id);
        }
        prev_var = Some(var_id);
    }
    Some(ChainLetMatch {
        captures: Match { slots },
        intermediate_vars: intermediates,
    })
}

fn is_var_with_id(expr: &IrExpr, target: VarId) -> bool {
    matches!(&expr.kind, IrExprKind::Var { id } if *id == target)
}

/// Count references to `target` across a slice of statements plus an
/// optional block tail. Used by the let-split matcher to skip fusions
/// that would orphan a still-live intermediate.
pub fn count_var_refs(stmts: &[IrStmt], tail: Option<&IrExpr>, target: VarId) -> usize {
    struct Counter { target: VarId, count: usize }
    impl IrVisitor for Counter {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Var { id } = &expr.kind {
                if *id == self.target { self.count += 1; }
            }
            walk_expr(self, expr);
        }
    }
    let mut v = Counter { target, count: 0 };
    for s in stmts { v.visit_stmt(s); }
    if let Some(t) = tail { v.visit_expr(t); }
    v.count
}

// ── Rewriter helpers ────────────────────────────────────────────────

/// Build a `matrix.<func>(args...)` call as an `IrExprKind`. The caller
/// wraps it with `IrExpr { ty, span }` inherited from the original.
pub fn build_matrix_call(func: &'static str, args: Vec<IrExpr>) -> IrExprKind {
    IrExprKind::Call {
        target: CallTarget::Module {
            module: sym("matrix"),
            func: sym(func),
        },
        args,
        type_args: vec![],
    }
}

// ── The rule registry ───────────────────────────────────────────────

/// All chain-fusion rules considered by `MatrixFusionPass`. Order is
/// deliberate: rules matching **more specific** / **deeper** patterns
/// come first so an outer rule can't fire before its inner
/// prerequisites are rewritten into their canonical form.
pub fn fusion_rules() -> Vec<FusionRule> {
    vec![
        // 4-deep: gelu(scale(add(mul(a, b), bias), alpha)) →
        //         fused_gemm_bias_scale_gelu(a, b, bias, alpha)
        FusionRule {
            name: "gemm_bias_scale_gelu",
            pattern: call("gelu", vec![
                call("scale", vec![
                    call("add", vec![
                        call("mul", vec![cap("a"), cap("b")]),
                        cap("bias"),
                    ]),
                    cap("alpha"),
                ]),
            ]),
            rewrite: |m| build_matrix_call("fused_gemm_bias_scale_gelu", vec![
                m.get("a"), m.get("b"), m.get("bias"), m.get("alpha"),
            ]),
        },
        // 3-deep: softmax_rows(scale(mul(q, kt), s)) → attention_weights(q, kt, s)
        FusionRule {
            name: "attention_weights",
            pattern: call("softmax_rows", vec![
                call("scale", vec![
                    call("mul", vec![cap("q"), cap("kt")]),
                    cap("scale"),
                ]),
            ]),
            rewrite: |m| build_matrix_call("attention_weights", vec![
                m.get("q"), m.get("kt"), m.get("scale"),
            ]),
        },
        // 2-deep: mul(attention_weights(q, kt, s), v) →
        //         scaled_dot_product_attention(q, kt, v, s)
        FusionRule {
            name: "scaled_dot_product_attention",
            pattern: call("mul", vec![
                call("attention_weights", vec![cap("q"), cap("kt"), cap("scale")]),
                cap("v"),
            ]),
            rewrite: |m| build_matrix_call("scaled_dot_product_attention", vec![
                m.get("q"), m.get("kt"), m.get("v"), m.get("scale"),
            ]),
        },
        // 2-deep: linear_row(layer_norm_rows(x, γ, β, ε), W, b) →
        //         pre_norm_linear(x, γ, β, ε, W, b)
        FusionRule {
            name: "pre_norm_linear",
            pattern: call("linear_row", vec![
                call("layer_norm_rows", vec![
                    cap("x"), cap("gamma"), cap("beta"), cap("eps"),
                ]),
                cap("w"),
                cap("bias"),
            ]),
            rewrite: |m| build_matrix_call("pre_norm_linear", vec![
                m.get("x"), m.get("gamma"), m.get("beta"), m.get("eps"),
                m.get("w"), m.get("bias"),
            ]),
        },
        // 2-deep: gelu(linear_row(x, W, b)) → linear_row_gelu(x, W, b)
        FusionRule {
            name: "linear_row_gelu",
            pattern: call("gelu", vec![
                call("linear_row", vec![cap("x"), cap("w"), cap("bias")]),
            ]),
            rewrite: |m| build_matrix_call("linear_row_gelu", vec![
                m.get("x"), m.get("w"), m.get("bias"),
            ]),
        },
        // 2-deep: mul(a, scale(b, s)) → mul_scaled(a, s, b)
        FusionRule {
            name: "mul_scaled_rhs",
            pattern: call("mul", vec![
                cap("a"),
                call("scale", vec![cap("b"), cap("s")]),
            ]),
            rewrite: |m| build_matrix_call("mul_scaled", vec![
                m.get("a"), m.get("s"), m.get("b"),
            ]),
        },
        // 2-deep: mul(scale(a, s), b) → mul_scaled(a, s, b). Same fused
        // runtime, different input shape — the lhs-scaled mirror
        // collapses to the same α(A@B) identity.
        FusionRule {
            name: "mul_scaled_lhs",
            pattern: call("mul", vec![
                call("scale", vec![cap("a"), cap("s")]),
                cap("b"),
            ]),
            rewrite: |m| build_matrix_call("mul_scaled", vec![
                m.get("a"), m.get("s"), m.get("b"),
            ]),
        },
    ]
}
