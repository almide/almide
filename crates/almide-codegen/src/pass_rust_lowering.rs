//! RustLoweringPass: Rust-specific IR rewrites that keep the walker target-agnostic.
//!
//! 1. **List push**: `xs = xs + [v]` → `Expr(Call(xs.push, [v]))`.
//!    Avoids a full list clone + concat for single-element append.
//!
//! 2. **Borrow index lift**: `xs[f(xs)] = v` → `{ let __idx = f(xs); xs[__idx] = v; }`
//!    Resolves Rust simultaneous mutable+immutable borrow conflicts in IndexAssign.

use std::collections::HashSet;
use almide_ir::*;
use almide_base::intern::sym;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct RustLoweringPass;

impl NanoPass for RustLoweringPass {
    fn name(&self) -> &str { "RustLowering" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["CloneInsertion"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
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
        // Vars whose Assign must STAY an Assign — their lvalue is not a direct
        // Rust place, so the `xs = xs + [v]` → `xs.push(v)` rewrite would push
        // onto a DISCARDED CLONE and silently lose the write:
        //   - shared cells (`SharedMut`): `xs.get().push(v)` (Closure v2 P6);
        //   - mutable TOP-LETS (`ModuleRc`): the Method renderer falls through
        //     to the module-var READ accessor `UPPER.with(|c| (**c.borrow())
        //     .clone()).push(v)` (#501). Left as an Assign, the walker emits
        //     the ModuleRc WRITE template, which is also alias-safe: the RHS
        //     (including reads of the same var) evaluates BEFORE borrow_mut.
        let mut shared: HashSet<VarId> = program.codegen_annotations.shared_mut_vars.clone();
        for tl in &program.top_lets {
            if tl.mutable { shared.insert(tl.var); }
        }
        for m in &program.modules {
            for tl in &m.top_lets {
                if tl.mutable { shared.insert(tl.var); }
            }
        }
        let IrProgram { functions, top_lets, modules, var_table, .. } = &mut program;
        for func in functions.iter_mut() {
            if rewrite_stmts_in_expr(&mut func.body, var_table, &shared) { changed = true; }
        }
        for tl in top_lets.iter_mut() {
            if rewrite_stmts_in_expr(&mut tl.value, var_table, &shared) { changed = true; }
        }
        for module in modules.iter_mut() {
            for func in module.functions.iter_mut() {
                if rewrite_stmts_in_expr(&mut func.body, var_table, &shared) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
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

/// True for a `fan.*` call (`fan.any`/`settle`/`race`/`map`, in either
/// `Module{fan}` or already-lowered `almide_rt_fan_*` form). Their closure
/// arguments run on threads and must stay raw (`Send + Sync`); `Rc<dyn Fn>` is
/// neither, so boxing them would break the Rust target.
fn is_fan_call(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Module { module, .. }, .. } => module.as_str() == "fan",
        IrExprKind::RuntimeCall { symbol, .. } => symbol.as_str().starts_with("almide_rt_fan"),
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
    if let IrExprKind::RcWrap { expr, .. } = &mut e.kind {
        if matches!(&expr.kind, IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. }) {
            let inner = std::mem::replace(expr.as_mut(), unit_ir());
            *e = inner;
            return true;
        }
    }
    false
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
fn box_closures_expr(expr: IrExpr, box_here: bool, changed: &mut bool) -> IrExpr {
    // Clear box_here for a `fan.*` call's WHOLE subtree: its thunks run on threads
    // and must stay raw `impl Fn + Send + Sync` — a boxed `Rc<dyn Fn>` is neither.
    // This reaches thunks nested in a LIST arg (`fan.race([() => …, () => …])`),
    // which an un-box of only the direct args would miss. Combinators are NOT
    // cleared: they take `Rc<dyn Fn>` directly, so their closures stay boxed (only
    // fused IterChain steps are un-boxed, in box_node).
    let child_box = box_here && !is_fan_call(&expr);
    let mut e = expr.map_children(&mut |c| box_closures_expr(c, child_box, changed));
    if box_here {
        if box_node(&mut e) { *changed = true; }
    }
    e
}

/// Box closures in THIS node's type-erased slots (non-recursive).
fn box_node(expr: &mut IrExpr) -> bool {
    use almide_lang::types::Ty;
    let node_ty = expr.ty.clone();
    // UNIFORM-REPR: box every fresh closure LITERAL by default — a closure value
    // is `Rc<dyn Fn>` everywhere it is stored/passed. Closures CONSUMED in place
    // (combinators / IterChain / fan, which take `impl Fn`) are un-boxed below at
    // their consumer node — that is exact for nesting (a closure STORED inside a
    // consumed lambda's body stays boxed), unlike subtree-wide box_here clearing.
    if matches!(&expr.kind, IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. })
        && matches!(&node_ty, Ty::Fn { .. })
    {
        // A top-level `fn` used as a VALUE (`FnRef`) is a fn item, not `Rc<dyn Fn>`
        // — box it too so it unifies with closures in the same slot
        // (`[dbl, (x) => …]`, a user-HOF arg). As a CALLEE it lives in `CallTarget`,
        // not as an expr node, so a direct call `dbl(x)` is never boxed.
        return box_closure_value(expr, &node_ty);
    }
    // fan.* thread-thunk boxing. The whole fan subtree was left RAW (box_here
    // cleared in box_closures_expr), so each thunk here is a bare Lambda / FnRef /
    // capture-clone `{ …; <lambda> }` block. Box per fan API (fan is still a
    // `Module{fan}` / `RuntimeCall{fan}` call here — FanLowering runs later):
    //   race/any/settle → `Box<dyn Fn + Send + Sync>`: distinct CAPTURING thunks
    //     cannot share one `impl Fn` type (E0308), but `Box<dyn Fn + Send + Sync>`
    //     is itself `Fn + Send + Sync`, so they unify as one element type AND
    //     satisfy the runtime's `Vec<impl Fn() -> _ + Send + Sync>` thunk bound.
    //   map → `Rc<dyn Fn>`: the runtime runs it SEQUENTIALLY over an `Rc<dyn Fn>`,
    //     which also accepts a closure VALUE in a var — a `Send + Sync` box can't,
    //     since the uniform repr of a stored closure is `Rc` (neither Send nor Sync).
    if let Some(method) = fan_method(expr) {
        let args = match &mut expr.kind {
            IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. } => args,
            _ => return false,
        };
        return match method.as_str() {
            "race" | "any" | "settle" => args.first_mut()
                .map(|a| box_fan_thunk_list(a, FnBox::BoxSendSync))
                .unwrap_or(false),
            "map" => args.get_mut(1)
                .map(|f| box_fan_thunk(f, FnBox::Rc))
                .unwrap_or(false),
            _ => false,
        };
    }
    // Storage of a closure (in a list/map/tuple/record/field/`if`-`match` join/
    // `??` fallback) needs NO rule — the default arm already boxed every closure
    // literal where it sits, and a `Var` holding a closure is already
    // `Rc<dyn Fn>`. Runtime HOFs take `Rc<dyn Fn>` too, so a boxed closure passed
    // to `almide_rt_list_map`/`_fold`/`unwrap_or`/… needs NO un-box. The ONLY
    // place a raw `impl Fn` is required is a FUSED `IterChain` step (below) and a
    // `fan.*` arg (above). This REPLACES the former ~14 per-position boxing rules
    // (and the consumed-vs-value allow-list) with: box by default, un-box only
    // the two structural bare-closure sites.
    match &mut expr.kind {
        // Fused combinator chain: un-box every consumed step lambda. Closures the
        // map PRODUCES (nested in the body) are already boxed by the default arm.
        IrExprKind::IterChain { steps, .. } => {
            let mut c = false;
            for step in steps.iter_mut() {
                match step {
                    IterStep::Map { lambda } | IterStep::Filter { lambda }
                    | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => {
                        c |= unbox_consumed(lambda);
                    }
                }
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
        _ => false,
    }
}

/// Box every Fn-typed position reachable in `value` given its expected uniform
/// element type `expected`. Descends through tuple/record element types so a
/// closure nested in `[(k, closure)]` (map.from_list) is boxed.
fn box_fn_in_value(value: &mut IrExpr, expected: &almide_lang::types::Ty) -> bool {
    use almide_lang::types::Ty;
    // A capture-clone-wrapped value (`{ let __cap = …; <tuple/record/closure> }`,
    // produced for a HOF mapper body that captures) hides the structural value
    // behind a Block — descend into the tail to reach the tuple/record beneath.
    if let IrExprKind::Block { expr: Some(tail), .. } = &mut value.kind {
        return box_fn_in_value(tail, expected);
    }
    match expected {
        // UNIFORM-REPR SPIKE: a `Var` of `Ty::Fn` is ALREADY `Rc<dyn Fn>` (boxed
        // at its binding / it is an `Rc<dyn Fn>` parameter), so storing it needs no
        // re-box — wrapping it again would yield `Rc<Rc<dyn Fn>>` (the a6 over-box).
        // Only fresh literals (Lambda, via control-flow joins) are boxed here.
        Ty::Fn { .. } => box_closure_value(value, expected),
        Ty::Tuple(comps) => {
            if let IrExprKind::Tuple { elements } = &mut value.kind {
                if elements.len() == comps.len() {
                    let mut c = false;
                    for (el, ct) in elements.iter_mut().zip(comps.iter()) {
                        if ty_contains_fn(ct) { c |= box_fn_in_value(el, ct); }
                    }
                    return c;
                }
            }
            false
        }
        Ty::Record { fields } => {
            if let IrExprKind::Record { fields: vfields, .. } = &mut value.kind {
                let mut c = false;
                for (fname, fty) in fields.iter() {
                    if !ty_contains_fn(fty) { continue; }
                    if let Some((_, fv)) = vfields.iter_mut().find(|(n, _)| n == fname) {
                        c |= box_fn_in_value(fv, fty);
                    }
                }
                return c;
            }
            false
        }
        _ => false,
    }
}

/// Wrap a single closure value in `RcWrap` as `Rc<dyn Fn>`. Only fresh closure
/// LITERALS (`Lambda`) are boxed; values already read out of a container/field
/// (`Var`, `Member`, …) are left untouched (already boxed there). `if`/`match`/
/// `Block` are descended so each leaf lambda unifies.
fn box_closure_value(slot: &mut IrExpr, fn_ty: &almide_lang::types::Ty) -> bool {
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

/// True if `ty` mentions a function type at a position a uniform container would
/// need to unify (directly, or inside a tuple/record element). Nested List/Map
/// are their own containers — not descended here.
fn ty_contains_fn(ty: &almide_lang::types::Ty) -> bool {
    use almide_lang::types::Ty;
    match ty {
        Ty::Fn { .. } => true,
        Ty::Tuple(ts) => ts.iter().any(ty_contains_fn),
        Ty::Record { fields } => fields.iter().any(|(_, t)| ty_contains_fn(t)),
        _ => false,
    }
}

/// Element type of `List[E]`.
fn list_elem_ty(ty: &almide_lang::types::Ty) -> Option<&almide_lang::types::Ty> {
    use almide_lang::types::{Ty, TypeConstructorId};
    if let Ty::Applied(TypeConstructorId::List, args) = ty { args.first() } else { None }
}

/// Value type of `Map[K, V]`.
fn map_value_ty(ty: &almide_lang::types::Ty) -> Option<&almide_lang::types::Ty> {
    use almide_lang::types::{Ty, TypeConstructorId};
    if let Ty::Applied(TypeConstructorId::Map, args) = ty { args.get(1) } else { None }
}

/// Walk all stmts in expressions recursively (Rust push/index peepholes).
fn rewrite_stmts_in_expr(expr: &mut IrExpr, vt: &mut VarTable, shared: &HashSet<VarId>) -> bool {
    let mut changed = false;
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts.iter_mut() {
                if rewrite_stmt(s, vt, shared) { changed = true; }
                rewrite_stmts_in_stmt(s, vt, shared, &mut changed);
            }
            if let Some(e) = tail { if rewrite_stmts_in_expr(e, vt, shared) { changed = true; } }
        }
        IrExprKind::If { cond, then, else_ } => {
            if rewrite_stmts_in_expr(cond, vt, shared) { changed = true; }
            if rewrite_stmts_in_expr(then, vt, shared) { changed = true; }
            if rewrite_stmts_in_expr(else_, vt, shared) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if rewrite_stmts_in_expr(subject, vt, shared) { changed = true; }
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_stmts_in_expr(g, vt, shared); }
                if rewrite_stmts_in_expr(&mut arm.body, vt, shared) { changed = true; }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            if rewrite_stmts_in_expr(iterable, vt, shared) { changed = true; }
            for s in body.iter_mut() {
                if rewrite_stmt(s, vt, shared) { changed = true; }
                rewrite_stmts_in_stmt(s, vt, shared, &mut changed);
            }
        }
        IrExprKind::While { cond, body } => {
            if rewrite_stmts_in_expr(cond, vt, shared) { changed = true; }
            for s in body.iter_mut() {
                if rewrite_stmt(s, vt, shared) { changed = true; }
                rewrite_stmts_in_stmt(s, vt, shared, &mut changed);
            }
        }
        IrExprKind::Lambda { body, .. } => {
            if rewrite_stmts_in_expr(body, vt, shared) { changed = true; }
        }
        IrExprKind::RuntimeCall { args, .. } => {
            for a in args.iter_mut() { if rewrite_stmts_in_expr(a, vt, shared) { changed = true; } }
        }
        // No nested statements to rewrite — listed explicitly so a new
        // statement-bearing IrExprKind is a compile error, not a silent miss.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. }
        | IrExprKind::FnRef { .. } | IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::Fan { .. } | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::Call { .. } | IrExprKind::TailCall { .. } | IrExprKind::List { .. }
        | IrExprKind::MapLiteral { .. } | IrExprKind::EmptyMap | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::Tuple { .. } | IrExprKind::Range { .. }
        | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. } | IrExprKind::StringInterp { .. } | IrExprKind::ResultOk { .. }
        | IrExprKind::ResultErr { .. } | IrExprKind::OptionSome { .. } | IrExprKind::OptionNone
        | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. } | IrExprKind::UnwrapOr { .. }
        | IrExprKind::ToOption { .. } | IrExprKind::OptionalChain { .. } | IrExprKind::Await { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
    changed
}

fn rewrite_stmts_in_stmt(stmt: &mut IrStmt, vt: &mut VarTable, shared: &HashSet<VarId>, changed: &mut bool) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            if rewrite_stmts_in_expr(value, vt, shared) { *changed = true; }
        }
        IrStmtKind::IndexAssign { index, value, target } => {
            if rewrite_stmts_in_expr(index, vt, shared) { *changed = true; }
            if rewrite_stmts_in_expr(value, vt, shared) { *changed = true; }
            // `xs[i] = closure` into a `List[Fn]` — box the stored closure.
            let ety = list_elem_ty(&vt.get(*target).ty).cloned();
            if let Some(et) = ety {
                if matches!(&et, almide_lang::types::Ty::Fn { .. }) && box_closure_value(value, &et) { *changed = true; }
            }
        }
        IrStmtKind::MapInsert { key, value, target } => {
            if rewrite_stmts_in_expr(key, vt, shared) { *changed = true; }
            if rewrite_stmts_in_expr(value, vt, shared) { *changed = true; }
            // `m[k] = closure` / `m = map.set(m,k,closure)` (lowered to MapInsert)
            // into a closure-valued map — box the stored closure. The expr-level
            // boxing pass can't reach a statement value.
            let vty = map_value_ty(&vt.get(*target).ty).cloned();
            if let Some(vt_) = vty {
                if matches!(&vt_, almide_lang::types::Ty::Fn { .. }) && box_closure_value(value, &vt_) { *changed = true; }
            }
        }
        IrStmtKind::Guard { cond, else_ } => {
            if rewrite_stmts_in_expr(cond, vt, shared) { *changed = true; }
            if rewrite_stmts_in_expr(else_, vt, shared) { *changed = true; }
        }
        IrStmtKind::Expr { expr } => {
            if rewrite_stmts_in_expr(expr, vt, shared) { *changed = true; }
        }
        // No statement value to descend — listed explicitly so a new
        // expr-bearing IrStmtKind is a compile error, not a silent miss.
        IrStmtKind::Comment { .. } | IrStmtKind::RcDec { .. } | IrStmtKind::RcInc { .. }
        | IrStmtKind::ListCopySlice { .. } | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListSwap { .. } => {}
    }
}

