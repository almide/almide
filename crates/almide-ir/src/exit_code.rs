//! The exit-code range rule, as a target-independent IR rewrite, plus the one
//! lowering-specific wall the WASI preview-1 build adds on top of it.
//!
//! `process.exit(code)` promises that the process terminates with `code`, the
//! POSIX exit status. Measured 2026-09-27 (#2780):
//!
//! | `process.exit(n)` | native | embedded host | stock WASI runtime (wasmtime 47) |
//! |---|---|---|---|
//! | 125 | 125 | 125 | 125 |
//! | 130 | 130 | 130 | 1, plus `exit with invalid exit status outside of [0..126)` |
//! | 255 | 255 | 255 | 1, same |
//! | 256 | 0 | 255 (clamped) | 1, same |
//! | -1 | 255 | 0 (clamped) | 1, same |
//!
//! Rust, Go, Python and Node all pass 0..=255 straight through and let the
//! operating system keep the low 8 bits of anything else. A wrapper that runs
//! a child and exits with its status depends on the pass-through: a child
//! killed by SIGINT answers 130, and a shell answers 126 and 127 itself.
//!
//! **The domain is 0..=255 on every target** ([`guard_exit_codes`]). A code in
//! it exits as itself on native and on the embedded host. Any other code is a
//! domain error in the ALS-R1 form — `Error: exit code must be in 0..=255` on
//! stderr, exit 1 — identically everywhere (C-350). This is where Almide departs
//! from the languages above: truncation turns `exit(256)` into a success the
//! caller cannot tell from a real one, and a model writing `process.exit(n)`
//! for a computed `n` should get a refusal, not a wrong status.
//!
//! **A WASI preview-1 build walls 126..=255** ([`wall_preview1_exit_codes`]
//! for the incumbent renderer, and the `to_wasi` exit shim in `almide-wasi`
//! for the structural leg). A stock runtime's `proc_exit` traps on 126 and
//! above, and the trap carries no exit request, so the band cannot be
//! delivered by that artifact (#2303). The build refuses it itself with
//! [`PREVIEW1_WALL_MSG`] and exit 1 — a diagnosed wall of one lowering, never a
//! silent 1. The embedded host does not go through `proc_exit`, which is why it
//! carries the whole domain.
//!
//! The 0.63 rule narrowed every target to 0..=125 to match the preview-1
//! artifact. That removed the pass-through from native programs, which is
//! #2780, and it located a property of one lowering in the language. The wall
//! is now in that lowering. A build whose exit carries a whole byte (the
//! component model's `exit-with-code(u8)`) may lift it; lifting is additive.
//!
//! # Why a rewrite and not four checks
//!
//! Four places turn `process.exit` into a host call: the native runtime shim,
//! the incumbent WAT renderer, the structural emitter and the interpreter.
//! Writing the rule four times would make it four rules that agree today. This
//! rewrite states it once, in the IR every leg consumes, out of nothing but
//! ordinary language constructs — a `let`, a comparison, `eprintln` and
//! `process.exit(1)` — so each leg renders it with machinery it already has.
//! In particular the incumbent WAT leg keeps its abort messages at fixed
//! addresses in a hand-laid static region; the message here is an ordinary
//! interned string literal instead, and that region does not move.
//!
//! # The shape
//!
//! ```text
//! process.exit(e)
//! ⟹
//! {
//!   let __exit_code_N = e
//!   if __exit_code_N < 0 or __exit_code_N > 255 {
//!     eprintln("Error: exit code must be in 0..=255")
//!     process.exit(1)
//!   } else process.exit(__exit_code_N)
//! }
//! ```
//!
//! Both arms diverge, so the replacement is `Never` exactly where the call was,
//! and `e` is bound once — it may be a call, and running it twice would be a
//! second set of effects. The preview-1 wall has the same shape with the
//! condition `__exit_code_N > 125` and its own line.
//!
//! A call whose argument is an integer LITERAL already in range is left
//! untouched: it cannot fail the rule, the guard would be dead code on every
//! leg, and leaving it alone keeps the assert desugar's `process.exit(1)` abort
//! tail byte-identical to what it rendered before.

