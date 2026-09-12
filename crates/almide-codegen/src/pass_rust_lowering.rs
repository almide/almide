//! RustLoweringPass: Rust-specific IR rewrites that keep the walker target-agnostic.
//!
//! 1. **List push**: `xs = xs + [v]` → `Expr(Call(xs.push, [v]))`.
//!    Avoids a full list clone + concat for single-element append.
//!
//! 2. **Borrow index lift**: `xs[f(xs)] = v` → `{ let __idx = f(xs); xs[__idx] = v; }`
//!    Resolves Rust simultaneous mutable+immutable borrow conflicts in IndexAssign.
//!
//! 3. **flat_map array return** (#1337): `list.flat_map(xs, |x| … [a, b])` →
//!    `almide_rt_list_flat_map_arr` with the tail literal emitted as a Rust
//!    ARRAY. Kills one heap allocation PER ELEMENT.

use std::collections::HashSet;
use almide_ir::*;
use almide_base::intern::{Sym, sym};
use super::pass::{NanoPass, PassResult, Target};
use super::pass_rust_lowering_stmts::rewrite_stmts_in_expr;

#[derive(Debug)]
pub struct RustLoweringPass;

impl NanoPass for RustLoweringPass {
    fn name(&self) -> &str { "RustLowering" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["CloneInsertion"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        // (A0, #1337) `flat_map` with a fixed-arity list literal in the lambda
        //     tail → the array-returning runtime twin. Runs BEFORE the boxing
        //     step below so that step sees the NEW symbol: the generated
        //     `takes_raw_fn_last_arg` registry reads the twin's `F: Fn` bound
        //     and un-boxes the closure, which is what keeps the per-element
        //     call static. See `lower_flat_map_arrays`.
        if lower_flat_map_arrays(&mut program) { changed = true; }
        // (A1, #2044) `fan.map` / list ops UNDER a `fan { … }` block with a pure
        //     lambda over Send-safe scalars → the thread-per-core runtime
        //     twins. Also BEFORE boxing, for the same reason as A0: the fan
        //     callback is re-tagged for its runtime, and `almide_rt_list_par_*`'s `F: Fn` last
        //     arg is un-boxed through the registry. See
        //     `pass_rust_lowering_fan`.
        if super::pass_rust_lowering_fan::route_fan_parallel(&mut program) { changed = true; }
        // (A) Box closures sitting in type-erased join slots as `Rc<dyn Fn>`.
        //     Single source of truth — see `box_closures_program` below.
        if box_closures_program(&mut program) { changed = true; }
        // (A2, #599) A race/any/settle thunk list bound to a `let` first reaches
        // the call as a `Var`; box_closures_program boxed its elements as the
        // uniform `Rc<dyn Fn>`, but race/any/settle need `Box<dyn Fn + Send +
        // Sync>` (Rc is neither). Collect the VarIds used as a race/any/settle
        // arg-0 and RE-TAG the matching bind's list elements to BoxSendSync —
        // the var-indirection twin of the inline-list boxing.
        if rebox_var_thunk_lists(&mut program) { changed = true; }
        let shared = collect_assign_exempt_vars(&program);
        if apply_rewrite_stmts_program(&mut program, &shared) { changed = true; }
        PassResult { program, changed }
    }
}

/// Vars whose `Assign` must STAY an `Assign` — their lvalue is not a direct
/// Rust place, so the `xs = xs + [v]` → `xs.push(v)` rewrite would push onto
/// a DISCARDED CLONE and silently lose the write:
///   - shared cells (`AlmideSharedMut`): `xs.get().push(v)` (Closure v2 P6);
///   - mutable TOP-LETS (`ModuleRc`): the Method renderer falls through
///     to the module-var READ accessor `UPPER.with(|c| (**c.borrow())
///     .clone()).push(v)` (#501). Left as an Assign, the walker emits
///     the ModuleRc WRITE template, which is also alias-safe: the RHS
///     (including reads of the same var) evaluates BEFORE borrow_mut.
/// Extracted from `RustLoweringPass::run` (cog>25 decomposition).
fn collect_assign_exempt_vars(program: &IrProgram) -> HashSet<VarId> {
    let mut shared: HashSet<VarId> = program.codegen_annotations.shared_mut_vars.clone();
    for tl in &program.top_lets {
        if tl.mutable { shared.insert(tl.var); }
    }
    for m in &program.modules {
        for tl in &m.top_lets {
            if tl.mutable { shared.insert(tl.var); }
        }
    }
    shared
}

/// Run [`rewrite_stmts_in_expr`] over every function body and top-let value,
/// top-level and per-module. Extracted from `RustLoweringPass::run`
/// (cog>25 decomposition).
fn apply_rewrite_stmts_program(program: &mut IrProgram, shared: &HashSet<VarId>) -> bool {
    let mut changed = false;
    let IrProgram { functions, top_lets, modules, var_table, .. } = program;
    for func in functions.iter_mut() {
        if rewrite_stmts_in_expr(&mut func.body, var_table, shared) { changed = true; }
    }
    for tl in top_lets.iter_mut() {
        if rewrite_stmts_in_expr(&mut tl.value, var_table, shared) { changed = true; }
    }
    for module in modules.iter_mut() {
        for func in module.functions.iter_mut() {
            if rewrite_stmts_in_expr(&mut func.body, var_table, shared) { changed = true; }
        }
    }
    changed
}

// ─────────────────────────────────────────────────────────────────────────
// Closure boxing at type-erased join slots (Rust target).
//
// A closure value is an anonymous Rust type. When two DISTINCT closures must
// collapse to ONE type — a uniform container's element, a `Map` value, an
// `if`/`match` branch result that is itself a closure, a `??` fallback — rustc
// cannot unify them (E0308 / E0562). We box such values to `Rc<dyn Fn>` (via
// `RcWrap`) so the erased slot infers a single boxed type. The call side needs
// no change: Rust's call operator auto-derefs `Rc<dyn Fn>`.
//
// This is the single source of truth that REPLACES the former per-API patches
// (List[Fn] binding, Map insert/get_or, UnwrapOr fallback). It keys on the
// SLOT's static type rather than the surrounding statement shape, so it also
// covers record-field containers, `if`/`match` joins, `list.push`, and
// `map.from_list` (the inner list-of-tuples literal) in one stroke.
//
// Standalone tuples/records are NOT join slots — their components keep distinct
// types (the binding-type `Fn`→`_` erasure handles them). We descend INTO a
// tuple/record only when it is the element of a uniform container that forces
// the whole element to unify, e.g. `map.from_list([(k, closure)])`.
// ─────────────────────────────────────────────────────────────────────────

fn box_closures_program(program: &mut IrProgram) -> bool {
    let mut changed = false;
    for f in program.functions.iter_mut() {
        let body = std::mem::replace(&mut f.body, unit_ir());
        f.body = box_closures_expr(body, true, &mut changed);
    }
    for tl in program.top_lets.iter_mut() {
        let v = std::mem::replace(&mut tl.value, unit_ir());
        tl.value = box_closures_expr(v, true, &mut changed);
    }
    for m in program.modules.iter_mut() {
        for f in m.functions.iter_mut() {
            let body = std::mem::replace(&mut f.body, unit_ir());
            f.body = box_closures_expr(body, true, &mut changed);
        }
        for tl in m.top_lets.iter_mut() {
            let v = std::mem::replace(&mut tl.value, unit_ir());
            tl.value = box_closures_expr(v, true, &mut changed);
        }
    }
    changed
}

fn unit_ir() -> IrExpr {
    IrExpr { kind: IrExprKind::Unit, ty: almide_lang::types::Ty::Unit, span: None, def_id: None }
}

// ─────────────────────────────────────────────────────────────────────────
// flat_map with a fixed-arity literal → array return (#1337)
//
// `list.range(0, n) |> list.flat_map((i) => [a, b])` is the materializing build
// idiom CLAUDE.md and docs/CHEATSHEET.md recommend, and it lowered to
// `almide_rt_list_flat_map(xs, Rc<dyn Fn(A) -> Vec<B>>)` — a signature that
// forces the lambda to HEAP-ALLOCATE its two-element intermediate on every
// iteration. Measured on the listbuild benchmark's shape at 2^22 elements
// (M4 Pro, `--release`, median of 9), building the same 8.4M-element result:
//
//   with_capacity + push (Rust ref)               33.3 ms
//   range-Vec + Rc<dyn Fn> + `vec![a, b]`         79.0 ms   ← what we emitted
//   range-Vec + Rc<dyn Fn> + `[a, b]`             35.2 ms
//   range-Vec + static fn  + `vec![a, b]`         71.3 ms
//   (0..n) iter  + static fn + `[a, b]`           35.8 ms
//
// The per-element `Vec` is 44 ms of the 79 — the entire gap. The `Rc<dyn Fn>`
// indirection is 8 ms and the materialized `list.range` intermediate is ~0, so
// neither is worth a pass on its own. Emitting the tail literal as an ARRAY is.
//
// The rewrite changes only the intermediate's REPRESENTATION — same order, same
// elements, same length, same lambda body — so it needs no guard beyond the
// shape itself: the tail of the lambda body must be a `List` literal whose
// arity is statically known. A tail that is a call, a branch, or a spread keeps
// the `Vec`-returning form.
// ─────────────────────────────────────────────────────────────────────────

/// Largest tail-literal arity that gets the array form. Above this the
/// per-element allocation is amortized over enough work that the `[T; N]`
/// move (copied through `extend` rather than pointer-swapped) stops paying,
/// and the shape stops being the one the idiom docs produce.
const FLAT_MAP_ARRAY_MAX_ARITY: usize = 8;

fn lower_flat_map_arrays(program: &mut IrProgram) -> bool {
    use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};

