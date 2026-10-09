//! Which runtime symbols a program CALLS, read from the IR (#3486).
//!
//! The CLI used to ask the generated Rust text — `rs_code.contains(
//! "almide_rt_prim_budget_")` — whether a program uses the metered prims, and a
//! user string literal is emitted into that text verbatim: `println(
//! "almide_rt_prim_budget_x")` refused a working program on the native
//! fallback. A `RuntimeCall` node is built only by the compiler, so its symbol
//! is a fact a literal cannot forge.

use crate::visit::{walk_expr, IrVisitor};
use crate::{IrExpr, IrExprKind, IrFunction, IrProgram, IrTopLet};

/// The prefixes of the deterministic-meter prims `fan.bounded` / `fan.race`
/// / `fan.timeout` lower to (`almide_rt_prim_budget_enter`, …). They have a
/// definition only on the metered legs (the wasm emitter, the v1 native
/// render, the interpreter); the v0 native codegen has none.
pub const METERED_PRIM_PREFIXES: &[&str] = &["almide_rt_prim_budget_", "almide_rt_prim_timeout_"];

/// Does any fn or top-level let of `program` (its modules included) contain a
/// `RuntimeCall` whose symbol satisfies `pred`?
pub fn calls_runtime_symbol(program: &IrProgram, pred: impl Fn(&str) -> bool) -> bool {
    struct Finder<F> {
        pred: F,
        found: bool,
    }
    impl<F: Fn(&str) -> bool> IrVisitor for Finder<F> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if self.found {
                return;
            }
            if let IrExprKind::RuntimeCall { symbol, .. } = &expr.kind {
                if (self.pred)(symbol.as_str()) {
                    self.found = true;
                    return;
                }
            }
            walk_expr(self, expr);
        }
    }
    fn bodies<'a>(fns: &'a [IrFunction], lets: &'a [IrTopLet]) -> impl Iterator<Item = &'a IrExpr> {
        fns.iter().map(|f| &f.body).chain(lets.iter().map(|t| &t.value))
    }
    let mut f = Finder { pred, found: false };
    let all = bodies(&program.functions, &program.top_lets)
        .chain(program.modules.iter().flat_map(|m| bodies(&m.functions, &m.top_lets)));
    for body in all {
        f.visit_expr(body);
        if f.found {
            break;
        }
    }
    f.found
}

/// Does `program` call a deterministic-meter prim ([`METERED_PRIM_PREFIXES`])?
pub fn uses_metered_prims(program: &IrProgram) -> bool {
    calls_runtime_symbol(program, |s| METERED_PRIM_PREFIXES.iter().any(|p| s.starts_with(p)))
}
