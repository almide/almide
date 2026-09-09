//! StreamFusionPass: `|>` chains of list combinators → one Rust iterator
//! expression (`IrExprKind::IterChain`), Rust target only (#2045).
//!
//! Runs right after `StdlibLowering`, on the `RuntimeCall { almide_rt_list_* }`
//! shape `IntrinsicLowering` produced. (The intercept that used to sit inside
//! `StdlibLowering` matched `Module { list, .. }`, which no longer exists by
//! then — the pass was dead, every stage materialised its `Vec`, and the egg
//! list rules that DID fire composed lambdas by substitution, duplicating a
//! callback's side effects. Both are gone.)
//!
//! Two rewrites, bottom-up:
//!
//! 1. **Single stage** — one `almide_rt_list_{map,filter,flat_map,filter_map,
//!    fold,find,any,all,count}` call whose callback is a lambda LITERAL
//!    becomes an `IterChain` with one step / collector. This never changes
//!    observable behaviour: the runtime twin is the same adapter over the same
//!    elements in the same order. What it removes is the `Rc<dyn Fn>` box and
//!    the indirect call per element.
//!
//! 2. **Merge** — an `IterChain` whose source is itself a `Collect`-ending
//!    `IterChain` is flattened into one chain (`list.take`, `list.sum` and
//!    `list.len` over a chain join as a `Take` step / `Sum` / `Len`
//!    collector). This DOES change the order callbacks run in — per element
//!    instead of per stage — so it fires only when that order is
//!    unobservable:
//!
//!    - every callback (and a `fold` init / `take` count) is **pure**: no
//!      `println`, no effect-module call, no `!`, no assignment, no unknown
//!      callee — see `Purity::pure`;
//!    - at most ONE stage can abort (index out of range, `/ 0`, a runtime fn
//!      that is not on the total allowlist): with two, which abort fires
//!      first depends on the order;
//!    - every stage BEFORE a short-circuit point (`take`, `find`, `any`,
//!      `all`) is total: stage-by-stage runs it over the whole list, the fused
//!      chain stops early, and an abort the spec leg reports must not vanish.
//!
//!    An impure chain stays nested (`(inner.collect()).into_iter()…`): one
//!    `Vec` per stage, exactly the spec order. The fallible `!` form never
//!    reaches here (it lowers to the self-hosted `__fallible_*` twins).
//!
//! Ablation: `ALMIDE_STREAM_FUSION_OFF=1` skips the pass entirely.

use std::collections::HashSet;

use almide_base::intern::{sym, Sym};
use almide_ir::*;

use super::pass::{NanoPass, PassResult, Target};
use super::pass_rust_lowering::rewrite_tail_list_to_array;
use super::pass_stdlib_lowering::{prepare_lambda, prepare_lambda_borrowed};

/// Ablation knob: set to skip the pass (the win is then attributable).
pub const ABLATION_ENV: &str = "ALMIDE_STREAM_FUSION_OFF";

#[derive(Debug)]
pub struct StreamFusionPass;

impl NanoPass for StreamFusionPass {
    fn name(&self) -> &str { "StreamFusion" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["StdlibLowering"] }
    fn run_before(&self) -> Vec<&'static str> { vec!["RustLowering"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        if std::env::var_os(ABLATION_ENV).is_some() {
            return PassResult { program, changed: false };
        }
        let purity = Purity::of(&program);
        let mut v = Fuser { purity: &purity, changed: false };
        for f in &mut program.functions { v.visit_expr_mut(&mut f.body); }
        for tl in &mut program.top_lets { v.visit_expr_mut(&mut tl.value); }
        for m in &mut program.modules {
            for f in &mut m.functions { v.visit_expr_mut(&mut f.body); }
            for tl in &mut m.top_lets { v.visit_expr_mut(&mut tl.value); }
        }
        let changed = v.changed;
        PassResult { program, changed }
    }
}

// ── Purity / totality vocabulary ──────────────────────────────────────

/// Runtime modules whose calls are effects (or read the outside world).
const EFFECT_MODULES: &[&str] = &[
    "args", "datetime", "env", "fan", "fs", "http", "io", "mem", "net",
    "process", "random", "sse", "testing", "time", "zlib",
];