/// Try to rewrite a single statement.
fn rewrite_stmt(stmt: &mut IrStmt, vt: &mut VarTable, shared: &HashSet<VarId>) -> bool {
    let span = stmt.span;
    // (1) xs = xs + [v] → xs.push(v) — but not for a shared cell (see run()).
    if let IrStmtKind::Assign { var, value } = &stmt.kind {
        if let Some(push_stmt) = try_rewrite_push(*var, value, span, shared) {
            *stmt = push_stmt;
            return true;
        }
    }
    // (2) xs[f(xs)] = v → { let __idx = f(xs); xs[__idx] = v; }
    if let IrStmtKind::IndexAssign { target, index, value } = &stmt.kind {
        if expr_references_var(index, *target) {
            let idx_var = vt.alloc(sym("__idx"), almide_lang::types::Ty::Int, Mutability::Let, None);
            let idx_bind = IrStmt {
                kind: IrStmtKind::Bind {
                    var: idx_var,
                    mutability: Mutability::Let,
                    ty: almide_lang::types::Ty::Int,
                    value: index.clone(),
                },
                span,
            };
            let idx_ref = IrExpr {
                kind: IrExprKind::Var { id: idx_var },
                ty: almide_lang::types::Ty::Int,
                span: None, def_id: None,
            };
            let new_assign = IrStmt {
                kind: IrStmtKind::IndexAssign {
                    target: *target,
                    index: idx_ref,
                    value: value.clone(),
                },
                span,
            };
            // Wrap in a Block statement
            stmt.kind = IrStmtKind::Expr {
                expr: IrExpr {
                    kind: IrExprKind::Block {
                        stmts: vec![idx_bind, new_assign],
                        expr: None,
                    },
                    ty: almide_lang::types::Ty::Unit,
                    span: None, def_id: None,
                },
            };
            return true;
        }
    }
    false
}

