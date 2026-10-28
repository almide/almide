//! A `Region` argument of an `@extern(wasm, …)` import crosses as two
//! scalars, its base and its length (ADR-0031 D3, D7): the host is handed the
//! length the runtime holds for the region, never one the caller typed, and
//! no `Int` spelling of an address exists in the program.
//!
//! An IR rewrite run before the emitter reads the program, like the
//! newtype eraser: each body-less `@extern(wasm)` fn with a Region parameter
//! becomes an ordinary fn of the same signature whose body calls a new import
//! stub `__region_import_<name>` taking `(…, base: Handle, len: Int, …)` in
//! the Region's place. The stub carries the original `@extern(wasm)` attr, so
//! the import keeps its `(module, name)` and the emitter's scalar host ABI
//! types it (the base crosses as the address scalar, typed `Int` for the JS
//! glue). The wrapper's
//! parameter keeps the Region — and so its owner Bytes — alive across the
//! host call, the #3420 lifetime by ownership.

use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrFunction, IrParam, IrProgram, Mutability, ParamBorrow, VarTable,
};
use almide_types::types::extern_abi::is_region;
use almide_types::types::Ty;

/// The rewritten program, or None when no wasm import takes a Region (the
/// common case: no clone).
pub(crate) fn split_region_params(ir: &IrProgram) -> Option<IrProgram> {
    let wants = |fs: &[IrFunction]| fs.iter().any(takes_region);
    if !wants(&ir.functions) && !ir.modules.iter().any(|m| wants(&m.functions)) {
        return None;
    }
    let mut out = ir.clone();
    split_in(&mut out.functions, &mut out.var_table);
    for m in &mut out.modules {
        split_in(&mut m.functions, &mut m.var_table);
    }
    Some(out)
}

fn is_wasm_import(f: &IrFunction) -> bool {
    matches!(f.body.kind, IrExprKind::Hole) && f.extern_attrs.iter().any(|a| a.target.as_str() == "wasm")
}

fn takes_region(f: &IrFunction) -> bool {
    is_wasm_import(f) && f.params.iter().any(|p| is_region(&p.ty))
}

fn expr(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn param(vars: &mut VarTable, name: String, ty: Ty) -> IrParam {
    let sym = almide_base::intern::sym(&name);
    let var = vars.alloc(sym, ty.clone(), Mutability::Let, None);
    IrParam {
        var,
        ty,
        name: sym,
        borrow: ParamBorrow::Own,
        is_mut: false,
        open_record: None,
        default: None,
        attrs: Vec::new(),
    }
}

fn split_in(functions: &mut Vec<IrFunction>, vars: &mut VarTable) {
    let mut stubs = Vec::new();
    for f in functions.iter_mut().filter(|f| takes_region(f)) {
        let stub_name = format!("__region_import_{}", f.name.as_str());
        let mut stub_params = Vec::new();
        let mut args = Vec::new();
        for p in &f.params {
            let var = expr(IrExprKind::Var { id: p.var }, p.ty.clone());
            if !is_region(&p.ty) {
                stub_params.push(param(vars, p.name.as_str().to_string(), p.ty.clone()));
                args.push(var);
                continue;
            }
            // The host sees two plain numbers (the JS glue marshals `Int`);
            // both are the address scalar on this target, so the base
            // crosses unchanged. The stub is compiler-made: no source names it.
            stub_params.push(param(vars, format!("{}_base", p.name.as_str()), Ty::Int));
            stub_params.push(param(vars, format!("{}_len", p.name.as_str()), Ty::Int));
            let field = |name: &str, ty: Ty| {
                expr(IrExprKind::Member { object: Box::new(var.clone()), field: almide_base::intern::sym(name) }, ty)
            };
            args.push(field("base", Ty::RawPtr));
            args.push(field("len", Ty::Int));
        }
        let mut stub = f.clone();
        stub.name = almide_base::intern::sym(&stub_name);
        stub.params = stub_params;
        stub.def_id = None;
        stubs.push(stub);
        // The wrapper: same name and signature, a body that calls the stub.
        f.extern_attrs.retain(|a| a.target.as_str() != "wasm");
        f.body = expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: almide_base::intern::sym(&stub_name) },
                args,
                type_args: Vec::new(),
            },
            f.ret_ty.clone(),
        );
    }
    functions.extend(stubs);
}