/// Runtime modules whose every fn is TOTAL: pure arithmetic / conversion
/// that never aborts. (`list`/`string`/`map` have aborting members —
/// `list.chunk(xs, 0)` — so a call into them counts as a possible abort.)
const TOTAL_MODULES: &[&str] = &["int", "float", "math", "bool"];

/// Stdlib modules a leftover `Module { .. }` call (bundled pure-Almide fn)
/// may name and still be pure.
fn stdlib_module_is_pure(module: &str) -> bool {
    almide_lang::stdlib_info::is_stdlib_module(module) && !EFFECT_MODULES.contains(&module)
}

/// Purity uses a greatest fixpoint; recursion alone is not an effect.
/// Totality uses a least fixpoint: a recursive cycle is not a termination proof.
struct Purity {
    pure: HashSet<Sym>,
    total: HashSet<Sym>,
}

impl Purity {
    fn of(program: &IrProgram) -> Purity {
        let mut bodies: Vec<(Vec<Sym>, &IrExpr)> = Vec::new();
        for f in &program.functions {
            if !f.is_effect && !f.is_test {
                bodies.push((vec![f.name], &f.body));
            }
        }
        for m in &program.modules {
            let mod_name = m.versioned_name.map(|v| v.to_string()).unwrap_or_else(|| m.name.to_string());
            let mod_ident = mod_name.replace('.', "_");
            for f in &m.functions {
                if f.is_effect || f.is_test { continue; }
                // Every spelling a call site may carry after StdlibLowering's
                // intra-module rename (see `prefix_intra_module_calls`).
                // A module-local bare name is not a root function alias.
                // Sharing it would let a pure module function prove an unrelated
                // root function with the same name pure (and even total).
                let spellings = vec![
                    sym(&format!("{}.{}", m.name, f.name)),
                    sym(&format!("almide_rt_{}_{}", mod_ident, f.name.as_str().replace('.', "_"))),
                ];
                bodies.push((spellings, &f.body));
            }
        }
        let fixpoint = |total: bool| -> HashSet<Sym> {
            let mut set: HashSet<Sym> = if total {
                HashSet::new()
            } else {
                bodies.iter().flat_map(|(names, _)| names.iter().copied()).collect()
            };
            loop {
                let before = set.len();
                let snapshot = set;
                set = HashSet::new();
                for (names, body) in &bodies {
                    let cx = Cx { fns: &snapshot, total };
                    if (total || names.iter().all(|n| snapshot.contains(n))) && expr_ok(body, &cx) {
                        set.extend(names.iter().copied());
                    }
                }
                if set.len() == before { break; }
            }
            set
        };
        Purity { pure: fixpoint(false), total: fixpoint(true) }
    }

    fn pure(&self, e: &IrExpr) -> bool { expr_ok(e, &Cx { fns: &self.pure, total: false }) }
    fn total(&self, e: &IrExpr) -> bool { expr_ok(e, &Cx { fns: &self.total, total: true }) }
}

struct Cx<'a> {
    fns: &'a HashSet<Sym>,
    /// `true`: also refuse anything that can abort (the totality check).
    total: bool,
}

fn all_ok(es: &[IrExpr], cx: &Cx) -> bool { es.iter().all(|e| expr_ok(e, cx)) }

fn stmts_ok(ss: &[IrStmt], cx: &Cx) -> bool {
    ss.iter().all(|s| match &s.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => expr_ok(value, cx),
        IrStmtKind::Expr { expr } => expr_ok(expr, cx),
        IrStmtKind::Comment { .. } => true,
        // Any mutation — of a local or a captured cell — is an effect whose
        // interleaving is observable.
        _ => false,
    })
}

fn runtime_ok(symbol: &str, cx: &Cx) -> bool {
    let Some(rest) = symbol.strip_prefix("almide_rt_") else { return false };
    let module = rest.split('_').next().unwrap_or("");
    if !stdlib_module_is_pure(module) { return false; }
    !cx.total || TOTAL_MODULES.contains(&module)
}

