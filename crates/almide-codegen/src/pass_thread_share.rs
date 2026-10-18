//! ThreadShareMarksPass: the thread boundary under ADR-0013's shared string
//! buffer (design point 3, Koka's thread-shared flip), spelled in the IR.
//!
//! Target: Rust only. Runs last, on the final IR.
//!
//! Every value a scoped thread can reach is marked before the threads start,
//! so the count of each `AlmideStr` block it holds turns atomic and only
//! those do. The values are the free variables of the threaded code: a
//! `fan { … }` block's spawned arms, or the arguments of a runtime helper whose
//! signature bounds `Send` (`runtime_fn_modes::crosses_threads`; the runtime
//! marks its own data arguments). Such a node becomes an `InlineRust` block
//! `{ (v).almide_share(); …; <node> }` whose args are the marked variables,
//! once each in `free_vars` order, and the node itself — the walker renders
//! it like any other template and decides nothing (#2186).
//!
//! LAST, after every pass that reads or rewrites a variable occurrence: a
//! mark is a read the earlier passes never saw while the walker computed it,
//! and must stay one — `RangeCountingVars` would materialize a range it
//! reads, `BorrowLowering` would respell a by-reference param under it. The
//! one walk that reads it afterwards, the ownership certifier, classifies an
//! inline-template argument as an `Operand` (a read that keeps the value
//! where it is), which consumes nothing.

use std::collections::HashSet;
use almide_base::intern::sym;
use almide_ir::*;
use almide_ir::annotations::CodegenAnnotations;
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct ThreadShareMarksPass;

impl NanoPass for ThreadShareMarksPass {
    fn name(&self) -> &str { "ThreadShareMarks" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    /// Every pass that reads or respells a variable occurrence, and the ones
    /// that settle the fan / runtime-call forms this pass recognises.
    fn depends_on(&self) -> Vec<&'static str> {
        vec![
            "FanLowering", "NormalizeRuntimeCalls", "SharedCellBorrow", "VarStorage",
            "RangeCountingVars", "TopLetStorage", "BorrowLowering", "SliceBinders", "StrMapKey",
        ]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut v = Marker { var_table: &program.var_table, ann: &program.codegen_annotations, changed: false };
        let mut bodies: Vec<&mut IrExpr> = Vec::new();
        bodies.extend(program.functions.iter_mut().map(|f| &mut f.body));
        bodies.extend(program.top_lets.iter_mut().map(|t| &mut t.value));
        for module in &mut program.modules {
            bodies.extend(module.functions.iter_mut().map(|f| &mut f.body));
            bodies.extend(module.top_lets.iter_mut().map(|t| &mut t.value));
        }
        for body in bodies {
            v.visit_expr_mut(body);
        }
        let changed = v.changed;
        PassResult { program, changed }
    }
}

/// The threaded code of `e`: the arms of a fan block that spawns them, or a
/// thread-crossing runtime call's arguments. A one-arm fan (#3341) and a fan
/// `FanLowering` marked inline (#3459) run on the calling thread.
///
/// A runtime call reaches the walker in two spellings: a `RuntimeCall`, and a
/// `Call { Module }` the walker renders as `almide_rt_{module}_{func}`
/// — the spelling `fan.map`'s parallel route (`fan.map_par`, RustLowering)
/// keeps. Both are judged by the one declaration the router's target carries:
/// the runtime helper's `Send` bound (`crosses_threads`).
fn threaded_code<'e>(e: &'e IrExpr, ann: &CodegenAnnotations) -> Option<&'e [IrExpr]> {
    match &e.kind {
        IrExprKind::Fan { exprs } if exprs.len() > 1 && !ann.is_inline_fan(e.span) => Some(exprs),
        IrExprKind::RuntimeCall { symbol, args } if crosses_threads(symbol.as_str()) => Some(args),
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if crosses_threads(&module_call_symbol(module.as_str(), func.as_str())) => Some(args),
        _ => None,
    }
}

fn crosses_threads(symbol: &str) -> bool {
    crate::generated::runtime_fn_modes::crosses_threads(symbol)
}

/// The runtime symbol a `Call { Module }` renders as — the walker's
/// `render_expr_call` spelling.
fn module_call_symbol(module: &str, func: &str) -> String {
    format!("almide_rt_{}_{}", almide_base::names::module_ident(module), func.replace('.', "_"))
}

/// The variables `exprs` read from outside, once each, in order.
fn thread_reachable(exprs: &[IrExpr]) -> Vec<VarId> {
    let mut seen = HashSet::new();
    exprs.iter()
        .flat_map(|e| almide_ir::free_vars::free_vars(e, &HashSet::new()))
        .filter(|id| seen.insert(*id))
        .collect()
}

/// `{ (v0).almide_share(); …; <node> }` as an `InlineRust` block.
fn wrap_in_marks(node: IrExpr, vars: &[VarId], var_table: &VarTable) -> IrExpr {
    let mut template = String::from("{ ");
    let mut args: Vec<(almide_base::intern::Sym, IrExpr)> = Vec::with_capacity(vars.len() + 1);
    for (i, id) in vars.iter().enumerate() {
        let name = format!("__almide_share{i}");
        template.push_str(&format!("({{{name}}}).almide_share(); "));
        let var = IrExpr { kind: IrExprKind::Var { id: *id }, ty: var_table.get(*id).ty.clone(), ..Default::default() };
        args.push((sym(&name), var));
    }
    template.push_str("{__almide_shared_node} }");
    let ty = node.ty.clone();
    args.push((sym("__almide_shared_node"), node));
    IrExpr { kind: IrExprKind::InlineRust { template, args }, ty, ..Default::default() }
}

struct Marker<'a> {
    var_table: &'a VarTable,
    ann: &'a CodegenAnnotations,
    changed: bool,
}

impl IrMutVisitor for Marker<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        // The reachable set is read off the node as it stands, before any
        // nested threaded code takes its own marks.
        let vars = threaded_code(e, self.ann).map(thread_reachable).unwrap_or_default();
        walk_expr_mut(self, e);
        if vars.is_empty() { return; }
        let node = std::mem::take(e);
        *e = wrap_in_marks(node, &vars, self.var_table);
        self.changed = true;
    }
}
