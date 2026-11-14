//! Native storage for the Region bridge.
//!
//! A v1 Bytes is an owned value that a record copy clones, so a Region's
//! address cannot be the address of its Bytes: a copy would keep the old
//! address while owning another buffer. The address becomes storage that
//! every copy of the Region shares instead, in one of two forms:
//!
//! - **Closed** (no address can cross a host boundary): a closure over a
//!   shared Bytes cell. It uses the ordinary MIR ownership rules; no raw
//!   address, foreign lifetime, or new writable-borrow rule is asserted.
//! - **Lent** (a Region is passed to an `@extern(rust)` fn): the runtime's
//!   `AlmideFfiBlock`, one fixed-size `Rc` allocation, an ordinary counted
//!   value. The foreign call gets `(address + off, len)` of the block for
//!   the call (`native_foreign.rs`); that address is the one trusted step.
//!
//! In both forms a slice observes writes through its parent and outlives the
//! parent, and the input Bytes remains a value snapshot. The public bounds
//! and permission checks stay in the checked ffi.almd bodies. Any other way
//! an address could cross a boundary (an `@export`, a C binding, a Region
//! inside another value, a returned or callback Region, a host call that
//! takes the address itself) is refused (see `lent`).
use almide_ir::{visit_mut::{walk_expr_mut, walk_stmt_mut, walk_pattern_mut}, *};
use almide_lang::{intern::sym, types::Ty};
use crate::lower::LowerError;
use std::collections::HashSet;

fn memory_ty() -> Ty {
    Ty::Fn { params: vec![Ty::Int, Ty::Bytes], ret: Box::new(Ty::Bytes), is_effect: false }
}
#[derive(Clone, Copy, PartialEq)]
enum Form { Closed, Lent }
fn physical(t: &Ty, form: Form) -> Ty {
    match (t, form) {
        (Ty::RawPtr, Form::Closed) => memory_ty(),
        (Ty::RawPtr, Form::Lent) => Ty::Named(sym(crate::native_runtime_types::FFI_BLOCK.0), vec![].into()),
        _ => t.map_children(&|c| physical(c, form)),
    }
}
fn has_address(t: &Ty) -> bool { t.any_node(&mut |t| *t == Ty::RawPtr) }

enum RegionOp { Address, Load, Store }

// The gate and the rewrite share one dispatch table. An operation is inside
// the closed domain exactly when this pass can translate that call's shape.
fn region_operation(name: &str, arity: usize) -> Option<RegionOp> {
    match (name, arity) {
        ("__address", 1) => Some(RegionOp::Address),
        ("__load", 3) => Some(RegionOp::Load),
        ("__store", 3) => Some(RegionOp::Store),
        _ => None,
    }
}

pub(crate) fn expand(ir: &mut IrProgram) -> Result<(), LowerError> {
    let gate = Gate::over(ir);
    if !gate.mint { return Ok(()); }
    let reach = AddressReach::new(&ir.type_decls);
    let form = if closed(ir, &gate, &reach) { Form::Closed }
        else if lent(ir, &gate, &reach) { Form::Lent }
        else {
            return Err(LowerError::at(gate.address_span,
                "native: Region storage needs a closed Almide-owned address domain"));
        };
    // Map every annotation together: the checked logical Handle becomes a
    // physical closure, including records, signatures and local binders.
    for info in &mut ir.var_table.entries { info.ty = physical(&info.ty, form); }
    let mut pass = Storage { vars: &mut ir.var_table, form };
    for f in &mut ir.functions {
        f.ret_ty = physical(&f.ret_ty, form);
        for p in &mut f.params {
            p.ty = physical(&p.ty, form);
            if let Some(e) = &mut p.default { pass.visit_expr_mut(e); }
        }
        pass.visit_expr_mut(&mut f.body);
    }
    for t in &mut ir.top_lets { t.ty = physical(&t.ty, form); pass.visit_expr_mut(&mut t.value); }
    for d in &mut ir.type_decls {
        match &mut d.kind {
            IrTypeDeclKind::Record { fields } => pass.fields(fields),
            IrTypeDeclKind::Alias { target } => *target = physical(target, form),
            IrTypeDeclKind::Variant { cases, .. } => for c in cases {
                match &mut c.kind {
                    IrVariantKind::Unit => {},
                    IrVariantKind::Tuple { fields } => for t in fields { *t = physical(t, form); },
                    IrVariantKind::Record { fields } => pass.fields(fields),
                }
            },
        }
    }
    Ok(())
}