/// Only the array template emitted by `rewrite_tail_list_to_array` has a
/// known effect contract. Absence of suspicious substrings proves nothing
/// about arbitrary Rust, which can call an effect through any function name.
fn inline_rust_ok(template: &str, _cx: &Cx) -> bool {
    let Some(inner) = template.strip_prefix('[').and_then(|t| t.strip_suffix(']')) else { return false };
    inner.is_empty() || inner.split(", ").all(|p| {
        p.strip_prefix("{e").and_then(|p| p.strip_suffix('}'))
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// A runtime HOF executes function arguments. Merely constructing a FnRef
/// is pure, but invoking it needs the callee's proof (and an unknown local
/// function value has none). Other argument expressions retain normal rules.
fn runtime_arg_ok(arg: &IrExpr, cx: &Cx) -> bool {
    if !arg.ty.is_fn() { return expr_ok(arg, cx); }
    match &arg.kind {
        IrExprKind::FnRef { name } => cx.fns.contains(name),
        IrExprKind::Lambda { body, .. } => expr_ok(body, cx),
        IrExprKind::Clone { expr } | IrExprKind::RcWrap { expr, .. }
        | IrExprKind::Borrow { expr, mutable: false, .. } => runtime_arg_ok(expr, cx),
        _ => false,
    }
}

/// Is `e` inside the vocabulary — pure, and (under `cx.total`) non-aborting?
fn expr_ok(e: &IrExpr, cx: &Cx) -> bool {
    // Disjoint expression families; Some(false) must stop the chain. Unknown
    // nodes, mutable borrows, propagation and loops have no purity proof.
    literal_ok(e, cx)
        .or_else(|| operator_ok(e, cx))
        .or_else(|| control_ok(e, cx))
        .or_else(|| aggregate_ok(e, cx))
        .or_else(|| wrapper_ok(e, cx))
        .or_else(|| call_ok(e, cx))
        .unwrap_or(false)
}

fn literal_ok(e: &IrExpr, _cx: &Cx) -> Option<bool> {
    use IrExprKind as K;
    Some(match &e.kind {
        K::LitInt { .. } | K::LitFloat { .. } | K::LitStr { .. } | K::LitBool { .. } | K::Unit
        | K::Var { .. } | K::FnRef { .. } | K::OptionNone | K::EmptyMap => true,
        _ => return None,
    })
}

fn operator_ok(e: &IrExpr, cx: &Cx) -> Option<bool> {
    use IrExprKind as K;
    Some(match &e.kind {
        K::BinOp { op, left, right } => {
            // `/ 0`, `% 0` and `^ -n` abort (C-001 family); a literal right
            // operand that is non-zero / non-negative cannot.
            let aborts = match op {
                BinOp::DivInt | BinOp::ModInt => !matches!(right.kind, K::LitInt { value } if value != 0),
                BinOp::PowInt => !matches!(right.kind, K::LitInt { value } if value >= 0),
                _ => false,
            };
            (!cx.total || !aborts) && expr_ok(left, cx) && expr_ok(right, cx)
        }
        K::UnOp { operand, .. } => expr_ok(operand, cx),
        _ => return None,
    })
}

fn control_ok(e: &IrExpr, cx: &Cx) -> Option<bool> {
    use IrExprKind as K;
    Some(match &e.kind {
        K::If { cond, then, else_ } => expr_ok(cond, cx) && expr_ok(then, cx) && expr_ok(else_, cx),
        K::Match { subject, arms } => {
            expr_ok(subject, cx)
                && arms.iter().all(|a| a.guard.as_ref().is_none_or(|g| expr_ok(g, cx)) && expr_ok(&a.body, cx))
        }
        K::Block { stmts, expr } => stmts_ok(stmts, cx) && expr.as_deref().is_none_or(|t| expr_ok(t, cx)),
        K::Lambda { body, .. } => expr_ok(body, cx),
        _ => return None,
    })
}

fn aggregate_ok(e: &IrExpr, cx: &Cx) -> Option<bool> {
    use IrExprKind as K;
    Some(match &e.kind {
        K::List { elements } | K::Tuple { elements } => all_ok(elements, cx),
        K::MapLiteral { entries } => entries.iter().all(|(k, v)| expr_ok(k, cx) && expr_ok(v, cx)),
        K::Record { fields, .. } => fields.iter().all(|(_, f)| expr_ok(f, cx)),
        K::SpreadRecord { base, fields } => expr_ok(base, cx) && fields.iter().all(|(_, f)| expr_ok(f, cx)),
        K::Range { start, end, .. } => expr_ok(start, cx) && expr_ok(end, cx),
        K::Member { object, .. } | K::TupleIndex { object, .. } => expr_ok(object, cx),
        K::MapAccess { object, key } => expr_ok(object, cx) && expr_ok(key, cx),
        // `xs[i]` aborts out of range.
        K::IndexAccess { object, index } => !cx.total && expr_ok(object, cx) && expr_ok(index, cx),
        K::StringInterp { parts } => parts.iter().all(|p| match p {
            IrStringPart::Lit { .. } => true,
            IrStringPart::Expr { expr } => expr_ok(expr, cx),
        }),
        _ => return None,
    })
}

fn wrapper_ok(e: &IrExpr, cx: &Cx) -> Option<bool> {
    use IrExprKind as K;
    Some(match &e.kind {
        K::OptionSome { expr } | K::ResultOk { expr } | K::ResultErr { expr }
        | K::ToOption { expr } | K::OptionalChain { expr, .. }
        | K::Clone { expr } | K::Deref { expr } | K::Borrow { expr, mutable: false, .. }
        | K::BoxNew { expr } | K::RcWrap { expr, .. } | K::ToVec { expr } => expr_ok(expr, cx),
        K::UnwrapOr { expr, fallback } => expr_ok(expr, cx) && expr_ok(fallback, cx),
        _ => return None,
    })
}

fn call_ok(e: &IrExpr, cx: &Cx) -> Option<bool> {
    use IrExprKind as K;
    Some(match &e.kind {
        K::Call { target, args, .. } => {
            all_ok(args, cx)
                && match target {
                    CallTarget::Named { name } => cx.fns.contains(name),
                    CallTarget::Module { module, func, .. } => {
                        cx.fns.contains(&sym(&format!("{}.{}", module, func)))
                            || (!cx.total && stdlib_module_is_pure(module.as_str())
                                && args.iter().all(|a| runtime_arg_ok(a, cx)))
                    }
                    CallTarget::Method { .. } | CallTarget::Computed { .. } => false,
                }
        }
        K::RuntimeCall { symbol, args } => runtime_ok(symbol.as_str(), cx) && args.iter().all(|a| runtime_arg_ok(a, cx)),
        K::InlineRust { template, args } => {
            inline_rust_ok(template, cx) && args.iter().all(|(_, a)| expr_ok(a, cx))
        }
        K::IterChain { source, steps, collector, .. } => {
            expr_ok(source, cx)
                && steps.iter().all(|s| match s {
                    IterStep::Take { n } => expr_ok(n, cx),
                    _ => s.lambda().is_some_and(|l| expr_ok(l, cx)),
                })
                && match collector {
                    IterCollector::Fold { init, lambda } => expr_ok(init, cx) && expr_ok(lambda, cx),
                    other => other.lambda().is_none_or(|l| expr_ok(l, cx)),
                }
        }
        _ => return None,
    })
}

// ── The rewrite ───────────────────────────────────────────────────────

struct Fuser<'a> {
    purity: &'a Purity,
    changed: bool,
}

impl<'a> IrMutVisitor for Fuser<'a> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        walk_expr_mut(self, expr);
        if let Some(fused) = self.rewrite(expr) {
            *expr = fused;
            self.changed = true;
        }
    }
}

