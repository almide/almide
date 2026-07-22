//! TopLetStorage — completeness-by-construction §4, Stage 1.
//!
//! THE single place that decides how a module-level `let`/`var` is stored
//! and named. Today that decision is re-derived at five sites (walker
//! pre-index, walker register, pass_clone, lowering's module_origin, the
//! wasm synonym registration) that must agree by convention — #486, #500,
//! #501 and #505 were all cells where two of them silently disagreed.
//!
//! Stage 1 (this module + `TopLetStoragePass` + the walker-side agreement
//! verifier): the attribute is COMPUTED once and ASSERTED equal to every
//! legacy predicate, converting the next drift into a `[COMPILER BUG]`
//! build failure. Stage 2 flips consumers onto the attribute and deletes
//! the legacy predicates.
//!
//! Every function here is pure; the pass is just the compute-once executor.

use std::collections::HashMap;
use almide_lang::types::Ty;
use crate::{IrExpr, IrExprKind, BinOp, TopLetKind, VarId, VarInfo, VarTable, IrTopLet};

/// Copy-ness classes — ONE predicate for what today is four divergent ones
/// (walker `Int|Float|Bool`, pass_clone heap-ness, RcCow exclusion,
/// shared-mut Copy test). Stage 1 mirrors the WALKER's storage rule exactly
/// (scalar = Int/Float/Bool); canonicalizing the other predicates onto this
/// enum is stage 2c, a behavior-reviewed change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyClass {
    /// Int / Float / Bool — the walker's `Cell` class.
    Scalar,
    /// Reserved for stage 2c (Float32 / Unit / all-numeric tuples).
    CopyComposite,
    /// `Ty::Unknown` — inference failed; treated as non-Copy.
    Opaque,
    /// Everything else, including TypeVar.
    Heap,
}

pub fn copy_class(ty: &Ty) -> CopyClass {
    match ty {
        Ty::Int | Ty::Float | Ty::Bool => CopyClass::Scalar,
        Ty::Unknown => CopyClass::Opaque,
        _ => CopyClass::Heap,
    }
}

// ── Copy-ness projections (§4 stage 2c, #531) ───────────────────────────
//
// ONE classifier, FOUR named projections. The four historic predicates
// (walker storage rule, pass_clone's needs_clone, the RcCow eligibility
// test, capture-clone's shared-cell test) were free-standing `matches!`
// lists that agreed only by coincidence; they now live HERE, side by side,
// and every edge-cell difference is explicit and intentional:
//
//   projection         Int/Float/Bool  sized-numeric  Unit/RawPtr  Unknown  numeric-tuple
//   storage Cell       yes             no             n/a          no       no
//   clone_free         yes             yes            yes          yes      yes
//   rccow_copyish      yes             no             Unit only    yes      no
//   capture_copy_cell  yes             no             no           no       no
//
// The conservative cells (sized numerics outside clone_free's column) are
// candidates for future REVIEWED widening — widening any of them changes
// generated storage and must come with its own fixture + byte-diff review.

/// pass_clone projection: types whose values move without a `.clone()` on
/// the Rust target (Copy or trivially-rebuildable). The exact complement of
/// the historic `needs_clone`.
pub fn clone_free(ty: &Ty) -> bool {
    match ty {
        Ty::String | Ty::Applied(_, _)
        | Ty::Record { .. } | Ty::OpenRecord { .. }
        | Ty::Named(_, _) | Ty::Matrix | Ty::Bytes
        | Ty::Variant { .. } | Ty::Fn { .. }
        | Ty::TypeVar(_) => false,
        Ty::Tuple(elements) => elements.iter().all(clone_free),
        _ => true,
    }
}

/// RcCow-eligibility projection: a mutable LOCAL of one of these types
/// stays a plain `let mut` (no COW wrapper) even when captured.
pub fn rccow_copyish(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit | Ty::Unknown)
}

/// Capture clone-wrap projection (pass_capture_clone): heap values captured
/// by a lambda get a `__cap` clone. Differs from `clone_free` in ONE cell —
/// tuples are NOT clone-wrapped here regardless of their elements (the
/// capture path moves tuples whole); widening that cell is a reviewed
/// future delta.
pub fn capture_clone_wrap(ty: &Ty) -> bool {
    matches!(ty,
        Ty::String | Ty::Applied(_, _)
        | Ty::Record { .. } | Ty::OpenRecord { .. }
        | Ty::Named(_, _) | Ty::Matrix | Ty::Bytes
        | Ty::Variant { .. } | Ty::Fn { .. }
        | Ty::TypeVar(_)
    )
}

