//! The clone half of `RegionWindowPass` (#1991): twin enums and `__rgn_`
//! fn clones for a window site's callee closure.
//!
//! A twin enum `__rgn_Tree` mirrors `Tree` case for case (`__rgn_Leaf`,
//! `__rgn_Node`) with every payload type substituted, and the walker
//! renders its recursive fields as `AlmideRgn<__rgn_Tree>` (a `Copy` handle
//! into the prelude arena) instead of `Box<Tree>`, plus `impl Copy`. A twin
//! fn `__rgn_check` is `check` with `Tree → __rgn_Tree` substituted in its
//! params, return type and every expression / binding / pattern type, its
//! ctor calls and ctor patterns renamed, its callees remapped inside the
//! closure, and fresh `VarId`s for every binder (the two bodies must not
//! share var-table rows — later passes decide clone / borrow / storage per
//! `VarId`).

use std::collections::{HashMap, HashSet};

use almide_base::intern::{sym, Sym};
use almide_ir::visit::{walk_expr, walk_pattern, walk_stmt, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, walk_pattern_mut, walk_stmt_mut, IrMutVisitor};
use almide_ir::*;
use almide_lang::types::Ty;

use super::pass_region_window::{is_scalar_ty, twin_name, Cx, TwinPlan};

/// Prefix of every synthesized twin (enum, ctor and fn).
pub const RGN_PREFIX: &str = "__rgn_";

fn variant_decl(decls: &[IrTypeDecl], name: Sym) -> Option<&IrTypeDecl> {
    decls.iter().find(|td| td.name == name && matches!(td.kind, IrTypeDeclKind::Variant { .. }))
}

/// The enums a `Copy` twin of `root` needs: `root` and every enum its
/// payloads reach. `None` when any of them is not admissible — generic, a
/// record-style case, or a payload that is neither scalar nor such an enum.
pub(crate) fn admissible_enums(root: Sym, decls: &[IrTypeDecl]) -> Option<HashSet<Sym>> {
    let mut set: HashSet<Sym> = HashSet::new();
    let mut todo = vec![root];
    while let Some(n) = todo.pop() {
        if !set.insert(n) {
            continue;
        }
        let td = variant_decl(decls, n)?;
        let IrTypeDeclKind::Variant { cases, is_generic, .. } = &td.kind else { return None };
        if *is_generic || td.generics.as_ref().is_some_and(|g| !g.is_empty()) {
            return None;
        }
        for c in cases {
            match &c.kind {
                IrVariantKind::Unit => {}
                IrVariantKind::Tuple { fields } => {
                    for f in fields {
                        match f {
                            t if is_scalar_ty(t) => {}
                            Ty::Named(e, args) if args.is_empty() => todo.push(*e),
                            _ => return None,
                        }
                    }
                }
                IrVariantKind::Record { .. } => return None,
            }
        }
    }
    Some(set)
}

/// Every `Named` / `Variant` type name inside `t`, nested ones included.
fn collect_named(t: &Ty, out: &mut Vec<Sym>) {
    if let Ty::Named(n, _) | Ty::Variant { name: n, .. } = t {
        out.push(*n);
    }
    for c in t.children() {
        collect_named(c, out);
    }
}