    struct Lower { changed: bool }
    impl IrMutVisitor for Lower {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            if try_flat_map_array(expr) { self.changed = true; }
        }
    }

    let mut l = Lower { changed: false };
    for f in program.functions.iter_mut() { l.visit_expr_mut(&mut f.body); }
    for tl in program.top_lets.iter_mut() { l.visit_expr_mut(&mut tl.value); }
    for m in program.modules.iter_mut() {
        for f in m.functions.iter_mut() { l.visit_expr_mut(&mut f.body); }
        for tl in m.top_lets.iter_mut() { l.visit_expr_mut(&mut tl.value); }
    }
    l.changed
}

/// Rewrite ONE `almide_rt_list_flat_map` node whose lambda tail is a
/// fixed-arity list literal. Returns whether it fired.
fn try_flat_map_array(expr: &mut IrExpr) -> bool {
    let IrExprKind::RuntimeCall { symbol, args } = &mut expr.kind else { return false };
    if symbol.as_str() != "almide_rt_list_flat_map" || args.len() != 2 { return false; }
    let Some(IrExpr { kind: IrExprKind::Lambda { body, .. }, .. }) = args.get_mut(1) else { return false };
    if !rewrite_tail_list_to_array(body) { return false; }
    *symbol = sym("almide_rt_list_flat_map_arr");
    true
}

