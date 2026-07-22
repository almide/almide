//! AliasCowPass: detect heap locals that are copy-aliased AND mutated in place,
//! so the WASM emitter can guard those mutation sites with a copy-on-write.
//!
//! ## Why
//!
//! Almide has value semantics (the `RcCow<T>` doc in `lib.rs`: "COW value type …
//! inspired by Swift"): after `var b = a` (or `let b = a`, `b = r.field`, a branch
//! arm, a destructure element), `a` and `b` denote INDEPENDENT values — mutating
//! one is invisible through the other. The Rust target realizes this with an eager
//! `.clone()` at the bind. The WASM target stores heap collections as shared,
//! refcounted pointers and mutates them IN PLACE, so without a guard `a[0]=v`
//! corrupts every binding that aliases `a`.
//!
//! ## What it computes
//!
//! `needs_cow` = the set of heap-typed function-local `VarId`s that are
//!   (1) members of a may-alias class of size > 1 (some OTHER live binding shares
//!       the same heap value via a provenance copy), AND
//!   (2) the target of at least one in-place mutation (IndexAssign / FieldAssign /
//!       ListSwap / ListReverse / ListRotateLeft / ListCopySlice, or an in-place
//!       stdlib mutator call such as `list.push` / `map.insert`).
//!
//! The WASM emitter reads `needs_cow` and, at each mutation site of a marked var,
//! calls `__cow_check` (which clones the block iff its refcount > 1) and writes the
//! returned pointer back to the var's local before mutating.
//!
//! ## Conservatism
//!
//! The may-alias graph is flow-INSENSITIVE: an edge `x—y` is added for every
//! provenance copy anywhere in the function, ignoring whether the alias is still
//! live at the mutation. This can only mark MORE vars (a redundant `__cow_check`
//! whose rc==1 is a no-op), never fewer — the safe, conservative direction.
//!
//! ## Exclusions
//!
//! - `shared_mut_vars` (closure-captured-and-mutated) are DELIBERATELY shared
//!   (reference semantics inside a closure); they get their own cell storage and
//!   must NOT be COW'd.
//! - Module-level vars route mutations through their global cell already and have
//!   single global storage (no source-level aliasing) — excluded.
//!
//! WASM-only: the Rust target fixes the same shapes by counting the in-place
//! mutation target as a use in `CloneInsertionPass` (forcing `.clone()` at the bind).

use std::collections::HashMap;
use almide_ir::*;
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};
use super::pass_closure_conversion::is_inplace_mutator;

#[derive(Debug)]
pub struct AliasCowPass;

impl NanoPass for AliasCowPass {
    fn name(&self) -> &str { "AliasCow" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Wasm])
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Pure analysis — no IR rewrite — so it is trivially order-safe. Collect
        // the alias classes and the in-place-mutated targets per function, then
        // intersect: a var needs COW iff it is mutated in place AND shares a heap
        // value with another binding.
        let mut needs_cow = std::collections::BTreeSet::new();

        let shared_mut = program.codegen_annotations.shared_mut_vars.clone();
        let module_vars = collect_module_vars(&program);

        let no_params: Vec<VarId> = Vec::new();
        let bodies = program.functions.iter()
            .map(|f| (&f.body, f.params.iter().map(|p| p.var).collect::<Vec<_>>()))
            .chain(program.top_lets.iter().map(|tl| (&tl.value, no_params.clone())))
            .chain(program.modules.iter().flat_map(|m|
                m.functions.iter().map(|f| (&f.body, f.params.iter().map(|p| p.var).collect::<Vec<_>>()))
                    .chain(m.top_lets.iter().map(|tl| (&tl.value, no_params.clone())))));

        for (body, params) in bodies {
            let mut a = AliasAnalysis::new(&program.var_table, &params);
            a.visit_expr(body);
            for v in a.finish() {
                // Exclude deliberately-shared closure cells and module globals.
                if shared_mut.contains(&v) || module_vars.contains(&v) { continue; }
                needs_cow.insert(v);
            }
        }

        if std::env::var("ALMIDE_COW_PROBE").is_ok() {
            for v in &needs_cow {
                eprintln!("[cow] needs_cow: {:?} name={} ty={:?}",
                    v, program.var_table.get(*v).name, program.var_table.get(*v).ty);
            }
        }
        program.codegen_annotations.needs_cow = needs_cow;
        PassResult { program, changed: false }
    }
}

/// Top-level (module) var ids — these have global cell storage, not function-local
/// aliasing, so they are never COW candidates.
fn collect_module_vars(program: &IrProgram) -> std::collections::HashSet<VarId> {
    let mut out = std::collections::HashSet::new();
    for tl in &program.top_lets { out.insert(tl.var); }
    for m in &program.modules {
        for tl in &m.top_lets { out.insert(tl.var); }
    }
    out
}