/// The callback arg as a lambda literal. A fresh literal wrapped in `Clone`
/// by CloneInsertion is the same thing; so is CaptureClone's
/// `{ let __cap = v.clone(); <lambda> }` block (a heap capture) — the block
/// renders as a closure expression and is kept whole, its tail is the lambda.
fn take_lambda(arg: IrExpr) -> Option<IrExpr> {
    match arg.kind {
        IrExprKind::Lambda { .. } => Some(arg),
        IrExprKind::Clone { expr } if matches!(expr.kind, IrExprKind::Lambda { .. }) => Some(*expr),
        IrExprKind::Block { ref expr, .. } if expr.as_deref().is_some_and(|t| matches!(t.kind, IrExprKind::Lambda { .. })) => {
            Some(arg)
        }
        _ => None,
    }
}

/// Apply `f` to the lambda node itself, through a capture-clone block.
fn map_lambda(callback: IrExpr, f: &dyn Fn(IrExpr) -> IrExpr) -> IrExpr {
    match callback.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } => IrExpr {
            kind: IrExprKind::Block { stmts, expr: Some(Box::new(f(*tail))) },
            ty: callback.ty, span: callback.span, def_id: callback.def_id,
        },
        _ => f(callback),
    }
}

/// The lambda's body, through a capture-clone block.
fn lambda_body_mut(callback: &mut IrExpr) -> Option<&mut IrExpr> {
    match &mut callback.kind {
        IrExprKind::Lambda { body, .. } => Some(body),
        IrExprKind::Block { expr: Some(tail), .. } => lambda_body_mut(tail),
        _ => None,
    }
}

