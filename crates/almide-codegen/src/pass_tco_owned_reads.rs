// ─────────── the owned-read census and its clone-wrapping decisions ───────────
//
// Split out of pass_tco_loop_rewrite.rs at the census boundary (max-lines).
// A pure text move: this file is `include!`d back at the end of that one, so
// it shares its imports and its `impl` scope exactly as before.

/// How a tracked param is read inside a region: `bare` = a consuming
/// `Var` (renders as a move if left alone); `other` = a read that stays a
/// reference (immediate Borrow child, an access-object, an in-place
/// mutation target, an RC op); `lambda` = any read inside a lambda body (a
/// closure capture — CaptureClone's domain, disqualifying).
#[derive(Default)]
struct OwnedReadCensus {
    bare: u32,
    other: u32,
    lambda: u32,
}

struct OwnedReadCensusVisitor<'a> {
    tracked: &'a HashSet<VarId>,
    lambda_depth: u32,
    out: &'a mut HashMap<VarId, OwnedReadCensus>,
}

impl OwnedReadCensusVisitor<'_> {
    fn note_bare(&mut self, id: VarId) {
        let c = self.out.entry(id).or_default();
        if self.lambda_depth > 0 { c.lambda += 1 } else { c.bare += 1 }
    }
    fn note_shielded(&mut self, id: VarId) {
        let c = self.out.entry(id).or_default();
        if self.lambda_depth > 0 { c.lambda += 1 } else { c.other += 1 }
    }
    /// The immediate tracked `Var` of a reference-taking position
    /// (Borrow child / access object), or None.
    fn shielded_var(&self, e: &IrExpr) -> Option<VarId> {
        match &e.kind {
            IrExprKind::Var { id } if self.tracked.contains(id) => Some(*id),
            _ => None,
        }
    }
}

impl almide_ir::visit::IrVisitor for OwnedReadCensusVisitor<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        use almide_ir::visit::walk_expr;
        match &e.kind {
            IrExprKind::Var { id } => {
                if self.tracked.contains(id) { self.note_bare(*id); }
            }
            IrExprKind::Borrow { expr, .. } => {
                if let Some(id) = self.shielded_var(expr) { self.note_shielded(id); }
                else { self.visit_expr(expr); }
            }
            IrExprKind::Member { object, .. } => {
                if let Some(id) = self.shielded_var(object) { self.note_shielded(id); }
                else { self.visit_expr(object); }
            }
            IrExprKind::IndexAccess { object, index } => {
                if let Some(id) = self.shielded_var(object) { self.note_shielded(id); }
                else { self.visit_expr(object); }
                self.visit_expr(index);
            }
            IrExprKind::MapAccess { object, key } => {
                if let Some(id) = self.shielded_var(object) { self.note_shielded(id); }
                else { self.visit_expr(object); }
                self.visit_expr(key);
            }
            IrExprKind::Lambda { body, .. } => {
                self.lambda_depth += 1;
                self.visit_expr(body);
                self.lambda_depth -= 1;
            }
            _ => walk_expr(self, e),
        }
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        use almide_ir::visit::walk_stmt;
        // An in-place mutation / RC op pins the var (reads-and-writes it
        // through its binding) — it must never be moved away from.
        match &s.kind {
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. }
            | IrStmtKind::ListSwap { target, .. }
            | IrStmtKind::ListReverse { target, .. }
            | IrStmtKind::ListRotateLeft { target, .. } => {
                if self.tracked.contains(target) { self.note_shielded(*target); }
            }
            IrStmtKind::ListCopySlice { dst, src, .. } => {
                if self.tracked.contains(dst) { self.note_shielded(*dst); }
                if self.tracked.contains(src) { self.note_shielded(*src); }
            }
            IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => {
                if self.tracked.contains(var) { self.note_shielded(*var); }
            }
            _ => {}
        }
        walk_stmt(self, s);
    }
}

fn census_owned_reads(
    e: &IrExpr,
    tracked: &HashSet<VarId>,
    in_lambda: bool,
    out: &mut HashMap<VarId, OwnedReadCensus>,
) {
    if tracked.is_empty() { return; }
    use almide_ir::visit::IrVisitor;
    let mut v = OwnedReadCensusVisitor {
        tracked,
        lambda_depth: if in_lambda { 1 } else { 0 },
        out,
    };
    v.visit_expr(e);
}