/// Rewrite the list literal a lambda body EVALUATES to — itself, or the tail
/// of the (possibly nested) block it ends in — into an `InlineRust` array
/// literal. Declines on a block with no tail expression, on a non-literal
/// tail (a call, a branch), and on an arity outside the array band.
/// Shared with `StreamFusionPass` (the fused `flat_map` step).
pub(crate) fn rewrite_tail_list_to_array(e: &mut IrExpr) -> bool {
    match &mut e.kind {
        IrExprKind::Block { expr: Some(tail), .. } => return rewrite_tail_list_to_array(tail),
        IrExprKind::List { elements } => {
            let arity = elements.len();
            if arity == 0 || arity > FLAT_MAP_ARRAY_MAX_ARITY { return false; }
            // `[{e0}, {e1}, …]` — the element exprs ride the IR to the walker
            // like any other InlineRust arg, so clone/borrow decoration on
            // them still renders.
            let template = format!(
                "[{}]",
                (0..arity).map(|i| format!("{{e{}}}", i)).collect::<Vec<_>>().join(", "),
            );
            let named: Vec<(Sym, IrExpr)> = std::mem::take(elements).into_iter()
                .enumerate()
                .map(|(i, el)| (sym(&format!("e{}", i)), el))
                .collect();
            e.kind = IrExprKind::InlineRust { template, args: named };
            true
        }
        _ => false,
    }
}