/// Is `ty` a heap-allocated value that aliases by pointer on WASM (so an in-place
/// mutation through one binding is observable through another that shares it)?
pub(crate) fn is_heap_aliasable(ty: &Ty) -> bool {
    use almide_lang::types::TypeConstructorId;
    match ty {
        Ty::String | Ty::Bytes | Ty::Matrix
        | Ty::Record { .. } | Ty::OpenRecord { .. }
        | Ty::Named(_, _) | Ty::Variant { .. } => true,
        Ty::Applied(ctor, _) => matches!(
            ctor,
            TypeConstructorId::List | TypeConstructorId::Map | TypeConstructorId::Set
        ),
        _ => false,
    }
}

// ── May-alias analysis (union-find over heap-typed VarIds) ──────────────

/// Flow-insensitive may-alias + in-place-mutation collection for one function body.
struct AliasAnalysis<'a> {
    var_table: &'a VarTable,
    /// Union-find parent map over VarIds that participate in a provenance copy.
    parent: HashMap<VarId, VarId>,
    /// Vars that are the target of an in-place mutation (statement kinds or an
    /// in-place stdlib mutator's `args[0]`).
    mutated: std::collections::HashSet<VarId>,
    /// Heap-typed fn params. A param's value is OWNED BY THE CALLER — the callee
    /// cannot see its aliases (`let e = touch(iv)` shares iv's buffer with `b`
    /// inside `touch`), so a param — and anything in its alias class — is
    /// treated as aliased unconditionally.
    params: std::collections::HashSet<VarId>,
    /// Monotone visit counter — a lexical position for the MOVE refinement.
    pos: usize,
    /// Innermost-loop stack: ids (= entry pos) of the `While`/`ForIn` bodies
    /// currently being walked.
    loop_stack: Vec<usize>,
    /// loop id → its body's end position (recorded on exit).
    loop_end: HashMap<usize, usize>,
    /// var → (decl position, innermost loop id at the decl) from its `Bind`.
    decl_at: HashMap<VarId, (usize, Option<usize>)>,
    /// var → positions of every READ (`Var` expr) and in-place MUTATION.
    occurrences: HashMap<VarId, Vec<usize>>,
    /// DIRECT `target = src` whole-var edges, deferred so `finish` can elide
    /// the ones that are MOVES (src never used after the edge — the
    /// `cur = merged` buffer-swap shape, #696). Derived provenance (`y =
    /// r.field`, branch-arm tails) unions immediately: the owner keeps access.
    deferred_edges: Vec<(VarId, VarId, usize, Option<usize>)>,
}

impl<'a> AliasAnalysis<'a> {
    fn new(var_table: &'a VarTable, params: &[VarId]) -> Self {
        let params = params.iter().copied()
            .filter(|v| (v.0 as usize) < var_table.len() && is_heap_aliasable(&var_table.get(*v).ty))
            .collect();
        AliasAnalysis {
            var_table, parent: HashMap::new(), mutated: Default::default(), params,
            pos: 0, loop_stack: Vec::new(), loop_end: HashMap::new(),
            decl_at: HashMap::new(), occurrences: HashMap::new(),
            deferred_edges: Vec::new(),
        }
    }

    fn is_heap_var(&self, v: VarId) -> bool {
        (v.0 as usize) < self.var_table.len() && is_heap_aliasable(&self.var_table.get(v).ty)
    }

    fn find(&mut self, v: VarId) -> VarId {
        let p = *self.parent.entry(v).or_insert(v);
        if p == v { return v; }
        let root = self.find(p);
        self.parent.insert(v, root);
        root
    }

    /// Record a provenance copy `y = (something derived from) x` — both are now in
    /// the same may-alias class. Only heap-typed vars participate.
    fn alias(&mut self, x: VarId, y: VarId) {
        if !self.is_heap_var(x) || !self.is_heap_var(y) { return; }
        if std::env::var("ALMIDE_COW_PROBE").is_ok() {
            eprintln!("[cow-edge] {:?}({}) — {:?}({})",
                x, self.var_table.get(x).name, y, self.var_table.get(y).name);
        }
        let rx = self.find(x);
        let ry = self.find(y);
        if rx != ry { self.parent.insert(rx, ry); }
    }

    fn mark_mutated(&mut self, v: VarId) {
        if self.is_heap_var(v) {
            self.mutated.insert(v);
            let p = self.pos;
            self.occurrences.entry(v).or_default().push(p);
        }
    }

