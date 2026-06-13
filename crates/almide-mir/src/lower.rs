//! Core-IR → MIR lowering — the single ownership+layout DECISION pass (§3.1).
//!
//! This is the v1 thesis made real: ONE pass decides, per binding, the
//! ownership (fresh `Alloc` / alias `Dup` / scope-end `Drop` / mutate
//! `MakeUnique`) and the layout ([`Repr`]) — replacing the five scattered
//! codegen passes (`pass_perceus`/`pass_clone`/`pass_borrow_inference`/
//! `pass_capture_clone`/`pass_box_deref`) with a single source the renderers
//! only translate. The produced MIR is checked by [`crate::verify_ownership`].
//!
//! Build order (§6, risk-first): it consumes the EXISTING frontend IR
//! (`almide_ir`) as a temporary feeder so the novel core is validated before
//! the frontend is greenfielded.
//!
//! # Scope of this brick
//! The value-semantics subset, on a LINEAR function body: `Bind` of a fresh
//! heap value (list/record/string literal) or an alias (`var b = a`) or a
//! scalar; `IndexAssign` (copy-on-write `MakeUnique`); scope-end `Drop`s.
//! Anything outside the subset (control flow, calls, …) returns
//! [`LowerError::Unsupported`] — never a silent drop (flight-grade totality).

use crate::{Init, MirFunction, Op, Repr, ValueId, PLACEHOLDER_LAYOUT};
use almide_ir::{IrExpr, IrExprKind, IrFunction, IrStmt, IrStmtKind, VarId};
use almide_lang::types::Ty;
use std::collections::HashMap;

/// A lowering could not proceed because the input is outside this brick's
/// subset (or violates a precondition such as concrete types). Carrying the
/// reason keeps the pass TOTAL — no case is silently skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    Unsupported(String),
}

/// Heap-managed types (need refcount: `Alloc`/`Dup`/`Drop`) vs `Copy` scalars.
/// Mirrors the old `pass_perceus::is_heap_type` / `emit_wasm` copy — but here it
/// is the SINGLE definition both renderers will read off the MIR.
pub fn is_heap_ty(ty: &Ty) -> bool {
    !matches!(
        ty,
        Ty::Int
            | Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
            | Ty::UInt64
            | Ty::Float
            | Ty::Float32
            | Ty::Float64
            | Ty::Bool
            | Ty::Unit
            | Ty::Never
            | Ty::RawPtr
            | Ty::ConstParam { .. }
            | Ty::ConstValue { .. }
    )
}

/// The [`Repr`] of a value of type `ty` — the LAYOUT decision, made once here.
/// Heap types get `Ptr` with a placeholder [`LayoutId`] (the layout pass, a
/// later brick, assigns real ids); scalars get their named byte width.
pub fn repr_of(ty: &Ty) -> Result<Repr, LowerError> {
    if matches!(ty, Ty::Unknown) {
        // Repr demands concrete types — the AllTypesConcrete precondition (§4).
        return Err(LowerError::Unsupported(
            "Unknown type reached MIR lowering (AllTypesConcrete precondition violated)".into(),
        ));
    }
    if is_heap_ty(ty) {
        return Ok(Repr::Ptr { layout: PLACEHOLDER_LAYOUT });
    }
    use crate::ScalarWidth;
    let w = match ty {
        Ty::Int | Ty::Int64 | Ty::UInt64 | Ty::Float | Ty::Float64 => ScalarWidth::Double,
        Ty::Int32 | Ty::UInt32 | Ty::Float32 => ScalarWidth::Word,
        Ty::Int16 | Ty::UInt16 => ScalarWidth::Half,
        Ty::Int8 | Ty::UInt8 => ScalarWidth::Byte,
        Ty::Bool => ScalarWidth::Word, // Bool ABI slot is 4 bytes
        // Unit/Never/RawPtr/Const* are not values that get a scalar slot here.
        other => {
            return Err(LowerError::Unsupported(format!(
                "no scalar Repr for {other:?}"
            )))
        }
    };
    Ok(Repr::Scalar { width: w })
}

/// Lower one function to MIR.
pub fn lower_function(func: &IrFunction) -> Result<MirFunction, LowerError> {
    lower_body(&func.body, func.name.as_str())
}

/// Lower a function body expression to MIR (the testable core; `lower_function`
/// is the thin wrapper over a whole [`IrFunction`]).
pub fn lower_body(body: &IrExpr, name: &str) -> Result<MirFunction, LowerError> {
    let mut ctx = LowerCtx::default();
    let stmts = match &body.kind {
        IrExprKind::Block { stmts, expr } => {
            // The subset is Unit-returning (the tail is Unit / absent); a heap
            // tail value would be a move-out (Consume), handled in a later brick.
            if let Some(tail) = expr {
                if is_heap_ty(&tail.ty) {
                    return Err(LowerError::Unsupported(
                        "heap-typed tail value (move-out) not in this brick".into(),
                    ));
                }
            }
            stmts
        }
        other => {
            return Err(LowerError::Unsupported(format!(
                "function body is not a Block: {}",
                kind_name(other)
            )))
        }
    };

    for stmt in stmts {
        ctx.lower_stmt(stmt)?;
    }
    // Scope end: release every live heap handle. Aliases share a ValueId, so one
    // Drop per HANDLE balances the Alloc(+1) and each aliasing Dup(+1).
    ctx.emit_scope_end_drops();

    Ok(MirFunction { name: name.to_string(), ops: ctx.ops, ..Default::default() })
}