/// Capture-cell projection: a `var` local of one of these types captured by
/// a closure becomes an `Rc<Cell<T>>` shared cell (Closure v2 P3); non-Copy
/// captures take the SharedMut heap-cell path (P6) instead.
pub fn capture_copy_cell(ty: &Ty) -> bool {
    copy_class(ty) == CopyClass::Scalar
}

/// The storage class of one top-let on the native target. WASM stores every
/// top-let as one mutable global; this enum still drives its init-order and
/// const-evaluability decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopLetStorage {
    /// Immutable, const-evaluable initializer → `const NAME: T = v;`
    Const,
    /// Immutable, runtime initializer → `static NAME: LazyLock<T>`.
    /// `eager_force` = the initializer can abort (integer `/` `%`), so the
    /// main wrapper forces it in declaration order (C-007 wasm parity).
    Lazy { eager_force: bool },
    /// Mutable scalar → `thread_local! { static NAME: Cell<T> }`.
    Cell,
    /// Mutable non-scalar → `thread_local! { static NAME: RefCell<Rc<T>> }`.
    RcRefCell,
}

/// The per-declaration storage record.
#[derive(Debug, Clone)]
pub struct GlobalInfo {
    pub storage: TopLetStorage,
    /// The emitted static identifier — THE one site that owns the
    /// `ALMIDE_RT_{ORIGIN}_{NAME}` format (mirrors the walker's
    /// `global_static_name`, byte-for-byte).
    pub static_name: String,
    /// The DECLARATION VarId (alias-resolve synthetic use-site ids to this).
    pub decl: VarId,
}

/// Table 1a of the §4 design: (mutability × copy-class × kind ×
/// abortability) → storage. TOTAL — no fallthrough arm.
pub fn classify_storage(mutable: bool, kind: TopLetKind, ty: &Ty, init_aborts: bool) -> TopLetStorage {
    if mutable {
        // Mutability overrides kind (a mutable Const-classified top-let is
        // still a cell — the walker checks storage before the const arm).
        match copy_class(ty) {
            CopyClass::Scalar => TopLetStorage::Cell,
            CopyClass::CopyComposite | CopyClass::Opaque | CopyClass::Heap => TopLetStorage::RcRefCell,
        }
    } else {
        match kind {
            TopLetKind::Const => TopLetStorage::Const,
            TopLetKind::Lazy => TopLetStorage::Lazy { eager_force: init_aborts },
        }
    }
}

/// The emitted static name — mirrors `walker::global_static_name` exactly.
pub fn static_name(vi: &VarInfo) -> String {
    match &vi.module_origin {
        Some(origin) => format!("ALMIDE_RT_{}_{}", origin.to_uppercase(), vi.name.as_str().to_uppercase()),
        None => vi.name.as_str().to_uppercase(),
    }
}

/// THE abortability predicate (today: integer `/` or `%`, which abort on a
/// zero divisor / MIN÷-1). Owned here so the native eager-force decision and
/// any future wasm init decision share one rule.
pub fn init_can_abort(expr: &IrExpr) -> bool {
    use crate::visit::{IrVisitor, walk_expr};
    struct Finder { found: bool }
    impl IrVisitor for Finder {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found { return; }
            if matches!(&e.kind, IrExprKind::BinOp { op: BinOp::DivInt | BinOp::ModInt, .. }) {
                self.found = true;
                return;
            }
            walk_expr(self, e);
        }
    }
    let mut f = Finder { found: false };
    f.visit_expr(expr);
    f.found
}

use std::collections::HashSet;
use crate::{CallTarget, IrFunction, IrProgram};

