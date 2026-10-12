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
use almide_lang::types::Ty;

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

/// The name prefix of an outlined metered region (`fan.bounded` / `fan.race`
/// / `fan.timeout`): the frontend synthesizes `__almd_bounded_N` per region,
/// the metered-clone pass roots its call graph there, and the standard native
/// codegen hands each one to the v1-rendered region module (#3669).
pub const METERED_REGION_PREFIX: &str = "__almd_bounded_";

/// Is `name` an outlined metered region fn ([`METERED_REGION_PREFIX`])?
pub fn is_metered_region(name: &str) -> bool {
    name.starts_with(METERED_REGION_PREFIX)
}

/// How a value of one type crosses the boundary between the standard native
/// codegen and a v1-rendered metered region (#3669), or `None` when the two
/// renderers disagree on its representation (a record is a struct on one
/// side and a slot block on the other) and the region cannot be handed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeteredBoundary {
    /// Same Rust type on both sides (`i64`, `f64`).
    Same,
    /// `bool` on the standard side, `i64` on the v1 side.
    Bool,
    /// `String` on the standard side, `&str` / `String` on the v1 side.
    Str,
    /// `Vec<i64>` on the standard side, `&[i64]` / `Vec<i64>` on the v1 side.
    IntList,
    /// `Result<i64, String>` on both sides (a return only).
    IntResult,
}

/// The [`MeteredBoundary`] of `ty`. The one definition both sides read: the
/// v1 region render refuses a region whose signature has a `None` here, and
/// the standard codegen hands over exactly the regions it accepts.
pub fn metered_boundary(ty: &Ty) -> Option<MeteredBoundary> {
    use almide_lang::types::constructor::TypeConstructorId as C;
    match ty {
        Ty::Int | Ty::Float => Some(MeteredBoundary::Same),
        Ty::Bool => Some(MeteredBoundary::Bool),
        Ty::String => Some(MeteredBoundary::Str),
        Ty::Applied(C::List, a) if matches!(a.as_slice(), [Ty::Int]) => Some(MeteredBoundary::IntList),
        Ty::Applied(C::Result, a) if matches!(a.as_slice(), [Ty::Int, Ty::String]) => Some(MeteredBoundary::IntResult),
        _ => None,
    }
}

/// Can the region `f` be handed to the v1 render: every param crosses as a
/// value, and the return crosses too ([`MeteredBoundary::IntResult`] is a
/// return-only shape).
pub fn metered_region_crosses(f: &IrFunction) -> bool {
    let param_ok = |t: &Ty| matches!(metered_boundary(t), Some(b) if b != MeteredBoundary::IntResult);
    f.params.iter().all(|p| param_ok(&p.ty)) && metered_boundary(&f.ret_ty).is_some()
}

/// The module of the standard native codegen's output that holds the
/// v1-rendered metered regions; each region fn delegates into it (#3669).
pub const METERED_SPLICE_MODULE: &str = "__almd_metered";