/// Un-box a freshly-boxed closure LITERAL back to a raw `impl Fn`
/// (`RcWrap(Lambda|FnRef)` → `Lambda|FnRef`). Used only where a closure must be a
/// bare `impl Fn`: a FUSED `IterChain` step (`.iter().map(move |x| …)`) and a
/// `fan.*` arg (threads need `Send + Sync`). Everywhere else a closure stays
/// `Rc<dyn Fn>` — the runtime HOFs take `Rc<dyn Fn>` directly, so no consumed/
/// value distinction and no per-API allow-list is needed. A non-literal stays
/// as-is (a stored `Rc<dyn Fn>` is already what every non-fused consumer wants).
fn unbox_consumed(e: &mut IrExpr) -> bool {
    match &mut e.kind {
        IrExprKind::RcWrap { expr, .. }
            if matches!(&expr.kind, IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. }) =>
        {
            let inner = std::mem::replace(expr.as_mut(), unit_ir());
            *e = inner;
            true
        }
        // CaptureClone's `{ let __cap = v.clone(); <lambda> }` — a fused step
        // keeps the block whole (it renders as a closure expression); the
        // boxed lambda is its tail.
        IrExprKind::Block { expr: Some(tail), .. } => unbox_consumed(tail),
        _ => false,
    }
}

/// `fan.*` method name (`map`/`race`/`any`/`settle`) for a fan call in
/// either `Module{fan, func}` or already-lowered `almide_rt_fan_*` form.
fn fan_method(expr: &IrExpr) -> Option<String> {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }
            if module.as_str() == "fan" => Some(func.as_str().to_string()),
        IrExprKind::RuntimeCall { symbol, .. } =>
            symbol.as_str().strip_prefix("almide_rt_fan_").map(str::to_string),
        _ => None,
    }
}

/// Box every thunk in a `fan.race/any/settle` thunk-LIST argument.
fn rebox_var_thunk_lists(program: &mut IrProgram) -> bool {
    use almide_ir::visit::{IrVisitor, walk_expr};
    use almide_ir::visit_mut::{IrMutVisitor, walk_stmt_mut};
    use std::collections::HashSet;

    fn fan_list_var(e: &IrExpr) -> Option<u32> {
        let (func, args) = match &e.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if module.as_str() == "fan" => (func.as_str().to_string(), args),
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                if name.as_str().starts_with("almide_rt_fan_") =>
                (name.as_str().trim_start_matches("almide_rt_fan_").to_string(), args),
            _ => return None,
        };
        if matches!(func.as_str(), "race" | "any" | "settle") {
            if let Some(IrExpr { kind: IrExprKind::Var { id }, .. }) = args.first() {
                return Some(id.0);
            }
        }
        None
    }

    struct Collect { vars: HashSet<u32> }
    impl IrVisitor for Collect {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let Some(id) = fan_list_var(e) { self.vars.insert(id); }
            walk_expr(self, e);
        }
    }
    let mut c = Collect { vars: HashSet::new() };
    for func in &program.functions { c.visit_expr(&func.body); }
    for m in &program.modules { for func in &m.functions { c.visit_expr(&func.body); } }
    if c.vars.is_empty() { return false; }

    struct Rebox { vars: HashSet<u32>, changed: bool }
    impl IrMutVisitor for Rebox {
        fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
            if let IrStmtKind::Bind { var, value, .. } = &mut stmt.kind {
                if self.vars.contains(&var.0) {
                    if let IrExprKind::List { elements } = &mut value.kind {
                        for el in elements.iter_mut() {
                            self.changed |= box_fan_thunk(el, FnBox::BoxSendSync);
                        }
                    }
                }
            }
            walk_stmt_mut(self, stmt);
        }
    }
    let mut r = Rebox { vars: c.vars, changed: false };
    for func in &mut program.functions { r.visit_expr_mut(&mut func.body); }
    for m in &mut program.modules { for func in &mut m.functions { r.visit_expr_mut(&mut func.body); } }
    r.changed
}

fn box_fan_thunk_list(arg: &mut IrExpr, to: FnBox) -> bool {
    if let IrExprKind::List { elements } = &mut arg.kind {
        let mut c = false;
        for el in elements.iter_mut() { c |= box_fan_thunk(el, to); }
        return c;
    }
    // A thunk list bound to a var (rare) — can't box the individual closures.
    false
}