/// The domain is closed when no address can cross a host boundary: no host
/// call consumes or creates one, the program is not metered, and no
/// `@extern`/`@export` signature can carry one (through named declarations,
/// containers, closures or a type parameter). A boundary fn over plain data
/// never receives, returns or mints an address, so it does not open the
/// domain.
fn closed(ir: &IrProgram, gate: &Gate, reach: &AddressReach) -> bool {
    !gate.foreign && !almide_ir::meter_plan::program_has_regions(ir)
        && !ir.functions.iter().any(|f| is_boundary(f) && signature_carries(f, reach))
}
/// The domain is lent when every boundary that can carry an address is an
/// `@extern(rust)` fn taking a Region directly as a by-value parameter (its
/// `(ptr, len)` pair, the shape v0 and wasm hand their hosts), and no host
/// call takes or makes an address itself. Nothing returns a Region, holds one
/// inside another value, or reaches one through a callback or a type
/// parameter, so foreign code sees the block only for the call.
fn lent(ir: &IrProgram, gate: &Gate, reach: &AddressReach) -> bool {
    use almide_lang::types::extern_abi::{is_region, NativeExtern};
    let lends = |f: &IrFunction| f.export_attrs.is_empty()
        && matches!(crate::native_foreign::selected(f), Some((NativeExtern::Rust, _)))
        && !reach.carries(&f.ret_ty)
        && f.params.iter().all(|p| !reach.carries(&p.ty) || (is_region(&p.ty) && !p.is_mut));
    !gate.foreign && !almide_ir::meter_plan::program_has_regions(ir)
        && ir.functions.iter().all(|f| !is_boundary(f) || !signature_carries(f, reach) || lends(f))
}
fn is_boundary(f: &IrFunction) -> bool { !f.extern_attrs.is_empty() || !f.export_attrs.is_empty() }
fn signature_carries(f: &IrFunction, reach: &AddressReach) -> bool {
    reach.carries(&f.ret_ty) || f.params.iter().any(|p| reach.carries(&p.ty))
}

/// Which types can hold an address: `Handle` itself, or anything whose
/// structure reaches one, following named declarations transitively. A type
/// parameter, or a name with no declaration in the program, counts as
/// reaching one: an unresolved shape is refused, never assumed closed.
struct AddressReach { carrying: HashSet<String>, declared: HashSet<String> }
impl AddressReach {
    fn new(decls: &[IrTypeDecl]) -> Self {
        fn members(d: &IrTypeDecl) -> Vec<&Ty> {
            match &d.kind {
                IrTypeDeclKind::Record { fields } => fields.iter().map(|f| &f.ty).collect(),
                IrTypeDeclKind::Alias { target } => vec![target],
                IrTypeDeclKind::Variant { cases, .. } => cases.iter().flat_map(|c| match &c.kind {
                    IrVariantKind::Unit => vec![],
                    IrVariantKind::Tuple { fields } => fields.iter().collect(),
                    IrVariantKind::Record { fields } => fields.iter().map(|f| &f.ty).collect(),
                }).collect(),
            }
        }
        let declared = decls.iter().map(|d| d.name.as_str().to_string()).collect();
        let mut reach = AddressReach { carrying: HashSet::new(), declared };
        // A declaration's own type parameters are filled at each use, where
        // `carries` inspects the arguments; inside the body they are not
        // addresses. Iterate to a fixpoint for mutually recursive types.
        loop {
            let before = reach.carrying.len();
            for d in decls {
                let own: Vec<&str> = d.generics.iter().flatten().map(|g| g.name.as_str()).collect();
                if !reach.carrying.contains(d.name.as_str()) && members(d).iter().any(|t| reach.reaches(t, &own)) {
                    reach.carrying.insert(d.name.as_str().to_string());
                }
            }
            if reach.carrying.len() == before { return reach; }
        }
    }
    fn reaches(&self, t: &Ty, bound: &[&str]) -> bool {
        t.any_node(&mut |t| match t {
            Ty::RawPtr => true,
            Ty::TypeVar(v) => !bound.contains(&v.as_str()),
            Ty::Named(n, _) => !self.declared.contains(n.as_str()) || self.carrying.contains(n.as_str()),
            _ => false,
        })
    }
    fn carries(&self, t: &Ty) -> bool { self.reaches(t, &[]) }
}

