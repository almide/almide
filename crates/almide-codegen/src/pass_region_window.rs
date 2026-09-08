//! RegionWindowPass (#1991): the native leg of the `consume(produce(scalars))`
//! REGION WINDOW the structural wasm leg got in #1961 / #1980
//! (`crates/almide-wasm/src/region.rs`).
//!
//! `check(make(depth))` builds a tree whose entire lifetime is that one
//! expression: one `Box::new` per node on the way in, one drop per node on
//! the way out. Measured (issue #1991, arm64): 228 ms against an arena's
//! 88 ms, and a hand-written Rust `Box` program measures the same 239 ms —
//! the emitted code is what a Rust programmer writes, and both lose to a
//! bump allocator by 2.6×.
//!
//! The recogniser is the wasm leg's, ported onto the IR this pipeline holds
//! after `ResolveCalls`: a Named call `consume(args)` opens a window iff
//! `consume` is REGION-PURE and returns a scalar, exactly one argument is
//! heap-typed and that argument is a Named call `produce(fargs)` with
//! `produce` region-pure, every `farg` a scalar expression and every
//! `produce` parameter scalar, and every other `consume` argument a scalar
//! expression. Region-pure is the greatest fixpoint over the root fns: no
//! effect / test / `main` / `mut` param, and a body inside the pure
//! vocabulary — literals, locals (no globals), scalar operators, `if` /
//! `match` / blocks / `while` / `for` over a range, tuples, records,
//! `Option` / `Result` wrappers, variant constructors, calls to other
//! region-pure fns and to the scalar stdlib modules.
//!
//! What the native leg does with a site — the wasm leg rewinds its own bump
//! allocator, this leg has rustc's `Box` — is in `pass_region_window_clone.rs`:
//! the site's callee closure is cloned as `__rgn_*` twins over `Copy` twin
//! enums whose recursive fields are `AlmideRgn<T>` handles into the prelude's
//! thread-local bump arena, and the site itself becomes
//! `almide_region_window(|| __rgn_consume(__rgn_produce(..)))` — save the
//! arena mark, run the pair, rewind. Nothing built inside can escape: the
//! result is a scalar, the twins write no global and hold no `mut` param,
//! and the twin types exist nowhere else in the program. The originals stay
//! untouched (`held_tree`-shaped uses keep their `Box`), so the twin enums
//! ride beside them rather than replacing them.
//!
//! v1 admission, beyond the wasm recogniser: the heap argument's type must
//! be a root-module, non-generic variant enum whose payloads are scalars or
//! other such enums (tuple / unit cases only) — the shape whose twin is
//! `Copy` — and every fn in the closure must be a root-module fn. Knobs as
//! on the wasm leg: `ALMIDE_REGION_OFF=1` disables the rewrite,
//! `ALMIDE_REGION_DEBUG=1` prints the pure set and every window site.

use std::collections::{HashMap, HashSet};

use almide_base::intern::{sym, Sym};
use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use almide_ir::*;
use almide_lang::types::Ty;

use super::pass::{NanoPass, PassResult, Postcondition, Target};
use super::pass_region_window_clone::{admissible_enums, closure_typable, synthesize_twins, RGN_PREFIX};

/// Stdlib modules whose every fn is side-effect free and touches no
/// variant block (the wasm leg's `SCALAR_MODULES`). After `ResolveCalls` a
/// bundled call to one of them is spelled `Named { almide_rt_<m>_<f> }`.
pub(crate) const SCALAR_MODULES: &[&str] = &["int", "float", "math", "bool"];

#[derive(Debug)]
pub struct RegionWindowPass;

impl NanoPass for RegionWindowPass {
    fn name(&self) -> &str { "RegionWindow" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    // The recogniser reads the post-ResolveCalls call spelling; the twin
    // enums must exist before BoxDeref computes the recursive-enum set.
    fn depends_on(&self) -> Vec<&'static str> { vec!["ResolveCalls"] }
    fn run_before(&self) -> Vec<&'static str> { vec!["BoxDeref"] }
    fn postconditions(&self) -> Vec<Postcondition> {
        vec![Postcondition::Custom(verify_twins_closed)]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        if std::env::var_os("ALMIDE_REGION_OFF").is_some() {
            return PassResult { program, changed: false };
        }
        let changed = rewrite_windows(&mut program);
        PassResult { program, changed }
    }
}

// ── the recogniser ───────────────────────────────────────────────────

/// Everything the purity walk and the site test look up.
pub(crate) struct Cx<'a> {
    /// Root fn name → index into `program.functions`.
    pub fns: HashMap<Sym, usize>,
    /// The region-pure root fns (by name).
    pub pure: HashSet<Sym>,
    /// Every top-let var, root and module.
    pub globals: HashSet<VarId>,
    /// Constructor name → the root variant enum declaring it.
    pub ctors: HashMap<Sym, Sym>,
    pub decls: &'a [IrTypeDecl],
}

