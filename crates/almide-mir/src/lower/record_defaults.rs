/// RECORD DEFAULT-FIELD FILL (a pre-lowering program pass, desugar-before-both): a
/// record literal that omits fields with declared defaults (`Opts {}` over
/// `type Opts = { verbose: Bool = false, retries: Int = 3 }`) gets the missing
/// fields APPENDED from the decl's default exprs. v0 does this at emit time
/// (walker/expressions.rs reads `ann.default_fields`); the v1 leg saw the bare
/// 0-field literal and built an empty block — every defaulted read was silent
/// garbage (regression_v0_11's "all defaults" 0-instead-of-3, 2026-07-17). Running
/// the fill ONCE on the linked program, in the SAME post-link fixup chain the
/// pipeline and the classify counter share, keeps the caps `mir == ir` invariant:
/// a call-bearing default is counted AND lowered from one tree.
pub fn fill_record_defaults(program: &mut almide_ir::IrProgram) {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{IrExpr, IrExprKind, IrTypeDeclKind};
    use almide_lang::intern::Sym;
    use std::collections::HashMap;

    fn collect(decls: &[almide_ir::IrTypeDecl], out: &mut HashMap<String, Vec<(Sym, IrExpr)>>) {
        for decl in decls {
            if let IrTypeDeclKind::Record { fields } = &decl.kind {
                let ds: Vec<(Sym, IrExpr)> = fields
                    .iter()
                    .filter_map(|f| f.default.as_ref().map(|d| (f.name, d.clone())))
                    .collect();
                if !ds.is_empty() {
                    out.insert(decl.name.as_str().to_string(), ds);
                }
            }
        }
    }

    struct Fill<'a> {
        defaults: &'a HashMap<String, Vec<(Sym, IrExpr)>>,
    }
    impl IrMutVisitor for Fill<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Record { name: Some(n), fields } = &mut e.kind {
                // Exact decl name first; a module-qualified literal (`dep.Opts { .. }`)
                // falls back to its base segment (the layout-alias discipline).
                let key = n.as_str();
                let ds = self
                    .defaults
                    .get(key)
                    .or_else(|| key.rsplit('.').next().and_then(|b| self.defaults.get(b)));
                if let Some(ds) = ds {
                    for (fname, dexpr) in ds {
                        if !fields.iter().any(|(k, _)| k == fname) {
                            fields.push((*fname, dexpr.clone()));
                        }
                    }
                }
            }
        }
    }

    let mut defaults: HashMap<String, Vec<(Sym, IrExpr)>> = HashMap::new();
    collect(&program.type_decls, &mut defaults);
    for m in &program.modules {
        collect(&m.type_decls, &mut defaults);
    }
    if defaults.is_empty() {
        return;
    }
    let mut fill = Fill { defaults: &defaults };
    for f in &mut program.functions {
        fill.visit_expr_mut(&mut f.body);
    }
    for m in &mut program.modules {
        for f in &mut m.functions {
            fill.visit_expr_mut(&mut f.body);
        }
        for tl in &mut m.top_lets {
            fill.visit_expr_mut(&mut tl.value);
        }
    }
    for tl in &mut program.top_lets {
        fill.visit_expr_mut(&mut tl.value);
    }
}