/// Rewrite `xs = xs + [v]` → `Expr(Call(xs.push, [v]))`.
fn try_rewrite_push(var: VarId, value: &IrExpr, span: Option<almide_base::Span>, shared: &HashSet<VarId>) -> Option<IrStmt> {
    // A shared-cell var keeps its `Assign` so the walker writes through the cell
    // (`xs.set(…)`); rewriting to `xs.push(v)` would push onto a discarded clone.
    if shared.contains(&var) { return None; }
    let IrExprKind::BinOp { op: BinOp::ConcatList, left, right } = &value.kind else { return None; };
    let IrExprKind::List { elements } = &right.kind else { return None; };
    if elements.len() != 1 { return None; }
    let is_self = match &left.kind {
        IrExprKind::Var { id } => *id == var,
        IrExprKind::Clone { expr } => matches!(&expr.kind, IrExprKind::Var { id } if *id == var),
        _ => false,
    };
    if !is_self { return None; }
    let push_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Method {
                object: Box::new(IrExpr {
                    kind: IrExprKind::Var { id: var },
                    ty: left.ty.clone(),
                    span: None, def_id: None,
                }),
                method: sym("push"),
            },
            args: vec![elements[0].clone()],
            type_args: vec![],
        },
        ty: almide_lang::types::Ty::Unit,
        span: None, def_id: None,
    };
    Some(IrStmt {
        kind: IrStmtKind::Expr { expr: push_call },
        span,
    })
}

