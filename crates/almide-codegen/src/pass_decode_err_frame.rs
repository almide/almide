//! DecodeErrFramePass (#2050): the per-field error frame of a derived
//! `T.decode` costs nothing on the success path.
//!
//! #1675 gave every field of a derived decode a path frame: the field's
//! `Result` goes through a per-type worker `T.__erratw_<mangle>(r, "key")`
//! that passes an `Ok` through and splices `key` into an `Err`. The worker
//! is the shape BOTH legs lower (a top-level fn matching on its own param —
//! see `wrap_err_at` in the frontend), but on the native leg it is paid on
//! EVERY field, failed or not: the walker renders the pass-through match as
//! `match _r.clone() { Ok(_) => _r, … }` — a clone of the decoded payload —
//! and the call site hands the worker an owned `"key".to_string()`. On the
//! 8-field `decode` perf row that is eleven frames per decode and 2× the
//! hand-written reference.
//!
//! This pass rewrites each `T.__erratw_M(<res>, "key")?` on the native leg
//! into `<res>.map_err(|_we| almide_rt___err_at(_we, "key".to_string()))?`:
//! the success path is the plain `?` on the field's own result, the key is
//! the `&'static str` literal until an error actually needs an owned copy,
//! and the error bytes are the ones the worker produced (`almide_rt___err_at`
//! is the runtime the worker called). A worker no call site names any more
//! is dropped from the program, so the emitted Rust carries no dead
//! `T___erratw_*` fns.
//!
//! Scope: only the exact shape the derive mints — a `Try` over a `Named`
//! call to a `T.__erratw_*` fn (spelled `T___erratw_*` once BuiltinLowering
//! has flattened it) with a literal segment. The index twin
//! (`T.__erratidx_*`, a `match` subject inside the list workers) and any
//! worker whose call sites take another shape stay as they are. Native
//! only: the wasm leg lowers the worker unchanged.
//!
//! Runs after BuiltinLowering, i.e. after BorrowInsertion (the field lookups
//! inside `<res>` already carry their borrow decoration; BorrowInsertion
//! treats `InlineRust` args as opaque, so the rewrite must not precede it)
//! and before NormalizeRuntimeCalls.

use std::collections::HashSet;

use almide_base::intern::{sym, Sym};
use almide_ir::*;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use super::pass::{NanoPass, PassResult, Target};

const FRAME_MARK: &str = "__erratw_";
const ERR_AT_SYM: &str = "almide_rt___err_at";

#[derive(Debug)]
pub struct DecodeErrFramePass;

impl NanoPass for DecodeErrFramePass {
    fn name(&self) -> &str { "DecodeErrFrame" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["BuiltinLowering"] }
    fn run_before(&self) -> Vec<&'static str> { vec!["NormalizeRuntimeCalls"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut rw = Rewriter { changed: false };
        for func in &mut program.functions {
            rw.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                rw.visit_expr_mut(&mut func.body);
            }
            for tl in &mut module.top_lets {
                rw.visit_expr_mut(&mut tl.value);
            }
        }
        if !rw.changed {
            return PassResult { program, changed: false };
        }

        // Drop every frame worker no call site names any more.
        let mut refs = FrameRefs { named: HashSet::new() };
        for func in &program.functions {
            refs.visit_expr(&func.body);
        }
        for tl in &program.top_lets {
            refs.visit_expr(&tl.value);
        }
        for module in &program.modules {
            for func in &module.functions {
                refs.visit_expr(&func.body);
            }
            for tl in &module.top_lets {
                refs.visit_expr(&tl.value);
            }
        }
        // A definition still spells `T.__erratw_M`; BuiltinLowering flattened
        // the calls to `T___erratw_M` and may have module-prefixed them, so a
        // worker stays whenever a surviving call name ends in its flat name.
        let keep = |f: &IrFunction| {
            if !is_frame_worker(f.name) { return true }
            let flat = flat_name(f.name);
            refs.named.iter().any(|n| n.as_str().ends_with(&flat))
        };
        program.functions.retain(keep);
        for module in &mut program.modules {
            module.functions.retain(keep);
        }
        PassResult { program, changed: true }
    }
}

/// Both spellings of a frame worker: the definition's `T.__erratw_M` and
/// the call's flattened `T___erratw_M`.
fn is_frame_worker(name: Sym) -> bool {
    name.as_str().contains(FRAME_MARK)
}

fn flat_name(name: Sym) -> String {
    name.as_str().replace('.', "_")
}

/// The segment literal, bare or as the `&str` borrow BorrowInsertion may
/// wrap it in.
fn seg_literal(e: &IrExpr) -> Option<&str> {
    match &e.kind {
        IrExprKind::LitStr { value } => Some(value),
        IrExprKind::Borrow { expr, .. } => match &expr.kind {
            IrExprKind::LitStr { value } => Some(value),
            _ => None,
        },
        _ => None,
    }
}

struct Rewriter {
    changed: bool,
}

impl IrMutVisitor for Rewriter {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        walk_expr_mut(self, expr);
        let IrExprKind::Try { expr: tried } = &mut expr.kind else { return };
        let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &mut tried.kind else { return };
        if !is_frame_worker(*name) || args.len() != 2 { return }
        let Some(seg) = seg_literal(&args[1]) else { return };
        // `{:?}` on a `str` is a valid Rust string literal for any segment
        // (quotes, backslashes and controls escaped).
        let template = format!("{{r}}.map_err(|_we| {ERR_AT_SYM}(_we, {seg:?}.to_string()))");
        let res = args.swap_remove(0);
        tried.kind = IrExprKind::InlineRust { template, args: vec![(sym("r"), res)] };
        self.changed = true;
    }
    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        walk_stmt_mut(self, stmt);
    }
}

/// Every frame worker still named by a call after the rewrite.
struct FrameRefs {
    named: HashSet<Sym>,
}

impl IrVisitor for FrameRefs {
    fn visit_expr(&mut self, expr: &IrExpr) {
        walk_expr(self, expr);
        match &expr.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, .. }
            | IrExprKind::TailCall { target: CallTarget::Named { name }, .. }
            | IrExprKind::FnRef { name } if is_frame_worker(*name) => {
                self.named.insert(*name);
            }
            _ => {}
        }
    }
    fn visit_stmt(&mut self, stmt: &IrStmt) {
        walk_stmt(self, stmt);
    }
}