use crate::visit_mut::{walk_expr_mut, IrMutVisitor};
use crate::{
    BinOp, CallTarget, IrExpr, IrExprKind, IrProgram, IrStmt, IrStmtKind, Mutability, VarTable,
};
use almide_lang::types::Ty;

/// The inclusive upper bound of the exit status. 0 is the lower one.
pub const MAX_EXIT_CODE: i64 = 255;

/// The one-line stderr message an out-of-range code aborts with.
pub const OUT_OF_RANGE_MSG: &str = "Error: exit code must be in 0..=255";

/// The last code a WASI preview-1 `proc_exit` delivers on a stock runtime.
pub const PREVIEW1_MAX_EXIT_CODE: i64 = 125;

/// The one-line stderr message a preview-1 build walls 126..=255 with. The
/// `to_wasi` exit shim writes the same bytes (plus the newline `eprintln`
/// adds); `tests/exit_code_range_test.rs` holds the two spellings equal.
pub const PREVIEW1_WALL_MSG: &str = "Error: a WASI preview-1 build cannot exit with a code in 126..=255";

/// Rewrite every `process.exit(code)` whose code is not a literal already in
/// range into the guarded form above. Returns the number of sites rewritten.
/// Every module owns its own [`VarTable`], so the slot has to be allocated in
/// the table that owns the body being rewritten — not in the program's
/// (#2374). Minting the entry program's next id and planting it in a module's
/// body produced a `VarId` with no row: `VarId(262)` against a 92-row table,
/// which the IR verifier reports and `use_count` reaches first as an
/// unchecked index panic. When the borrowed id happens to be IN range for the
/// module's table it does not panic — it binds over whatever row already lives
/// at that index, which is the quiet half of the same defect.
pub fn guard_exit_codes(program: &mut IrProgram) -> usize {
    rewrite_exits(program, Rule::DOMAIN)
}

/// The WASI preview-1 wall: rewrite every `process.exit(code)` whose code is
/// not a literal in 0..=125 so that 126..=255 prints [`PREVIEW1_WALL_MSG`] and
/// exits 1. Runs on the incumbent wasm renderer's IR only, after
/// [`guard_exit_codes`] has already refused everything outside 0..=255 — the
/// incumbent's artifact always calls `proc_exit` directly. The structural leg
/// does NOT take this pass: its module is also run by the embedded host, which
/// carries the whole domain, so its wall lives in the `to_wasi` exit shim,
/// the one step that turns it into a preview-1 command.
pub fn wall_preview1_exit_codes(program: &mut IrProgram) -> usize {
    rewrite_exits(program, Rule::PREVIEW1)
}

/// One range rule: codes outside `lo..=hi` print `msg` and exit 1. `lo` is
/// `None` when the lower side is already guaranteed by an earlier rule.
#[derive(Clone, Copy)]
struct Rule {
    lo: Option<i64>,
    hi: i64,
    msg: &'static str,
}

impl Rule {
    const DOMAIN: Rule = Rule { lo: Some(0), hi: MAX_EXIT_CODE, msg: OUT_OF_RANGE_MSG };
    const PREVIEW1: Rule = Rule { lo: None, hi: PREVIEW1_MAX_EXIT_CODE, msg: PREVIEW1_WALL_MSG };
}

fn rewrite_exits(program: &mut IrProgram, rule: Rule) -> usize {
    let IrProgram { functions, modules, top_lets, var_table, .. } = program;
    let mut rewritten = 0;
    let mut g = Guard { var_table, rewritten: 0, rule };
    for f in functions.iter_mut() {
        g.visit_expr_mut(&mut f.body);
    }
    for tl in top_lets.iter_mut() {
        g.visit_expr_mut(&mut tl.value);
    }
    rewritten += g.rewritten;
    for m in modules.iter_mut() {
        let mut g = Guard { var_table: &mut m.var_table, rewritten: 0, rule };
        for f in m.functions.iter_mut() {
            g.visit_expr_mut(&mut f.body);
        }
        for tl in m.top_lets.iter_mut() {
            g.visit_expr_mut(&mut tl.value);
        }
        rewritten += g.rewritten;
    }
    rewritten
}