pub(crate) fn is_scalar_ty(t: &Ty) -> bool {
    matches!(t, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit)
}

/// A `Named` callee spelled by `ResolveCalls` for a scalar stdlib module.
pub(crate) fn is_scalar_module_call(name: &str) -> bool {
    SCALAR_MODULES.iter().any(|m| name.starts_with(&format!("almide_rt_{m}_")))
}

fn seed_ok(f: &IrFunction) -> bool {
    !f.is_effect
        && !f.is_test
        && f.name.as_str() != "main"
        && f.generics.is_none()
        && f.extern_attrs.is_empty()
        && f.params.iter().all(|p| !p.is_mut)
}

/// The region-pure subset of the root fns — a greatest fixpoint: start
/// from every candidate and drop a fn whose body reaches anything outside
/// the pure vocabulary (a dropped fn included), until nothing changes.
fn region_pure_fns(program: &IrProgram, cx: &mut Cx) {
    let mut pure: HashSet<Sym> = program.functions.iter().filter(|f| seed_ok(f)).map(|f| f.name).collect();
    loop {
        let before = pure.len();
        let snapshot = pure;
        pure = snapshot
            .iter()
            .copied()
            .filter(|n| expr_pure(&program.functions[cx.fns[n]].body, &PureCx { pure: &snapshot, cx }))
            .collect();
        if pure.len() == before {
            break;
        }
    }
    if std::env::var_os("ALMIDE_REGION_DEBUG").is_some() {
        let mut names: Vec<&str> = pure.iter().map(|n| n.as_str()).collect();
        names.sort_unstable();
        eprintln!("[region:native] pure fns: {}", names.join(" "));
    }
    cx.pure = pure;
}

struct PureCx<'a, 'b> {
    pure: &'a HashSet<Sym>,
    cx: &'a Cx<'b>,
}

fn all_pure(es: &[IrExpr], p: &PureCx) -> bool {
    es.iter().all(|e| expr_pure(e, p))
}

fn stmts_pure(ss: &[IrStmt], p: &PureCx) -> bool {
    ss.iter().all(|s| match &s.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => expr_pure(value, p),
        IrStmtKind::Assign { var, value } => !p.cx.globals.contains(var) && expr_pure(value, p),
        IrStmtKind::Expr { expr } => expr_pure(expr, p),
        IrStmtKind::Comment { .. } => true,
        _ => false,
    })
}

fn call_pure(target: &CallTarget, args: &[IrExpr], p: &PureCx) -> bool {
    if !all_pure(args, p) {
        return false;
    }
    match target {
        CallTarget::Named { name } => {
            p.cx.ctors.contains_key(name) || p.pure.contains(name) || is_scalar_module_call(name.as_str())
        }
        CallTarget::Module { module, .. } => SCALAR_MODULES.contains(&module.as_str()),
        _ => false,
    }
}

