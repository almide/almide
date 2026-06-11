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