struct Guard<'a> {
    var_table: &'a mut VarTable,
    rewritten: usize,
    rule: Rule,
}

impl IrMutVisitor for Guard<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Children first: a nested `process.exit` inside the argument (nothing
        // writes one, but the IR permits it) is rewritten before this node is
        // replaced, and the replacement's own two calls are never revisited.
        walk_expr_mut(self, expr);
        if !needs_guard(expr, self.rule.hi) {
            return;
        }
        let IrExprKind::Call { target, args, .. } = &mut expr.kind else { return };
        let code = args.remove(0);
        let slot = self.var_table.alloc(
            almide_base::intern::sym(&format!("__exit_code_{}", self.var_table.len())),
            Ty::Int,
            Mutability::Let,
            code.span,
        );
        *expr = guarded_exit(expr.ty.clone(), expr.span, target.clone(), slot, code, self.rule);
        self.rewritten += 1;
    }
}

/// `process.exit(<one arg>)` whose argument is not an integer literal already
/// in `0..=hi`.
fn needs_guard(expr: &IrExpr, hi: i64) -> bool {
    let IrExprKind::Call { target, args, .. } = &expr.kind else { return false };
    let CallTarget::Module { module, func, .. } = target else { return false };
    if module.as_str() != "process" || func.as_str() != "exit" || args.len() != 1 {
        return false;
    }
    !matches!(&args[0].kind, IrExprKind::LitInt { value } if (0..=hi).contains(value))
}

/// The replacement block. `target` is the ORIGINAL call target, cloned, so the
/// two calls below keep the resolved `DefId` the call site already carried.
fn guarded_exit(
    ty: Ty,
    span: Option<almide_base::span::Span>,
    target: CallTarget,
    slot: crate::VarId,
    code: IrExpr,
    rule: Rule,
) -> IrExpr {
    let at = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span, def_id: None };
    let slot_ref = || at(IrExprKind::Var { id: slot }, Ty::Int);
    let int = |value: i64| at(IrExprKind::LitInt { value }, Ty::Int);
    let cmp = |op: BinOp, left: IrExpr, right: IrExpr| {
        at(IrExprKind::BinOp { op, left: Box::new(left), right: Box::new(right) }, Ty::Bool)
    };
    let exit = |arg: IrExpr| {
        at(
            IrExprKind::Call { target: target.clone(), args: vec![arg], type_args: vec![] },
            Ty::Never,
        )
    };
    let above = cmp(BinOp::Gt, slot_ref(), int(rule.hi));
    let out_of_range = match rule.lo {
        Some(lo) => cmp(BinOp::Or, cmp(BinOp::Lt, slot_ref(), int(lo)), above),
        None => above,
    };
    let complain = at(
        IrExprKind::Call {
            target: CallTarget::Named { name: almide_base::intern::sym("eprintln") },
            args: vec![at(
                IrExprKind::LitStr { value: rule.msg.to_string() },
                Ty::String,
            )],
            type_args: vec![],
        },
        Ty::Unit,
    );
    let abort = at(
        IrExprKind::Block {
            stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: complain }, span }],
            expr: Some(Box::new(exit(int(1)))),
        },
        Ty::Never,
    );
    let branch = at(
        IrExprKind::If {
            cond: Box::new(out_of_range),
            then: Box::new(abort),
            else_: Box::new(exit(slot_ref())),
        },
        Ty::Never,
    );
    at(
        IrExprKind::Block {
            stmts: vec![IrStmt {
                kind: IrStmtKind::Bind {
                    var: slot,
                    mutability: Mutability::Let,
                    ty: Ty::Int,
                    value: code,
                },
                span,
            }],
            expr: Some(Box::new(branch)),
        },
        ty,
    )
}
