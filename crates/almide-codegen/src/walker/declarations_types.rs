// ── part of declarations.rs, include!-spliced (split for the 800-line budget) ──

/// `IrExprKind::ForIn` / `IrExprKind::While` case of `collect_anon_from_expr`,
/// extracted verbatim — both arms shared the identical "recurse the head
/// expr, then walk body stmts" shape (`iterable`/`cond` as the head), so
/// they now share one helper instead of two copies.
fn collect_anon_from_loop_body(head: &IrExpr, body: &[IrStmt], named: &RecordShapeIndex, seen: &mut HashSet<Vec<String>>) {
    collect_anon_from_expr(head, named, seen);
    for s in body { collect_anon_from_stmt(s, named, seen); }
}

fn collect_anon_from_stmt(stmt: &IrStmt, named: &RecordShapeIndex, seen: &mut HashSet<Vec<String>>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, ty, .. } => {
            collect_anon_from_ty(ty, named, seen);
            collect_anon_from_expr(value, named, seen);
        }
        // A destructured value is often a record LITERAL that no other
        // statement mentions (`let { a, c } = { a: 1, c: 3 }`): unless its
        // shape registers here, neither the literal nor the pattern has an
        // `AlmdRec_*` struct to name and both fall back to the joined field
        // names (`a_c { .. }`), which rustc rejects (#2657).
        IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. }
        | IrStmtKind::BindDestructure { value, .. } => {
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            collect_anon_from_expr(key, named, seen);
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            collect_anon_from_expr(index, named, seen);
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::Guard { cond, else_ } => {
            collect_anon_from_expr(cond, named, seen);
            collect_anon_from_expr(else_, named, seen);
        }
        IrStmtKind::Expr { expr } => collect_anon_from_expr(expr, named, seen),
        _ => {}
    }
}

/// True if `ty` mentions a function type anywhere — directly, nested in a
/// container/tuple/record, or through a fn-blocked NAMED type from the
/// precomputed transitive set. Such a type lowers to `Rc<dyn Fn>` (or a
/// container thereof), which is neither `Debug` nor `PartialEq`.
///
/// This is `ty_has_fn` widened by a precomputed transitive set: a `Named` reference to
/// a fn-blocked user type counts as holding a function value (#1674 — the
/// analysis was one level deep, so `type Schema = | Str(Check)` over a
/// closure-carrying `Check` still derived Debug/PartialEq and the generated
/// Rust did not compile). Mirror of `ty_blocks_eq_with`.
pub(super) fn ty_has_fn_with(ty: &Ty, fn_blocked: &HashSet<String>) -> bool {
    ty_holds_with(ty, fn_blocked, &|t| matches!(t, Ty::Fn { .. }))
}

/// True if `ty` mentions a `leaf` type anywhere — directly, nested in a
/// container/tuple/record, or through a NAMED type of the precomputed
/// transitive `blocked` set ([`compute_blocked_types`]).
fn ty_holds_with(ty: &Ty, blocked: &HashSet<String>, leaf: &dyn Fn(&Ty) -> bool) -> bool {
    if leaf(ty) {
        return true;
    }
    match ty {
        Ty::Named(name, args) => {
            blocked.contains(name.as_str())
                || args.iter().any(|t| ty_holds_with(t, blocked, leaf))
        }
        Ty::Tuple(elems) | Ty::Applied(_, elems) => elems.iter().any(|t| ty_holds_with(t, blocked, leaf)),
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            fields.iter().any(|(_, t)| ty_holds_with(t, blocked, leaf))
        }
        _ => false,
    }
}

/// A leaf whose native value is `Rc`-backed, so neither `Send` nor `Sync`:
/// a closure (`Rc<dyn Fn>`) and the `AlmideRcCow` value types `Bytes` /
/// `Matrix` (#617, #3287).
fn ty_is_rc_leaf(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    matches!(ty, Ty::Fn { .. } | Ty::Bytes | Ty::Matrix | Ty::Applied(TC::Matrix, _))
}

/// True if `ty` mentions a raw pointer anywhere — `*mut u8` has no `AlmideRepr`
/// and no `Display`, so a newtype over one gets neither impl (#2496).
pub(super) fn ty_has_raw_ptr(ty: &Ty) -> bool {
    ty.any_child_recursive(&|t| matches!(t, Ty::RawPtr))
}