/// Check if expr references the given variable (for borrow conflict detection).
fn expr_references_var(expr: &IrExpr, var: VarId) -> bool {
    match &expr.kind {
        IrExprKind::Var { id } => *id == var,
        IrExprKind::BinOp { left, right, .. } => {
            expr_references_var(left, var) || expr_references_var(right, var)
        }
        IrExprKind::UnOp { operand, .. } => expr_references_var(operand, var),
        IrExprKind::Call { target, args, .. } => {
            let t = match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => expr_references_var(object, var),
                _ => false,
            };
            t || args.iter().any(|a| expr_references_var(a, var))
        }
        IrExprKind::RuntimeCall { args, .. } => {
            args.iter().any(|a| expr_references_var(a, var))
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            expr_references_var(object, var) || expr_references_var(index, var)
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            expr_references_var(object, var)
        }
        IrExprKind::Clone { expr: e } | IrExprKind::Borrow { expr: e, .. }
        | IrExprKind::Deref { expr: e } | IrExprKind::ToVec { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e } => {
            expr_references_var(e, var)
        }
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            expr_references_var(e, var) || expr_references_var(f, var)
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().any(|e| expr_references_var(e, var))
        }
        IrExprKind::If { cond, then, else_ } => {
            expr_references_var(cond, var) || expr_references_var(then, var) || expr_references_var(else_, var)
        }
        _ => false,
    }
}