    /// `IrExprKind::RuntimeCall` in-place-mutator check of `visit_expr`,
    /// extracted verbatim (cog>30 decomposition, pattern 1 — independent
    /// checks in sequence, each only ever calls `mark_mutated`, no state
    /// shared between them). In-place stdlib mutator: `list.push(a, x)`,
    /// `map.insert(a, k, v)`, …
    fn mark_mutated_from_runtime_call(&mut self, expr: &IrExpr) {
        if let IrExprKind::RuntimeCall { symbol, args } = &expr.kind {
            if is_inplace_mutator(symbol.as_str()) || symbol.as_str() == "almide_rt_bytes_set" {
                if let Some(IrExprKind::Var { id }) = args.first().map(|a| &a.kind) {
                    self.mark_mutated(*id);
                }
            }
        }
    }

    /// `IrExprKind::Call { target: Module, .. }` in-place-mutator check of
    /// `visit_expr`, extracted verbatim (cog>30 decomposition).
    /// `bytes.set(x, i, v)` is VALUE-returning in the oracle (native clones),
    /// but the wasm emitter's `x = bytes.set(x, …)` Assign peephole stores in
    /// place — count it as a mutation of `x` so an aliased/param-reachable
    /// target lands in needs_cow and VETOES that fast path (the general emit
    /// then clones, mirroring native).
    ///
    /// The whole &mut bytes family (`set_at`/`push`/`fill`/`write_*`/…) can ALSO
    /// arrive in this MODULE-call spelling (the wasm dispatcher emits it
    /// directly), not just as a RuntimeCall — mark those through the same
    /// `is_inplace_mutator` truth the RuntimeCall arm uses, or an aliased
    /// `var b = a; bytes.set_at(b, …)` writes through `a` on wasm while native
    /// keeps value semantics (found by spec/lang/rccow_value_semantics_test).
    fn mark_mutated_from_module_call(&mut self, expr: &IrExpr) {
        if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &expr.kind {
            let mutates = (module.as_str() == "bytes" && func.as_str() == "set")
                || is_inplace_mutator(&format!(
                    "almide_rt_{}_{}",
                    module.as_str(),
                    func.as_str()
                ));
            if mutates {
                if let Some(IrExprKind::Var { id }) = args.first().map(|a| &a.kind) {
                    self.mark_mutated(*id);
                }
            }
        }
    }

    /// If `value` is a bare/derived reference to a single heap var, return its id.
    /// Recognizes `x`, `x.clone()` (Clone), `*x` (Deref), and `r.field`/tuple-index
    /// provenance — the binding `y = value` then aliases `y` to that var.
    fn provenance_var(value: &IrExpr) -> Option<VarId> {
        match &value.kind {
            IrExprKind::Var { id } => Some(*id),
            IrExprKind::Clone { expr } | IrExprKind::Deref { expr } => Self::provenance_var(expr),
            IrExprKind::Member { object, .. } => Self::provenance_var(object),
            IrExprKind::TupleIndex { object, .. } => Self::provenance_var(object),
            _ => None,
        }
    }

    /// Add alias edges from a bind/assign `target = value`, descending into branch
    /// arms (`if`/`match`) and blocks whose tail can be a provenance var. Each arm
    /// that tails into a heap var aliases `target` to it.
    fn alias_from_value(&mut self, target: VarId, value: &IrExpr) {
        match &value.kind {
            IrExprKind::If { then, else_, .. } => {
                self.alias_from_value(target, then);
                self.alias_from_value(target, else_);
            }
            IrExprKind::Match { arms, .. } => {
                for arm in arms { self.alias_from_value(target, &arm.body); }
            }
            IrExprKind::Block { expr: Some(tail), .. } => self.alias_from_value(target, tail),
            // A DIRECT whole-var copy is a MOVE candidate: defer it so `finish`
            // can elide the edge when src is never used after it (#696's
            // `cur = merged` swap). Everything derived stays immediate.
            IrExprKind::Var { id: src } => {
                if self.is_heap_var(target) && self.is_heap_var(*src) {
                    let (p, l) = (self.pos, self.loop_stack.last().copied());
                    self.deferred_edges.push((target, *src, p, l));
                }
            }
            _ => {
                if let Some(src) = Self::provenance_var(value) {
                    self.alias(target, src);
                }
            }
        }
    }

