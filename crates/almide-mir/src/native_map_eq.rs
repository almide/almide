//! Map equality with dynamic Value payloads, expressed in ordinary typed IR.
//! Map keys compare independently of insertion order; Value objects retain
//! their existing order-sensitive deep equality. No Rust Eq assumption is
//! made about Value (in particular, NaN is not reflexive).
use almide_ir::{visit_mut::{walk_expr_mut, IrMutVisitor}, *};
use almide_lang::{intern::sym, types::{constructor::TypeConstructorId as TC, Ty}};

pub(crate) fn expand(ir: &mut IrProgram) {
    let mut pass = MapEquality { vars: &mut ir.var_table };
    for f in &mut ir.functions { pass.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { pass.visit_expr_mut(&mut t.value); }
}

struct MapEquality<'a> { vars: &'a mut VarTable }
impl IrMutVisitor for MapEquality<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let IrExprKind::BinOp { op, left, right } = &e.kind else { return; };
        if !matches!(op, BinOp::Eq | BinOp::Neq) || left.ty != right.ty { return; }
        let Ty::Applied(TC::Map, args) = &left.ty else { return; };
        if args.len() != 2 || !(crate::lower::is_value_ty(&args[1]) || args.contains(&Ty::Float32)) { return; }
        let mut result = self.compare(left, right, &args[0], &args[1]);
        if *op == BinOp::Neq { result = eq(result, boolean(false)); }
        result.span = e.span;
        *e = result;
    }
}

impl MapEquality<'_> {
    fn compare(&mut self, left: &IrExpr, right: &IrExpr, key: &Ty, value: &Ty) -> IrExpr {
        let ty = &left.ty;
        let pair = Ty::Tuple(vec![key.clone(), value.clone()].into());
        let a = self.vars.alloc_fresh("map_eq_left", ty.clone(), Mutability::Let, None);
        let b = self.vars.alloc_fresh("map_eq_right", ty.clone(), Mutability::Let, None);
        let equal = self.vars.alloc_fresh("map_eq_equal", Ty::Bool, Mutability::Var, None);
        let entry = self.vars.alloc_fresh("map_eq_entry", pair.clone(), Mutability::Let, None);
        let found = self.vars.alloc_fresh("map_eq_found", value.clone(), Mutability::Let, None);
        let field = |index, ty: &Ty| node(IrExprKind::TupleIndex {
            object: Box::new(var(entry, &pair)), index,
        }, ty.clone());
        let lookup = call("get", vec![var(b, ty), field(0, key)], Ty::option(value.clone()));
        let compare = node(IrExprKind::Match { subject: Box::new(lookup), arms: vec![
            IrMatchArm { pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind {
                var: found, ty: value.clone(),
            }) }, guard: None, body: eq(field(1, value), var(found, value)) },
            IrMatchArm { pattern: IrPattern::None, guard: None, body: boolean(false) },
        ] }, Ty::Bool);
        let next = node(IrExprKind::If { cond: Box::new(var(equal, &Ty::Bool)),
            then: Box::new(compare), else_: Box::new(boolean(false)),
        }, Ty::Bool);
        let iter = node(IrExprKind::ForIn { var: entry, var_tuple: None,
            iterable: Box::new(call("to_list", vec![var(a, ty)], Ty::list(pair.clone()))),
            body: vec![stmt(IrStmtKind::Assign { var: equal, value: next })],
        }, Ty::Unit);
        let size_equal = eq(call("len", vec![var(a, ty)], Ty::Int),
            call("len", vec![var(b, ty)], Ty::Int));
        node(IrExprKind::Block { stmts: vec![
            bind(a, left.clone(), Mutability::Let),
            bind(b, right.clone(), Mutability::Let),
            bind(equal, size_equal, Mutability::Var),
            stmt(IrStmtKind::Expr { expr: iter }),
        ], expr: Some(Box::new(var(equal, &Ty::Bool))) }, Ty::Bool)
    }
}

fn node(kind: IrExprKind, ty: Ty) -> IrExpr { IrExpr { kind, ty, span: None, def_id: None } }
fn var(id: VarId, ty: &Ty) -> IrExpr { node(IrExprKind::Var { id }, ty.clone()) }
fn boolean(value: bool) -> IrExpr { node(IrExprKind::LitBool { value }, Ty::Bool) }
fn stmt(kind: IrStmtKind) -> IrStmt { IrStmt { kind, span: None } }
fn bind(var: VarId, value: IrExpr, mutability: Mutability) -> IrStmt {
    stmt(IrStmtKind::Bind { var, mutability, ty: value.ty.clone(), value })
}
fn eq(left: IrExpr, right: IrExpr) -> IrExpr {
    node(IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(left), right: Box::new(right) }, Ty::Bool)
}
fn call(func: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
    node(IrExprKind::Call { target: CallTarget::Module {
        module: sym("map"), func: sym(func), def_id: None,
    }, args, type_args: Vec::new() }, ty)
}
