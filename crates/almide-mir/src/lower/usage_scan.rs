// ── The fused injection-gate scan (#1232) ──
//
// The pipeline's drop-source gates each answered "does any expression carry
// type X" with their OWN full-IR walk — 11 traversals per compile, every one
// visiting every expression of every function. This file fuses them into ONE
// walk that evaluates every predicate per node, plus the (cheap, decl-only)
// `list_str_drop_field` scan. Each predicate is the EXACT test its original
// walker applied, over the exact same coverage (return type, param types,
// every expression's type — and the `Lambda` expression kind for `closures`),
// so the fused answer is equal by construction; the per-walk originals were
// deleted with this landing (their doc rationale moved onto the fields below).

/// One flag per injection gate the pipeline consults. Field order mirrors the
/// pipeline's gating block.
pub struct ProgramUsage {
    /// `Result[Option[String], String]` anywhere — gates `RES_OPT_STR` handling.
    pub result_option_str: bool,
    /// A `Lambda` expr or `Ty::Fn`-typed value anywhere — gates `CLOSURE_DROP_SRC`.
    pub closures: bool,
    /// `List[<Fn>]` anywhere — gates `LIST_CLOSURE_DROP_SRC`.
    pub closure_list: bool,
    /// `Map[String, <Fn>]` anywhere — gates `MAP_MCLO_DROP_SRC`.
    pub map_closure: bool,
    /// `List[(String, <Fn>)]` anywhere — gates `LIST_STR_CLO_DROP_SRC`.
    pub str_clo_pairs: bool,
    /// `Option[(String, String)]` anywhere — gates `OPT_STR_STR_DROP_SRC`.
    pub opt_str_str: bool,
    /// A len-loop list type anywhere — gates `LENLIST_DROP_SRC`.
    pub lenlist_elem_lists: bool,
    /// A `Record`/`OpenRecord` with a `__drop_list_str`-routed field anywhere —
    /// one half of the `LIST_STR_DROP_SRC` gate.
    pub anon_list_str_record: bool,
    /// A type-decl record OR variant field routing to `__drop_list_str` — the
    /// other half of the `LIST_STR_DROP_SRC` gate (decl scan, not an IR walk).
    pub list_str_drop_field: bool,
    /// `Result[List[Int], List[String]]` anywhere — gates `$__drop_res_ilsl`.
    pub res_intlist_strlist: bool,
    pub res_fs: bool,
    /// A heap-Ok `Result[List[<one-level-exact>], String]` anywhere (fs.read_lines /
    /// fold_lines_ls wrappers) — gates `$__drop_res_lsl`.
    pub res_lenlist_str: bool,
    /// Either fs.fold_lines msi Result class anywhere — gates
    /// `$__drop_res_msi` / `$__drop_res_lmsi`.
    pub res_map_si: bool,
    /// `Option[(String, <scalar>)]` anywhere — gates `OPT_STR_INT_DROP_SRC`
    /// (#840: type-driven, never the retired `map.find` name heuristic).
    pub opt_str_scalar: bool,
}

fn usage_is_result_opt_str(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let Ty::Applied(TypeConstructorId::Result, a) = ty else { return false };
    if a.len() != 2 || !matches!(a[1], Ty::String) {
        return false;
    }
    matches!(&a[0], Ty::Applied(TypeConstructorId::Option, oa)
        if oa.len() == 1 && matches!(oa[0], Ty::String))
}

fn usage_is_closure_list(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::List, a)
        if a.len() == 1 && matches!(a[0], Ty::Fn { .. }))
}

fn usage_is_str_clo_pairs(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::List, a)
        if a.len() == 1
            && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                && matches!(ts[0], Ty::String) && matches!(ts[1], Ty::Fn { .. })))
}

fn usage_is_opt_str_scalar(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Option, a)
        if a.len() == 1
            && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                && matches!(ts[0], Ty::String) && !crate::lower::is_heap_ty(&ts[1])))
}

fn usage_is_opt_str_str(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Option, a)
        if a.len() == 1
            && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                && matches!(ts[0], Ty::String) && matches!(ts[1], Ty::String)))
}