/// Is this expression inside the region-pure vocabulary?
fn expr_pure(e: &IrExpr, p: &PureCx) -> bool {
    use IrExprKind as K;
    match &e.kind {
        K::LitInt { .. } | K::LitFloat { .. } | K::LitStr { .. } | K::LitBool { .. } | K::Unit => true,
        K::OptionNone | K::Break | K::Continue => true,
        K::Var { id } => !p.cx.globals.contains(id),
        K::BinOp { left, right, .. } => expr_pure(left, p) && expr_pure(right, p),
        K::UnOp { operand, .. } => expr_pure(operand, p),
        K::If { cond, then, else_ } => expr_pure(cond, p) && expr_pure(then, p) && expr_pure(else_, p),
        K::Match { subject, arms } => {
            expr_pure(subject, p)
                && arms.iter().all(|a| a.guard.as_ref().is_none_or(|g| expr_pure(g, p)) && expr_pure(&a.body, p))
        }
        K::Block { stmts, expr } => stmts_pure(stmts, p) && expr.as_deref().is_none_or(|t| expr_pure(t, p)),
        K::While { cond, body } => expr_pure(cond, p) && stmts_pure(body, p),
        K::ForIn { iterable, body, .. } => {
            matches!(iterable.kind, K::Range { .. }) && expr_pure(iterable, p) && stmts_pure(body, p)
        }
        K::Range { start, end, .. } => expr_pure(start, p) && expr_pure(end, p),
        K::Call { target, args, .. } => call_pure(target, args, p),
        K::Tuple { elements } => all_pure(elements, p),
        K::Record { fields, .. } => fields.iter().all(|(_, fe)| expr_pure(fe, p)),
        K::Member { object, .. } | K::TupleIndex { object, .. } => expr_pure(object, p),
        K::OptionSome { expr } | K::ResultOk { expr } | K::ResultErr { expr } | K::Unwrap { expr } => {
            expr_pure(expr, p)
        }
        K::UnwrapOr { expr, fallback } => expr_pure(expr, p) && expr_pure(fallback, p),
        _ => false,
    }
}

/// A scalar argument the window may evaluate: literals, locals, scalar
/// operators and region-pure calls over the same.
fn scalar_arg(e: &IrExpr, cx: &Cx) -> bool {
    use IrExprKind as K;
    match &e.kind {
        K::LitInt { .. } | K::LitFloat { .. } | K::LitBool { .. } | K::Unit => true,
        K::Var { id } => !cx.globals.contains(id),
        K::BinOp { left, right, .. } => scalar_arg(left, cx) && scalar_arg(right, cx),
        K::UnOp { operand, .. } => scalar_arg(operand, cx),
        K::Call { target, args, .. } => {
            args.iter().all(|a| scalar_arg(a, cx))
                && match target {
                    CallTarget::Named { name } => cx.pure.contains(name) || is_scalar_module_call(name.as_str()),
                    CallTarget::Module { module, .. } => SCALAR_MODULES.contains(&module.as_str()),
                    _ => false,
                }
        }
        _ => false,
    }
}

/// One window site: the pair and the enum the produced value has.
#[derive(Debug, Clone)]
pub(crate) struct Site {
    pub consume: Sym,
    pub produce: Sym,
    pub heap_ty: Sym,
}

/// Does the Named call `consume(args)` open a region window? See the
/// module doc for the rule; `None` when any clause fails.
pub(crate) fn window_site(target: &CallTarget, args: &[IrExpr], program: &IrProgram, cx: &Cx) -> Option<Site> {
    let CallTarget::Named { name: consume } = target else { return None };
    if !cx.pure.contains(consume) {
        return None;
    }
    let cf = &program.functions[cx.fns[consume]];
    if !is_scalar_ty(&cf.ret_ty) || cf.params.len() != args.len() {
        return None;
    }
    let mut heap: Option<(Sym, Sym)> = None;
    for (a, p) in args.iter().zip(&cf.params) {
        if is_scalar_ty(&p.ty) {
            if !scalar_arg(a, cx) {
                return None;
            }
            continue;
        }
        if heap.is_some() {
            return None;
        }
        heap = Some((produce_of(a, &p.ty, program, cx)?, enum_name(&p.ty)?));
    }
    let (produce, heap_ty) = heap?;
    Some(Site { consume: *consume, produce, heap_ty })
}

pub(crate) fn enum_name(t: &Ty) -> Option<Sym> {
    match t {
        Ty::Named(n, args) if args.is_empty() => Some(*n),
        _ => None,
    }
}

/// The heap argument must be `produce(scalars)` over a region-pure fn whose
/// every param is scalar, returning exactly the param's type.
fn produce_of(a: &IrExpr, param_ty: &Ty, program: &IrProgram, cx: &Cx) -> Option<Sym> {
    let IrExprKind::Call { target: CallTarget::Named { name }, args: fargs, .. } = &a.kind else { return None };
    if !cx.pure.contains(name) || !fargs.iter().all(|x| scalar_arg(x, cx)) {
        return None;
    }
    let pf = &program.functions[cx.fns[name]];
    let ok = pf.params.iter().all(|p| is_scalar_ty(&p.ty)) && &pf.ret_ty == param_ty && &a.ty == param_ty;
    ok.then_some(*name)
}