/// Every `Named` type name mentioned anywhere in a fn: signature, expression
/// types, binding and pattern types.
fn named_types_in(f: &IrFunction) -> HashSet<Sym> {
    struct Names(HashSet<Sym>);
    impl Names {
        fn ty(&mut self, t: &Ty) {
            let mut found = Vec::new();
            collect_named(t, &mut found);
            self.0.extend(found);
        }
    }
    impl IrVisitor for Names {
        fn visit_expr(&mut self, e: &IrExpr) {
            self.ty(&e.ty);
            if let IrExprKind::Call { type_args, .. } = &e.kind {
                for t in type_args {
                    self.ty(t);
                }
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { ty, .. } = &s.kind {
                self.ty(ty);
            }
            walk_stmt(self, s);
        }
        fn visit_pattern(&mut self, p: &IrPattern) {
            if let IrPattern::Bind { ty, .. } | IrPattern::As { ty, .. } = p {
                self.ty(ty);
            }
            walk_pattern(self, p);
        }
    }
    let mut names = Names(HashSet::new());
    for p in &f.params {
        names.ty(&p.ty);
    }
    names.ty(&f.ret_ty);
    names.visit_expr(&f.body);
    names.0
}

/// Does the decl named `n` reach any member of `enums` through its field
/// types? (A record holding a `Tree` cannot be typed once `Tree` is twinned
/// inside the clone, so such a closure is refused.)
fn decl_reaches(n: Sym, enums: &HashSet<Sym>, decls: &[IrTypeDecl], seen: &mut HashSet<Sym>) -> bool {
    if !seen.insert(n) {
        return false;
    }
    let Some(td) = decls.iter().find(|td| td.name == n) else { return false };
    let mut tys: Vec<&Ty> = Vec::new();
    match &td.kind {
        IrTypeDeclKind::Record { fields } => tys.extend(fields.iter().map(|f| &f.ty)),
        IrTypeDeclKind::Alias { target } => tys.push(target),
        IrTypeDeclKind::Variant { cases, .. } => {
            for c in cases {
                match &c.kind {
                    IrVariantKind::Tuple { fields } => tys.extend(fields.iter()),
                    IrVariantKind::Record { fields } => tys.extend(fields.iter().map(|f| &f.ty)),
                    IrVariantKind::Unit => {}
                }
            }
        }
    }
    let mut mentioned: Vec<Sym> = Vec::new();
    for t in tys {
        collect_named(t, &mut mentioned);
    }
    mentioned.into_iter().any(|m| enums.contains(&m) || decl_reaches(m, enums, decls, seen))
}

/// Can every fn in the closure be cloned with `enums` twinned? Every named
/// type it mentions is either twinned or unable to reach a twinned enum.
pub(crate) fn closure_typable(program: &IrProgram, cx: &Cx, fns: &HashSet<Sym>, enums: &HashSet<Sym>) -> bool {
    fns.iter().all(|n| {
        let f = &program.functions[cx.fns[n]];
        named_types_in(f).into_iter().all(|t| {
            enums.contains(&t) || !decl_reaches(t, enums, cx.decls, &mut HashSet::new())
        })
    })
}

/// `Tree → __rgn_Tree` everywhere inside a type.
fn subst_ty(t: &Ty, twins: &HashMap<Sym, Sym>) -> Ty {
    match t {
        Ty::Named(n, args) if twins.contains_key(n) => {
            Ty::Named(twins[n], args.iter().map(|a| subst_ty(a, twins)).collect())
        }
        Ty::Variant { name, .. } if twins.contains_key(name) => Ty::Named(twins[name], vec![]),
        _ => t.map_children(&|c| subst_ty(c, twins)),
    }
}

/// Append the twin enums and twin fns of `plan` to the root program and
/// record the twin enum names for the walker.
pub(crate) fn synthesize_twins(program: &mut IrProgram, plan: &TwinPlan) {
    let twins: HashMap<Sym, Sym> = plan.enums.iter().map(|e| (*e, twin_name(*e))).collect();
    let mut ctors: HashMap<String, String> = HashMap::new();
    let mut new_decls = Vec::new();
    for e in &plan.enums {
        let td = program.type_decls.iter().find(|td| td.name == *e).expect("admitted enum decl");
        new_decls.push(twin_decl(td, &twins, &mut ctors));
    }
    let mut new_fns = Vec::new();
    for n in &plan.fns {
        let f = program.functions.iter().find(|f| f.name == *n).expect("closure fn");
        new_fns.push(twin_fn(f, &mut program.var_table, &twins, &ctors, &plan.fns));
    }
    program.codegen_annotations.region_enums.extend(twins.values().map(|s| s.as_str().to_string()));
    program.type_decls.extend(new_decls);
    program.functions.extend(new_fns);
}

fn twin_decl(td: &IrTypeDecl, twins: &HashMap<Sym, Sym>, ctors: &mut HashMap<String, String>) -> IrTypeDecl {
    let IrTypeDeclKind::Variant { cases, .. } = &td.kind else { unreachable!("admitted enum is a variant") };
    let cases = cases
        .iter()
        .map(|c| {
            let name = twin_name(c.name);
            ctors.insert(c.name.as_str().to_string(), name.as_str().to_string());
            let kind = match &c.kind {
                IrVariantKind::Unit => IrVariantKind::Unit,
                IrVariantKind::Tuple { fields } => {
                    IrVariantKind::Tuple { fields: fields.iter().map(|t| subst_ty(t, twins)).collect() }
                }
                IrVariantKind::Record { .. } => unreachable!("admitted enum has no record case"),
            };
            IrVariantDecl { name, kind }
        })
        .collect();
    IrTypeDecl {
        name: twins[&td.name],
        kind: IrTypeDeclKind::Variant {
            cases,
            is_generic: false,
            boxed_args: HashSet::new(),
            boxed_record_fields: HashSet::new(),
        },
        // The twin derives what the original derives (an `Ord` tree compared
        // inside the closure needs `PartialOrd` on the twin too); the
        // prelude's `AlmideRgn` forwards every such impl to the pointee.
        deriving: td.deriving.clone(),
        generics: None,
        visibility: IrVisibility::Public,
        doc: None,
        blank_lines_before: 1,
    }
}

/// Every binder a fn body introduces (params are handled by the caller).
fn binders_in(body: &IrExpr) -> Vec<VarId> {
    struct B(Vec<VarId>);
    impl IrVisitor for B {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::ForIn { var, var_tuple, .. } => {
                    self.0.push(*var);
                    self.0.extend(var_tuple.iter().flatten().copied());
                }
                IrExprKind::Lambda { params, .. } => self.0.extend(params.iter().map(|(v, _)| *v)),
                _ => {}
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { var, .. } = &s.kind {
                self.0.push(*var);
            }
            walk_stmt(self, s);
        }
        fn visit_pattern(&mut self, p: &IrPattern) {
            if let IrPattern::Bind { var, .. } | IrPattern::As { var, .. } = p {
                self.0.push(*var);
            }
            walk_pattern(self, p);
        }
    }
    let mut b = B(Vec::new());
    b.visit_expr(body);
    b.0
}

