/// TRANSPARENT-NEWTYPE ERASURE (a pre-lowering program pass): `mod type SafeHtml =
/// String` is a purely NOMINAL wrapper — the frontend rejects every operation that
/// could observe the wrapper at runtime (direct `println`, arithmetic, off-type
/// passing), so by IR time the newtype exists ONLY as (1) `Ty::Named(name, [])`
/// tags, (2) a 1-arg ctor CALL `SafeHtml(s)` (which would render as an unlinked
/// `$SafeHtml` — an honest wall), and (3) a 1-arg ctor PATTERN `SafeHtml(s) => …`.
/// Erase all three to the inner type: the value IS its payload (v0's runtime is the
/// same `#[repr(transparent)]` story), equality/print/drop all follow the inner ty.
/// Alias CHAINS (`type A = B; type B = String`) resolve to a fixpoint first.
/// GENERIC aliases keep their decl (the target mentions type params) — untouched.
/// Runs on the WHOLE linked program (pipeline + classify — desugar-before-both:
/// the caps `mir == ir` count sees the erased tree on BOTH sides by construction).
pub fn erase_transparent_newtypes(program: &mut almide_ir::IrProgram) {
    use almide_ir::visit_mut::{walk_expr_mut, walk_pattern_mut, walk_stmt_mut, IrMutVisitor};
    use almide_ir::{IrExpr, IrPattern, IrStmt, IrTypeDeclKind};
    use almide_lang::types::{Ty, VariantPayload};
    use std::collections::HashMap;

    fn collect(decls: &[almide_ir::IrTypeDecl], map: &mut HashMap<String, Ty>) {
        for td in decls {
            if let IrTypeDeclKind::Alias { target } = &td.kind {
                if td.generics.is_none() {
                    map.insert(td.name.as_str().to_string(), target.clone());
                }
            }
        }
    }
    fn subst(ty: &Ty, map: &HashMap<String, Ty>) -> Ty {
        match ty {
            Ty::Named(name, args) => {
                if args.is_empty() {
                    if let Some(t) = map.get(name.as_str()) {
                        return t.clone();
                    }
                }
                Ty::Named(*name, args.iter().map(|a| subst(a, map)).collect())
            }
            Ty::Applied(id, args) => {
                Ty::Applied(id.clone(), args.iter().map(|a| subst(a, map)).collect())
            }
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|a| subst(a, map)).collect()),
            Ty::Union(ts) => Ty::Union(ts.iter().map(|a| subst(a, map)).collect()),
            Ty::Record { fields } => Ty::Record {
                fields: fields.iter().map(|(n, t)| (*n, subst(t, map))).collect(),
            },
            Ty::OpenRecord { fields } => Ty::OpenRecord {
                fields: fields.iter().map(|(n, t)| (*n, subst(t, map))).collect(),
            },
            Ty::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(|a| subst(a, map)).collect(),
                ret: Box::new(subst(ret, map)),
            },
            Ty::Variant { name, cases } => Ty::Variant {
                name: *name,
                cases: cases
                    .iter()
                    .map(|c| almide_lang::types::VariantCase {
                        name: c.name,
                        payload: match &c.payload {
                            VariantPayload::Unit => VariantPayload::Unit,
                            VariantPayload::Tuple(ts) => VariantPayload::Tuple(
                                ts.iter().map(|a| subst(a, map)).collect(),
                            ),
                            VariantPayload::Record(fs) => VariantPayload::Record(
                                fs.iter().map(|(n, t)| (*n, subst(t, map))).collect(),
                            ),
                        },
                    })
                    .collect(),
            },
            Ty::ConstParam { name, ty } => {
                Ty::ConstParam { name: *name, ty: Box::new(subst(ty, map)) }
            }
            Ty::ConstValue { ty, value } => {
                Ty::ConstValue { ty: Box::new(subst(ty, map)), value: *value }
            }
            _ => ty.clone(),
        }
    }

    let mut map: HashMap<String, Ty> = HashMap::new();
    collect(&program.type_decls, &mut map);
    for m in &program.modules {
        collect(&m.type_decls, &mut map);
    }
    // SELF-HOST REP table: opaque STDLIB nominals whose v1 self-host owns the
    // representation (`stdlib/json_path.almd` — JsonPath = a List[String] of
    // segments). Publishing the rep here makes every user-side bind/param of the
    // opaque type route the CORRECT drop (heap_elem_list str). Only when the
    // program does not declare its own type of the same name.
    let declared: std::collections::HashSet<&str> = program
        .type_decls
        .iter()
        .map(|td| td.name.as_str())
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter().map(|td| td.name.as_str())))
        .collect();
    if !declared.contains("JsonPath") {
        map.insert(
            "JsonPath".to_string(),
            Ty::Applied(
                almide_lang::types::constructor::TypeConstructorId::List,
                vec![Ty::String],
            ),
        );
    }
    // HttpResponse — the self-host rep is `[status, body, k1, v1, …]`
    // (stdlib/http_response.almd, the same List[String] discipline).
    if !declared.contains("HttpResponse") {
        map.insert(
            "HttpResponse".to_string(),
            Ty::Applied(
                almide_lang::types::constructor::TypeConstructorId::List,
                vec![Ty::String],
            ),
        );
    }
    // FileStat — the fs.stat Ok payload. Its decl lives in the BUNDLED stdlib fs module,
    // which `source_to_ir` skips (defs come from the self-host registry), so the nominal
    // `Named(FileStat)` never reaches `record_layouts` and a `meta.size` member read walls.
    // Erase it to the STRUCTURAL record (the SAME field order stdlib/fs.almd declares and
    // stdlib/fs_stat.almd constructs — `aggregate_field_tys` resolves a `Ty::Record`
    // without any registry), so member reads land on the right uniform slots.
    if !declared.contains("FileStat") {
        use almide_lang::intern::sym;
        map.insert(
            "FileStat".to_string(),
            Ty::Record {
                fields: vec![
                    (sym("size"), Ty::Int),
                    (sym("is_dir"), Ty::Bool),
                    (sym("is_file"), Ty::Bool),
                    (sym("modified"), Ty::Int),
                ],
            },
        );
    }
    if map.is_empty() {
        return;
    }
    // Resolve alias-of-alias chains to a fixpoint (bounded — a cycle would be a
    // frontend error; the bound just keeps this total).
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

    struct Eraser<'a> {
        map: &'a HashMap<String, Ty>,
    }
    impl Eraser<'_> {
        fn subst(&self, ty: &Ty) -> Ty {
            subst(ty, self.map)
        }
    }
    impl IrMutVisitor for Eraser<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            use almide_ir::{CallTarget, IrExprKind};
            walk_expr_mut(self, e);
            e.ty = self.subst(&e.ty);
            if let IrExprKind::Lambda { params, .. } = &mut e.kind {
                for (_, ty) in params.iter_mut() {
                    *ty = self.subst(ty);
                }
            }
            // The 1-arg newtype ctor CALL is the payload itself (same block, same
            // ownership — the arg was already erased/visited above).
            let is_newtype_ctor = matches!(&e.kind,
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                    if args.len() == 1 && self.map.contains_key(name.as_str()));
            if is_newtype_ctor {
                if let IrExprKind::Call { args, .. } = &mut e.kind {
                    let payload = args.pop().expect("1-arg ctor");
                    *e = payload;
                }
            }
            // A match REDUCED to one bare-Bind arm by the pattern erasure
            // (`match h { SafeHtml(s) => s }` → `match h { s => s }`) is a `let`:
            // `{ let s = h; body }`. Count-invariant (subject + body appear once).
            let is_unit_bind_match = matches!(&e.kind,
                IrExprKind::Match { arms, .. }
                    if arms.len() == 1
                        && arms[0].guard.is_none()
                        && matches!(arms[0].pattern, IrPattern::Bind { .. }));
            if is_unit_bind_match {
                if let IrExprKind::Match { subject, arms } = &mut e.kind {
                    let arm = arms.pop().expect("1 arm");
                    if let IrPattern::Bind { var, ty } = arm.pattern {
                        let span = e.span.clone();
                        let bind = IrStmt {
                            kind: almide_ir::IrStmtKind::Bind {
                                var,
                                ty,
                                value: (**subject).clone(),
                                mutability: almide_ir::Mutability::Let,
                            },
                            span: span.clone(),
                        };
                        let body_ty = arm.body.ty.clone();
                        let def_id = arm.body.def_id;
                        *e = IrExpr {
                            kind: IrExprKind::Block {
                                stmts: vec![bind],
                                expr: Some(Box::new(arm.body)),
                            },
                            ty: body_ty,
                            span,
                            def_id,
                        };
                    }
                }
            }
        }
        fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
            use almide_ir::IrStmtKind;
            walk_stmt_mut(self, s);
            if let IrStmtKind::Bind { ty, .. } = &mut s.kind {
                *ty = self.subst(ty);
            }
        }
        fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
            walk_pattern_mut(self, p);
            if let IrPattern::Bind { ty, .. } = p {
                *ty = self.subst(ty);
            }
            // The 1-arg newtype ctor PATTERN always matches — it IS the inner pattern.
            let inner = if let IrPattern::Constructor { name, args } = p {
                if args.len() == 1 && self.map.contains_key(name.as_str()) {
                    Some(args.remove(0))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(ip) = inner {
                *p = ip;
            }
        }
    }

    let mut v = Eraser { map: &map };
    fn rewrite_fns(
        v: &mut Eraser<'_>,
        map: &HashMap<String, Ty>,
        fns: &mut [almide_ir::IrFunction],
    ) {
        for f in fns.iter_mut() {
            for p in f.params.iter_mut() {
                p.ty = subst(&p.ty, map);
            }
            f.ret_ty = subst(&f.ret_ty, map);
            v.visit_expr_mut(&mut f.body);
        }
    }
    rewrite_fns(&mut v, &map, &mut program.functions);
    let mut modules = std::mem::take(&mut program.modules);
    for m in modules.iter_mut() {
        rewrite_fns(&mut v, &map, &mut m.functions);
        for tl in m.top_lets.iter_mut() {
            tl.ty = subst(&tl.ty, &map);
            v.visit_expr_mut(&mut tl.value);
        }
    }
    program.modules = modules;
    for tl in program.top_lets.iter_mut() {
        tl.ty = subst(&tl.ty, &map);
        v.visit_expr_mut(&mut tl.value);
    }
    // Other type decls may carry alias-typed fields (a record holding a SafeHtml).
    for td in program.type_decls.iter_mut() {
        match &mut td.kind {
            almide_ir::IrTypeDeclKind::Record { fields } => {
                for f in fields.iter_mut() {
                    f.ty = subst(&f.ty, &map);
                }
            }
            almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
                for c in cases.iter_mut() {
                    match &mut c.kind {
                        almide_ir::IrVariantKind::Unit => {}
                        almide_ir::IrVariantKind::Tuple { fields } => {
                            for t in fields.iter_mut() {
                                *t = subst(t, &map);
                            }
                        }
                        almide_ir::IrVariantKind::Record { fields } => {
                            for f in fields.iter_mut() {
                                f.ty = subst(&f.ty, &map);
                            }
                        }
                    }
                }
            }
            almide_ir::IrTypeDeclKind::Alias { .. } => {}
        }
    }
}