// ── the rewrite ──────────────────────────────────────────────────────

/// Build the recogniser context for the root program.
pub(crate) fn build_cx(program: &IrProgram) -> Cx<'_> {
    let fns = program.functions.iter().enumerate().map(|(i, f)| (f.name, i)).collect();
    let globals = program
        .top_lets
        .iter()
        .map(|t| t.var)
        .chain(program.modules.iter().flat_map(|m| m.top_lets.iter().map(|t| t.var)))
        .collect();
    let mut ctors = HashMap::new();
    for td in &program.type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            ctors.extend(cases.iter().map(|c| (c.name, td.name)));
        }
    }
    let mut cx = Cx { fns, pure: HashSet::new(), globals, ctors, decls: &program.type_decls };
    region_pure_fns(program, &mut cx);
    cx
}

/// Find every window site in the root fns, synthesize the twins once, and
/// rewrite each site onto them. Returns whether anything changed.
fn rewrite_windows(program: &mut IrProgram) -> bool {
    let plan = {
        let cx = build_cx(program);
        if cx.pure.is_empty() {
            return false;
        }
        let sites = find_sites(program, &cx);
        if sites.is_empty() {
            return false;
        }
        match plan_twins(program, &cx, sites) {
            Some(plan) => plan,
            None => return false,
        }
    };
    synthesize_twins(program, &plan);
    let admitted: HashSet<(Sym, Sym)> = plan.sites.iter().map(|s| (s.consume, s.produce)).collect();
    let mut rw = SiteRewriter { admitted: &admitted };
    for f in &mut program.functions {
        rw.visit_expr_mut(&mut f.body);
    }
    true
}

/// Every window site in the root fn bodies (a site nested in another
/// site's argument is impossible — window arguments are scalar).
fn find_sites(program: &IrProgram, cx: &Cx) -> Vec<Site> {
    struct Finder<'a> {
        program: &'a IrProgram,
        cx: &'a Cx<'a>,
        sites: Vec<Site>,
    }
    impl IrVisitor for Finder<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target, args, .. } = &e.kind
                && let Some(site) = window_site(target, args, self.program, self.cx)
            {
                self.sites.push(site);
            }
            walk_expr(self, e);
        }
    }
    let mut finder = Finder { program, cx, sites: Vec::new() };
    for f in &program.functions {
        finder.visit_expr(&f.body);
    }
    finder.sites
}

/// What the clone step synthesizes: the admitted sites, the twin enum set
/// and the callee closure (all by ORIGINAL name).
pub(crate) struct TwinPlan {
    pub sites: Vec<Site>,
    pub enums: HashSet<Sym>,
    pub fns: HashSet<Sym>,
}

/// Admit each site's enum (v1: `Copy`-admissible root variant), take the
/// union of the callee closures, and refuse the whole plan when a closure
/// fn mentions a non-twinned named type that reaches a twinned one (its
/// clone could not be typed). Sites whose enum is not admissible are
/// dropped; `None` when nothing survives.
fn plan_twins(program: &IrProgram, cx: &Cx, sites: Vec<Site>) -> Option<TwinPlan> {
    let mut enums: HashSet<Sym> = HashSet::new();
    let mut admitted: Vec<Site> = Vec::new();
    for s in sites {
        if let Some(set) = admissible_enums(s.heap_ty, cx.decls) {
            enums.extend(set);
            admitted.push(s);
        }
    }
    if admitted.is_empty() {
        return None;
    }
    let fns = callee_closure(program, cx, admitted.iter().flat_map(|s| [s.consume, s.produce]));
    if !closure_typable(program, cx, &fns, &enums) {
        return None;
    }
    if std::env::var_os("ALMIDE_REGION_DEBUG").is_some() {
        for s in &admitted {
            eprintln!("[region:native] window {}({}(..)) over {}", s.consume, s.produce, s.heap_ty);
        }
    }
    Some(TwinPlan { sites: admitted, enums, fns })
}

