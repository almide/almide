//! `map.map(list.group_by(xs, key), (group) => list.len(group))` needs
//! only each group's count, not its list of elements. The key callback
//! still runs once per source element in the original order. The removed
//! callback is exactly list.len of its sole parameter: no effects, capture,
//! trap, or additional reads may disappear with the group lists.

use almide_ir::{CallTarget, IrExpr, IrExprKind};
use wasm_encoder::{BlockType, MemArg};

use crate::collections::entry_layout;
use crate::emitter::Emitter;
use crate::work::Helper;
use crate::*;

fn group_count_parts<'e>(m: &'e IrExpr, count: &IrExpr) -> Option<(&'e IrExpr, &'e IrExpr)> {
    let IrExprKind::Lambda { params, body, .. } = &count.kind else { return None };
    let [(param, _)] = params.as_slice() else { return None };
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &body.kind else { return None };
    if module.as_str() != "list" || func.as_str() != "len" { return None; }
    let [arg] = args.as_slice() else { return None };
    if !matches!(arg.kind, IrExprKind::Var { id } if id == *param) { return None; }
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &m.kind else { return None };
    if module.as_str() != "list" || func.as_str() != "group_by" { return None; }
    let [xs, key] = args.as_slice() else { return None };
    let IrExprKind::Lambda { params, .. } = &key.kind else { return None };
    (params.len() == 1).then_some((xs, key))
}

/// The mapper is the only remaining user callback. It may have effects or
/// abort: neither an identity key nor list.len can observe or abort between
/// its invocations. Stored scalar keys retain value semantics under COW.
fn mapped_identity_source<'e>(xs: &'e IrExpr, key: &IrExpr) -> Option<(&'e IrExpr, &'e IrExpr)> {
    let IrExprKind::Lambda { params, body, .. } = &key.kind else { return None };
    let [(param, _)] = params.as_slice() else { return None };
    if !matches!(body.kind, IrExprKind::Var { id } if id == *param) { return None; }
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &xs.kind else { return None };
    if module.as_str() != "list" || func.as_str() != "map" { return None; }
    let [source, callback] = args.as_slice() else { return None };
    let IrExprKind::Lambda { params, .. } = &callback.kind else { return None };
    (params.len() == 1).then_some((source, callback))
}

impl Emitter<'_> {
    pub(crate) fn lower_group_count(&mut self, m: &IrExpr, count: &IrExpr) -> ArmResult {
        let Some((xs, key)) = group_count_parts(m, count) else { return Ok(None) };
        // With an identity key, map's output is only used as the counted
        // key. Its one callback can run directly over the source: hashing
        // and incrementing have no user observations or traps to reorder.
        // Do not fuse through a nonidentity key or a second map stage.
        let streamed = mapped_identity_source(xs, key);
        let (source, callback) = streamed.unwrap_or((xs, key));
        let (params, body) = self.hof_lambda(callback, 1)?;
        let kt = self.infer(body)?;
        // The ordinary group_by arm admits scalar keys. Keep its fallback
        // for every other shape, without partially emitting instructions.
        if !matches!(kt, SliceTy::Scalar(_)) { return Ok(None); }
        self.witness_inline_arg(m);
        self.witness_inline_arg(count);
        if streamed.is_some() {
            self.witness_inline_arg(xs);
            self.witness_inline_arg(key);
        }
        // Keep a block source when the callback can mutate its binding or
        // elements. This is the same snapshot rule as a materialized map.
        // A range can instead be counted, with both bounds evaluated first.
        let src = self.loop_src_open(source, body)?;
        let scan = self.keyed_find(kt)?;
        let append = self.keyed_append(kt);
        let reserve = self.work.helper(Helper::MapReserve);
        let (koff, voff, esz) = entry_layout(kt, INT);
        let value_slot = MemArg { offset: u64::from(voff), align: 2, memory_index: 0 };
        let hm = self.hold_i32()?;
        let he = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hkey = self.hold_for(kt)?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).call(F_ALLOC).local_set(hm);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.loop_src_exit(src);
        self.loop_src_load(src);
        self.f.instructions().local_set(params[0]);
        self.witness_callback_open(callback, None);
        self.lower(body, Some(kt))?;
        let owned = self.rc_owned_result(body);
        // Only the key is retained: absent moves/shares it into the map,
        // present releases an owned result. The element is never stored.
        self.witness_member_step(body, kt, owned);
        {
            let mut i = self.f.instructions();
            i.local_set(hkey);
            i.local_get(hm).i32_const(esz as i32).i32_const(koff as i32).local_get(hkey);
            i.call(scan).local_tee(he).if_(BlockType::Empty);
        }
        if owned { self.emit_release_hold(hkey, kt); }
        {
            let mut i = self.f.instructions();
            i.local_get(he).local_get(he).i64_load(value_slot);
            i.i64_const(1).i64_add().i64_store(value_slot);
            i.else_();
            i.local_get(hm).local_set(ho);
            i.local_get(hm).i32_const(esz as i32).call(reserve).local_set(hm);
            i.local_get(hm).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hm).i32_load(len_memarg()).i32_add().local_set(he);
            i.local_get(he).i32_const(koff as i32).i32_add().local_get(hkey);
        }
        if !owned { self.share_handle_top(kt); }
        self.store_ty_slot_at(kt);
        {
            let mut i = self.f.instructions();
            i.local_get(he).i64_const(1).i64_store(value_slot);
            i.local_get(hm);
            i.local_get(hm).i32_load(len_memarg()).i32_const(esz as i32).i32_add();
            i.i32_store(len_memarg());
            // No allocation between reserve and append: a relocated map's
            // address must be retired before the allocator can reuse it.
            if let Some(append) = append {
                i.local_get(ho).local_get(hm).i32_const(esz as i32).i32_const(koff as i32).call(append).drop();
            }
            i.end();
        }
        self.loop_src_step(src);
        self.f.instructions().local_get(hm);
        self.release_for(kt);
        for _ in 0..3 { self.release_i32(); }
        self.loop_src_close(src);
        Ok(Some(Lowered::owned(SliceTy::Map(self.types.intern(kt), self.types.intern(INT)))))
    }
}