/// `(source, consume)`: a borrowed source (`&xs`, the `&[A]` runtime twins'
/// arg shape) iterates as `.iter().cloned()`, anything else is consumed.
fn source_of(arg: IrExpr) -> (IrExpr, bool) {
    match arg.kind {
        IrExprKind::Borrow { expr, .. } => (*expr, false),
        _ => (arg, true),
    }
}

fn chain(expr: &IrExpr, source: IrExpr, consume: bool, steps: Vec<IterStep>, collector: IterCollector) -> IrExpr {
    IrExpr {
        kind: IrExprKind::IterChain { source: Box::new(source), consume, steps, collector },
        ty: expr.ty.clone(),
        span: expr.span,
        def_id: None,
    }
}

impl<'a> Fuser<'a> {
    fn rewrite(&self, expr: &mut IrExpr) -> Option<IrExpr> {
        let IrExprKind::RuntimeCall { symbol, args } = &expr.kind else { return None };
        let op = symbol.as_str().strip_prefix("almide_rt_list_")?;
        let single = match op {
            "map" | "filter" | "flat_map" | "filter_map" | "find" | "any" | "all" | "count" if args.len() == 2 => {
                self.single_stage(expr, op)?
            }
            "fold" if args.len() == 3 => self.single_stage(expr, op)?,
            "take" if args.len() == 2 => return self.take(expr),
            "sum" | "sum_float" | "len" if args.len() == 1 => return self.reducer(expr, op),
            _ => return None,
        };
        Some(self.merge(single))
    }

    /// Rewrite 1: one runtime combinator call with a lambda literal → a
    /// one-step / one-collector chain. Order-preserving by construction.
    fn single_stage(&self, expr: &IrExpr, op: &str) -> Option<IrExpr> {
        let IrExprKind::RuntimeCall { args, .. } = &expr.kind else { return None };
        let callback = take_lambda(args.last()?.clone())?;
        let (source, consume) = source_of(args[0].clone());
        // Rust hands `filter` / `find` / the `count` filter a `&T`; every
        // other adapter (and the `.iter().cloned()` borrowed source) an owned `T`.
        let borrowed = matches!(op, "filter" | "find" | "count");
        let lambda = map_lambda(callback, if borrowed { &prepare_lambda_borrowed } else { &prepare_lambda });
        let (steps, collector) = match op {
            "map" => (vec![IterStep::Map { lambda: Box::new(lambda) }], IterCollector::Collect),
            "filter" => (vec![IterStep::Filter { lambda: Box::new(lambda) }], IterCollector::Collect),
            "filter_map" => (vec![IterStep::FilterMap { lambda: Box::new(lambda) }], IterCollector::Collect),
            "flat_map" => {
                // #1337: a fixed-arity list literal tail becomes an ARRAY, so
                // the fused `flat_map` allocates nothing per element (the
                // runtime twin's `_arr` form, kept for the ablated pipeline).
                let mut lambda = lambda;
                if let Some(body) = lambda_body_mut(&mut lambda) {
                    rewrite_tail_list_to_array(body);
                }
                (vec![IterStep::FlatMap { lambda: Box::new(lambda) }], IterCollector::Collect)
            }
            "fold" => (vec![], IterCollector::Fold { init: Box::new(args[1].clone()), lambda: Box::new(lambda) }),
            "find" => (vec![], IterCollector::Find { lambda: Box::new(lambda) }),
            "any" => (vec![], IterCollector::Any { lambda: Box::new(lambda) }),
            "all" => (vec![], IterCollector::All { lambda: Box::new(lambda) }),
            "count" => (vec![], IterCollector::Count { lambda: Box::new(lambda) }),
            _ => return None,
        };
        Some(chain(expr, source, consume, steps, collector))
    }

