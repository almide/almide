/// Inference types, type variables, and constraints for the constraint-based checker.

use crate::types::Ty;
use crate::intern::sym;

/// A fresh type variable for constraint-based inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TyVarId(pub u32);

#[derive(Debug)]
#[derive(Clone)]
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
    /// The value of an ANNOTATED `let x: T = <call>` (#2653). When the call
    /// yields `Result[T, _]` against a plain `T`, the mismatch is the missing
    /// `!` — the same shape E005 (`f(g())`) and E041 (`let x = g()`) name.
    /// `span` is the call's; `can_propagate` is whether a `!` at its end is a
    /// one-place edit here (an effect fn body, outside any lambda).
    LetCallValue { span: crate::ast::Span, can_propagate: bool },
    /// An `err(..)` match arm whose `String` payload may be a typed error a
    /// callback's `!` erased into its `String` channel (#2722): the erased
    /// type and that `!`'s span, reported when the arm's slot is that type.
    ErrArmErased { erased: Ty, at: Option<crate::ast::Span> },
    /// A match arm / if branch reported against the PEER that fixed the
    /// join's type (#2927). `anchor` is where that peer is; `declared` is
    /// the type the construct had to produce when the peer was chosen
    /// because it agrees with it (the fn's declared return), `None` when
    /// the peer was chosen only for being first. `bang` is the span and text of an
    /// un-`!`ed Result call wrapped in `ok(..)` / `some(..)` inside the
    /// blamed branch — the ADR-0008 missing `!`.
    ArmBlame {
        anchor: Option<crate::ast::Span>,
        declared: Option<Ty>,
        bang: Option<(crate::ast::Span, String)>,
        /// The blamed peer's own type, shown against `declared`: the join
        /// compares effect-fn peers with their `Result` stripped, which is
        /// not the type the declared return is compared with.
        real: Ty,
    },
}

/// The type an expression in TAIL position must produce (#2927): set for a
/// fn body from its declared return, and carried into a block's tail, a
/// match's arms and an if's branches. `effect_body` accepts the effect-fn
/// leniency `constrain_effect_body` applies (a bare `T` for `Result[T, E]`,
/// a `Result[T, _]` for a plain `T`).
#[derive(Debug, Clone)]
pub(crate) struct TailExpect {
    pub ty: Ty,
    pub effect_body: bool,
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
    /// Per class: an anonymous record literal's DEFERRED type (#3290). Its
    /// binding starts as the literal's structural record and becomes the
    /// nominal record it is unified with; a structural unification never
    /// replaces a nominal binding.
    literal: Vec<bool>,
}

impl UnionFind {
    pub fn new() -> Self {
        UnionFind { parent: Vec::new(), rank: Vec::new(), bound: Vec::new(), literal: Vec::new() }
    }

    /// Allocate a fresh, unbound type variable.
    pub fn fresh(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(id);
        self.rank.push(0);
        self.bound.push(None);
        self.literal.push(false);
        id
    }

    /// A fresh class for an anonymous record literal, bound to its structural
    /// record `rec` until a nominal unification names it (#3290).
    pub fn fresh_record_literal(&mut self, rec: Ty) -> u32 {
        let id = self.fresh();
        self.bound[id as usize] = Some(rec);
        self.literal[id as usize] = true;
        id
    }

    /// Is `id`'s class the deferred type of an anonymous record literal?
    pub fn is_record_literal(&self, id: u32) -> bool {
        let root = self.find(id);
        self.literal.get(root as usize).copied().unwrap_or(false)
    }

