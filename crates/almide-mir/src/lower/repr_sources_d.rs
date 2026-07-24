
/// Does the program reference the `Result[Option[String], String]` shape anywhere (a function
/// signature or an expression type)? Gates `$__drop_opt_str` emission in
/// [`generate_record_drop_sources`] — the recursive-drop leaf `try_lower_result_option_scalar_str_ctor`
/// routes an `ok(some(<string>))` / `ok(none)` `Result[Option[String], String]` through
/// (`resrec:opt_str`). Only that shape needs the generated fn; a scalar Option leaf frees flat. Scans
/// the SAME positions as [`collect_recursive_anon_records`] (ret/param/body-expr types).
pub fn program_uses_result_option_str(program: &almide_ir::IrProgram) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    fn is_result_opt_str(ty: &Ty) -> bool {
        let Ty::Applied(TypeConstructorId::Result, a) = ty else { return false };
        if a.len() != 2 || !matches!(a[1], Ty::String) {
            return false;
        }
        matches!(&a[0], Ty::Applied(TypeConstructorId::Option, oa)
            if oa.len() == 1 && matches!(oa[0], Ty::String))
    }
    struct Finder {
        found: bool,
    }
    impl almide_ir::visit::IrVisitor for Finder {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if is_result_opt_str(&expr.ty) {
                self.found = true;
            }
            almide_ir::visit::walk_expr(self, expr);
        }
    }
    let mut finder = Finder { found: false };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if is_result_opt_str(&f.ret_ty) || f.params.iter().any(|p| is_result_opt_str(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// Does the program create or carry FIRST-CLASS FUNCTION values (a `Lambda` expr or a
/// `Ty::Fn`-typed value anywhere)? Gates the injection of [`CLOSURE_DROP_SRC`] — a program
/// with no closures pays neither the second lowering pass nor the dead drop routine.
pub fn program_uses_closures(program: &almide_ir::IrProgram) -> bool {
    struct Finder {
        found: bool,
    }
    impl almide_ir::visit::IrVisitor for Finder {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if matches!(expr.kind, almide_ir::IrExprKind::Lambda { .. })
                || matches!(expr.ty, Ty::Fn { .. })
            {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if matches!(f.ret_ty, Ty::Fn { .. }) || f.params.iter().any(|p| matches!(p.ty, Ty::Fn { .. }))
        {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// Does the program carry a `List[<Fn>]` LITERAL anywhere (a bind/return/call-arg type) —
/// gates `LIST_CLOSURE_DROP_SRC`'s injection (a program with closures but no closure LIST
/// pays no dead drop routine, unlike the broader `program_uses_closures` gate).
pub fn program_uses_closure_list(program: &almide_ir::IrProgram) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let is_closure_list = |ty: &Ty| {
        matches!(ty, Ty::Applied(TypeConstructorId::List, a)
            if a.len() == 1 && matches!(a[0], Ty::Fn { .. }))
    };
    struct Finder<'a> {
        found: bool,
        pred: &'a dyn Fn(&Ty) -> bool,
    }
    impl almide_ir::visit::IrVisitor for Finder<'_> {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if (self.pred)(&expr.ty) {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false, pred: &is_closure_list };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if is_closure_list(&f.ret_ty) || f.params.iter().any(|p| is_closure_list(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// Does the program carry a `List[(String, <Fn>)]` anywhere — gates
/// `LIST_STR_CLO_DROP_SRC`'s injection (the closure-valued map's from_list
/// pairs-list drop), the exact sibling of [`program_uses_closure_list`]'s gate.
pub fn program_uses_str_clo_pairs(program: &almide_ir::IrProgram) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let is_pairs = |ty: &Ty| {
        matches!(ty, Ty::Applied(TypeConstructorId::List, a)
            if a.len() == 1
                && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                    && matches!(ts[0], Ty::String) && matches!(ts[1], Ty::Fn { .. })))
    };
    struct Finder<'a> {
        found: bool,
        pred: &'a dyn Fn(&Ty) -> bool,
    }
    impl almide_ir::visit::IrVisitor for Finder<'_> {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if (self.pred)(&expr.ty) {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false, pred: &is_pairs };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if is_pairs(&f.ret_ty) || f.params.iter().any(|p| is_pairs(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// Does the program carry an `Option[(String, String)]` anywhere — gates
/// `OPT_STR_STR_DROP_SRC`'s injection (the if-merged `some((s1, s2))` ctor's
/// recursive drop), the exact sibling of [`program_uses_map_closure`]'s gate.
pub fn program_uses_opt_str_str(program: &almide_ir::IrProgram) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let is_oss = |ty: &Ty| {
        matches!(ty, Ty::Applied(TypeConstructorId::Option, a)
            if a.len() == 1
                && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                    && matches!(ts[0], Ty::String) && matches!(ts[1], Ty::String)))
    };
    struct Finder<'a> {
        found: bool,
        pred: &'a dyn Fn(&Ty) -> bool,
    }
    impl almide_ir::visit::IrVisitor for Finder<'_> {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if (self.pred)(&expr.ty) {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false, pred: &is_oss };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if is_oss(&f.ret_ty) || f.params.iter().any(|p| is_oss(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// Does the program carry a `Map[String, <Fn>]` anywhere (a bind/return/call-arg type) —
/// gates `MAP_MCLO_DROP_SRC`'s injection (the closure-valued map's recursive drop), the
/// exact sibling of [`program_uses_closure_list`]'s gate.
pub fn program_uses_map_closure(program: &almide_ir::IrProgram) -> bool {
    let is_mclo = |ty: &Ty| crate::lower::is_map_fn_ty(ty);
    struct Finder<'a> {
        found: bool,
        pred: &'a dyn Fn(&Ty) -> bool,
    }
    impl almide_ir::visit::IrVisitor for Finder<'_> {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if (self.pred)(&expr.ty) {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false, pred: &is_mclo };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if is_mclo(&f.ret_ty) || f.params.iter().any(|p| is_mclo(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// Does the program carry an `Option[(String, <scalar>)]` anywhere (a bind/return/call-arg
/// or expression type) — gates `OPT_STR_INT_DROP_SRC`'s injection. This is the EXACT type
/// predicate every `variant_drop_handles = "opt_str_int"` router uses (`(String, !is_heap)`
/// tuple payload: control.rs's match subject, binds_p4_b's `some((s, n))` ctor,
/// heap_result_arm's arm-position mirror), so the routine is emitted exactly when a drop
/// can route to it. The retired `program_calls_map_find` name-heuristic only covered the
/// `map.find` PRODUCER and missed the same type built by a plain `some((s, n))` literal —
/// the fuzzer-found #840 escape, where the routed call dangled and the WAT failed to parse.
pub fn program_uses_opt_str_scalar(program: &almide_ir::IrProgram) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let is_osi = |ty: &Ty| {
        matches!(ty, Ty::Applied(TypeConstructorId::Option, a)
            if a.len() == 1
                && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                    && matches!(ts[0], Ty::String) && !crate::lower::is_heap_ty(&ts[1])))
    };
    struct Finder<'a> {
        found: bool,
        pred: &'a dyn Fn(&Ty) -> bool,
    }
    impl almide_ir::visit::IrVisitor for Finder<'_> {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if (self.pred)(&expr.ty) {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false, pred: &is_osi };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if is_osi(&f.ret_ty) || f.params.iter().any(|p| is_osi(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// The element-drop class a `List[Option/Result]` LITERAL's elements take — the SINGLE