/// Per-expression scan: the global top-let VarIds an expression reads DIRECTLY
/// (resolved through `alias`) and the callees it invokes by identity. Callees
/// are keyed so the interprocedural read-set can be folded in (`#632`: a
/// top-let initializer that *calls* `cfg.banner()` transitively reads
/// `cfg.APP_NAME`, with no direct `Var` to it).
fn scan_reads_and_calls(
    expr: &IrExpr,
    alias: &HashMap<VarId, VarId>,
    decls: &HashSet<VarId>,
) -> (Vec<VarId>, Vec<FnKey>) {
    use crate::visit::{IrVisitor, walk_expr};
    struct Collector<'a> {
        alias: &'a HashMap<VarId, VarId>,
        decls: &'a HashSet<VarId>,
        reads: Vec<VarId>,
        calls: Vec<FnKey>,
    }
    impl IrVisitor for Collector<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Var { id } => {
                    let decl = self.alias.get(id).copied().unwrap_or(*id);
                    if self.decls.contains(&decl) && !self.reads.contains(&decl) {
                        self.reads.push(decl);
                    }
                }
                IrExprKind::Call { target, .. } => {
                    if let Some(k) = fn_key_of_target(target) {
                        if !self.calls.contains(&k) { self.calls.push(k); }
                    }
                }
                _ => {}
            }
            walk_expr(self, e);
        }
    }
    let mut c = Collector { alias, decls, reads: Vec::new(), calls: Vec::new() };
    c.visit_expr(expr);
    (c.reads, c.calls)
}

/// Identity of a callable user function: its bare name (free fn / variant ctor)
/// or `module::func`. Stdlib calls resolve to no user `IrFunction`, so they
/// contribute no globals and are simply absent from the function index.
type FnKey = String;

fn fn_key_of_target(target: &CallTarget) -> Option<FnKey> {
    match target {
        CallTarget::Named { name } => Some(name.as_str().to_string()),
        CallTarget::Module { module, func, .. } =>
            Some(format!("{}::{}", module.as_str(), func.as_str())),
        // Method/Computed callees are not name-resolved here; a global read
        // reached only through such a callee falls back to the module-level
        // safety net in `dependency_init_order`.
        CallTarget::Method { .. } | CallTarget::Computed { .. } => None,
    }
}

fn fn_key_of_function(f: &IrFunction) -> FnKey {
    match &f.module_origin {
        Some(m) => format!("{}::{}", m, f.name.as_str()),
        None => f.name.as_str().to_string(),
    }
}

/// #632: dependency-respecting global init order. WASM evaluates top-let
/// initializers EAGERLY in `global_init_order`; native forces abortable lazies
/// in that same order (C-007). When an importing module's top-let reads an
/// imported module's heap global — directly OR through a function it calls —
/// that global must be initialized FIRST, or the wasm read hits a still-zero
/// header (null ptr + zero len). The legacy order (root top-lets, then
/// per-module) put a root let BEFORE the submodule global it depends on, so the
/// cross-module read miscompiled on wasm only.
///
/// Dependencies are computed interprocedurally: a top-let depends on every
/// global its initializer reads directly, PLUS every global transitively read
/// by the user functions it calls. As a safety net for reads reachable only
/// through un-named callees (method/computed/stdlib HOFs), a root top-let also
/// depends on every module global, and a module top-let on every global of a
/// module it (transitively) references — submodules are dependencies of their
/// importer by construction, so this can never invert a real ordering.
///
/// We stable-topo-sort the legacy declaration order so every top-let comes
/// after the globals it depends on; ties keep the original (root-then-module)
/// sequence for host-arch determinism. A dependency cycle (no legal source
/// program produces one across distinct globals) degrades gracefully to the
/// legacy order.
pub fn dependency_init_order(
    program: &IrProgram,
    alias: &HashMap<VarId, VarId>,
) -> Vec<VarId> {
    let decl_order = build_decl_order(program);
    let decls: HashSet<VarId> = decl_order.iter().map(|(v, _, _)| *v).collect();
    let (fn_reads, fn_calls) = build_fn_index(program, alias, &decls);
    let module_globals = build_module_globals(&decl_order);
    let deps = compute_dependency_sets(&decl_order, alias, &decls, &fn_reads, &fn_calls, &module_globals);
    topo_sort_emit(&decl_order, &deps)
}

/// Step 1: the legacy emission order — (decl VarId, owning module, &initializer).
/// `None` module = root.
fn build_decl_order(program: &IrProgram) -> Vec<(VarId, Option<&str>, &IrExpr)> {
    let mut decl_order: Vec<(VarId, Option<&str>, &IrExpr)> = Vec::new();
    for tl in &program.top_lets {
        decl_order.push((tl.var, None, &tl.value));
    }
    for m in &program.modules {
        for tl in &m.top_lets {
            decl_order.push((tl.var, Some(m.name.as_str()), &tl.value));
        }
    }
    decl_order
}