/// The transitive Named-callee closure of `roots` inside the pure set.
fn callee_closure(program: &IrProgram, cx: &Cx, roots: impl Iterator<Item = Sym>) -> HashSet<Sym> {
    struct Callees<'a>(&'a mut Vec<Sym>);
    impl IrVisitor for Callees<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &e.kind {
                self.0.push(*name);
            }
            walk_expr(self, e);
        }
    }
    let mut done: HashSet<Sym> = HashSet::new();
    let mut todo: Vec<Sym> = roots.collect();
    while let Some(n) = todo.pop() {
        if !cx.pure.contains(&n) || !done.insert(n) {
            continue;
        }
        let mut found = Vec::new();
        Callees(&mut found).visit_expr(&program.functions[cx.fns[&n]].body);
        todo.extend(found);
    }
    done
}

/// Replace every admitted `consume(produce(..), ..)` by the window form
/// over the `__rgn_` twins.
struct SiteRewriter<'a> {
    admitted: &'a HashSet<(Sym, Sym)>,
}

impl IrMutVisitor for SiteRewriter<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        if let Some(call) = self.window_call(e) {
            let template = "almide_region_window(|| {call})".to_string();
            e.kind = IrExprKind::InlineRust { template, args: vec![(sym("call"), call)] };
            e.def_id = None;
        }
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        walk_stmt_mut(self, s);
    }
}

impl SiteRewriter<'_> {
    /// The twin call for an admitted site, or `None`.
    fn window_call(&self, e: &IrExpr) -> Option<IrExpr> {
        let IrExprKind::Call { target: CallTarget::Named { name: consume }, args, type_args } = &e.kind else {
            return None;
        };
        let produce = args.iter().find_map(|a| match &a.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, .. } if !is_scalar_ty(&a.ty) => Some(*name),
            _ => None,
        })?;
        if !self.admitted.contains(&(*consume, produce)) {
            return None;
        }
        let args = args.iter().cloned().map(|a| twin_produce_arg(a, produce)).collect();
        Some(IrExpr {
            kind: IrExprKind::Call { target: CallTarget::Named { name: twin_name(*consume) }, args, type_args: type_args.clone() },
            ty: e.ty.clone(),
            span: e.span,
            def_id: None,
        })
    }
}

/// The site's heap argument re-pointed at the `__rgn_` producer and typed
/// as the twin enum; every other (scalar) argument passes through.
fn twin_produce_arg(mut a: IrExpr, produce: Sym) -> IrExpr {
    let heap = !is_scalar_ty(&a.ty);
    if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &mut a.kind
        && *name == produce
        && heap
    {
        *name = twin_name(produce);
        a.ty = Ty::Named(twin_name(enum_name(&a.ty).expect("site enum")), vec![]);
        a.def_id = None;
    }
    a
}

pub(crate) fn twin_name(n: Sym) -> Sym {
    sym(&format!("{RGN_PREFIX}{}", n.as_str()))
}

// ── postcondition ────────────────────────────────────────────────────

/// Every `__rgn_` fn body calls only `__rgn_` fns / ctors and the scalar
/// stdlib modules — a twin that reached an original would carry a region
/// value across the window edge.
fn verify_twins_closed(program: &IrProgram) -> Vec<String> {
    struct Check<'a> {
        fn_name: &'a str,
        out: &'a mut Vec<String>,
    }
    impl IrVisitor for Check<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Call { target: CallTarget::Named { name }, .. } => {
                    let n = name.as_str();
                    if !n.starts_with(RGN_PREFIX) && !is_scalar_module_call(n) {
                        self.out.push(format!("RegionWindow: twin `{}` calls non-twin `{n}`", self.fn_name));
                    }
                }
                IrExprKind::Call { target: CallTarget::Module { module, .. }, .. }
                    if !SCALAR_MODULES.contains(&module.as_str()) =>
                {
                    self.out.push(format!("RegionWindow: twin `{}` calls module `{module}`", self.fn_name));
                }
                IrExprKind::Call { target: CallTarget::Method { .. } | CallTarget::Computed { .. }, .. }
                | IrExprKind::Lambda { .. } => {
                    self.out.push(format!("RegionWindow: twin `{}` holds a computed call or lambda", self.fn_name));
                }
                _ => {}
            }
            walk_expr(self, e);
        }
    }
    let mut out = Vec::new();
    for f in program.functions.iter().filter(|f| f.name.as_str().starts_with(RGN_PREFIX)) {
        Check { fn_name: f.name.as_str(), out: &mut out }.visit_expr(&f.body);
    }
    out
}