/// Measure every gate in one pass over the IR (plus the decl-only scan).
/// `type_decls` must be the pipeline's `all_type_decls` INCLUDING the shadow
/// synthetics, so `flat_variant_type_names` sees the same universe the old
/// per-gate calls saw.
pub fn measure_program_usage(
    program: &almide_ir::IrProgram,
    type_decls: &[almide_ir::IrTypeDecl],
) -> ProgramUsage {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    let flat_names = flat_variant_type_names(type_decls);
    let anon_record_matches = |t: &Ty| {
        matches!(t, Ty::Record { fields } | Ty::OpenRecord { fields }
            if fields.iter().any(|(_, ft)| is_list_str_field(ft, &flat_names)))
    };
    let list_str_drop_field = type_decls.iter().any(|d| match &d.kind {
        IrTypeDeclKind::Record { fields } => {
            fields.iter().any(|f| is_list_str_field(&f.ty, &flat_names))
        }
        IrTypeDeclKind::Variant { cases, .. } => cases.iter().any(|c| {
            let tys: Vec<&Ty> = match &c.kind {
                IrVariantKind::Unit => vec![],
                IrVariantKind::Tuple { fields } => fields.iter().collect(),
                IrVariantKind::Record { fields } => fields.iter().map(|f| &f.ty).collect(),
            };
            tys.iter().any(|t| is_list_str_field(t, &flat_names))
        }),
        _ => false,
    });

    struct UsageFinder<'a> {
        u: ProgramUsage,
        anon_record_matches: &'a dyn Fn(&Ty) -> bool,
    }
    impl UsageFinder<'_> {
        fn check_ty(&mut self, ty: &Ty) {
            let u = &mut self.u;
            u.result_option_str |= usage_is_result_opt_str(ty);
            u.closures |= matches!(ty, Ty::Fn { .. });
            u.closure_list |= usage_is_closure_list(ty);
            u.map_closure |= crate::lower::is_map_fn_ty(ty);
            u.str_clo_pairs |= usage_is_str_clo_pairs(ty);
            u.opt_str_str |= usage_is_opt_str_str(ty);
            u.lenlist_elem_lists |= is_lenlist_list_ty(ty);
            u.anon_list_str_record |= (self.anon_record_matches)(ty);
            u.res_intlist_strlist |= is_res_intlist_strlist_ty(ty);
            u.res_fs |= is_res_fs_ty(ty);
            u.res_lenlist_str |= crate::lower::is_res_lenlist_str_ty(ty);
            u.res_map_si |= is_res_map_si_ty(ty) || is_res_list_map_si_ty(ty);
            u.opt_str_scalar |= usage_is_opt_str_scalar(ty);
        }
        fn all_found(&self) -> bool {
            let u = &self.u;
            u.result_option_str
                && u.closures
                && u.closure_list
                && u.map_closure
                && u.str_clo_pairs
                && u.opt_str_str
                && u.lenlist_elem_lists
                && u.anon_list_str_record
                && u.res_intlist_strlist
                // EVERY gated flag must sit in this early-exit conjunction: one
                // missing here can stay false when all the LISTED flags are found
                // first — the walk stops, the drop-SOURCE gate misses while the
                // route sites still seed the call, and the module ships a
                // dangling `$__drop_*` name (invalid wasm; the derived-codec
                // decode unit test caught exactly that for res_lenlist_str, and
                // res_fs carried the same latent omission).
                && u.res_fs
                && u.res_lenlist_str
                && u.res_map_si
                && u.opt_str_scalar
        }
    }
    impl almide_ir::visit::IrVisitor for UsageFinder<'_> {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            self.check_ty(&expr.ty);
            if matches!(expr.kind, almide_ir::IrExprKind::Lambda { .. }) {
                self.u.closures = true;
            }
            if !self.all_found() {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }

    let mut finder = UsageFinder {
        u: ProgramUsage {
            result_option_str: false,
            closures: false,
            closure_list: false,
            map_closure: false,
            str_clo_pairs: false,
            opt_str_str: false,
            lenlist_elem_lists: false,
            anon_list_str_record: false,
            list_str_drop_field,
            res_intlist_strlist: false,
            res_fs: false,
            res_lenlist_str: false,
            res_map_si: false,
            opt_str_scalar: false,
        },
        anon_record_matches: &anon_record_matches,
    };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        finder.check_ty(&f.ret_ty);
        for p in &f.params {
            finder.check_ty(&p.ty);
        }
        if !finder.all_found() {
            almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        }
    }
    finder.u
}
