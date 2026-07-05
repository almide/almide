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
fn is_heap_aliasable(ty: &Ty) -> bool {
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
}

impl<'a> AliasAnalysis<'a> {
    fn new(var_table: &'a VarTable, params: &[VarId]) -> Self {
        let params = params.iter().copied()
            .filter(|v| (v.0 as usize) < var_table.len() && is_heap_aliasable(&var_table.get(*v).ty))
            .collect();
        AliasAnalysis { var_table, parent: HashMap::new(), mutated: Default::default(), params }
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
        let rx = self.find(x);
        let ry = self.find(y);
        if rx != ry { self.parent.insert(rx, ry); }
    }

    fn mark_mutated(&mut self, v: VarId) {
        if self.is_heap_var(v) { self.mutated.insert(v); }
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
        // In-place stdlib mutator: `list.push(a, x)`, `map.insert(a, k, v)`, …
        if let IrExprKind::RuntimeCall { symbol, args } = &expr.kind {
            if is_inplace_mutator(symbol.as_str()) || symbol.as_str() == "almide_rt_bytes_set" {
                if let Some(IrExprKind::Var { id }) = args.first().map(|a| &a.kind) {
                    self.mark_mutated(*id);
                }
            }
        }
        // `bytes.set(x, i, v)` is VALUE-returning in the oracle (native clones),
        // but the wasm emitter's `x = bytes.set(x, …)` Assign peephole stores in
        // place — count it as a mutation of `x` so an aliased/param-reachable
        // target lands in needs_cow and VETOES that fast path (the general emit
        // then clones, mirroring native).
        if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &expr.kind {
            if module.as_str() == "bytes" && func.as_str() == "set" {
                if let Some(IrExprKind::Var { id }) = args.first().map(|a| &a.kind) {
                    self.mark_mutated(*id);
                }
            }
        }
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        match &stmt.kind {
            IrStmtKind::Bind { var, value, .. } => self.alias_from_value(*var, value),
            IrStmtKind::Assign { var, value } => self.alias_from_value(*var, value),
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
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}
