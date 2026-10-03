// Statement-list normalizations, run once per fn after the block-argument hoist
// (`hoist_block_call_args`) in the shared desugar chain: each rewrites a statement
// into the spelling the lowering already executes. Split out of desugar_guard.rs
// (max-lines). `include!`d from mod_c_tail.rs.

/// Apply the statement-list normalizations below to every block's statements in
/// `body`, innermost first.
pub(crate) fn normalize_stmt_lists(body: &mut IrExpr, vt: &mut almide_ir::VarTable) {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct N<'a> {
        vt: &'a mut almide_ir::VarTable,
    }
    impl IrMutVisitor for N<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Block { stmts, .. } = &mut e.kind else { return };
            bind_nested_list_literal_elems(stmts, self.vt);
        }
    }
    N { vt }.visit_expr_mut(body);
}

/// `let t = [[[1, 2]], [[3]]]`  ≡  `let e0 = [[1, 2]]; let e1 = [[3]]; let t = [e0, e1]` —
/// a bound list literal whose elements are themselves list literals OF heap elements (a
/// three-level `List[List[List[Int]]]` tower) binds each such element to a fresh `let`
/// first, in order, so the outer literal holds plain variables: the shape the list
/// builder materializes (a `Dup` per slot, the nested sweep as its drop), which the
/// nested literal itself is not (#3084). Only when EVERY element is such a literal or a
/// variable or a literal, so no element with effects is reordered past another. A
/// hoisted element that is itself such a tower is split again on the next pass of the
/// loop. Two-level literals (`[[1, 2], []]`, elements of a scalar list type) stay as
/// they are.
fn bind_nested_list_literal_elems(stmts: &mut Vec<almide_ir::IrStmt>, vt: &mut almide_ir::VarTable) {
    use almide_ir::{IrStmt, IrStmtKind, Mutability};
    use almide_lang::types::constructor::TypeConstructorId;
    let nested = |e: &IrExpr| {
        matches!(&e.kind, IrExprKind::List { elements } if !elements.is_empty())
            && matches!(&e.ty, Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && crate::lower::is_heap_ty(&a[0]))
    };
    let plain = |e: &IrExpr| {
        matches!(
            e.kind,
            IrExprKind::Var { .. }
                | IrExprKind::LitInt { .. }
                | IrExprKind::LitFloat { .. }
                | IrExprKind::LitBool { .. }
                | IrExprKind::LitStr { .. }
        )
    };
    let mut i = 0;
    while i < stmts.len() {
        let IrStmtKind::Bind { value, .. } = &mut stmts[i].kind else {
            i += 1;
            continue;
        };
        let IrExprKind::List { elements } = &mut value.kind else {
            i += 1;
            continue;
        };
        if !elements.iter().any(|e| nested(e)) || !elements.iter().all(|e| nested(e) || plain(e)) {
            i += 1;
            continue;
        }
        let mut binds = Vec::new();
        for el in elements.iter_mut().filter(|e| nested(e)) {
            let ty = el.ty.clone();
            let span = el.span.clone();
            let var = vt.alloc(almide_base::intern::sym("__elem"), ty.clone(), Mutability::Let, None);
            let value = std::mem::replace(
                el,
                IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: span.clone(), def_id: None },
            );
            binds.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span });
        }
        // Re-examine from the first inserted bind: a hoisted element may be a tower too.
        stmts.splice(i..i, binds);
    }
}
