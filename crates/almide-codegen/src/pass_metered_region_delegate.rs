//! MeteredRegionDelegatePass (#3669) — the standard native codegen hands each
//! metered region (`fan.bounded` / `fan.race` / `fan.timeout`) to the v1 render.
//!
//! The deterministic meter is MIR: `almide_mir::charge_probe` charges the
//! lowered region fns and the metered clones of their callees, and a region's
//! spend — which decides `fan.race`'s winner and `fan.bounded`'s verdict — is
//! a function of that MIR. This codegen has no meter. When the whole-program
//! v1 render walls (on any construct, usually one far from the region), the
//! CLI links a module of v1-rendered regions into this codegen's output
//! (`almide_mir::pipeline::try_render_metered_splice`), and this pass makes
//! every region fn here a delegate into that module, so the program runs the
//! one metered body the wasm leg and the v1 render run.
//!
//! A region whose signature the two renders spell differently stays as it is
//! (`almide_ir::runtime_use::metered_region_crosses` — the splice refuses the
//! same regions, so such a program is refused rather than run unmetered).

use almide_ir::runtime_use::{metered_boundary, metered_region_crosses, MeteredBoundary};
use almide_ir::*;
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct MeteredRegionDelegatePass;

impl NanoPass for MeteredRegionDelegatePass {
    fn name(&self) -> &str { "MeteredRegionDelegate" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }

    /// Every pass that decides a param's mode or a call's argument spelling
    /// must see the delegate, not the region body it replaces.
    fn run_before(&self) -> Vec<&'static str> { vec!["BorrowInsertion"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in program.functions.iter_mut() {
            if runtime_use::is_metered_region(func.name.as_str()) && metered_region_crosses(func) {
                func.body = delegate_body(func);
                changed = true;
            }
        }
        PassResult { program, changed }
    }
}

/// How one param is handed to the v1 region fn (`{a}` is the param).
fn param_handover(ty: &Ty) -> &'static str {
    match metered_boundary(ty) {
        Some(MeteredBoundary::Bool) => "(({a}) as i64)",
        Some(MeteredBoundary::Str) => "&*{a}",
        Some(MeteredBoundary::IntList) => "&({a})[..]",
        _ => "{a}",
    }
}

/// `__almd_metered::<region>(params…)`, each param in the v1 render's
/// spelling, and a `Bool` result read back from the v1 render's `i64`.
fn delegate_body(func: &IrFunction) -> IrExpr {
    let mut args = Vec::with_capacity(func.params.len());
    let mut handed = Vec::with_capacity(func.params.len());
    for (i, p) in func.params.iter().enumerate() {
        let name = format!("a{i}");
        handed.push(param_handover(&p.ty).replace("{a}", &format!("{{{name}}}")));
        let var = IrExpr { kind: IrExprKind::Var { id: p.var }, ty: p.ty.clone(), span: func.body.span, def_id: None };
        args.push((almide_base::intern::sym(&name), var));
    }
    let call = format!("{}::{}({})", runtime_use::METERED_SPLICE_MODULE, func.name, handed.join(", "));
    let template = match metered_boundary(&func.ret_ty) {
        Some(MeteredBoundary::Bool) => format!("({call} != 0)"),
        _ => call,
    };
    IrExpr { kind: IrExprKind::InlineRust { template, args }, ty: func.ret_ty.clone(), span: func.body.span, def_id: None }
}
