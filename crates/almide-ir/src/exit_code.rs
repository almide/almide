//! The exit-code range rule, as a target-independent IR rewrite.
//!
//! `process.exit(code)` promises that the process terminates with `code`. That
//! promise was only keepable on two of the three places a build can run:
//!
//! | `process.exit(n)` | native | embedded host | stock WASI runtime |
//! |---|---|---|---|
//! | 125 | 125 | 125 | 125 |
//! | 126 | 126 | 126 | 1, plus a runtime error on stderr |
//! | 200 | 200 | 200 | 1, same |
//! | 256 | 0 | 0 | 1, same |
//!
//! The restriction belongs to ONE lowering, not to "wasm": WASI preview-1's
//! `proc_exit` is specified over `[0, 126)` and a stock runtime turns anything
//! else into a host trap, while the component model's `wasi:cli/exit#exit-with-
//! code(u8)` carries the full 0..255 without trapping. The artifact `almide
//! build --target wasm` ships is the preview-1 one, so 126 and above cannot be
//! delivered by the build we hand people — and no leg change can alter that.
//! The divergence had to be closed by narrowing the promise (#2303).
//!
//! **0..=125 is the intersection of what ships, not an arbitrary cut.** POSIX
//! carries only the low 8 bits of a status to the parent, which is why
//! `exit(256)` already answered 0 natively — a silent truncation this rewrite
//! also removes. A shell reserves 126 (found, not executable), 127 (not found)
//! and 128+n (killed by signal n), so a program exiting in that band
//! misreports itself to its own caller. Preview-1's range is what is left.
//!
//! So: a code in 0..=125 exits with that code on every target, and any other
//! code is a domain error in the ALS-R1 form — `Error: exit code must be in
//! 0..=125` on stderr, exit 1 — identically everywhere (C-350).
//!
//! The bound is a FLOOR that widens with the shipped ABI, never a permanent
//! cut: when the default artifact stops going through `proc_exit`, 0..=255
//! becomes deliverable and the range may grow. Widening is additive, which is
//! why this direction is the reversible one. Surveying the neighbouring
//! languages first (`../almide-references`, 2026-09-19) found the trade made
//! every other way — `u8` by type in Wado, Zig and Lean; unbounded `Int` in
//! Grain, Vera and vibe — and none of them state what happens at 126 on a
//! stock preview-1 host: Grain hands the code straight to `proc_exit` and
//! inherits the trap, and a host that receives it cannot even attribute it
//! ("an out-of-range status carries no `I32Exit`, so nothing is recorded and
//! `main` treats it as an ordinary host trap"). An undiagnosable trap is the
//! outcome this rule replaces with one sentence on stderr.
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
//!   if __exit_code_N < 0 or __exit_code_N > 125 {
//!     eprintln("Error: exit code must be in 0..=125")
//!     process.exit(1)
//!   } else process.exit(__exit_code_N)
//! }
//! ```
//!
//! Both arms diverge, so the replacement is `Never` exactly where the call was,
//! and `e` is bound once — it may be a call, and running it twice would be a
//! second set of effects.
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

/// The inclusive upper bound. 0 is the lower one.
pub const MAX_EXIT_CODE: i64 = 125;

/// The one-line stderr message an out-of-range code aborts with.
pub const OUT_OF_RANGE_MSG: &str = "Error: exit code must be in 0..=125";

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
    let IrProgram { functions, modules, top_lets, var_table, .. } = program;
    let mut rewritten = 0;
    let mut g = Guard { var_table, rewritten: 0 };
    for f in functions.iter_mut() {
        g.visit_expr_mut(&mut f.body);
    }
    for tl in top_lets.iter_mut() {
        g.visit_expr_mut(&mut tl.value);
    }
    rewritten += g.rewritten;
    for m in modules.iter_mut() {
        let mut g = Guard { var_table: &mut m.var_table, rewritten: 0 };
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
}

impl IrMutVisitor for Guard<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Children first: a nested `process.exit` inside the argument (nothing
        // writes one, but the IR permits it) is rewritten before this node is
        // replaced, and the replacement's own two calls are never revisited.
        walk_expr_mut(self, expr);
        if !needs_guard(expr) {
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
        *expr = guarded_exit(expr.ty.clone(), expr.span, target.clone(), slot, code);
        self.rewritten += 1;
    }
}

/// `process.exit(<one arg>)` whose argument is not an integer literal already
/// in 0..=125.
fn needs_guard(expr: &IrExpr) -> bool {
    let IrExprKind::Call { target, args, .. } = &expr.kind else { return false };
    let CallTarget::Module { module, func, .. } = target else { return false };
    if module.as_str() != "process" || func.as_str() != "exit" || args.len() != 1 {
        return false;
    }
    !matches!(&args[0].kind, IrExprKind::LitInt { value } if (0..=MAX_EXIT_CODE).contains(value))
}

/// The replacement block. `target` is the ORIGINAL call target, cloned, so the
/// two calls below keep the resolved `DefId` the call site already carried.
fn guarded_exit(
    ty: Ty,
    span: Option<almide_base::span::Span>,
    target: CallTarget,
    slot: crate::VarId,
    code: IrExpr,
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
    let out_of_range = cmp(
        BinOp::Or,
        cmp(BinOp::Lt, slot_ref(), int(0)),
        cmp(BinOp::Gt, slot_ref(), int(MAX_EXIT_CODE)),
    );
    let complain = at(
        IrExprKind::Call {
            target: CallTarget::Named { name: almide_base::intern::sym("eprintln") },
            args: vec![at(
                IrExprKind::LitStr { value: OUT_OF_RANGE_MSG.to_string() },
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