    /// `list.take(chain, n)` → a `Take` step on the chain — a short-circuit
    /// point, so it needs the merge's totality rule.
    fn take(&self, expr: &IrExpr) -> Option<IrExpr> {
        let IrExprKind::RuntimeCall { args, .. } = &expr.kind else { return None };
        let (source, consume) = source_of(args[0].clone());
        let n = args[1].clone();
        let outer = chain(expr, source, consume, vec![IterStep::Take { n: Box::new(n) }], IterCollector::Collect);
        let merged = self.merge(outer);
        // A `take` over a non-chain (or an unmergeable one) is left to the runtime.
        match &merged.kind {
            IrExprKind::IterChain { steps, .. } if steps.len() > 1 => Some(merged),
            _ => None,
        }
    }

    /// `list.sum(&chain)` / `list.len(&chain)` → the chain with a `Sum` /
    /// `Len` collector. Same elements, same order, one fewer `Vec` — no
    /// purity condition.
    fn reducer(&self, expr: &IrExpr, op: &str) -> Option<IrExpr> {
        let IrExprKind::RuntimeCall { args, .. } = &expr.kind else { return None };
        let (inner, _) = source_of(args[0].clone());
        let IrExprKind::IterChain { source, consume, steps, collector: IterCollector::Collect } = inner.kind else { return None };
        let collector = match op {
            "sum" => IterCollector::Sum { float: false },
            "sum_float" => IterCollector::Sum { float: true },
            _ => IterCollector::Len,
        };
        Some(chain(expr, *source, consume, steps, collector))
    }

    /// Rewrite 2: flatten `outer(inner.collect())` when the interleaving is
    /// unobservable (module doc, "Merge"). Otherwise `outer` is returned as
    /// built — nested, one `Vec` per stage.
    fn merge(&self, outer: IrExpr) -> IrExpr {
        let IrExprKind::IterChain { source, consume, steps, collector } = outer.kind else { unreachable!() };
        let inner_is_chain = matches!(&source.kind,
            IrExprKind::IterChain { collector: IterCollector::Collect, .. });
        if !inner_is_chain || !self.mergeable(&source, &steps, &collector) {
            let kind = IrExprKind::IterChain { source, consume, steps, collector };
            return IrExpr { kind, ty: outer.ty, span: outer.span, def_id: None };
        }
        let IrExprKind::IterChain { source: inner_source, consume, steps: mut inner_steps, .. } = source.kind else { unreachable!() };
        inner_steps.extend(steps);
        IrExpr {
            kind: IrExprKind::IterChain { source: inner_source, consume, steps: inner_steps, collector },
            ty: outer.ty, span: outer.span, def_id: None,
        }
    }

