//! LoopAppendersPass: which lists a loop only appends to (#4906; ADR-0037
//! implementation notes: appends in loops).
//!
//! Target: Rust only. Runs last, on the final IR the walker renders.
//!
//! A loop whose only use of a list is appending to it is rendered with an
//! appender that keeps the list's length in a local
//! (`walker/loop_appender.rs`). This pass publishes, per loop, the list vars
//! its append statements target — the runtime call `almide_rt_list_push(xs, x)`
//! or the method `xs.push(x)` of the `xs = xs + [x]` rewrite, on a `List` var —
//! with the number of append statements each, in order of first append. Nested
//! loops count toward the enclosing one. A loop that holds a closure (a lambda
//! or a fan) publishes nothing: a push inside a closure is not a push of the
//! loop's pass. The walker checks the rest — that the append statements are
//! every mention of the list in the rendered loop — and spells the appender.
//!
//! The facts are keyed by the loop head's address (`CodegenAnnotations::loop_key`),
//! which is why this pass is a barrier at the very end: no rewrite after it
//! may rebuild a loop.

use std::collections::HashMap;
use almide_ir::annotations::{CodegenAnnotations, LoopKey};
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct LoopAppendersPass;

impl NanoPass for LoopAppendersPass {
    fn name(&self) -> &str { "LoopAppenders" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    /// Keys its facts by the address of each loop head in the final IR, so
    /// every pass declared before it stays before it, the shuffle included.
    fn barrier(&self) -> bool { true }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut v = Loops::default();
        for func in &program.functions {
            v.visit_fn(func);
        }
        for tl in &program.top_lets {
            v.visit_expr(&tl.value);
        }
        for module in &program.modules {
            for func in &module.functions {
                v.visit_fn(func);
            }
            for tl in &module.top_lets {
                v.visit_expr(&tl.value);
            }
        }
        let ann = &program.codegen_annotations;
        for e in ann.default_fields.values().chain(ann.case_default_fields.values()) {
            v.visit_expr(e);
        }
        program.codegen_annotations.loop_appends = v.out;
        PassResult { program, changed: false }
    }
}

/// Every loop of the program, nested ones included.
#[derive(Default)]
struct Loops {
    out: HashMap<LoopKey, Vec<(VarId, usize)>>,
}

impl Loops {
    fn visit_fn(&mut self, func: &IrFunction) {
        for p in &func.params {
            if let Some(d) = &p.default {
                self.visit_expr(d);
            }
        }
        self.visit_expr(&func.body);
    }

    fn record(&mut self, head: &IrExpr, body: &[IrStmt]) {
        let mut scan = Scan { pushes: Vec::new(), closure: false };
        scan.visit_expr(head);
        for s in body {
            scan.visit_stmt(s);
        }
        if scan.closure || scan.pushes.is_empty() {
            return;
        }
        let mut targets: Vec<(VarId, usize)> = Vec::new();
        for id in scan.pushes {
            match targets.iter_mut().find(|(t, _)| *t == id) {
                Some((_, n)) => *n += 1,
                None => targets.push((id, 1)),
            }
        }
        self.out.insert(CodegenAnnotations::loop_key(head), targets);
    }
}

impl IrVisitor for Loops {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::While { cond, body } => self.record(cond, body),
            IrExprKind::ForIn { iterable, body, .. } => self.record(iterable, body),
            _ => {}
        }
        walk_expr(self, e);
    }
}

/// The append statements of one loop (one entry per statement, by target) and
/// whether it holds a closure.
struct Scan {
    pushes: Vec<VarId>,
    closure: bool,
}

impl IrVisitor for Scan {
    fn visit_expr(&mut self, e: &IrExpr) {
        if matches!(e.kind, IrExprKind::Lambda { .. } | IrExprKind::Fan { .. }) {
            self.closure = true;
            return;
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        if let IrStmtKind::Expr { expr } = &s.kind
            && let Some(id) = push_target(expr)
        {
            self.pushes.push(id);
        }
        walk_stmt(self, s);
    }
}

/// The list var a `push` statement appends one element to: the runtime call
/// `almide_rt_list_push(xs, x)` or the method `xs.push(x)` of the `xs = xs + [x]`
/// rewrite, on a `List` var.
fn push_target(e: &IrExpr) -> Option<VarId> {
    let target = match &e.kind {
        IrExprKind::RuntimeCall { symbol, args } if symbol.as_str() == "almide_rt_list_push" && args.len() == 2 => &args[0],
        IrExprKind::Call { target: CallTarget::Method { object, method }, args, .. } if method.as_str() == "push" && args.len() == 1 => object,
        _ => return None,
    };
    let inner = match &target.kind {
        IrExprKind::Borrow { expr, mutable: true, .. } => expr,
        _ => target,
    };
    let IrExprKind::Var { id } = &inner.kind else { return None };
    matches!(&inner.ty, Ty::Applied(TypeConstructorId::List, _)).then_some(*id)
}