/// Scan one function's body and fold its reads/calls into the running index.
fn index_fn_reads_calls(
    f: &IrFunction,
    alias: &HashMap<VarId, VarId>,
    decls: &HashSet<VarId>,
    fn_reads: &mut HashMap<FnKey, Vec<VarId>>,
    fn_calls: &mut HashMap<FnKey, Vec<FnKey>>,
) {
    let (reads, calls) = scan_reads_and_calls(&f.body, alias, decls);
    let key = fn_key_of_function(f);
    fn_reads.entry(key.clone()).or_default().extend(reads);
    fn_calls.entry(key).or_default().extend(calls);
}

/// Step 2: index every user function by identity, with the globals it reads
/// directly and the callees it invokes.
fn build_fn_index(
    program: &IrProgram,
    alias: &HashMap<VarId, VarId>,
    decls: &HashSet<VarId>,
) -> (HashMap<FnKey, Vec<VarId>>, HashMap<FnKey, Vec<FnKey>>) {
    let mut fn_reads: HashMap<FnKey, Vec<VarId>> = HashMap::new();
    let mut fn_calls: HashMap<FnKey, Vec<FnKey>> = HashMap::new();
    for f in &program.functions {
        index_fn_reads_calls(f, alias, decls, &mut fn_reads, &mut fn_calls);
    }
    for m in &program.modules {
        for f in &m.functions {
            index_fn_reads_calls(f, alias, decls, &mut fn_reads, &mut fn_calls);
        }
    }
    (fn_reads, fn_calls)
}

/// Step 3: transitive global read-set per function (fixpoint over the call
/// graph; cycle-safe via the visited set).
fn fn_global_reads(
    key: &FnKey,
    fn_reads: &HashMap<FnKey, Vec<VarId>>,
    fn_calls: &HashMap<FnKey, Vec<FnKey>>,
    seen: &mut HashSet<FnKey>,
    out: &mut Vec<VarId>,
) {
    if !seen.insert(key.clone()) { return; }
    if let Some(rs) = fn_reads.get(key) {
        for &r in rs { if !out.contains(&r) { out.push(r); } }
    }
    if let Some(cs) = fn_calls.get(key) {
        for c in cs { fn_global_reads(c, fn_reads, fn_calls, seen, out); }
    }
}

/// Step 4: module → its top-let global decls (for the coarse safety net).
fn build_module_globals<'a>(
    decl_order: &[(VarId, Option<&'a str>, &IrExpr)],
) -> HashMap<&'a str, Vec<VarId>> {
    let mut module_globals: HashMap<&str, Vec<VarId>> = HashMap::new();
    for &(v, m, _) in decl_order {
        if let Some(mn) = m { module_globals.entry(mn).or_default().push(v); }
    }
    module_globals
}

/// Interprocedural half of step 5: fold globals read (transitively) through
/// every callee of a single top-let's initializer into `dep`.
fn add_transitive_callee_reads(
    calls: &[FnKey],
    fn_reads: &HashMap<FnKey, Vec<VarId>>,
    fn_calls: &HashMap<FnKey, Vec<FnKey>>,
    dep: &mut Vec<VarId>,
) {
    for c in calls {
        let mut seen = HashSet::new();
        let mut reads = Vec::new();
        fn_global_reads(c, fn_reads, fn_calls, &mut seen, &mut reads);
        for r in reads { if !dep.contains(&r) { dep.push(r); } }
    }
}

/// Safety-net half of step 5: a root top-let depends on every module global;
/// this is sound because submodules are always dependencies of the root (the
/// root imports them, never vice versa). Catches reads reachable only
/// through method/computed/stdlib-HOF callees that step 2 can't name.
fn add_module_globals_safety_net(module_globals: &HashMap<&str, Vec<VarId>>, dep: &mut Vec<VarId>) {
    for (_, gs) in module_globals {
        for &g in gs { if !dep.contains(&g) { dep.push(g); } }
    }
}

