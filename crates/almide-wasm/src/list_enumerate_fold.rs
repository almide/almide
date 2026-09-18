//! Scalar enumerate/fold: a scalar snapshot and one private pair slot,
//! no intermediate tuple list. The snapshot preserves enumerate's eager
//! observation of the source even if a callback mutates the original list.
//! Reusing the pair is sound only when it cannot escape: the callback may
//! access its scalar fields but may not read/pass/capture the pair itself.
use almide_ir::{CallTarget, IrExpr, IrExprKind, VarId};
use almide_ir::visit::{IrVisitor, walk_expr};
use wasm_encoder::BlockType;
use crate::emitter::Emitter;
use crate::*;

fn projections_only(body: &IrExpr, pair: VarId) -> bool {
    struct Scan { pair: VarId, ok: bool }
    impl IrVisitor for Scan {
        fn visit_stmt(&mut self, s: &almide_ir::IrStmt) {
            use almide_ir::IrStmtKind;
            if matches!(&s.kind,
                IrStmtKind::Assign { var, .. } if *var == self.pair)
                || matches!(&s.kind,
                    IrStmtKind::FieldAssign { target, .. } | IrStmtKind::IndexAssign { target, .. }
                    | IrStmtKind::MapInsert { target, .. } if *target == self.pair)
            { self.ok = false; }
            if self.ok { almide_ir::visit::walk_stmt(self, s); }
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::TupleIndex { object, index } if *index < 2
                    && matches!(object.kind, IrExprKind::Var { id } if id == self.pair) => return,
                IrExprKind::Var { id } if *id == self.pair => self.ok = false,
                // Deferred bodies could observe the pair after the next iteration.
                IrExprKind::Lambda { .. } | IrExprKind::Fan { .. } => self.ok = false,
                _ => {}
            }
            if self.ok { walk_expr(self, e); }
        }
    }
    let mut scan = Scan { pair, ok: true };
    scan.visit_expr(body);
    scan.ok
}

impl Emitter<'_> {
    pub(crate) fn lower_enumerate_fold(
        &mut self, xs: &IrExpr, init: &IrExpr, cb: &IrExpr,
    ) -> Result<Option<Option<Lowered>>, EmitError> {
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &xs.kind else { return Ok(None) };
        if module.as_str() != "list" || func.as_str() != "enumerate" || args.len() != 1 { return Ok(None); }
        let IrExprKind::Lambda { params, body, .. } = &cb.kind else { return Ok(None) };
        if params.len() != 2 || !projections_only(body, params[1].0) { return Ok(None); }
        let Some(acc) = slice_ty_of(&init.ty, self.types) else { return Ok(None) };
        let Some(SliceTy::List(h)) = slice_ty_of(&args[0].ty, self.types) else { return Ok(None) };
        let elem = self.types.el(h);
        if !matches!(acc, INT | FLOAT | BOOL) || !matches!(elem, INT | FLOAT | BOOL) { return Ok(None); }
        let (locals, body) = self.hof_lambda(cb, 2)?;
        let (acc_p, pair_p) = (locals[0], locals[1]);
        let pair_ty = self.types.tuple(vec![INT, elem]);
        let def = self.types.tuple_def(pair_ty);
        let (index_offset, value_offset, size) = (def.fields[0].1, def.fields[1].1, def.size);
        self.lower_arg(init, Some(acc), ArgMode::Retain)?;
        self.f.instructions().local_set(acc_p);
        let (_, base, count, index) = self.hof_loop_open(&args[0])?;
        self.f.instructions().local_get(base).call(F_BLOCK_COPY).local_set(base);
        self.f.instructions().i32_const(size as i32).call(F_ALLOC).local_set(pair_p);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.f.instructions().local_get(index).local_get(count).i32_ge_u().br_if(1);
        self.f.instructions().local_get(pair_p).local_get(index).i64_extend_i32_u();
        self.store_ty_slot(INT, index_offset);
        self.f.instructions().local_get(pair_p).local_get(base).local_get(index)
            .i32_const(elem.slot_size() as i32).i32_mul().i32_add();
        self.load_ty_slot(elem, 0);
        self.store_ty_slot(elem, value_offset);
        self.lower(body, Some(acc))?;
        self.f.instructions().local_set(acc_p);
        self.hof_step(index);
        self.f.instructions().local_get(pair_p).call(F_DEC_FLAT)
            .local_get(base).call(F_DEC_FLAT).local_get(acc_p);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(Some(Lowered::scalar(acc))))
    }
}
