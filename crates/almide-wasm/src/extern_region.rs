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
            // A slice's view starts `off` bytes past its block's base. The
            // sum is still an address into the Region's block, so it stays
            // typed `Handle`: the exit plan keys its raw-address rule (the
            // frame holds the Region across the host call) on that type.
            let start = IrExprKind::BinOp {
                op: almide_ir::BinOp::AddInt,
                left: Box::new(field("base", Ty::RawPtr)),
                right: Box::new(field("off", Ty::Int)),
            };
            args.push(expr(start, Ty::RawPtr));
            args.push(field("len", Ty::Int));
        }
        let mut stub = f.clone();
        stub.name = almide_base::intern::sym(&stub_name);
        stub.attrs.push(address_params_attr(&stub_params, f));
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

/// The stamp on the compiler-made wrapper of an `@export(wasm)` fn returning
/// a Region (ADR-0031 amendment A1): the emitter exports an adapter that
/// splits the wrapper's block into `(ptr: i32, len: i32)`. A declaration the
/// rewrite below writes, read by the emitter — never a name.
pub(crate) const REGION_EXPORT_ATTR: &str = "region_export";

/// Does the emitter export this fn through the `(ptr, len)` adapter?
pub(crate) fn is_region_export(f: &IrFunction) -> bool {
    f.attrs.iter().any(|a| a.name.as_str() == REGION_EXPORT_ATTR)
}

/// ADR-0031 amendment A1: an `@export(wasm, "sym")` fn returning a Region
/// keeps its name and body but loses the export; a wrapper
/// `__region_export_<name>` of the same parameters takes the export and
/// returns `ffi.__export_view(f(…))` (`f(…)!` for an `effect fn`): the
/// Region's block, one share of it, and the view's offset and length, so the
/// Region's own record is released by the ordinary ownership rules. The
/// emitter's adapter hands the host the view's data address and length, and
/// files the address against the block's share (region_export.rs).
/// None when no export returns a Region (no clone).
pub(crate) fn wrap_region_exports(ir: &IrProgram) -> Option<IrProgram> {
    let wants = |fs: &[IrFunction]| fs.iter().any(returns_region_export);
    if !wants(&ir.functions) && !ir.modules.iter().any(|m| wants(&m.functions)) {
        return None;
    }
    let mut out = ir.clone();
    wrap_in(&mut out.functions, &mut out.var_table);
    for m in &mut out.modules {
        wrap_in(&mut m.functions, &mut m.var_table);
    }
    Some(out)
}

fn returns_region_export(f: &IrFunction) -> bool {
    is_region(&f.ret_ty) && f.export_attrs.iter().any(|a| a.target.as_str() == "wasm")
}

fn wrap_in(functions: &mut Vec<IrFunction>, vars: &mut VarTable) {
    let mut wrappers = Vec::new();
    for f in functions.iter_mut().filter(|f| returns_region_export(f)) {
        let mut w = f.clone();
        w.name = almide_base::intern::sym(&format!("__region_export_{}", f.name.as_str()));
        w.params = f.params.iter().map(|p| param(vars, p.name.as_str().to_string(), p.ty.clone())).collect();
        let args = w.params.iter().map(|p| expr(IrExprKind::Var { id: p.var }, p.ty.clone())).collect();
        let call_ty = if f.is_effect {
            Ty::Applied(almide_types::types::TypeConstructorId::Result, vec![f.ret_ty.clone(), Ty::String].into())
        } else {
            f.ret_ty.clone()
        };
        let call = expr(
            IrExprKind::Call { target: CallTarget::Named { name: f.name }, args, type_args: Vec::new() },
            call_ty,
        );
        let region = if f.is_effect { expr(IrExprKind::Unwrap { expr: Box::new(call) }, f.ret_ty.clone()) } else { call };
        w.body = expr(
            IrExprKind::Call {
                target: CallTarget::Module {
                    module: almide_base::intern::sym("ffi"),
                    func: almide_base::intern::sym("__export_view"),
                    def_id: None,
                },
                args: vec![region],
                type_args: Vec::new(),
            },
            export_view_ty(),
        );
        w.ret_ty = export_view_ty();
        w.export_attrs.retain(|a| a.target.as_str() == "wasm");
        w.extern_attrs.clear();
        w.attrs = vec![almide_lang_attr(REGION_EXPORT_ATTR)];
        w.def_id = None;
        w.doc = None;
        w.mutated_params.clear();
        w.visibility = almide_ir::IrVisibility::Public;
        f.export_attrs.retain(|a| a.target.as_str() != "wasm");
        // The export is the wrapper's now: the fn itself must not also
        // export under its own name as an entry `pub fn` would (a record
        // block the host has no reading of). Visibility is past checking here;
        // the emitter reads it only to pick the implicit exports.
        f.visibility = almide_ir::IrVisibility::Mod;
        wrappers.push(w);
    }
    functions.extend(wrappers);
}

fn almide_lang_attr(name: &str) -> almide_lang::ast::Attribute {
    almide_lang::ast::Attribute { name: almide_base::intern::sym(name), args: Vec::new(), span: None }
}

/// `ffi.RegionExportView`, the record `__export_view` answers.
fn export_view_ty() -> Ty {
    Ty::Named(almide_base::intern::sym("RegionExportView"), Vec::new().into())
}

/// The stamp on a Region import stub naming its address and length
/// parameters: they cross in the target memory's address width
/// (imports::declare). A declaration this rewrite writes, never a name.
const ADDRESS_PARAMS_ATTR: &str = "address_params";

fn address_params_attr(stub_params: &[IrParam], f: &IrFunction) -> almide_lang::ast::Attribute {
    // Each Region parameter of `f` became two stub parameters, in order.
    let mut positions = Vec::new();
    let mut k = 0i64;
    for p in &f.params {
        if is_region(&p.ty) {
            positions.push(k);
            positions.push(k + 1);
            k += 2;
        } else {
            k += 1;
        }
    }
    debug_assert_eq!(k as usize, stub_params.len());
    almide_lang::ast::Attribute {
        name: almide_base::intern::sym(ADDRESS_PARAMS_ATTR),
        args: positions
            .into_iter()
            .map(|v| almide_lang::ast::AttrArg { name: None, value: almide_lang::ast::AttrValue::Int { value: v } })
            .collect(),
        span: None,
    }
}

/// The parameter positions of an import stub that carry an address or a
/// length, from the stamp above (none for any other fn).
pub(crate) fn address_params(f: &IrFunction) -> Vec<u32> {
    f.attrs
        .iter()
        .filter(|a| a.name.as_str() == ADDRESS_PARAMS_ATTR)
        .flat_map(|a| a.args.iter())
        .filter_map(|a| match a.value {
            almide_lang::ast::AttrValue::Int { value } => u32::try_from(value).ok(),
            _ => None,
        })
        .collect()
}