/// Precompute the user-defined type names that transitively contain a function
/// value — fixed point over the whole decl set, exactly like
/// `compute_eq_blocked_types`.
pub(super) fn compute_fn_blocked_types(type_decls: &[IrTypeDecl]) -> HashSet<String> {
    compute_blocked_types(type_decls, &|t| matches!(t, Ty::Fn { .. }))
}

/// The user-defined type names that transitively contain an `Rc`-backed leaf
/// ([`ty_is_rc_leaf`]): a value of one is not `Sync`, so it cannot live in a
/// `static LazyLock` (#3287).
pub(super) fn compute_rc_blocked_types(type_decls: &[IrTypeDecl]) -> HashSet<String> {
    compute_blocked_types(type_decls, &ty_is_rc_leaf)
}

/// Fixed point over the whole decl set: the type names whose fields hold a
/// `leaf` type, directly or through another such named type.
fn compute_blocked_types(type_decls: &[IrTypeDecl], leaf: &dyn Fn(&Ty) -> bool) -> HashSet<String> {
    let mut blocked: HashSet<String> = HashSet::new();
    loop {
        let mut changed = false;
        for td in type_decls {
            if blocked.contains(td.name.as_str()) { continue }
            let holds = |t: &Ty| ty_holds_with(t, &blocked, leaf);
            let blocks = match &td.kind {
                IrTypeDeclKind::Record { fields } => fields.iter().any(|f| holds(&f.ty)),
                IrTypeDeclKind::Variant { cases, .. } => cases.iter().any(|c| match &c.kind {
                    IrVariantKind::Unit => false,
                    IrVariantKind::Tuple { fields } => fields.iter().any(&holds),
                    IrVariantKind::Record { fields } => fields.iter().any(|f| holds(&f.ty)),
                }),
                _ => false,
            };
            if blocks {
                blocked.insert(td.name.to_string());
                changed = true;
            }
        }
        if !changed { break }
    }
    blocked
}

/// Does this type transitively hold a value that doesn't implement `PartialEq`
/// in the Rust runtime? Returning true means the enclosing struct cannot
/// derive PartialEq.
///
/// `eq_blocked_types` lists user-defined record names that have already
/// been determined to lack PartialEq (computed in a first pass); referring
/// to any of those transitively blocks eq as well.
pub(super) fn ty_blocks_eq_with(ty: &Ty, eq_blocked_types: &HashSet<String>) -> bool {
    match ty {
        // Burn Tensor doesn't implement PartialEq
        Ty::Matrix => true,
        // Function pointers can't be compared structurally
        Ty::Fn { .. } => true,
        // User-defined Named type: check against the precomputed blocked set
        Ty::Named(name, args) => {
            eq_blocked_types.contains(name.as_str())
                || args.iter().any(|t| ty_blocks_eq_with(t, eq_blocked_types))
        }
        // Structural recursion for containers
        Ty::Tuple(elems) => elems.iter().any(|t| ty_blocks_eq_with(t, eq_blocked_types)),
        Ty::Applied(_, args) => args.iter().any(|t| ty_blocks_eq_with(t, eq_blocked_types)),
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            fields.iter().any(|(_, t)| ty_blocks_eq_with(t, eq_blocked_types))
        }
        _ => false,
    }
}

/// Precompute the set of user-defined type names whose generated Rust struct
/// cannot derive `PartialEq` (because a field type blocks it, transitively).
/// Reaches a fixed point by repeating until no new blocked type is added.
pub(super) fn compute_eq_blocked_types(type_decls: &[IrTypeDecl]) -> HashSet<String> {
    let mut blocked: HashSet<String> = HashSet::new();
    loop {
        let mut changed = false;
        for td in type_decls {
            if blocked.contains(td.name.as_str()) { continue }
            let blocks = match &td.kind {
                IrTypeDeclKind::Record { fields } => {
                    fields.iter().any(|f| ty_blocks_eq_with(&f.ty, &blocked))
                }
                IrTypeDeclKind::Variant { cases, .. } => {
                    cases.iter().any(|c| match &c.kind {
                        IrVariantKind::Unit => false,
                        IrVariantKind::Tuple { fields } => {
                            fields.iter().any(|t| ty_blocks_eq_with(t, &blocked))
                        }
                        IrVariantKind::Record { fields } => {
                            fields.iter().any(|f| ty_blocks_eq_with(&f.ty, &blocked))
                        }
                    })
                }
                _ => false,
            };
            if blocks {
                blocked.insert(td.name.to_string());
                changed = true;
            }
        }
        if !changed { break }
    }
    blocked
}