#[derive(Default)]
struct LowerCtx {
    ops: Vec<Op>,
    /// VarId → the MIR value it denotes. Aliases map to the SAME ValueId.
    value_of: HashMap<VarId, ValueId>,
    /// Heap handles in binding order, for scope-end drops (one Drop per handle).
    live_heap_handles: Vec<ValueId>,
    next_value: u32,
}

impl LowerCtx {
    fn fresh_value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    fn lower_stmt(&mut self, stmt: &IrStmt) -> Result<(), LowerError> {
        match &stmt.kind {
            IrStmtKind::Bind { var, ty, value, .. } => self.lower_bind(*var, ty, value),
            IrStmtKind::IndexAssign { target, .. } => {
                // Copy-on-write: the write must land on a uniquely-owned buffer.
                let v = self.value_for(*target)?;
                self.ops.push(Op::MakeUnique { v });
                Ok(())
            }
            other => Err(LowerError::Unsupported(format!(
                "statement {} not in the value-semantics subset",
                stmt_kind_name(other)
            ))),
        }
    }

    fn lower_bind(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        if !is_heap_ty(ty) {
            // Scalar binding: define a Copy value, no ownership accounting.
            let dst = self.fresh_value();
            self.value_of.insert(var, dst);
            self.ops.push(Op::Const { dst });
            return Ok(());
        }
        match &value.kind {
            // Alias: `var b = a` — b is a NEW handle denoting the SAME heap
            // object as a, acquiring its own owned reference (the single
            // fresh-vs-alias decision).
            IrExprKind::Var { id } => {
                let src = self.value_for(*id)?;
                let dst = self.fresh_value();
                self.value_of.insert(var, dst);
                self.ops.push(Op::Dup { dst, src });
                self.live_heap_handles.push(dst);
                Ok(())
            }
            // A fresh heap value (literal container / string).
            IrExprKind::List { .. }
            | IrExprKind::MapLiteral { .. }
            | IrExprKind::EmptyMap
            | IrExprKind::Record { .. }
            | IrExprKind::Tuple { .. }
            | IrExprKind::LitStr { .. }
            | IrExprKind::StringInterp { .. } => {
                let dst = self.fresh_value();
                let repr = repr_of(ty)?;
                let init = alloc_init(value);
                self.value_of.insert(var, dst);
                self.ops.push(Op::Alloc { dst, repr, init });
                self.live_heap_handles.push(dst);
                Ok(())
            }
            other => Err(LowerError::Unsupported(format!(
                "heap bind from {} not in this brick",
                kind_name(other)
            ))),
        }
    }

    fn value_for(&self, var: VarId) -> Result<ValueId, LowerError> {
        self.value_of
            .get(&var)
            .copied()
            .ok_or_else(|| LowerError::Unsupported(format!("use of unbound var {var:?}")))
    }

    fn emit_scope_end_drops(&mut self) {
        // Reverse binding order (LIFO scope teardown).
        for v in self.live_heap_handles.iter().rev() {
            self.ops.push(Op::Drop { v: *v });
        }
    }
}

/// Extract a concrete initializer from a fresh-heap bind value. A `List[Int]`
/// literal yields [`Init::IntList`]; everything else is [`Init::Opaque`] (the
/// computation is carried by a later brick).
fn alloc_init(value: &IrExpr) -> Init {
    if let IrExprKind::List { elements } = &value.kind {
        let ints: Option<Vec<i64>> = elements
            .iter()
            .map(|e| match &e.kind {
                IrExprKind::LitInt { value } => Some(*value),
                _ => None,
            })
            .collect();
        if let Some(ints) = ints {
            return Init::IntList(ints);
        }
    }
    Init::Opaque
}

fn stmt_kind_name(k: &IrStmtKind) -> &'static str {
    match k {
        IrStmtKind::Bind { .. } => "Bind",
        IrStmtKind::BindDestructure { .. } => "BindDestructure",
        IrStmtKind::Assign { .. } => "Assign",
        IrStmtKind::IndexAssign { .. } => "IndexAssign",
        IrStmtKind::MapInsert { .. } => "MapInsert",
        IrStmtKind::FieldAssign { .. } => "FieldAssign",
        IrStmtKind::Guard { .. } => "Guard",
        IrStmtKind::Expr { .. } => "Expr",
        IrStmtKind::Comment { .. } => "Comment",
        IrStmtKind::RcInc { .. } => "RcInc",
        IrStmtKind::RcDec { .. } => "RcDec",
        IrStmtKind::ListSwap { .. } => "ListSwap",
        IrStmtKind::ListReverse { .. } => "ListReverse",
        IrStmtKind::ListRotateLeft { .. } => "ListRotateLeft",
        IrStmtKind::ListCopySlice { .. } => "ListCopySlice",
    }
}

