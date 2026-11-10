//! Test assertion identities and diagnostics. Only the frontend's remaining
//! builtin assert calls are outlined: ordinary-function controlled aborts have
//! already been desugared and must not acquire libtest unwind semantics.
use almide_ir::{visit_mut::{walk_expr_mut, IrMutVisitor}, *};
use almide_lang::{intern::{sym, Sym}, types::Ty};
use std::{cell::RefCell, collections::{BTreeMap, BTreeSet}};

#[derive(Clone)]
struct Diagnostic { message: bool, line: Option<usize> }
thread_local! {
    static DIAGNOSTICS: RefCell<BTreeMap<String, Diagnostic>> = const { RefCell::new(BTreeMap::new()) };
}
pub(crate) struct Scope(BTreeMap<String, Diagnostic>);
impl Scope {
    pub(crate) fn enter() -> Self { Self(DIAGNOSTICS.with(|s| s.replace(BTreeMap::new()))) }
}
impl Drop for Scope {
    fn drop(&mut self) { DIAGNOSTICS.with(|s| s.replace(std::mem::take(&mut self.0))); }
}

pub(crate) fn outline(ir: &mut IrProgram) {
    let mut pass = Outline {
        vars: &mut ir.var_table, used: ir.functions.iter().map(|f| f.name).collect(),
        helpers: Vec::new(),
    };
    for f in &mut ir.functions { pass.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { pass.visit_expr_mut(&mut t.value); }
    ir.functions.extend(pass.helpers);
}

struct Outline<'a> { vars: &'a mut VarTable, used: BTreeSet<Sym>, helpers: Vec<IrFunction> }
impl IrMutVisitor for Outline<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = &e.kind else { return; };
        // The frontend has already lowered equality assertions to a lazy
        // diagnostic message and this primitive assert call.
        if !type_args.is_empty() || name.as_str() != "assert" || !matches!(args.len(), 1 | 2) { return; }
        let builtin = *name;
        let mut args = args.clone();
        // The message of assert(condition, message) is evaluated only on failure.
        let condition = if builtin.as_str() == "assert" && args.len() == 2 { Some(args.remove(0)) } else { None };
        let diagnostic = Diagnostic { message: condition.is_some(), line: e.span.as_ref().map(|s| s.line as usize) };
        let mut ordinal = self.used.len();
        let helper = loop {
            let name = sym(&format!("__almd_test_assert_{ordinal}")); ordinal += 1;
            if self.used.insert(name) { break name; }
        };
        let params: Vec<_> = args.iter().map(|a| {
            let var = self.vars.alloc_fresh("assert_argument", a.ty.clone(), Mutability::Let, None);
            IrParam { var, ty: a.ty.clone(), name: self.vars.get(var).name, borrow: ParamBorrow::Own,
                is_mut: false, open_record: None, default: None, attrs: Vec::new() }
        }).collect();
        let operands: Vec<_> = params.iter().map(|p| IrExpr {
            kind: IrExprKind::Var { id: p.var }, ty: p.ty.clone(), span: e.span, def_id: None,
        }).collect();
        let body = if condition.is_some() {
            // The caller has selected the failing arm. Keep the diagnostic
            // itself as Die's checked operand, so rendering never reads an
            // argument that MIR considered dead after building another text.
            let handle = IrExpr { kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym("prim"), func: sym("handle"), def_id: None },
                args: operands, type_args: Vec::new(),
            }, ty: Ty::Int, span: e.span, def_id: None };
            IrExpr { kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym("prim"), func: sym("die"), def_id: None },
                args: vec![handle], type_args: Vec::new(),
            }, ty: Ty::Unit, span: e.span, def_id: None }
        } else {
            IrExpr { kind: IrExprKind::Call { target: CallTarget::Named { name: builtin }, args: operands, type_args: Vec::new() },
                ty: Ty::Unit, span: e.span, def_id: None }
        };
        self.helpers.push(IrFunction {
            name: helper, params, ret_ty: Ty::Unit, body, is_effect: false, is_test: false,
            generics: None, extern_attrs: Vec::new(), export_attrs: Vec::new(), attrs: Vec::new(),
            visibility: IrVisibility::Private, doc: None, blank_lines_before: 0,
            def_id: None, module_origin: None,
            mutated_params: Vec::new(), // fresh-fn: reads immutable assertion arguments
        });
        DIAGNOSTICS.with(|s| s.borrow_mut().insert(helper.to_string(), diagnostic));
        let call = IrExpr { kind: IrExprKind::Call { target: CallTarget::Named { name: helper }, args, type_args: Vec::new() },
            ty: Ty::Unit, span: e.span, def_id: None };
        e.kind = match condition {
            Some(cond) => IrExprKind::If { cond: Box::new(cond), then: Box::new(IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: e.span, def_id: None }), else_: Box::new(call) },
            None => call.kind,
        };
        e.def_id = None;
    }
}

/// Called only at the outlined body's checked Die branch. Comparing values
/// and choosing that branch remains ordinary certified MIR; Rust formatting
/// and unwinding are trusted native harness operations.
pub(crate) fn failure(func: &crate::MirFunction, message: crate::ValueId) -> Option<String> {
    DIAGNOSTICS.with(|s| {
        let table = s.borrow();
        let d = table.get(&func.name)?;
        let site = d.line.map(|n| format!("at line {n}")).unwrap_or_default();
        Some(if d.message {
            format!("panic!(\"{{}} ({site})\", {})", crate::render_native::var(message))
        } else {
            format!("panic!({:?})", format!("assertion failed: {site}"))
        })
    })
}
