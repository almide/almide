//! Native storage for the closed, Almide-owned Region bridge.
//!
//! A Region's internal address becomes a closure over a shared Bytes cell.
//! Its copies share that cell, so a slice observes writes through its parent
//! and outlives the parent. The input Bytes remains a value snapshot. The
//! public bounds/permission checks stay in the checked ffi.almd bodies.
//! The generated closure and cell use the ordinary MIR ownership rules;
//! no raw address, foreign lifetime, or new writable-borrow rule is asserted.
//! Foreign addresses and exported ABI boundaries remain outside this pass:
//! a program whose live code can pass an address across `@extern`/`@export`
//! is refused (see `closed`).
use almide_ir::{visit_mut::{walk_expr_mut, walk_stmt_mut, walk_pattern_mut}, *};
use almide_lang::{intern::sym, types::Ty};
use crate::lower::LowerError;
use std::collections::HashSet;

fn memory_ty() -> Ty {
    Ty::Fn { params: vec![Ty::Int, Ty::Bytes], ret: Box::new(Ty::Bytes), is_effect: false }
}
fn physical(t: &Ty) -> Ty {
    if *t == Ty::RawPtr { memory_ty() } else { t.map_children(&physical) }
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
    let mut gate = Gate::over(ir);
    if !gate.mint { return Ok(()); }
    let reach = AddressReach::new(&ir.type_decls);
    if !closed(ir, &gate, &reach) {
        // A package brings its whole module set, so a program can hold host
        // bindings over Regions that none of its roots ever calls. Code no
        // root reaches never runs: drop the unreached functions that lead to
        // an address, then judge the program that remains.
        let span = gate.address_span;
        if drop_unreached_address_code(ir, &reach) {
            gate = Gate::over(ir);
            if !gate.mint { return Ok(()); }
        }
        if !closed(ir, &gate, &reach) {
            return Err(LowerError::at(gate.address_span.or(span),
                "native: Region storage needs a closed Almide-owned address domain"));
        }
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
fn is_boundary(f: &IrFunction) -> bool { !f.extern_attrs.is_empty() || !f.export_attrs.is_empty() }
fn signature_carries(f: &IrFunction, reach: &AddressReach) -> bool {
    reach.carries(&f.ret_ty) || f.params.iter().any(|p| reach.carries(&p.ty))
}

/// Drop every function that no root reaches and that leads to an address
/// (it holds one, or calls something that does). The roots are the ones the
/// link's own pruning keeps beyond the entry module (`pipeline_native_bundled::
/// retain_reached`): `main`, tests and their harness entries, functions the
/// package's native code calls, C exports, any `@export`, and whatever a
/// top-level binding or a declared default references. A kept function never
/// calls a dropped one (a caller of a function that leads to an address leads
/// to one too). Returns whether anything was dropped.
fn drop_unreached_address_code(ir: &mut IrProgram, reach: &AddressReach) -> bool {
    let names: Vec<&str> = ir.functions.iter().map(|f| f.name.as_str()).collect();
    let resolve = |r: &Ref| -> Vec<usize> {
        names.iter().enumerate().filter(|(_, n)| match r {
            Ref::Exact(r) => *n == r,
            // A qualified or method reference left after the link may name a
            // body by its bare or qualified spelling: keep every candidate.
            Ref::Member(r) => **n == r || n.rsplit('.').next() == r.rsplit('.').next(),
        }).map(|(i, _)| i).collect()
    };
    let callees: Vec<Vec<usize>> = ir.functions.iter().map(|f| {
        let mut refs = Refs::default();
        refs.visit_expr(&f.body);
        for p in &f.params { if let Some(d) = &p.default { refs.visit_expr(d); } }
        refs.0.iter().flat_map(&resolve).collect()
    }).collect();
    let mut reached = vec![false; names.len()];
    let mut pending: Vec<usize> = ir.functions.iter().enumerate()
        .filter(|(_, f)| f.is_test || f.name.as_str() == "main" || !f.export_attrs.is_empty()
            || crate::native_test_harness::is_entry(f.name.as_str())
            || crate::native_callbacks::is_root(f.name.as_str())
            || crate::native_foreign::is_c_export(f))
        .map(|(i, _)| i).collect();
    let mut top = Refs::default();
    for t in &ir.top_lets { top.visit_expr(&t.value); }
    for d in &ir.type_decls {
        let fields: Vec<&IrFieldDecl> = match &d.kind {
            IrTypeDeclKind::Record { fields } => fields.iter().collect(),
            IrTypeDeclKind::Variant { cases, .. } => cases.iter().flat_map(|c| match &c.kind {
                IrVariantKind::Record { fields } => fields.iter().collect(),
                _ => Vec::new(),
            }).collect(),
            IrTypeDeclKind::Alias { .. } => Vec::new(),
        };
        for f in fields { if let Some(e) = &f.default { top.visit_expr(e); } }
    }
    pending.extend(top.0.iter().flat_map(&resolve));
    while let Some(i) = pending.pop() {
        if !std::mem::replace(&mut reached[i], true) { pending.extend(callees[i].iter().copied()); }
    }
    let mut leads: Vec<bool> = ir.functions.iter().map(|f| holds_address(f, reach)).collect();
    loop {
        let mut changed = false;
        for i in 0..leads.len() {
            if !leads[i] && callees[i].iter().any(|&c| leads[c]) { leads[i] = true; changed = true; }
        }
        if !changed { break; }
    }
    let before = ir.functions.len();
    let mut keep = (0..before).map(|i| reached[i] || !leads[i]);
    ir.functions.retain(|_| keep.next().unwrap_or(true));
    ir.functions.len() != before
}

/// A function holds an address when its signature, a host-boundary
/// signature it declares, or any expression or binder it evaluates has a
/// type that holds `Handle` (directly or through a declaration).
fn holds_address(f: &IrFunction, reach: &AddressReach) -> bool {
    struct Holds<'a> { reach: &'a AddressReach, found: bool }
    impl IrVisitor for Holds<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found { return; }
            self.found = self.reach.holds(&e.ty);
            almide_ir::walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { ty, .. } = &s.kind { self.found |= self.reach.holds(ty); }
            almide_ir::walk_stmt(self, s);
        }
    }
    if (is_boundary(f) && signature_carries(f, reach)) || reach.holds(&f.ret_ty)
        || f.params.iter().any(|p| reach.holds(&p.ty)) { return true; }
    let mut h = Holds { reach, found: false };
    h.visit_expr(&f.body);
    h.found
}

enum Ref { Exact(String), Member(String) }
#[derive(Default)]
struct Refs(Vec<Ref>);
impl IrVisitor for Refs {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Call { target, .. } | IrExprKind::TailCall { target, .. } => match target {
                CallTarget::Named { name } => self.0.push(Ref::Exact(name.as_str().to_string())),
                CallTarget::Module { module, func, .. } => self.0.push(Ref::Member(format!("{module}.{func}"))),
                CallTarget::Method { method, .. } => self.0.push(Ref::Member(method.as_str().to_string())),
                CallTarget::Computed { .. } => {},
            },
            IrExprKind::FnRef { name } | IrExprKind::ClosureCreate { func_name: name, .. } =>
                self.0.push(Ref::Exact(name.as_str().to_string())),
            _ => {},
        }
        almide_ir::walk_expr(self, e);
    }
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
    /// Strictly holds one: `Handle`, or a declaration known to carry one.
    fn holds(&self, t: &Ty) -> bool {
        t.any_node(&mut |t| match t {
            Ty::RawPtr => true,
            Ty::Named(n, _) => self.carrying.contains(n.as_str()),
            _ => false,
        })
    }
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