#[derive(Default)]
struct Gate { mint: bool, foreign: bool, address_span: Option<almide_ir::Span> }
impl Gate {
    fn over(ir: &IrProgram) -> Self {
        let mut gate = Gate::default();
        for f in &ir.functions { gate.visit_expr(&f.body); }
        for t in &ir.top_lets { gate.visit_expr(&t.value); }
        gate
    }
}
impl IrVisitor for Gate {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let operation = (module.as_str() == "ffi").then(|| region_operation(func.as_str(), args.len())).flatten();
                let mint = matches!(operation, Some(RegionOp::Address));
                self.mint |= mint;
                if mint && self.address_span.is_none() {
                    self.address_span = e.span;
                }
                // A host call that consumes or creates any address-bearing
                // value cannot use the private cell representation.
                self.foreign |= operation.is_none() && (has_address(&e.ty) || args.iter().any(|a| has_address(&a.ty)));
            }
            IrExprKind::RuntimeCall { .. } | IrExprKind::InlineRust { .. }
            | IrExprKind::RenderedCall { .. } => self.foreign = true,
            _ => {},
        }
        almide_ir::walk_expr(self, e);
    }
}

struct Storage<'a> { vars: &'a mut VarTable, form: Form }
impl Storage<'_> {
    fn fields(&mut self, fields: &mut [IrFieldDecl]) {
        for f in fields { f.ty = physical(&f.ty, self.form); if let Some(e) = &mut f.default { self.visit_expr_mut(e); } }
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
        let form = self.form;
        e.ty = physical(&e.ty, form);
        match &mut e.kind {
            IrExprKind::Call { type_args, .. } => for t in type_args { *t = physical(t, form); },
            IrExprKind::Lambda { params, .. } => for (_, t) in params { *t = physical(t, form); },
            _ => {},
        }
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind else { return; };
        if module.as_str() != "ffi" { return; }
        if form == Form::Lent {
            let Some(operation) = region_operation(func.as_str(), args.len()) else { return };
            *e = IrExpr { span: e.span, ..block_operation(operation, args.clone(), e.ty.clone()) };
            return;
        }
        let replacement = match region_operation(func.as_str(), args.len()) {
            Some(RegionOp::Address) => self.address(args[0].clone()),
            Some(RegionOp::Load) => self.access(args, false),
            Some(RegionOp::Store) => self.access(args, true),
            None => return,
        };
        *e = IrExpr { span: e.span, ..replacement };
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        walk_stmt_mut(self, s);
        if let IrStmtKind::Bind { ty, .. } = &mut s.kind { *ty = physical(ty, self.form); }
    }
    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        walk_pattern_mut(self, p);
        if let IrPattern::Bind { ty, .. } = p { *ty = physical(ty, self.form); }
    }
}
/// A Region operation over the lent block: the same operands, renamed to the
/// block forms ffi.almd declares. The store's Result is always `ok`; it is
/// unwrapped in place, inside the checked `ffi.write` that reached it.
fn block_operation(operation: RegionOp, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
    let call = |func: &str, ty: Ty| node(IrExprKind::Call {
        target: CallTarget::Module { module: sym("ffi"), func: sym(func), def_id: None },
        args, type_args: vec![] }, ty);
    match operation {
        RegionOp::Address => call("__block", ty),
        RegionOp::Load => call("__block_load", ty),
        RegionOp::Store => {
            let result = Ty::Applied(almide_lang::types::TypeConstructorId::Result, vec![Ty::Unit, Ty::String].into());
            node(IrExprKind::Unwrap { expr: Box::new(call("__block_store", result)) }, ty)
        }
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