/// Step 5: per-top-let dependency set.
fn compute_dependency_sets(
    decl_order: &[(VarId, Option<&str>, &IrExpr)],
    alias: &HashMap<VarId, VarId>,
    decls: &HashSet<VarId>,
    fn_reads: &HashMap<FnKey, Vec<VarId>>,
    fn_calls: &HashMap<FnKey, Vec<FnKey>>,
    module_globals: &HashMap<&str, Vec<VarId>>,
) -> HashMap<VarId, Vec<VarId>> {
    let mut deps: HashMap<VarId, Vec<VarId>> = HashMap::new();
    for &(v, owner, expr) in decl_order {
        let (direct, calls) = scan_reads_and_calls(expr, alias, decls);
        let mut dep: Vec<VarId> = direct;
        add_transitive_callee_reads(&calls, fn_reads, fn_calls, &mut dep);
        if owner.is_none() {
            add_module_globals_safety_net(module_globals, &mut dep);
        }
        dep.retain(|d| *d != v);
        deps.insert(v, dep);
    }
    deps
}

/// Depth-first dependency visit used by [`topo_sort_emit`]. `on_stack` breaks
/// cycles by emitting the node anyway in its legacy slot.
fn topo_visit(
    v: VarId,
    deps: &HashMap<VarId, Vec<VarId>>,
    done: &mut HashSet<VarId>,
    on_stack: &mut HashSet<VarId>,
    emitted: &mut Vec<VarId>,
) {
    if done.contains(&v) || on_stack.contains(&v) { return; }
    on_stack.insert(v);
    if let Some(ds) = deps.get(&v) {
        for &d in ds { topo_visit(d, deps, done, on_stack, emitted); }
    }
    on_stack.remove(&v);
    if done.insert(v) { emitted.push(v); }
}

/// Step 6: stable topological emit — walk the legacy order; before emitting a
/// node, emit its not-yet-emitted dependencies (depth-first).
fn topo_sort_emit(
    decl_order: &[(VarId, Option<&str>, &IrExpr)],
    deps: &HashMap<VarId, Vec<VarId>>,
) -> Vec<VarId> {
    let mut emitted: Vec<VarId> = Vec::with_capacity(decl_order.len());
    let mut done: HashSet<VarId> = HashSet::new();
    let mut on_stack: HashSet<VarId> = HashSet::new();
    for &(v, _, _) in decl_order {
        topo_visit(v, deps, &mut done, &mut on_stack, &mut emitted);
    }
    emitted
}

/// Alias-resolution key: (normalized module origin, UPPERCASE name). The
/// use-site synthetic Var carries the SCREAMING_CASE spelling and a
/// dot-normalized origin; the declaration keeps the source name and the
/// lowering-set origin. Normalizing both sides makes the match total.
fn alias_key(vi: &VarInfo) -> (String, String) {
    (
        vi.module_origin.as_deref().unwrap_or("").to_uppercase().replace('.', "_"),
        vi.name.as_str().to_uppercase(),
    )
}

/// Build the decl table + resolve every module-origin use-site VarId to its
/// declaration. Returns (globals, alias map, unresolved offenders).
pub fn build_global_tables(
    top_lets: &[(bool, TopLetKind, VarId, bool)],
    var_table: &VarTable,
) -> (HashMap<VarId, GlobalInfo>, HashMap<VarId, VarId>, Vec<String>) {
    let mut globals: HashMap<VarId, GlobalInfo> = HashMap::new();
    let mut by_key: HashMap<(String, String), VarId> = HashMap::new();
    for &(mutable, kind, var, init_aborts) in top_lets {
        let vi = var_table.get(var);
        let storage = classify_storage(mutable, kind, &vi.ty, init_aborts);
        globals.insert(var, GlobalInfo { storage, static_name: static_name(vi), decl: var });
        by_key.insert(alias_key(vi), var);
    }
    let mut alias: HashMap<VarId, VarId> = HashMap::new();
    let mut offenders: Vec<String> = Vec::new();
    for (i, vi) in var_table.entries.iter().enumerate() {
        let id = VarId(i as u32);
        if vi.module_origin.is_none() || globals.contains_key(&id) {
            continue;
        }
        match by_key.get(&alias_key(vi)) {
            Some(&decl) => { alias.insert(id, decl); }
            None => offenders.push(format!(
                "var #{} `{}` (origin {:?})",
                i, vi.name.as_str(), vi.module_origin
            )),
        }
    }
    (globals, alias, offenders)
}