fn kind_name(k: &IrExprKind) -> &'static str {
    match k {
        IrExprKind::LitInt { .. } => "LitInt",
        IrExprKind::LitFloat { .. } => "LitFloat",
        IrExprKind::LitStr { .. } => "LitStr",
        IrExprKind::LitBool { .. } => "LitBool",
        IrExprKind::Unit => "Unit",
        IrExprKind::Var { .. } => "Var",
        IrExprKind::List { .. } => "List",
        IrExprKind::Record { .. } => "Record",
        IrExprKind::Tuple { .. } => "Tuple",
        IrExprKind::Block { .. } => "Block",
        IrExprKind::Call { .. } => "Call",
        IrExprKind::RuntimeCall { .. } => "RuntimeCall",
        _ => "<other>",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{verify_ownership, ViolationKind};
    use almide_lang::types::constructor::TypeConstructorId;

    fn ir_expr(kind: IrExprKind, ty: Ty) -> IrExpr {
        IrExpr { kind, ty, span: None, def_id: None }
    }
    fn stmt(kind: IrStmtKind) -> IrStmt {
        IrStmt { kind, span: None }
    }
    fn list_int() -> Ty {
        // Any Applied/heap type works for the ownership logic; List[Int] is the
        // value-semantics shape under test.
        Ty::Applied(TypeConstructorId::List, vec![Ty::Int])
    }
    fn bind(var: u32, ty: Ty, value: IrExpr) -> IrStmt {
        stmt(IrStmtKind::Bind {
            var: VarId(var),
            mutability: almide_ir::Mutability::Var,
            ty,
            value,
        })
    }
    /// Build a Unit-returning body block (avoids constructing a full IrFunction).
    fn body(stmts: Vec<IrStmt>) -> IrExpr {
        ir_expr(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
    }

    #[test]
    fn alias_then_cow_lowers_to_balanced_mir() {
        // var a = [1,2,3]; var b = a; a[0] = 9
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(1, list_int(), ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
            stmt(IrStmtKind::IndexAssign {
                target: VarId(0),
                index: ir_expr(IrExprKind::LitInt { value: 0 }, Ty::Int),
                value: ir_expr(IrExprKind::LitInt { value: 9 }, Ty::Int),
            }),
        ]);
        let mir = lower_body(&b, "main").expect("lowers");

        // Expect: Alloc(a=V0), Dup(b=V1 from V0), MakeUnique(a=V0), Drop, Drop.
        assert!(matches!(mir.ops[0], Op::Alloc { dst: ValueId(0), .. }));
        assert!(matches!(mir.ops[1], Op::Dup { dst: ValueId(1), src: ValueId(0) }));
        assert!(matches!(mir.ops[2], Op::MakeUnique { v: ValueId(0) }));
        let drops = mir.ops.iter().filter(|o| matches!(o, Op::Drop { .. })).count();
        assert_eq!(drops, 2, "two handles (a, b) → two scope-end drops");

        // The single ownership decision must be balanced by construction.
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn scalar_bind_needs_no_ownership() {
        // let n = 5
        let b = body(vec![bind(
            0,
            Ty::Int,
            ir_expr(IrExprKind::LitInt { value: 5 }, Ty::Int),
        )]);
        let mir = lower_body(&b, "main").expect("lowers");
        assert_eq!(mir.ops, vec![Op::Const { dst: ValueId(0) }]);
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn fresh_heap_bind_allocs_and_drops() {
        // var s = "hi"
        let b = body(vec![bind(
            0,
            Ty::String,
            ir_expr(IrExprKind::LitStr { value: "hi".into() }, Ty::String),
        )]);
        let mir = lower_body(&b, "main").expect("lowers");
        assert!(matches!(mir.ops[0], Op::Alloc { .. }));
        assert!(matches!(mir.ops[1], Op::Drop { .. }));
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn out_of_subset_is_an_explicit_error_not_silent() {
        // A bare expression statement is outside this brick → explicit Unsupported.
        let b = body(vec![stmt(IrStmtKind::Expr {
            expr: ir_expr(IrExprKind::LitInt { value: 1 }, Ty::Int),
        })]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(_)) => {}
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_is_rejected_at_repr() {
        assert!(matches!(repr_of(&Ty::Unknown), Err(LowerError::Unsupported(_))));
    }

    #[test]
    fn use_after_free_caught_if_decision_were_wrong() {
        // Sanity that the verifier guards the lowering: a hand-broken MIR with a
        // missing alias Dup would leave the alias' Drop unbalanced.
        let broken = MirFunction {
            name: "broken".into(),
            ops: vec![
                Op::Alloc { dst: ValueId(0), repr: Repr::Ptr { layout: PLACEHOLDER_LAYOUT }, init: Init::Opaque },
                Op::Drop { v: ValueId(0) },
                Op::Drop { v: ValueId(0) }, // second drop with no Dup → double free
            ],
            ..Default::default()
        };
        let errs = verify_ownership(&broken).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::DoubleFree));
    }
}
