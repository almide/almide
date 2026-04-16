/// Inference types, type variables, and constraints for the constraint-based checker.

use crate::types::Ty;
use crate::intern::sym;

/// A fresh type variable for constraint-based inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TyVarId(pub u32);

#[derive(Debug)]
pub struct Constraint {
    pub expected: Ty,
    pub actual: Ty,
    pub context: String,
    /// Source span captured when the constraint was added. Used for error
    /// reporting; without it, mismatches reported during `solve_constraints`
    /// attach to whichever expression the checker happened to visit last,
    /// which produces wildly misleading error locations.
    pub span: Option<crate::ast::Span>,
    /// Optional syntactic hint captured at constraint creation time to
    /// specialize `try:` snippets — e.g. the name of the trailing `let`
    /// binding in a fn body that caused a Unit-leak E001.
    pub fix_hint: Option<FixHint>,
}

/// Context-specific info captured at constraint emission time, surfaced
/// back at diagnostic emission time to turn generic snippets into
/// concrete copy-pasteable code.
#[derive(Debug, Clone)]
pub enum FixHint {
    /// Name of the last `let` binding in a block whose type is the actual
    /// (usually Unit). The fix is typically to add the binding's name as a
    /// trailing expression.
    LastLetName(String),
    /// One arm of an `if/else` is a bare assignment `x = ...` (returns Unit),
    /// producing an if-branch type mismatch. Carries the name being assigned
    /// and which arm (then/else) is the offender, so the `try:` snippet can
    /// show `let new_x = if cond then <v> else x` with the real variable.
    IfArmAssign { arm: IfArm, var_name: String },
    /// Both if-arms are statement-only (assignments or bare `let`). Report
    /// the names so the snippet can show a rebinding on the combined result.
    IfArmsAssign { then_var: Option<String>, else_var: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfArm { Then, Else }

/// Check if a Ty is an inference variable (?N).
pub fn is_inference_var(ty: &Ty) -> Option<TyVarId> {
    if let Ty::TypeVar(name) = ty {
        if name.starts_with('?') {
            if let Ok(id) = name[1..].parse::<u32>() {
                return Some(TyVarId(id));
            }
        }
    }
    None
}

// ── Union-Find for type inference ────────────────────────────────────

/// Disjoint-set (Union-Find) structure for type variable equivalence classes.
/// Each type variable is a node. `union` merges equivalence classes;
/// `find` returns the canonical representative. Concrete types are bound
/// to roots — information never lost on merge.
#[derive(Debug, Clone, PartialEq)]
pub struct UnionFind {
    parent: Vec<u32>,
    rank: Vec<u8>,
    bound: Vec<Option<Ty>>,
}

impl UnionFind {
    pub fn new() -> Self {
        UnionFind { parent: Vec::new(), rank: Vec::new(), bound: Vec::new() }
    }

    /// Allocate a fresh, unbound type variable.
    pub fn fresh(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(id);
        self.rank.push(0);
        self.bound.push(None);
        id
    }

    /// Find the root representative of `id`'s equivalence class.
    /// Uses path halving (every other node points to grandparent) for amortized
    /// near-constant time without requiring &mut self.
    pub fn find(&self, mut id: u32) -> u32 {
        while self.parent[id as usize] != id {
            id = self.parent[id as usize];
        }
        id
    }

    /// Merge the equivalence classes of `a` and `b`. Union-by-rank keeps
    /// tree depth logarithmic. If either root carries a concrete type binding,
    /// it is preserved on the winner.
    pub fn union(&mut self, a: u32, b: u32) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb { return; }
        let (winner, loser) = if self.rank[ra as usize] >= self.rank[rb as usize] { (ra, rb) } else { (rb, ra) };
        self.parent[loser as usize] = winner;
        if self.rank[winner as usize] == self.rank[loser as usize] {
            self.rank[winner as usize] += 1;
        }
        // Merge bound types: prefer the one that has a concrete binding
        let loser_bound = self.bound[loser as usize].take();
        if self.bound[winner as usize].is_none() {
            self.bound[winner as usize] = loser_bound;
        }
    }

    /// Bind a concrete type to `id`'s root. If the root already has a binding,
    /// returns the existing binding for the caller to unify structurally.
    pub fn bind(&mut self, id: u32, ty: Ty) -> Option<Ty> {
        let root = self.find(id);
        let existing = self.bound[root as usize].take();
        self.bound[root as usize] = Some(ty);
        existing
    }

    /// Get the concrete type bound to `id`'s root, if any.
    pub fn resolve(&self, id: u32) -> Option<&Ty> {
        let root = self.find(id);
        self.bound[root as usize].as_ref()
    }

    /// Check whether `var` occurs anywhere inside `ty` (infinite type prevention).
    pub fn occurs(&self, var: u32, ty: &Ty) -> bool {
        match ty {
            Ty::TypeVar(name) if name.starts_with('?') => {
                if let Ok(id) = name[1..].parse::<u32>() {
                    self.find(var) == self.find(id)
                        || self.resolve(id).map_or(false, |s| self.occurs(var, s))
                } else { false }
            }
            Ty::Applied(_, args) => args.iter().any(|a| self.occurs(var, a)),
            Ty::Tuple(elems) => elems.iter().any(|e| self.occurs(var, e)),
            Ty::Fn { params, ret } => params.iter().any(|p| self.occurs(var, p)) || self.occurs(var, ret),
            _ => false,
        }
    }
}

// ── Type resolution ──────────────────────────────────────────────────

/// Resolve all inference variables in `ty` through the Union-Find,
/// replacing each `?N` with its bound concrete type.
pub fn resolve_ty(ty: &Ty, uf: &UnionFind) -> Ty {
    match ty {
        Ty::TypeVar(name) if name.starts_with('?') => {
            if let Ok(id) = name[1..].parse::<u32>() {
                match uf.resolve(id) {
                    Some(bound) => resolve_ty(bound, uf),
                    None => {
                        // Point to canonical root (may differ from original id)
                        let root = uf.find(id);
                        if root != id { Ty::TypeVar(sym(&format!("?{}", root))) } else { ty.clone() }
                    }
                }
            } else {
                ty.clone()
            }
        }
        Ty::Applied(id, args) => Ty::Applied(id.clone(), args.iter().map(|a| resolve_ty(a, uf)).collect()),
        Ty::Tuple(elems) => Ty::Tuple(elems.iter().map(|e| resolve_ty(e, uf)).collect()),
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|p| resolve_ty(p, uf)).collect(),
            ret: Box::new(resolve_ty(ret, uf)),
        },
        Ty::Named(name, args) if !args.is_empty() => {
            Ty::Named(name.clone(), args.iter().map(|a| resolve_ty(a, uf)).collect())
        }
        Ty::Record { fields } => Ty::Record {
            fields: fields.iter().map(|(n, t)| (n.clone(), resolve_ty(t, uf))).collect(),
        },
        Ty::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|(n, t)| (n.clone(), resolve_ty(t, uf))).collect(),
        },
        _ => ty.clone(),
    }
}