    /// Find the root representative of `id`'s equivalence class.
    /// Uses path halving (every other node points to grandparent) for amortized
    /// near-constant time without requiring &mut self.
    pub fn find(&self, mut id: u32) -> u32 {
        // An id past the end of THIS UnionFind was never registered here, so it is
        // its own representative (a free singleton). This happens when a TyVarId
        // leaks in from an outer scope after a fresh UnionFind is swapped in for
        // module/nested inference — returning `id` instead of indexing keeps the
        // checker total instead of panicking (#653).
        while (id as usize) < self.parent.len() && self.parent[id as usize] != id {
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
        // A root past the end is a foreign/leaked id (see `find`) — this
        // UnionFind owns no slot for it, so there is nothing to merge (#653).
        if (ra as usize) >= self.parent.len() || (rb as usize) >= self.parent.len() { return; }
        let (winner, loser) = if self.rank[ra as usize] >= self.rank[rb as usize] { (ra, rb) } else { (rb, ra) };
        self.parent[loser as usize] = winner;
        if self.rank[winner as usize] == self.rank[loser as usize] {
            self.rank[winner as usize] += 1;
        }
        // Merge bound types: prefer the one that has a concrete binding —
        // and, in a record literal's class, a nominal binding over a
        // structural one (#3290).
        let loser_bound = self.bound[loser as usize].take();
        let literal = self.literal[winner as usize] || self.literal[loser as usize];
        self.literal[winner as usize] = literal;
        let take_loser = match (&self.bound[winner as usize], &loser_bound) {
            (None, _) => true,
            (Some(w), Some(l)) => literal && is_structural_record(w) && matches!(l, Ty::Named(..)),
            _ => false,
        };
        if take_loser {
            self.bound[winner as usize] = loser_bound;
        }
    }

    /// Bind a concrete type to `id`'s root. If the root already has a binding,
    /// returns the existing binding for the caller to unify structurally.
    pub fn bind(&mut self, id: u32, ty: Ty) -> Option<Ty> {
        let root = self.find(id);
        // A foreign/leaked root (past this UnionFind's slots) cannot be bound here
        // (#653); drop the binding rather than panic — it belongs to another scope.
        if (root as usize) >= self.bound.len() { return None; }
        let existing = self.bound[root as usize].take();
        self.bound[root as usize] = Some(ty);
        existing
    }

    /// Get the concrete type bound to `id`'s root, if any.
    pub fn resolve(&self, id: u32) -> Option<&Ty> {
        let root = self.find(id);
        self.bound.get(root as usize).and_then(|b| b.as_ref())
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
            Ty::Fn { params, ret, is_effect: _ } => params.iter().any(|p| self.occurs(var, p)) || self.occurs(var, ret),
            _ => false,
        }
    }
}

/// A structural record type (closed or open).
pub fn is_structural_record(ty: &Ty) -> bool {
    matches!(ty, Ty::Record { .. } | Ty::OpenRecord { .. })
}

/// [`resolve_ty`] for UNIFICATION (#3290): an anonymous record literal's
/// deferred variable that is still bound to its structural record stays a
/// variable, so a nominal type unified with a type that CONTAINS it
/// (`List[?e]` against `List[b.Extent]`) still reaches and names it. Every
/// other variable resolves exactly as [`resolve_ty`] does.
pub fn resolve_ty_keeping_literals(ty: &Ty, uf: &UnionFind) -> Ty {
    if let Ty::TypeVar(name) = ty
        && let Some(id) = name.strip_prefix('?').and_then(|n| n.parse::<u32>().ok())
        && uf.is_record_literal(id)
        && uf.resolve(id).is_some_and(is_structural_record)
    {
        return Ty::TypeVar(sym(&format!("?{}", uf.find(id))));
    }
    match ty {
        Ty::TypeVar(name) if name.starts_with('?') => match name[1..].parse::<u32>().ok().and_then(|id| uf.resolve(id)) {
            Some(bound) => resolve_ty_keeping_literals(bound, uf),
            None => resolve_ty(ty, uf),
        },
        Ty::Applied(id, args) => Ty::Applied(id.clone(), args.iter().map(|a| resolve_ty_keeping_literals(a, uf)).collect()),
        Ty::Tuple(elems) => Ty::Tuple(elems.iter().map(|e| resolve_ty_keeping_literals(e, uf)).collect()),
        Ty::Fn { params, ret, is_effect } => Ty::Fn {
            params: params.iter().map(|p| resolve_ty_keeping_literals(p, uf)).collect(),
            ret: Box::new(resolve_ty_keeping_literals(ret, uf)),
            is_effect: *is_effect,
        },
        Ty::Named(name, args) if !args.is_empty() => {
            Ty::Named(name.clone(), args.iter().map(|a| resolve_ty_keeping_literals(a, uf)).collect())
        }
        Ty::Record { fields } => Ty::Record {
            fields: fields.iter().map(|(n, t)| (n.clone(), resolve_ty_keeping_literals(t, uf))).collect(),
        },
        Ty::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|(n, t)| (n.clone(), resolve_ty_keeping_literals(t, uf))).collect(),
        },
        _ => ty.clone(),
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
        Ty::Fn { params, ret, is_effect } => Ty::Fn {
            params: params.iter().map(|p| resolve_ty(p, uf)).collect(),
            ret: Box::new(resolve_ty(ret, uf)),
            is_effect: *is_effect,
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