/// classifier the injection pre-scan ([`program_uses_lenlist_elem_lists`]) and the literal
/// builder (`try_lower_record_list_literal_as`) BOTH consult, so `$__drop_list_lenlist` is
/// emitted exactly when a list routes to it (the `field_displayable` agree-by-construction
/// precedent).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CtorElemClass {
    /// The element block owns NO heap (`Option[Int/Bool/Float]` — a scalar payload at
    /// data\[0\] under len-as-tag): the flat per-element `rc_dec` (`DropListStr` via
    /// `heap_elem_lists`) frees it EXACTLY.
    Flat,
    /// The element block's first `len` slots are OWNED handles (`Option[String]` Some =
    /// len 1 + payload; `Result[scalar, String]` Ok = len 0 / Err = len 1 + message;
    /// `Result[String, String]` = the cap-as-tag 1-slot form, len 1 either way): the
    /// len-loop `$__drop_list_lenlist` frees each element's owned slots then the element.
    LenLoop,
}

/// Classify a list-literal ELEMENT type as ctor-materializable, or `None` (the caller keeps
/// the record/tuple/wall paths). Only payload types whose OWN drop is one-level-exact are
/// admitted — an `Option[<heap-field record>]` element would leak its record's fields under
/// the len-loop (its wrapper needs `DropWrapperRec`), so it stays walled.
pub fn lenlist_elem_class(elem_ty: &Ty) -> Option<CtorElemClass> {
    use almide_lang::types::constructor::TypeConstructorId;
    // A one-level-exact HEAP payload: freeing it with ONE rc_dec is exact (no owned interior).
    let flat_heap = |t: &Ty| {
        matches!(t, Ty::String)
            || matches!(t, Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && !is_heap_ty(&a[0]))
    };
    match elem_ty {
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
            if !is_heap_ty(&a[0]) {
                Some(CtorElemClass::Flat)
            } else if flat_heap(&a[0]) {
                Some(CtorElemClass::LenLoop)
            } else {
                None
            }
        }
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
            let ok_admits = !is_heap_ty(&a[0]) || flat_heap(&a[0]);
            let err_admits = flat_heap(&a[1]);
            if ok_admits && err_admits {
                Some(CtorElemClass::LenLoop)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Is `ty` a `List` whose ELEMENT type routes to the len-loop drop ([`lenlist_elem_class`]
/// = `LenLoop`) — the TYPE-driven registration the call-result / merged-bind sites consult
/// (a value of this type must free via `$__drop_list_lenlist`, never the flat
/// `heap_elem_lists` `DropListStr` that would leak each element's owned slots).
pub fn is_lenlist_list_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::List, a)
        if a.len() == 1 && lenlist_elem_class(&a[0]) == Some(CtorElemClass::LenLoop))
}

/// Does the program CARRY a len-loop list type anywhere (a literal, a call result, a
/// param/return — any expression's type)? Gates the injection of [`LENLIST_DROP_SRC`] — a
/// program never touching such a type pays no dead drop routine. (A `Flat` element list
/// reuses `DropListStr` and needs no generated source.) Type-based (not literal-based) so a
/// CALLER that only binds a callee's returned list still gets the drop routine linked.
pub fn program_uses_lenlist_elem_lists(program: &almide_ir::IrProgram) -> bool {
    struct Finder {
        found: bool,
    }
    impl almide_ir::visit::IrVisitor for Finder {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if is_lenlist_list_ty(&expr.ty) {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if is_lenlist_list_ty(&f.ret_ty) || f.params.iter().any(|p| is_lenlist_list_ty(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}