/// The params whose clone/move decisions this pass takes over. Everything
/// with a competing ownership protocol opts out: a kept-borrow Bytes param
/// (never owned), an `always_clone_vars` id, a closure/type-var param
/// (CloneInsertion's own `always` class), a clone-free scalar (nothing to
/// decide), and any param read inside a lambda (its capture handling
/// belongs to CaptureClone). A dec-managed param does NOT opt out: its
/// `RcInc`/`RcDec` protocol renders to NOTHING on the Rust target
/// (walker/statements.rs emits the empty string — Perceus RC is the wasm
/// renderer's concern, and wasm never consumes this pass's output), so a
/// move past a no-op Dec is exactly as safe as any other move here and
/// rustc re-proves it. Excluding them would leave the STRING accumulator
/// (`acc + "x"` — ConcatStr is a fresh alloc, so such params are always
/// dec-managed) on the O(n²) clone path this fix exists to close.
fn tco_owned_candidates(
    func: &IrFunction,
    kept_borrow: &HashSet<usize>,
    always_clone_vars: &HashSet<VarId>,
) -> HashSet<VarId> {
    let mut set: HashSet<VarId> = func.params.iter().enumerate().filter_map(|(i, p)| {
        if kept_borrow.contains(&i) { return None; }
        if always_clone_vars.contains(&p.var) { return None; }
        if matches!(p.ty, Ty::Fn { .. } | Ty::TypeVar(_)) { return None; }
        if almide_ir::top_let_storage::clone_free(&p.ty) { return None; }
        Some(p.var)
    }).collect();
    if set.is_empty() { return set; }
    let mut census: HashMap<VarId, OwnedReadCensus> = HashMap::new();
    census_owned_reads(&func.body, &set, false, &mut census);
    set.retain(|p| census.get(p).is_none_or(|c| c.lambda == 0));
    set
}

/// Wrap every bare consuming read of a var in `wrap` (minus `except`) in an
/// explicit `Clone`. Reference-taking positions keep their bare Var — a
/// Borrow child (wrapping would borrow a temporary and lose writes through
/// an `&mut`) and access objects (CloneInsertion's own IndexAccess /
/// MapAccess / Member arms strip container clones and clone the ELEMENT).
/// Lambda bodies are left untouched (owned params are proven lambda-free).
/// Recurse into `child` UNLESS it is already a bare `Var`, which such a
/// position must keep: wrapping a `Borrow`'s child would borrow a temporary and
/// lose writes through an `&mut`, an access object's clone would copy the
/// container instead of the element, and an existing `Clone` must never
/// double-wrap.
fn keep_bare_var(
    child: Box<IrExpr>,
    wrap: &HashSet<VarId>,
    except: &HashSet<VarId>,
) -> Box<IrExpr> {
    if matches!(&child.kind, IrExprKind::Var { .. }) {
        child
    } else {
        Box::new(wrap_owned_reads_except(*child, wrap, except))
    }
}

fn wrap_owned_reads_except(expr: IrExpr, wrap: &HashSet<VarId>, except: &HashSet<VarId>) -> IrExpr {
    if wrap.is_empty() { return expr; }
    let IrExpr { kind, ty, span, def_id } = expr;
    let kind = match kind {
        IrExprKind::Var { id } if wrap.contains(&id) && !except.contains(&id) => {
            IrExprKind::Clone {
                expr: Box::new(IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id }),
            }
        }
        // Reference-taking positions and an existing `Clone` keep a bare Var
        // child untouched — see [`keep_bare_var`].
        IrExprKind::Borrow { expr: inner, as_str, mutable } => IrExprKind::Borrow {
            expr: keep_bare_var(inner, wrap, except),
            as_str,
            mutable,
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: keep_bare_var(object, wrap, except),
            field,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: keep_bare_var(object, wrap, except),
            index: Box::new(wrap_owned_reads_except(*index, wrap, except)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: keep_bare_var(object, wrap, except),
            key: Box::new(wrap_owned_reads_except(*key, wrap, except)),
        },
        IrExprKind::Clone { expr: inner } => IrExprKind::Clone {
            expr: keep_bare_var(inner, wrap, except),
        },
        IrExprKind::Lambda { params, body, lambda_id } => {
            IrExprKind::Lambda { params, body, lambda_id }
        }
        other => {
            return IrExpr { kind: other, ty, span, def_id }
                .map_children(&mut |c| wrap_owned_reads_except(c, wrap, except));
        }
    };
    IrExpr { kind, ty, span, def_id }
}

/// [`wrap_owned_reads_except`] with no exceptions — for the non-terminal
/// regions (conditions, match subjects and guards, leading block
/// statements) where no read is provably final.
fn wrap_owned_reads(expr: IrExpr, owned: &HashSet<VarId>) -> IrExpr {
    if owned.is_empty() { return expr; }
    wrap_owned_reads_except(expr, owned, &HashSet::new())
}

fn wrap_owned_reads_stmt(stmt: IrStmt, owned: &HashSet<VarId>) -> IrStmt {
    if owned.is_empty() { return stmt; }
    let except = HashSet::new();
    stmt.map_exprs(&mut |e| wrap_owned_reads_except(e, owned, &except))
}

/// Returns true if we can produce a valid default value for this type.
/// Types that fail this check should not be TCO'd (the result variable
/// cannot be initialized without unsafe code).
fn can_default_init(ty: &Ty) -> bool {
    match ty {
        Ty::Int | Ty::Float | Ty::Bool | Ty::String | Ty::Unit => true,
        Ty::Applied(TypeConstructorId::Result, args) => {
            args.first().map_or(true, |inner| can_default_init(inner))
        }
        Ty::Applied(TypeConstructorId::Option, _) => true,
        Ty::Applied(TypeConstructorId::List, _) => true,
        Ty::Applied(TypeConstructorId::Map, _) => true,
        Ty::Tuple(elems) => elems.iter().all(|t| can_default_init(t)),
        _ => false,
    }
}
