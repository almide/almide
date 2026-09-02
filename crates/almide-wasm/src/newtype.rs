//! Transparent-newtype erasure for the structural leg (#1423 stage 4):
//! `mod type SafeHtml = String` is a purely NOMINAL wrapper — the checker
//! rejects every operation that could observe it, so by IR time the
//! newtype survives only as (1) `Ty::Named(name, [])` tags, (2) a 1-arg
//! ctor CALL `SafeHtml(s)` and (3) a 1-arg ctor PATTERN `SafeHtml(s) =>`.
//! All three erase to the inner type — the value IS its payload, so a
//! bind, a param, equality, print and drop follow the inner type (the
//! incumbent's newtype_erase.rs doctrine, re-spelled here because the
//! emitter reads the linked IR directly and cannot depend on almide-mir).
//! An IR REWRITE, not a lowering-time special case: a ctor call erased
//! at its call site would hand the bind a BORROWED payload while the
//! bind's RC rule reads a Call as a fresh owner. Fn types keep their
//! `is_effect` — the C-221 adapter decision reads it.

use std::collections::HashMap;

use almide_ir::visit_mut::{walk_expr_mut, walk_pattern_mut, walk_stmt_mut, IrMutVisitor};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrFunction, IrPattern, IrProgram, IrStmt, IrStmtKind,
    IrTypeDecl, IrTypeDeclKind, IrVariantKind, Mutability,
};
use almide_types::types::Ty;

type AliasMap = HashMap<String, Ty>;

/// The erased program, or None when it declares no transparent alias
/// (the common case: no clone, the caller keeps its borrow).
pub(crate) fn erase_transparent_aliases(ir: &IrProgram) -> Option<IrProgram> {
    let map = alias_map(ir);
    if map.is_empty() {
        return None;
    }
    let mut out = ir.clone();
    let mut v = Eraser { map: &map };
    erase_fns(&mut v, &mut out.functions);
    for m in out.modules.iter_mut() {
        erase_fns(&mut v, &mut m.functions);
        for tl in m.top_lets.iter_mut() {
            tl.ty = subst(&tl.ty, &map);
            v.visit_expr_mut(&mut tl.value);
        }
    }
    for tl in out.top_lets.iter_mut() {
        tl.ty = subst(&tl.ty, &map);
        v.visit_expr_mut(&mut tl.value);
    }
    // Other decls may hold alias-typed fields (a record carrying a SafeHtml).
    let module_decls = out.modules.iter_mut().flat_map(|m| m.type_decls.iter_mut());
    for d in out.type_decls.iter_mut().chain(module_decls) {
        erase_decl(d, &map);
    }
    Some(out)
}