    /// Intersect: a var is in `needs_cow` iff it is mutated in place AND either
    /// its may-alias class has size > 1 (some other heap binding shares its
    /// value) or the class reaches a fn param (the CALLER may still hold the
    /// value — cross-function aliasing the per-function analysis cannot see).
    fn finish(mut self) -> Vec<VarId> {
        // Resolve the deferred DIRECT edges: an edge `target = src` at position
        // E is a MOVE (elided) iff src has no read/mutation after E within its
        // proven-dead extent — (a) src is DECLARED in the same innermost loop
        // body as the edge and has no occurrence in (E, loop_end]: the next
        // iteration re-declares src before any use can see the old value
        // (scoping forbids use-before-decl); or (b) the edge is outside any
        // loop and src has no occurrence after E anywhere. Everything else is
        // a LIVE alias and unions as before (#696).
        let edges = std::mem::take(&mut self.deferred_edges);
        for (target, src, e_pos, e_loop) in edges {
            let occ_after = |hi: Option<usize>| -> bool {
                self.occurrences.get(&src).map_or(false, |ps| ps.iter().any(|&p| {
                    p > e_pos && hi.map_or(true, |h| p <= h)
                }))
            };
            let is_move = match e_loop {
                Some(l) => {
                    let decl_in_same_loop =
                        self.decl_at.get(&src).map_or(false, |&(_, dl)| dl == Some(l));
                    let end = self.loop_end.get(&l).copied();
                    decl_in_same_loop && !occ_after(end)
                }
                None => !occ_after(None),
            };
            if is_move {
                if std::env::var("ALMIDE_COW_PROBE").is_ok() {
                    eprintln!("[cow-move] {:?}({}) = {:?}({}) — src dead after edge, elided",
                        target, self.var_table.get(target).name,
                        src, self.var_table.get(src).name);
                }
            } else {
                self.alias(target, src);
            }
        }
        // class size per root.
        let mutated: Vec<VarId> = self.mutated.iter().copied().collect();
        let members: Vec<VarId> = self.parent.keys().copied().collect();
        let mut class_size: HashMap<VarId, usize> = HashMap::new();
        for m in members { let r = self.find(m); *class_size.entry(r).or_insert(0) += 1; }
        let params: Vec<VarId> = self.params.iter().copied().collect();
        let param_roots: std::collections::HashSet<VarId> =
            params.into_iter().map(|p| self.find(p)).collect();
        let mut out = Vec::new();
        for v in mutated {
            let r = self.find(v);
            if class_size.get(&r).copied().unwrap_or(0) > 1 || param_roots.contains(&r) {
                out.push(v);
            }
        }
        out
    }
}

impl IrVisitor for AliasAnalysis<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        self.pos += 1;
        // Positions/loops for the MOVE refinement: every `Var` read is an
        // occurrence; `While`/`ForIn` bodies open a loop scope whose end pos
        // bounds the same-loop dead-extent check.
        if let IrExprKind::Var { id } = &expr.kind {
            if self.is_heap_var(*id) {
                let p = self.pos;
                self.occurrences.entry(*id).or_default().push(p);
            }
        } else if matches!(expr.kind, IrExprKind::While { .. } | IrExprKind::ForIn { .. }) {
            let loop_id = self.pos;
            self.loop_stack.push(loop_id);
            walk_expr(self, expr);
            self.loop_stack.pop();
            let end = self.pos;
            self.loop_end.insert(loop_id, end);
            return;
        }
        // In-place stdlib mutator: `list.push(a, x)`, `map.insert(a, k, v)`, …
        self.mark_mutated_from_runtime_call(expr);
        self.mark_mutated_from_module_call(expr);
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        self.pos += 1;
        match &stmt.kind {
            // Walk the value FIRST so its own reads land BEFORE the edge
            // position — otherwise `cur = merged` would count merged's RHS
            // read as \"after the edge\" and never elide the move.
            IrStmtKind::Bind { var, value, .. } => {
                self.visit_expr(value);
                let (p, l) = (self.pos, self.loop_stack.last().copied());
                self.decl_at.entry(*var).or_insert((p, l));
                self.alias_from_value(*var, value);
                return;
            }
            IrStmtKind::Assign { var, value } => {
                self.visit_expr(value);
                self.alias_from_value(*var, value);
                return;
            }
            // In-place mutation statement kinds — `target` is read-and-written.
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::FieldAssign { target, .. }
            | IrStmtKind::ListSwap { target, .. }
            | IrStmtKind::ListReverse { target, .. }
            | IrStmtKind::ListRotateLeft { target, .. } => self.mark_mutated(*target),
            // Note: MapInsert (the `a[k]=v` statement) lowers on WASM to
            // `a = map.set(a, k, v)` (immutable, allocs fresh + writes back) — it
            // is already value-safe, so it is NOT a mutation target. The in-place
            // `map.insert(a, ...)` STDLIB CALL (handled in visit_expr) is the one
            // that mutates in place.
            IrStmtKind::ListCopySlice { dst, .. } => self.mark_mutated(*dst),
            // Explicit-preserve: no alias edge, no mutation target — children
            // are still walked by walk_stmt below.
            IrStmtKind::BindDestructure { .. } | IrStmtKind::MapInsert { .. }
            | IrStmtKind::Guard { .. } | IrStmtKind::Comment { .. }
            | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. }
            | IrStmtKind::Expr { .. } => {}
        }
        walk_stmt(self, stmt);
    }
}
