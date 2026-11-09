//! Native storage for the closed, Almide-owned Region bridge.
//!
//! A Region's internal address becomes a closure over a shared Bytes cell.
//! Its copies share that cell, so a slice observes writes through its parent
//! and outlives the parent. The input Bytes remains a value snapshot. The
//! public bounds/permission checks stay in the checked ffi.almd bodies.
//! The generated closure and cell use the ordinary MIR ownership rules;
//! no raw address, foreign lifetime, or new writable-borrow rule is asserted.
//! Foreign addresses and exported ABI boundaries remain outside this pass.
use almide_ir::{visit_mut::{walk_expr_mut, walk_stmt_mut, walk_pattern_mut}, *};
use almide_lang::{intern::sym, types::Ty};
use crate::lower::LowerError;

fn memory_ty() -> Ty {
    Ty::Fn { params: vec![Ty::Int, Ty::Bytes], ret: Box::new(Ty::Bytes), is_effect: false }
}
fn physical(t: &Ty) -> Ty {
    if *t == Ty::RawPtr { memory_ty() } else { t.map_children(&physical) }
}
fn has_address(t: &Ty) -> bool { t.any_node(&mut |t| *t == Ty::RawPtr) }

pub(crate) fn expand(ir: &mut IrProgram) -> Result<(), LowerError> {
    let mut gate = Gate::default();
    for f in &ir.functions { gate.visit_expr(&f.body); }
    for t in &ir.top_lets { gate.visit_expr(&t.value); }
    if !gate.mint { return Ok(()); }
    if gate.foreign || almide_ir::meter_plan::program_has_regions(ir) || ir.functions.iter().any(|f|
        !f.extern_attrs.is_empty() || !f.export_attrs.is_empty()) {
        return Err(LowerError::Unsupported("native: Region storage needs a closed Almide-owned address domain".into()));
    }
    // Map every annotation together: the checked logical Handle becomes a
    // physical closure, including records, signatures and local binders.
    for info in &mut ir.var_table.entries { info.ty = physical(&info.ty); }
    let mut pass = Storage { vars: &mut ir.var_table };
    for f in &mut ir.functions {
        f.ret_ty = physical(&f.ret_ty);
        for p in &mut f.params {
            p.ty = physical(&p.ty);
            if let Some(e) = &mut p.default { pass.visit_expr_mut(e); }
        }
        pass.visit_expr_mut(&mut f.body);
    }
    for t in &mut ir.top_lets { t.ty = physical(&t.ty); pass.visit_expr_mut(&mut t.value); }
    for d in &mut ir.type_decls {
        match &mut d.kind {
            IrTypeDeclKind::Record { fields } => pass.fields(fields),
            IrTypeDeclKind::Alias { target } => *target = physical(target),
            IrTypeDeclKind::Variant { cases, .. } => for c in cases {
                match &mut c.kind {
                    IrVariantKind::Unit => {},
                    IrVariantKind::Tuple { fields } => for t in fields { *t = physical(t); },
                    IrVariantKind::Record { fields } => pass.fields(fields),
                }
            },
        }
    }
    Ok(())
}

#[derive(Default)]
struct Gate { mint: bool, foreign: bool }
impl IrVisitor for Gate {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let raw = module.as_str() == "ffi" && matches!(func.as_str(), "__address" | "__load" | "__store");
                self.mint |= raw && func.as_str() == "__address";
                // A host call that consumes or creates any address-bearing
                // value cannot use the private cell representation.
                self.foreign |= !raw && (has_address(&e.ty) || args.iter().any(|a| has_address(&a.ty)));
            }
            IrExprKind::RuntimeCall { .. } | IrExprKind::InlineRust { .. }
            | IrExprKind::RenderedCall { .. } => self.foreign = true,
            _ => {},
        }
        almide_ir::walk_expr(self, e);
    }
}