/// Wrap a single fan thunk in `RcWrap { wrap: to }`, reaching THROUGH a
/// capture-clone `{ let __cap = …; <lambda> }` block to box the inner closure
/// (mirrors `box_closure_value`, so the boxed inner stays a bare Lambda whose
/// params the `as` cast can annotate, and the block still evaluates to the boxed
/// value). A bare VAR — a stored closure value — is left as-is: for `fan.map` it
/// is already `Rc<dyn Fn>`; for race/any/settle a var-thunk cannot be made
/// `Send + Sync` here (a documented edge of literal-thunk-list boxing).
fn box_fan_thunk(slot: &mut IrExpr, to: FnBox) -> bool {
    match &mut slot.kind {
        IrExprKind::Block { expr: Some(tail), .. } => return box_fan_thunk(tail, to),
        // Defensive: a closure already boxed by some earlier path — just re-tag.
        IrExprKind::RcWrap { wrap, .. } => {
            if *wrap != to { *wrap = to; return true; }
            return false;
        }
        IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. } => { /* wrap below */ }
        _ => return false,
    }
    let inner = std::mem::replace(slot, unit_ir());
    let fn_ty = inner.ty.clone();
    *slot = IrExpr {
        ty: fn_ty.clone(),
        span: inner.span,
        kind: IrExprKind::RcWrap { expr: Box::new(inner), cast_ty: Some(Box::new(fn_ty)), wrap: to },
        def_id: None,
    };
    true
}

/// Top-down walk: recurse into children boxing every closure literal by default
/// (`box_node`), then — for a combinator / fan node — un-box its DIRECT consumed
/// closure args. Un-boxing (rather than clearing `box_here`) keeps nesting exact:
/// a closure STORED inside a consumed lambda's body stays boxed.
///
/// A `fan.*` call gets NO subtree-wide exemption (#2059). It used to: the whole
/// subtree was walked with `box_here = false` so the thread thunks stayed raw,
/// but that also left every closure literal INSIDE a thunk's body raw — and a
/// `list.map((x) => …)` nested in a `fan.map` callback then handed a bare
/// `move |x| …` to `almide_rt_list_map`, whose parameter is `Rc<dyn Fn>`, so
/// rustc had nothing to infer `x` from (E0282) while the wasm leg ran it. The
/// thunks themselves are re-tagged at the fan node instead (`try_box_fan_thunks`:
/// race/any/settle → `Box<dyn Fn + Send + Sync>`, `map_par` → un-boxed to the raw
/// `impl Fn` its `F: Fn` bound wants, `map` keeps the uniform `Rc<dyn Fn>`), and a
/// list of thunks is reached element by element — so the bodies get the same
/// uniform boxing as any other lambda, and a nested combinator's closure is a
/// typed `Rc<dyn Fn(T) -> U>` exactly as it is outside a fan.
fn box_closures_expr(expr: IrExpr, box_here: bool, changed: &mut bool) -> IrExpr {
    let mut e = expr.map_children(&mut |c| box_closures_expr(c, box_here, changed));
    if box_here && box_node(&mut e) { *changed = true; }
    e
}

/// Box closures in THIS node's type-erased slots (non-recursive).
fn box_node(expr: &mut IrExpr) -> bool {
    if let Some(result) = try_box_closure_literal(expr) { return result; }
    if let Some(result) = try_box_fan_thunks(expr) { return result; }
    box_node_unbox_consumed(expr)
}

/// Closure-LITERAL boxing check of `box_node`, extracted (cog>30
/// decomposition, pattern 1 — independent checks in sequence, each
/// unconditionally decides the whole function's result once its own
/// pattern matches, `None` otherwise so the caller falls through to the
/// next check). UNIFORM-REPR: box every fresh closure LITERAL by
/// default — a closure value is `Rc<dyn Fn>` everywhere it is
/// stored/passed. Closures CONSUMED in place (combinators / IterChain /
/// fan, which take `impl Fn`) are un-boxed at their consumer node instead
/// (see `box_node_unbox_consumed`) — that is exact for nesting (a closure
/// STORED inside a consumed lambda's body stays boxed), unlike
/// subtree-wide `box_here` clearing.
fn try_box_closure_literal(expr: &mut IrExpr) -> Option<bool> {
    use almide_lang::types::Ty;
    let node_ty = expr.ty.clone();
    if matches!(&expr.kind, IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. })
        && matches!(&node_ty, Ty::Fn { .. })
    {
        // A top-level `fn` used as a VALUE (`FnRef`) is a fn item, not `Rc<dyn Fn>`
        // — box it too so it unifies with closures in the same slot
        // (`[dbl, (x) => …]`, a user-HOF arg). As a CALLEE it lives in `CallTarget`,
        // not as an expr node, so a direct call `dbl(x)` is never boxed.
        return Some(box_closure_value(expr, &node_ty));
    }
    None
}

