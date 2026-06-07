//! Whole-program synthesis: assemble a `fn main` body from a sequence
//! of typed `let` bindings, then print the observable ones.
//!
//! The shape is deliberately a *data-flow chain*: each binding may
//! reference earlier ones, and every binding is observed (printed) or
//! consumed by a later binding, so the program has no dead code (which
//! would trip the unused-variable warning the checker emits — and which
//! we treat as a generator bug, not a finding).

use crate::rng::SplitMix64;

use super::term::{gen_expr, Builder, ScopeVar, MAX_GOAL_DEPTH, START_FUEL};
use super::types::{render_print, GenType};
use super::{Generated, Origin, Signature};

/// Number of top-level `let` bindings in a synthesized program, chosen
/// from this inclusive range. Small programs minimize compile time and
/// keep repros readable; the range gives variety without bloat.
const MIN_BINDINGS: i64 = 2;
const MAX_BINDINGS: i64 = 6;

/// Scalar goal types the driver picks top-level bindings from, with
/// weights. String and Float dominate because they carry the densest
/// divergence surface (Unicode case/offsets, float formatting); the
/// compound types appear less often since they nest more cheaply via
/// the term generator's own recursion.
const GOAL_TYPE_WEIGHTS: &[(GoalKind, u32)] = &[
    (GoalKind::String, 10),
    (GoalKind::Float, 8),
    (GoalKind::Int, 7),
    (GoalKind::Bool, 4),
    (GoalKind::List, 6),
    (GoalKind::Option, 4),
    (GoalKind::Result, 4),
    (GoalKind::Tuple, 3),
    (GoalKind::Map, 3),
];

/// The top-level goal-type shapes the driver chooses from.
#[derive(Debug, Clone, Copy)]
enum GoalKind {
    Int,
    Float,
    String,
    Bool,
    List,
    Option,
    Result,
    Tuple,
    Map,
}

/// Scalar element types used when building a compound goal type.
const ELEM_CHOICES: &[GenType] = &[GenType::Int, GenType::String, GenType::Bool, GenType::Float];

/// Pick a concrete top-level goal type. ELEM_CHOICES are all depth-1,
/// so any compound goal here is at most depth-2 — within
/// `MAX_GOAL_DEPTH` by construction (asserted in debug builds).
fn pick_goal(rng: &mut SplitMix64) -> GenType {
    let weights: Vec<u32> = GOAL_TYPE_WEIGHTS.iter().map(|(_, w)| *w).collect();
    let kind = GOAL_TYPE_WEIGHTS[rng.pick_weighted(&weights)].0;

    let goal = match kind {
        GoalKind::Int => GenType::Int,
        GoalKind::Float => GenType::Float,
        GoalKind::String => GenType::String,
        GoalKind::Bool => GenType::Bool,
        GoalKind::List => GenType::List(Box::new(pick_elem(rng))),
        GoalKind::Option => GenType::Option(Box::new(pick_elem(rng))),
        GoalKind::Result => GenType::Result(Box::new(pick_elem(rng))),
        GoalKind::Tuple => GenType::Tuple2(Box::new(pick_elem(rng)), Box::new(pick_elem(rng))),
        GoalKind::Map => GenType::Map(Box::new(pick_elem(rng))),
    };
    debug_assert!(
        goal.depth() <= MAX_GOAL_DEPTH,
        "synthesized goal type exceeded MAX_GOAL_DEPTH"
    );
    // Every top-level goal must be printable+comparable across targets,
    // since each binding is observed. This holds by construction for the
    // chosen kinds; the assert documents and guards the invariant.
    debug_assert!(
        goal.is_observable(),
        "synthesized goal type is not observable"
    );
    goal
}

/// Pick a depth-1 scalar element type for a compound goal.
fn pick_elem(rng: &mut SplitMix64) -> GenType {
    ELEM_CHOICES[rng.below(ELEM_CHOICES.len() as u32) as usize].clone()
}

/// Synthesize one complete program from the current RNG state.
pub fn synthesize(rng: &mut SplitMix64, catalogue: &[Signature]) -> Generated {
    let n_bindings = rng.in_range(MIN_BINDINGS, MAX_BINDINGS) as usize;

    let mut builder = Builder {
        rng,
        catalogue,
        scope: Vec::new(),
        stmts: Vec::new(),
        fuel: 0,
        fresh: 0,
    };

    // The observables: (binding name, type) we will println at the end.
    let mut observables: Vec<(String, GenType)> = Vec::new();

    for _ in 0..n_bindings {
        let goal = pick_goal(builder.rng);
        builder.fuel = START_FUEL;
        let value_src = gen_expr(&mut builder, &goal);

        // Bind the result to a fresh name (unless gen_expr already
        // hoisted and returned a bare reference — in which case we still
        // add a thin alias binding so every top-level value is observed
        // uniformly). Binding-then-printing also defeats the unused-var
        // warning for hoisted intermediates.
        let name = format!("r{}", builder.fresh);
        builder.fresh += 1;
        builder
            .stmts
            .push(format!("let {name}: {} = {value_src}", goal.render()));
        builder.scope.push(ScopeVar {
            name: name.clone(),
            ty: goal.clone(),
        });
        observables.push((name, goal));
    }

    // Print every observable. This consumes all bindings (incl. hoisted
    // intermediates that flow into these), so no unused-variable warning
    // fires.
    let mut body = builder.stmts.clone();
    for (name, ty) in &observables {
        body.push(render_print(name, ty));
    }

    let source = render_main(&body);

    Generated {
        source,
        origin: Origin::Synthesis,
    }
}

/// Wrap a list of statement lines into a complete `fn main` program.
fn render_main(body: &[String]) -> String {
    let mut out = String::new();
    out.push_str("// generated by xtarget-fuzz (type-directed synthesis)\n");
    out.push_str("fn main() -> Unit = {\n");
    for line in body {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("}\n");
    out
}