fn fresh_var(vt: &mut VarTable, old: VarId, twins: &HashMap<Sym, Sym>) -> VarId {
    let info = vt.get(old).clone();
    vt.alloc(info.name, subst_ty(&info.ty, twins), info.mutability, info.span)
}

fn twin_fn(
    f: &IrFunction,
    vt: &mut VarTable,
    twins: &HashMap<Sym, Sym>,
    ctors: &HashMap<String, String>,
    fns: &HashSet<Sym>,
) -> IrFunction {
    let mut vars: HashMap<VarId, VarId> = HashMap::new();
    let params = f
        .params
        .iter()
        .map(|p| {
            let var = fresh_var(vt, p.var, twins);
            vars.insert(p.var, var);
            IrParam { var, ty: subst_ty(&p.ty, twins), borrow: ParamBorrow::Own, default: None, ..p.clone() }
        })
        .collect();
    for old in binders_in(&f.body) {
        let var = fresh_var(vt, old, twins);
        vars.insert(old, var);
    }
    let mut body = f.body.clone();
    CloneRewriter { vars: &vars, twins, ctors, fns }.visit_expr_mut(&mut body);
    IrFunction {
        name: twin_name(f.name),
        params,
        ret_ty: subst_ty(&f.ret_ty, twins),
        body,
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Public,
        doc: None,
        blank_lines_before: 1,
        def_id: None,
        // A clone propagates its original's flags (fn_clone_discipline): the
        // pure seed admits no `mut` param, so this is empty in practice, but
        // the twin must never DROP a writeback flag its original carried.
        mutated_params: f.mutated_params.clone(),
        module_origin: None,
    }
}

/// The body rewrite: fresh vars, substituted types, renamed ctors and
/// callees, no `def_id` (a stale one would render the ORIGINAL fn).
struct CloneRewriter<'a> {
    vars: &'a HashMap<VarId, VarId>,
    twins: &'a HashMap<Sym, Sym>,
    ctors: &'a HashMap<String, String>,
    fns: &'a HashSet<Sym>,
}

impl CloneRewriter<'_> {
    fn var(&self, v: &mut VarId) {
        if let Some(n) = self.vars.get(v) {
            *v = *n;
        }
    }
    fn callee(&self, name: &mut Sym) {
        if self.fns.contains(name) {
            *name = twin_name(*name);
        } else if let Some(t) = self.ctors.get(name.as_str()) {
            *name = sym(t);
        }
    }
}

impl IrMutVisitor for CloneRewriter<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        e.ty = subst_ty(&e.ty, self.twins);
        e.def_id = None;
        match &mut e.kind {
            IrExprKind::Var { id } => self.var(id),
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.var(var);
                for v in var_tuple.iter_mut().flatten() {
                    self.var(v);
                }
            }
            IrExprKind::Call { target: CallTarget::Named { name }, type_args, .. } => {
                self.callee(name);
                for t in type_args.iter_mut() {
                    *t = subst_ty(t, self.twins);
                }
            }
            IrExprKind::Lambda { params, .. } => {
                for (v, t) in params.iter_mut() {
                    self.var(v);
                    *t = subst_ty(t, self.twins);
                }
            }
            _ => {}
        }
        walk_expr_mut(self, e);
    }

    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        match &mut s.kind {
            IrStmtKind::Bind { var, ty, .. } => {
                self.var(var);
                *ty = subst_ty(ty, self.twins);
            }
            IrStmtKind::Assign { var, .. } => self.var(var),
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. } => self.var(target),
            _ => {}
        }
        walk_stmt_mut(self, s);
    }

    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        match p {
            IrPattern::Bind { var, ty } | IrPattern::As { var, ty, .. } => {
                self.var(var);
                *ty = subst_ty(ty, self.twins);
            }
            IrPattern::Constructor { name, .. } | IrPattern::RecordPattern { name, .. } => {
                if let Some(t) = self.ctors.get(name.as_str()) {
                    *name = t.clone();
                }
            }
            _ => {}
        }
        walk_pattern_mut(self, p);
    }
}