/// `fan.*` thread-thunk boxing check of `box_node`, extracted (cog>30
/// decomposition). The children were already walked with the default
/// boxing, so each thunk here is an `RcWrap{Rc}` over a Lambda / FnRef (a
/// capture-clone `{ …; <lambda> }` block carries it in its tail); a bare
/// Lambda / FnRef is still accepted. Re-tag per fan API (fan is still a
/// `Module{fan}` / `RuntimeCall{fan}` call here — FanLowering runs later):
///   race/any/settle → `Box<dyn Fn + Send + Sync>`: distinct CAPTURING thunks
///     cannot share one `impl Fn` type (E0308), but `Box<dyn Fn + Send + Sync>`
///     is itself `Fn + Send + Sync`, so they unify as one element type AND
///     satisfy the runtime's `Vec<impl Fn() -> _ + Send + Sync>` thunk bound.
///   map → `Rc<dyn Fn>`: the runtime runs it SEQUENTIALLY over an `Rc<dyn Fn>`,
///     which also accepts a closure VALUE in a var — a `Send + Sync` box can't,
///     since the uniform repr of a stored closure is `Rc` (neither Send nor Sync).
///   map_par → RAW `impl Fn`: the parallel twin `route_fan_parallel` routed to
///     is generic over `F: Fn(A) -> Result<B, String> + Send + Sync`, and an
///     `Rc<dyn Fn>` is neither Send nor Sync, so the mapper is un-boxed back to
///     the literal (the registry un-box in `box_node_unbox_consumed` only sees
///     the `almide_rt_*` spelling, and the call is still `Module{fan}` here).
fn try_box_fan_thunks(expr: &mut IrExpr) -> Option<bool> {
    let method = fan_method(expr)?;
    let args = match &mut expr.kind {
        IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. } => args,
        _ => return Some(false),
    };
    Some(match method.as_str() {
        "race" | "any" | "settle" => args.first_mut()
            .map(|a| box_fan_thunk_list(a, FnBox::BoxSendSync))
            .unwrap_or(false),
        // The T2-3 mapper forms ride the same sequential Rc<dyn Fn> mode as
        // fan.map (arg 1 is the item mapper, arg 0 the plain item list).
        "map" | "any_map" => args.get_mut(1)
            .map(|f| box_fan_thunk(f, FnBox::Rc))
            .unwrap_or(false),
        "map_par" => args.get_mut(1)
            .map(unbox_fan_thunk)
            .unwrap_or(false),
        _ => false,
    })
}

/// Un-box a fan mapper back to its raw closure literal, reaching THROUGH a
/// capture-clone `{ let __cap = …; <lambda> }` block the way `box_fan_thunk`
/// does on the way in. A bare Lambda / FnRef / Var is left as-is.
fn unbox_fan_thunk(slot: &mut IrExpr) -> bool {
    match &mut slot.kind {
        IrExprKind::Block { expr: Some(tail), .. } => unbox_fan_thunk(tail),
        _ => unbox_consumed(slot),
    }
}