/// Every non-generic alias declaration (program + modules), alias-of-alias
/// chains resolved to a bounded fixpoint (a cycle is a checker error; the
/// bound keeps this total and a leftover Named walls downstream honestly).
fn alias_map(ir: &IrProgram) -> AliasMap {
    let mut map = AliasMap::new();
    let decls = ir.type_decls.iter().chain(ir.modules.iter().flat_map(|m| m.type_decls.iter()));
    for td in decls {
        if let IrTypeDeclKind::Alias { target } = &td.kind
            && td.generics.is_none()
        {
            map.insert(td.name.as_str().to_string(), target.clone());
        }
    }
    for _ in 0..8 {
        let snapshot = map.clone();
        let mut changed = false;
        for t in map.values_mut() {
            let nt = subst(t, &snapshot);
            if nt != *t {
                *t = nt;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    map
}

fn subst(ty: &Ty, map: &AliasMap) -> Ty {
    let all = |ts: &[Ty]| ts.iter().map(|t| subst(t, map)).collect::<Vec<_>>();
    let fields = |fs: &[(almide_types::intern::Sym, Ty)]| {
        fs.iter().map(|(n, t)| (*n, subst(t, map))).collect::<Vec<_>>()
    };
    match ty {
        Ty::Named(name, args) if args.is_empty() => {
            map.get(name.as_str()).cloned().unwrap_or_else(|| ty.clone())
        }
        Ty::Named(name, args) => Ty::Named(*name, all(args)),
        Ty::Applied(id, args) => Ty::Applied(id.clone(), all(args)),
        Ty::Tuple(ts) => Ty::Tuple(all(ts)),
        Ty::Union(ts) => Ty::Union(all(ts)),
        Ty::Record { fields: fs } => Ty::Record { fields: fields(fs) },
        Ty::OpenRecord { fields: fs } => Ty::OpenRecord { fields: fields(fs) },
        Ty::Fn { params, ret, is_effect } => Ty::Fn {
            params: all(params),
            ret: Box::new(subst(ret, map)),
            is_effect: *is_effect,
        },
        _ => ty.clone(),
    }
}

fn erase_fns(v: &mut Eraser<'_>, fns: &mut [IrFunction]) {
    for f in fns.iter_mut() {
        for p in f.params.iter_mut() {
            p.ty = subst(&p.ty, v.map);
        }
        f.ret_ty = subst(&f.ret_ty, v.map);
        v.visit_expr_mut(&mut f.body);
    }
}

fn erase_decl(d: &mut IrTypeDecl, map: &AliasMap) {
    match &mut d.kind {
        IrTypeDeclKind::Record { fields } => {
            for f in fields.iter_mut() {
                f.ty = subst(&f.ty, map);
            }
        }
        IrTypeDeclKind::Variant { cases, .. } => {
            for c in cases.iter_mut() {
                match &mut c.kind {
                    IrVariantKind::Tuple { fields } => {
                        for t in fields.iter_mut() {
                            *t = subst(t, map);
                        }
                    }
                    IrVariantKind::Record { fields } => {
                        for f in fields.iter_mut() {
                            f.ty = subst(&f.ty, map);
                        }
                    }
                    IrVariantKind::Unit => {}
                }
            }
        }
        IrTypeDeclKind::Alias { .. } => {}
    }
}

struct Eraser<'a> {
    map: &'a AliasMap,
}

impl IrMutVisitor for Eraser<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        e.ty = subst(&e.ty, self.map);
        if let IrExprKind::Lambda { params, .. } = &mut e.kind {
            for (_, ty) in params.iter_mut() {
                *ty = subst(ty, self.map);
            }
        }
        self.erase_ctor_call(e);
        fold_unit_bind_match(e);
    }

    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        walk_stmt_mut(self, s);
        if let IrStmtKind::Bind { ty, .. } = &mut s.kind {
            *ty = subst(ty, self.map);
        }
    }

    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        walk_pattern_mut(self, p);
        if let IrPattern::Bind { ty, .. } = p {
            *ty = subst(ty, self.map);
        }
        // The 1-arg newtype ctor PATTERN always matches — it IS the inner.
        let inner = match p {
            IrPattern::Constructor { name, args }
                if args.len() == 1 && self.map.contains_key(name.as_str()) =>
            {
                Some(args.remove(0))
            }
            _ => None,
        };
        if let Some(ip) = inner {
            *p = ip;
        }
    }
}

impl Eraser<'_> {
    /// The 1-arg newtype ctor CALL is its payload (same block, same
    /// ownership — the arg was already visited).
    fn erase_ctor_call(&self, e: &mut IrExpr) {
        let is_ctor = matches!(&e.kind,
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                if args.len() == 1 && self.map.contains_key(name.as_str()));
        if !is_ctor {
            return;
        }
        if let IrExprKind::Call { args, .. } = &mut e.kind {
            let payload = args.pop().expect("1-arg ctor");
            *e = payload;
        }
    }
}

/// A match REDUCED to one bare-Bind arm by the pattern erasure
/// (`match h { SafeHtml(s) => s }` → `match h { s => s }`) is a `let`:
/// `{ let s = h; body }` — the incumbent's tree, lowered by the ordinary
/// bind path.
fn fold_unit_bind_match(e: &mut IrExpr) {
    let is_unit = matches!(&e.kind,
        IrExprKind::Match { arms, .. }
            if arms.len() == 1
                && arms[0].guard.is_none()
                && matches!(arms[0].pattern, IrPattern::Bind { .. }));
    if !is_unit {
        return;
    }
    let IrExprKind::Match { subject, arms } = &mut e.kind else { return };
    let arm = arms.pop().expect("1 arm");
    let IrPattern::Bind { var, ty } = arm.pattern else { return };
    let span = e.span;
    let bind = IrStmt {
        kind: IrStmtKind::Bind {
            var,
            ty,
            value: (**subject).clone(),
            mutability: Mutability::Let,
        },
        span,
    };
    let body_ty = arm.body.ty.clone();
    let def_id = arm.body.def_id;
    *e = IrExpr {
        kind: IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(arm.body)) },
        ty: body_ty,
        span,
        def_id,
    };
}