    fn mergeable(&self, inner: &IrExpr, outer_steps: &[IterStep], collector: &IterCollector) -> bool {
        let IrExprKind::IterChain { steps: inner_steps, .. } = &inner.kind else { return false };
        // One entry per stage: (callback / count exprs, short-circuits?).
        let mut stages: Vec<(Vec<&IrExpr>, bool)> = Vec::new();
        for s in inner_steps.iter().chain(outer_steps.iter()) {
            match s {
                IterStep::Take { n } => stages.push((vec![n.as_ref()], true)),
                _ => stages.push((vec![s.lambda().expect("non-take step has a lambda")], false)),
            }
        }
        match collector {
            IterCollector::Fold { init, lambda } => stages.push((vec![init.as_ref(), lambda.as_ref()], false)),
            IterCollector::Find { lambda } | IterCollector::Any { lambda } | IterCollector::All { lambda } => {
                stages.push((vec![lambda.as_ref()], true))
            }
            IterCollector::Count { lambda } => stages.push((vec![lambda.as_ref()], false)),
            IterCollector::Collect | IterCollector::Sum { .. } | IterCollector::Len => {}
        }
        if !stages.iter().all(|(es, _)| es.iter().all(|e| self.purity.pure(e))) {
            return false;
        }
        let total: Vec<bool> = stages.iter().map(|(es, _)| es.iter().all(|e| self.purity.total(e))).collect();
        if total.iter().filter(|t| !**t).count() > 1 {
            return false;
        }
        // Every stage before a short-circuit point must be total.
        let mut seen_partial = false;
        for (i, (_, short)) in stages.iter().enumerate() {
            if *short && seen_partial { return false; }
            if !total[i] { seen_partial = true; }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_lang::types::Ty;

    fn function(name: &str, callee: Option<&str>) -> IrFunction {
        let kind = callee.map_or(IrExprKind::LitInt { value: 1 }, |name| IrExprKind::Call {
            target: CallTarget::Named { name: sym(name) }, args: vec![], type_args: vec![],
        });
        IrFunction {
            name: sym(name), params: vec![], ret_ty: Ty::Int,
            body: IrExpr { kind, ty: Ty::Int, span: None, def_id: None },
            is_effect: false, is_test: false, generics: None, extern_attrs: vec![],
            export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Private,
            doc: None, blank_lines_before: 0, def_id: None, mutated_params: vec![], module_origin: None, // fresh-fn: parameterless purity test fixture
        }
    }

    #[test]
    fn recursive_functions_are_pure_but_not_proven_total() {
        let program = IrProgram { functions: vec![
            function("direct", Some("direct")), function("left", Some("right")),
            function("right", Some("left")), function("caller", Some("direct")),
            function("outer", Some("inner")), function("inner", Some("leaf")), function("leaf", None),
        ], ..Default::default() };
        let purity = Purity::of(&program);
        for name in ["direct", "left", "right", "caller"] {
            assert!(purity.pure.contains(&sym(name)));
            assert!(!purity.total.contains(&sym(name)), "recursion is not a termination proof: {name}");
        }
        for name in ["outer", "inner", "leaf"] {
            assert!(purity.total.contains(&sym(name)), "acyclic total calls must propagate: {name}");
        }
    }

    #[test]
    fn a_function_argument_needs_a_callee_proof() {
        let fns = HashSet::from([sym("known")]);
        let cx = Cx { fns: &fns, total: false };
        let fn_ty = Ty::Fn { params: vec![], ret: Box::new(Ty::Int), is_effect: false };
        for (name, expected) in [("known", true), ("prints", false)] {
            let arg = IrExpr { kind: IrExprKind::FnRef { name: sym(name) },
                ty: fn_ty.clone(), span: None, def_id: None };
            assert_eq!(runtime_arg_ok(&arg, &cx), expected);
        }
        let borrowed = IrExpr { kind: IrExprKind::Borrow {
            expr: Box::new(IrExpr::default()), as_str: false, mutable: true,
        }, ty: Ty::Int, span: None, def_id: None };
        assert!(!expr_ok(&borrowed, &cx));
    }

    #[test]
    fn unknown_rust_and_effectful_runtime_calls_are_not_pure() {
        let fns = HashSet::new();
        let cx = Cx { fns: &fns, total: false };
        for template in ["emit_event({e0})", "[{event()}]", "[{e0}; side_effect()]"] {
            assert!(!inline_rust_ok(template, &cx), "unproven template: {template}");
        }
        for symbol in ["almide_rt_net_connect", "almide_rt_sse_openai_chat", "almide_rt_mem_restore", "almide_rt_unknown_action"] {
            assert!(!runtime_ok(symbol, &cx), "unproven runtime call: {symbol}");
        }
        assert!(inline_rust_ok("[{e0}, {e1}]", &cx));
        assert!(runtime_ok("almide_rt_int_to_string", &cx));
    }
}