struct Storage<'a> { vars: &'a mut VarTable }
impl Storage<'_> {
    fn fields(&mut self, fields: &mut [IrFieldDecl]) {
        for f in fields { f.ty = physical(&f.ty); if let Some(e) = &mut f.default { self.visit_expr_mut(e); } }
    }
    fn fresh(&mut self, name: &str, ty: Ty, mutable: bool) -> VarId {
        self.vars.alloc_fresh(name, ty, if mutable { Mutability::Var } else { Mutability::Let }, None)
    }
    fn bind(&mut self, value: IrExpr, stmts: &mut Vec<IrStmt>) -> IrExpr {
        let v = self.fresh("region_operand", value.ty.clone(), false);
        let out = read(v, value.ty.clone());
        stmts.push(bind(v, value, Mutability::Let)); out
    }
    fn address(&mut self, value: IrExpr) -> IrExpr {
        let storage = self.fresh("region_storage", Ty::Bytes, true);
        let offset = self.fresh("region_offset", Ty::Int, false);
        let data = self.fresh("region_data", Ty::Bytes, false);
        let buffer = read(storage, Ty::Bytes);
        let bytes = read(data, Ty::Bytes);
        let off = read(offset, Ty::Int);
        let write = bytes_call("copy_from", vec![buffer.clone(), bytes.clone(), off.clone(), int(0),
            bytes_call("len", vec![bytes], Ty::Int)], Ty::Unit);
        let conditional = node(IrExprKind::If {
            cond: Box::new(binary(BinOp::Gte, off, int(0), Ty::Bool)),
            then: Box::new(write), else_: Box::new(node(IrExprKind::Unit, Ty::Unit)),
        }, Ty::Unit);
        let body = block(vec![stmt(IrStmtKind::Expr { expr: conditional })], buffer);
        let closure = node(IrExprKind::Lambda { params: vec![(offset, Ty::Int), (data, Ty::Bytes)],
            body: Box::new(body), lambda_id: None }, memory_ty());
        block(vec![bind(storage, value, Mutability::Var)], closure)
    }
    fn access(&mut self, args: &[IrExpr], write: bool) -> IrExpr {
        let mut stmts = Vec::new();
        // Snapshot operands before invoking the address; each runs once in
        // source order, even if a later operand changes an earlier value.
        let args: Vec<_> = args.iter().map(|a| self.bind(a.clone(), &mut stmts)).collect();
        let (off, data) = if write { (args[1].clone(), args[2].clone()) }
            else { (int(-1), bytes_call("zeros", vec![int(0)], Ty::Bytes)) };
        let access = node(IrExprKind::Call { target: CallTarget::Computed { callee: Box::new(args[0].clone()) },
            args: vec![off, data], type_args: vec![] }, Ty::Bytes);
        let tail = if write {
            stmts.push(stmt(IrStmtKind::Expr { expr: access }));
            node(IrExprKind::Unit, Ty::Unit)
        } else {
            bytes_call("slice", vec![access, args[1].clone(),
                binary(BinOp::AddInt, args[1].clone(), args[2].clone(), Ty::Int)], Ty::Bytes)
        };
        block(stmts, tail)
    }
}
impl IrMutVisitor for Storage<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        e.ty = physical(&e.ty);
        match &mut e.kind {
            IrExprKind::Call { type_args, .. } => for t in type_args { *t = physical(t); },
            IrExprKind::Lambda { params, .. } => for (_, t) in params { *t = physical(t); },
            _ => {},
        }
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind else { return; };
        if module.as_str() != "ffi" { return; }
        let replacement = match (func.as_str(), args.as_slice()) {
            ("__address", [b]) => self.address(b.clone()),
            ("__load", [_, _, _]) => self.access(args, false),
            ("__store", [_, _, _]) => self.access(args, true),
            _ => return,
        };
        *e = IrExpr { span: e.span, ..replacement };
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        walk_stmt_mut(self, s);
        if let IrStmtKind::Bind { ty, .. } = &mut s.kind { *ty = physical(ty); }
    }
    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        walk_pattern_mut(self, p);
        if let IrPattern::Bind { ty, .. } = p { *ty = physical(ty); }
    }
}
fn node(kind: IrExprKind, ty: Ty) -> IrExpr { IrExpr { kind, ty, span: None, def_id: None } }
fn stmt(kind: IrStmtKind) -> IrStmt { IrStmt { kind, span: None } }
fn read(id: VarId, ty: Ty) -> IrExpr { node(IrExprKind::Var { id }, ty) }
fn int(value: i64) -> IrExpr { node(IrExprKind::LitInt { value }, Ty::Int) }
fn bind(var: VarId, value: IrExpr, mutability: Mutability) -> IrStmt {
    stmt(IrStmtKind::Bind { var, ty: value.ty.clone(), value, mutability })
}
fn block(stmts: Vec<IrStmt>, expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone(); node(IrExprKind::Block { stmts, expr: Some(Box::new(expr)) }, ty)
}
fn binary(op: BinOp, left: IrExpr, right: IrExpr, ty: Ty) -> IrExpr {
    node(IrExprKind::BinOp { op, left: Box::new(left), right: Box::new(right) }, ty)
}
fn bytes_call(func: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
    node(IrExprKind::Call { target: CallTarget::Module { module: sym("bytes"), func: sym(func), def_id: None },
        args, type_args: vec![] }, ty)
}