/// True when `name` (a generic param) appears anywhere in `ty`, as either a
/// `TypeVar` or a `Named` reference (lowering may leave either spelling).
fn ty_mentions_param(ty: &Ty, name: &str) -> bool {
    match ty {
        Ty::TypeVar(n) => n.as_str() == name,
        Ty::Named(n, args) => n.as_str() == name || args.iter().any(|a| ty_mentions_param(a, name)),
        Ty::Applied(_, args) => args.iter().any(|a| ty_mentions_param(a, name)),
        Ty::Tuple(elems) => elems.iter().any(|e| ty_mentions_param(e, name)),
        Ty::Fn { is_effect: _, params, ret } => params.iter().any(|p| ty_mentions_param(p, name)) || ty_mentions_param(ret, name),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| ty_mentions_param(t, name)),
        _ => false,
    }
}

/// Record types declaring generic params NONE of which are referenced by any
/// field — Rust's `error[E0392]` rejects such an unused param, so we strip the
/// generics from the emitted struct and from every reference to it (#621).
/// Only fully-phantom records qualify: a partly-used param list still needs its
/// (used) params, so it is left alone here.
pub(super) fn compute_phantom_param_structs(type_decls: &[IrTypeDecl]) -> HashSet<String> {
    let mut out = HashSet::new();
    for td in type_decls {
        let Some(generics) = &td.generics else { continue };
        if generics.is_empty() { continue }
        let IrTypeDeclKind::Record { fields } = &td.kind else { continue };
        let any_used = generics.iter().any(|g| {
            let pname = g.name.as_str();
            fields.iter().any(|f| ty_mentions_param(&f.ty, pname))
        });
        if !any_used {
            out.insert(td.name.to_string());
        }
    }
    out
}

thread_local! {
    /// Sorted field-name keys of anonymous records that have a closure (`Fn`) field.
    /// Their generated struct must derive `Clone` only (a closure is not `Debug` /
    /// `PartialEq`) — the same `has_fn_fields` relaxation a `type`-declared record
    /// gets. Populated during `collect_anon_records`; read back into
    /// `anon_records_with_fn`. Membership-only (never iterated for output), so it
    /// does not affect host-deterministic emit. (Closure codegen cross-target gaps.)
    static ANON_FN_KEYS: std::cell::RefCell<HashSet<Vec<String>>> =
        std::cell::RefCell::new(HashSet::new());
    /// Fn-blocked named types for the CURRENT `collect_anon_records` run
    /// (#1674) — consulted by `collect_anon_from_ty` so an anon record whose
    /// field is a fn-blocked NAMED type also lands in `ANON_FN_KEYS`.
    static ANON_FN_BLOCKED: std::cell::RefCell<HashSet<String>> =
        std::cell::RefCell::new(HashSet::new());
}

fn collect_anon_from_ty(ty: &Ty, named: &RecordShapeIndex, seen: &mut HashSet<Vec<String>>) {
    // Record/OpenRecord: register anonymous record fields
    if let Ty::Record { fields } | Ty::OpenRecord { fields } = ty {
        let mut names: Vec<String> = fields.iter().map(|(n, _)| n.to_string()).collect();
        names.sort();
        // A declared record's names with other field types is still an
        // anonymous record, so it needs its own struct (#3189).
        if named.lookup(fields).is_none() {
            // A field whose type CONTAINS a closure anywhere (`Fn`, `List[Fn]`,
            // `Map[_, Fn]`, `(Fn, _)`, …) lowers to a type that is neither `Debug`
            // nor `PartialEq`, so the generated struct must derive `Clone` only.
            // Matching `Ty::Fn` alone missed boxed-closure containers.
            if fields.iter().any(|(_, t)| ANON_FN_BLOCKED.with(|b| ty_has_fn_with(t, &b.borrow()))) {
                ANON_FN_KEYS.with(|s| { s.borrow_mut().insert(names.clone()); });
            }
            seen.insert(names);
        }
    }
    // Recurse into all children uniformly
    for child in ty.children() {
        collect_anon_from_ty(child, named, seen);
    }
}
