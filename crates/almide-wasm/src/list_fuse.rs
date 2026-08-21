//! map/filter → fold FUSION (deforestation): one pass over the source,
//! zero intermediate lists. SOUND only when every callback is
//! OBSERVATION-FREE — the unfused oracle runs all maps, then all
//! filters, then the fold, so a printing callback would interleave
//! differently. The purity scan is conservative: any Named call (user
//! fns are opaque, and println IS a Named call), any observable-module
//! call, any RuntimeCall/Fan/Lambda refuses fusion and the generic
//! staged lowering runs instead. Deterministic fuel is symmetric: HOF
//! internals never charge on either leg (the interp's pool-body rule),
//! and callback-body charges are order-free sums within a region.

use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

/// A fusable pre-fold stage.
enum Stage<'a> {
    Map(&'a IrExpr),
    Filter(&'a IrExpr),
}

fn observation_free(e: &IrExpr) -> bool {
    struct Scan {
        ok: bool,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Call { target, .. } => match target {
                    CallTarget::Named { .. } | CallTarget::Computed { .. } => self.ok = false,
                    CallTarget::Module { module, .. } => {
                        if matches!(
                            module.as_str(),
                            "fs" | "io" | "http" | "process" | "env" | "random" | "fan"
                        ) {
                            self.ok = false;
                        }
                    }
                    _ => self.ok = false,
                },
                IrExprKind::RuntimeCall { .. }
                | IrExprKind::Fan { .. }
                | IrExprKind::Lambda { .. } => self.ok = false,
                _ => {}
            }
            if self.ok {
                walk_expr(self, e);
            }
        }
    }
    let mut s = Scan { ok: true };
    s.visit_expr(e);
    s.ok
}

impl Emitter<'_> {
    /// Fused `src |> map* |> filter* |> fold(init, f)`. Ok(None) = the
    /// chain is not fusable here; take the generic staged path.
    pub(crate) fn lower_list_fold_fused(
        &mut self,
        xs: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        // Walk the chain source-side: fold(filter(map(src))).
        let mut stages_rev: Vec<Stage> = Vec::new();
        let mut cur = xs;
        while let IrExprKind::Call {
            target: CallTarget::Module { module, func, .. }, args, ..
        } = &cur.kind
        {
            if module.as_str() != "list" {
                break;
            }
            match (func.as_str(), args.as_slice()) {
                ("map", [inner, f]) => {
                    stages_rev.push(Stage::Map(f));
                    cur = inner;
                }
                ("filter", [inner, f]) => {
                    stages_rev.push(Stage::Filter(f));
                    cur = inner;
                }
                _ => break,
            }
        }
        if stages_rev.is_empty() {
            return Ok(None);
        }
        // Every callback literal + observation-free, fold's included.
        for st in &stages_rev {
            let f = match st {
                Stage::Map(f) | Stage::Filter(f) => *f,
            };
            let IrExprKind::Lambda { body, .. } = &f.kind else {
                return Ok(None);
            };
            if !observation_free(body) {
                return Ok(None);
            }
        }
        {
            let IrExprKind::Lambda { body, .. } = &cb.kind else {
                return Ok(None);
            };
            if !observation_free(body) {
                return Ok(None);
            }
        }
        let stages: Vec<&Stage> = stages_rev.iter().rev().collect();

        // fold acc setup
        let (fold_params, fold_body) = self.hof_lambda(cb, 2)?;
        let Some(acc_ty) = slice_ty_of(&init.ty, self.types) else {
            return unsup(&format!("list-fold-acc:{}", ty_name(&init.ty)));
        };
        self.lower(init, Some(acc_ty))?;
        self.f.instructions().local_set(fold_params[0]);

        let (elem0, bh, ch, ih) = self.hof_loop_open(cur)?;
        // element value rides a typed hold between stages.
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.f.instructions().local_get(ih).local_get(ch).i32_ge_u().br_if(1);
        // The skip-block opens BEFORE the element value exists — a wasm
        // block cannot receive operands from outside (the validator
        // caught the push-then-open draft immediately). Filter's br_if
        // targets this block to drop the element and still step.
        self.f.instructions().block(BlockType::Empty);
        self.f
            .instructions()
            .local_get(bh)
            .local_get(ih)
            .i32_const(elem0.slot_size() as i32)
            .i32_mul()
            .i32_add();
        self.load_ty_slot(elem0, 0);
        let mut cur_ty = elem0;
        for st in stages {
            match st {
                Stage::Map(f) => {
                    let (p, body) = self.hof_lambda(f, 1)?;
                    self.f.instructions().local_set(p[0]);
                    let Some(u) = slice_ty_of(&body.ty, self.types) else {
                        return unsup(&format!("fuse-map-ret:{}", ty_name(&body.ty)));
                    };
                    self.lower(body, Some(u))?;
                    cur_ty = u;
                }
                Stage::Filter(f) => {
                    let (p, body) = self.hof_lambda(f, 1)?;
                    self.f.instructions().local_set(p[0]);
                    self.lower(body, Some(BOOL))?;
                    // false → skip this element
                    self.f.instructions().i32_eqz().br_if(0);
                    self.f.instructions().local_get(p[0]);
                }
            }
        }
        // fold update: acc = f(acc, cur)
        self.f.instructions().local_set(fold_params[1]);
        self.lower(fold_body, Some(acc_ty))?;
        self.f.instructions().local_set(fold_params[0]);
        self.f.instructions().end(); // skip-block
        self.hof_step(ih);
        self.f.instructions().local_get(fold_params[0]);
        let _ = cur_ty;
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(Some(acc_ty)))
    }
}
