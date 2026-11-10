//! Keep the raw `panic` builtin's identity through ordinary MIR checking.
//! A test build unwinds it, including calls from ordinary helpers; an abort
//! probe child terminates instead. A typed identity helper marks the message;
//! the actual Die stays in the caller, including at value-producing joins.
//! Neither message text nor a user's function spelling grants this behavior.
use almide_ir::{visit_mut::{walk_expr_mut, IrMutVisitor}, *};
use almide_lang::{intern::{sym, Sym}, types::Ty};

pub(crate) fn outline(ir: &mut IrProgram) {
    let mut ordinal = 0;
    let name = loop {
        let candidate = sym(&format!("__almd_test_panic_{ordinal}"));
        ordinal += 1;
        if !ir.functions.iter().any(|f| f.name == candidate) { break candidate; }
    };
    let mut rewrite = Rewrite { name, used: false, vars: &mut ir.var_table };
    for f in &mut ir.functions {
        if host_bound(f) { continue; }
        rewrite.visit_expr_mut(&mut f.body);
    }
    for t in &mut ir.top_lets { rewrite.visit_expr_mut(&mut t.value); }
    if !rewrite.used { return; }
    let var = ir.var_table.alloc_fresh("panic_message", Ty::String, Mutability::Let, None);
    let message = IrExpr { kind: IrExprKind::Var { id: var }, ty: Ty::String, span: None, def_id: None };
    ir.functions.push(IrFunction {
        name, params: vec![IrParam {
            var, ty: Ty::String, name: ir.var_table.get(var).name, borrow: ParamBorrow::Own,
            is_mut: false, open_record: None, default: None, attrs: Vec::new(),
        }],
        ret_ty: Ty::String, body: message,
        is_effect: false, is_test: false, generics: None,
        extern_attrs: Vec::new(), export_attrs: Vec::new(), attrs: Vec::new(),
        visibility: IrVisibility::Private, doc: None, blank_lines_before: 0,
        def_id: None, module_origin: None,
        mutated_params: Vec::new(), // fresh-fn: returns an immutable message unchanged
    });
    super::native_test_harness::register_raw_panic(name.to_string());
}

struct Rewrite<'a> { name: Sym, used: bool, vars: &'a mut VarTable }
impl IrMutVisitor for Rewrite<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        if let Some(text) = typed_hole_text(e) {
            let message = IrExpr { kind: IrExprKind::LitStr { value: text }, ty: Ty::String, span: e.span, def_id: None };
            e.kind = IrExprKind::Call { target: CallTarget::Named { name: sym("panic") }, args: vec![message], type_args: Vec::new() };
            // The hole checked as whatever its context demanded; the abort
            // that stands for it yields nothing, as `panic` does.
            e.ty = Ty::Never;
        }
        let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = &e.kind else { return; };
        if name.as_str() != "panic" || args.len() != 1 || args[0].ty != Ty::String || !type_args.is_empty() { return; }
        let marked = IrExpr { kind: IrExprKind::Call { target: CallTarget::Named { name: self.name }, args: args.clone(), type_args: Vec::new() },
            ty: Ty::String, span: e.span, def_id: None };
        let var = self.vars.alloc_fresh("panic_payload", Ty::String, Mutability::Let, None);
        let payload = IrExpr { kind: IrExprKind::Var { id: var }, ty: Ty::String, span: e.span, def_id: None };
        e.kind = IrExprKind::Block {
            stmts: vec![IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty: Ty::String, value: marked }, span: e.span }],
            expr: Some(Box::new(prim(sym("die"), prim(sym("handle"), payload, Ty::Int), e.ty.clone()))),
        };
        e.def_id = None;
        self.used = true;
    }
}

/// A declaration whose body the native render takes from its host: a native
/// foreign target or an intrinsic. Its bodyless `Hole` is not a typed hole.
fn host_bound(f: &IrFunction) -> bool {
    crate::native_foreign::selected(f).is_some()
        || f.attrs.iter().any(|a| matches!(a.name.as_str(), "intrinsic" | "inline_rust" | "wasm_intrinsic"))
}

/// A typed hole is the abort it is (#4640): `todo("m")`, a written `_`, and
/// the bodyless declaration of a host this target does not have all panic
/// with the text the standard render gives them. A compiler-made `Hole`
/// carries no span and is left alone.
fn typed_hole_text(e: &IrExpr) -> Option<String> {
    match (&e.kind, e.span) {
        (IrExprKind::Todo { message }, _) => Some(format!("not yet implemented: {message}")),
        (IrExprKind::Hole, Some(span)) => Some(format!("not yet implemented: hole at line {}", span.line)),
        _ => None,
    }
}

/// Follow only identity/handle operations from this Die operand to its
/// compiler-created marker. An arbitrary function or prefixed string cannot
/// select the test panic path.
pub(crate) fn is_message(func: &crate::MirFunction, mut value: crate::ValueId) -> bool {
    use crate::{Op, PrimKind};
    for _ in 0..func.ops.len() {
        let mut parent = None;
        for op in &func.ops {
            match op {
                Op::CallFn { dst: Some(dst), name, .. } if *dst == value => return super::native_test_harness::raw_panic(name),
                Op::Dup { dst, src } if *dst == value => { parent = Some(*src); break; }
                Op::Prim { kind: PrimKind::Handle, dst: Some(dst), args } if *dst == value => { parent = args.first().copied(); break; }
                _ => {}
            }
        }
        let Some(next) = parent else { return false; };
        value = next;
    }
    false
}

fn prim(name: Sym, arg: IrExpr, ty: Ty) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("prim"), func: name, def_id: None },
            args: vec![arg], type_args: Vec::new(),
        }, ty, span: None, def_id: None,
    }
}