/// Structural bare-closure un-box check of `box_node`, extracted (cog>30
/// decomposition). Storage of a closure (in a list/map/tuple/record/field/
/// `if`-`match` join/ `??` fallback) needs NO rule — the default arm
/// already boxed every closure literal where it sits, and a `Var` holding
/// a closure is already `Rc<dyn Fn>`. Runtime HOFs take `Rc<dyn Fn>` too,
/// so a boxed closure passed to `almide_rt_list_map`/`_fold`/`unwrap_or`/…
/// needs NO un-box. The ONLY place a raw `impl Fn` is required is a FUSED
/// `IterChain` step and a `fan.*` arg (handled by `try_box_fan_thunks`
/// above). This REPLACES the former ~14 per-position boxing rules (and the
/// consumed-vs-value allow-list) with: box by default, un-box only the two
/// structural bare-closure sites.
fn box_node_unbox_consumed(expr: &mut IrExpr) -> bool {
    match &mut expr.kind {
        // Fused combinator chain: un-box every consumed step AND collector
        // lambda — `Iterator::fold/any/all/find/filter` want an `impl FnMut`,
        // which `Rc<dyn Fn>` is not. Closures the map PRODUCES (nested in
        // the body) are already boxed by the default arm.
        IrExprKind::IterChain { steps, collector, .. } => {
            let mut c = false;
            for step in steps.iter_mut() {
                match step {
                    IterStep::Map { lambda } | IterStep::Filter { lambda }
                    | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => {
                        c |= unbox_consumed(lambda);
                    }
                    IterStep::Take { .. } | IterStep::Enumerate => {}
                }
            }
            match collector {
                IterCollector::Fold { lambda, .. } | IterCollector::Any { lambda }
                | IterCollector::All { lambda } | IterCollector::Find { lambda }
                | IterCollector::Count { lambda } => c |= unbox_consumed(lambda),
                IterCollector::Collect | IterCollector::Sum { .. } | IterCollector::Len => {}
            }
            c
        }
        // Runtime helpers whose LAST positional argument is a GENERIC `F: Fn(T) -> _`
        // element codec (NOT `Rc<dyn Fn>`) — the value codec combinators
        // (`almide_rt_value_{encode,decode}_list`, `almide_rt_value_option_encode`,
        // `almide_rt_value_decode_option_custom`, …). Their fn argument must stay a
        // raw `impl Fn` (the default arm above boxed it to `Rc<dyn Fn>`); un-box it.
        // The set is DERIVED from the runtime signatures themselves (a `F: Fn` bound)
        // by build.rs, so adding a codec combinator needs no edit here — see
        // `RAW_FN_LAST_ARG_HELPERS` (generated/runtime_fn_modes.rs).
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            if crate::generated::runtime_fn_modes::takes_raw_fn_last_arg(name.as_str()) =>
        {
            args.last_mut().map(unbox_consumed).unwrap_or(false)
        }
        // Same set, RuntimeCall spelling: an intrinsic-lowered stdlib call
        // (`fs.fold_lines_range` → `almide_rt_fs_fold_lines_range`) reaches
        // this pass as a `RuntimeCall`, and its raw-`F: Fn` last arg needs
        // the identical un-box (the threaded `fold_lines_chunked` also NEEDS
        // it — `Rc<dyn Fn>` can never satisfy its Send + Sync bound).
        IrExprKind::RuntimeCall { symbol, args }
            if crate::generated::runtime_fn_modes::takes_raw_fn_last_arg(symbol.as_str()) =>
        {
            args.last_mut().map(unbox_consumed).unwrap_or(false)
        }
        _ => false,
    }
}

/// Wrap a single closure value in `RcWrap` as `Rc<dyn Fn>`. Only fresh closure
/// LITERALS (`Lambda`) are boxed; values already read out of a container/field
/// (`Var`, `Member`, …) are left untouched (already boxed there). `if`/`match`/
/// `Block` are descended so each leaf lambda unifies.
pub(super) fn box_closure_value(slot: &mut IrExpr, fn_ty: &almide_lang::types::Ty) -> bool {
    match &mut slot.kind {
        IrExprKind::If { then, else_, .. } => {
            let a = box_closure_value(then, fn_ty);
            let b = box_closure_value(else_, fn_ty);
            return a || b;
        }
        IrExprKind::Match { arms, .. } => {
            let mut c = false;
            for arm in arms.iter_mut() { c |= box_closure_value(&mut arm.body, fn_ty); }
            return c;
        }
        IrExprKind::Block { expr: Some(tail), .. } => {
            return box_closure_value(tail, fn_ty);
        }
        IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. } => { /* fall through to wrap */ }
        _ => return false,
    }
    let inner = std::mem::replace(slot, unit_ir());
    *slot = IrExpr {
        ty: inner.ty.clone(),
        span: inner.span,
        kind: IrExprKind::RcWrap { expr: Box::new(inner), cast_ty: Some(Box::new(fn_ty.clone())), wrap: FnBox::Rc },
        def_id: None,
    };
    true
}