/// Convenience: extract the classification inputs from an `IrTopLet`.
pub fn top_let_inputs(tl: &IrTopLet) -> (bool, TopLetKind, VarId, bool) {
    (tl.mutable, tl.kind, tl.var, init_can_abort(&tl.value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallTarget, IrExpr, IrExprKind, IrFunction, IrModule, IrProgram, IrTopLet,
        IrVisibility, TopLetKind, VarTable};
    use almide_base::intern::sym;

    fn var(id: u32) -> IrExpr {
        IrExpr { kind: IrExprKind::Var { id: VarId(id) }, ty: Ty::String, span: None, def_id: None }
    }
    fn lit() -> IrExpr {
        IrExpr { kind: IrExprKind::LitStr { value: "x".into() }, ty: Ty::String, span: None, def_id: None }
    }
    fn call_mod(module: &str, func: &str) -> IrExpr {
        IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym(module), func: sym(func), def_id: None },
                args: vec![],
                type_args: vec![],
            },
            ty: Ty::String, span: None, def_id: None,
        }
    }
    fn tl(var_id: u32, value: IrExpr) -> IrTopLet {
        IrTopLet { var: VarId(var_id), ty: Ty::String, value, kind: TopLetKind::Lazy,
            mutable: false, doc: None, blank_lines_before: 0, def_id: None }
    }
    fn module_fn(name: &str, module: &str, body: IrExpr) -> IrFunction {
        IrFunction {
            name: sym(name), params: vec![], ret_ty: Ty::String, body,
            is_effect: false, is_async: false, is_test: false,
            generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![],
            visibility: IrVisibility::Public, doc: None, blank_lines_before: 0, def_id: None,
            mutated_params: vec![], module_origin: Some(module.to_string()),
        }
    }
    fn empty_module(name: &str) -> IrModule {
        IrModule { name: sym(name), versioned_name: None, type_decls: vec![],
            functions: vec![], top_lets: vec![], var_table: VarTable::new(),
            exports: vec![], imports: vec![] }
    }

    /// Root top-let (declared FIRST) reads a module global DIRECTLY through an
    /// aliased use-site VarId; the module global must be ordered first.
    #[test]
    fn direct_cross_module_read_reorders_module_first() {
        let mut program = IrProgram::default();
        // root: let GREETING(0) = APP_NAME(2)  where var2 aliases cfg's decl var1
        program.top_lets.push(tl(0, var(2)));
        let mut cfg = empty_module("cfg");
        cfg.top_lets.push(tl(1, lit())); // cfg: let APP_NAME(1) = "x"
        program.modules.push(cfg);
        // alias: use-site var2 → declaration var1
        let mut alias: HashMap<VarId, VarId> = HashMap::new();
        alias.insert(VarId(2), VarId(1));

        let order = dependency_init_order(&program, &alias);
        let p0 = order.iter().position(|v| *v == VarId(1)).unwrap();
        let p1 = order.iter().position(|v| *v == VarId(0)).unwrap();
        assert!(p0 < p1, "cfg.APP_NAME (var1) must init before root GREETING (var0): {:?}", order);
    }

    /// Root top-let reads the module global ONLY through a function call; the
    /// interprocedural read-set must still pull the module global earlier.
    #[test]
    fn interprocedural_read_reorders_module_first() {
        let mut program = IrProgram::default();
        // root: let BANNER(0) = cfg::banner()   (no direct Var to the global)
        program.top_lets.push(tl(0, call_mod("cfg", "banner")));
        let mut cfg = empty_module("cfg");
        cfg.top_lets.push(tl(1, lit()));                    // cfg: let APP_NAME(1)
        cfg.functions.push(module_fn("banner", "cfg", var(1))); // banner reads var1
        program.modules.push(cfg);

        let order = dependency_init_order(&program, &HashMap::new());
        let p_global = order.iter().position(|v| *v == VarId(1)).unwrap();
        let p_banner = order.iter().position(|v| *v == VarId(0)).unwrap();
        assert!(p_global < p_banner,
            "cfg.APP_NAME (var1) read via cfg::banner() must init before BANNER (var0): {:?}", order);
    }

    /// No cross dependency → the legacy (root-then-module) order is preserved.
    #[test]
    fn independent_globals_keep_legacy_order() {
        let mut program = IrProgram::default();
        program.top_lets.push(tl(0, lit()));
        let mut cfg = empty_module("cfg");
        cfg.top_lets.push(tl(1, lit()));
        program.modules.push(cfg);
        let order = dependency_init_order(&program, &HashMap::new());
        // Root let (var0) has no dep; its safety net depends on the module
        // global (var1), so var1 still comes first. Both are present exactly once.
        assert_eq!(order.len(), 2);
        assert!(order.contains(&VarId(0)) && order.contains(&VarId(1)));
    }
}
