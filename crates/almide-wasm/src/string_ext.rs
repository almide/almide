//! String surface extensions (push/take/replace/join) — split from the
//! module-call chain for the complexity budget; consulted FIRST for
//! module calls (Ok(None) falls through to the chain).

use almide_ir::{CallTarget, IrExpr, IrExprKind};
use wasm_encoder::{BlockType, MemArg, ValType};

/// Payload-relative byte address (align 0 — a byte load may not carry
/// the 4-byte hint slot_memarg advertises).
fn str_byte() -> MemArg {
    MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 }
}

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    pub(crate) fn lower_string_ext(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        match target {
            // mut append (native s.push_str): var write-back of concat.
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string" && func.as_str() == "push" && args.len() == 2 =>
            {
                let IrExprKind::Var { id } = &args[0].kind else {
                    return unsup("string-push-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                if var_ty != STR {
                    return unsup(&format!("string-push-of:{var_ty:?}"));
                }
                self.f.instructions().local_get(var_idx);
                self.lower(&args[1], Some(STR))?;
                self.f.instructions().call(F_CONCAT).local_set(var_idx);
                Ok(None)
            }
            // First n CHARS (native `s.chars().take(n as usize)`): a
            // NEGATIVE n reinterprets huge and takes the WHOLE string —
            // deliberately not the C-054 clamp; cp_off clamps past-end.
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string" && func.as_str() == "take" && args.len() == 2 =>
            {
                self.lower(&args[0], Some(STR))?;
                let hs = self.hold_i32()?;
                self.f.instructions().local_set(hs);
                self.lower(&args[1], Some(INT))?;
                let hn = self.hold_i64()?;
                let hoff = self.hold_i32()?;
                let hb = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hn);
                i.local_get(hn).i64_const(0).i64_lt_s();
                i.if_(BlockType::Result(ValType::I32));
                i.local_get(hs).i32_load(len_memarg());
                i.else_();
                i.local_get(hs).local_get(hn).call(F_CP_OFF);
                i.end();
                i.local_set(hoff);
                i.local_get(hoff).call(F_ALLOC).local_set(hb);
                i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hs).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hoff);
                i.memory_copy(0, 0);
                i.local_get(hb);
                let _ = i;
                self.release_i32();
                self.release_i32();
                self.release_i64();
                self.release_i32();
                Ok(Some(STR))
            }
            // Rust str::replace / replace_first byte-for-byte via the
            // shared helper (the `first` flag selects the form). The
            // empty-pattern char-boundary rule (C-100) lives in the helper.
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string"
                    && matches!(func.as_str(), "replace" | "replace_first")
                    && args.len() == 3 =>
            {
                let first = func.as_str() == "replace_first";
                for a in args {
                    self.lower(a, Some(STR))?;
                }
                self.f
                    .instructions()
                    .i32_const(i32::from(first))
                    .call(F_STR_REPLACE);
                Ok(Some(STR))
            }
            // Byte-prefix compare (native str::starts_with): for valid
            // UTF-8 the byte test IS the char test.
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string"
                    && func.as_str() == "starts_with"
                    && args.len() == 2 =>
            {
                self.lower(&args[0], Some(STR))?;
                let hs = self.hold_i32()?;
                self.f.instructions().local_set(hs);
                self.lower(&args[1], Some(STR))?;
                let hp = self.hold_i32()?;
                let hn = self.hold_i32()?;
                let hk = self.hold_i32()?;
                let hr = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hp);
                i.local_get(hp).i32_load(len_memarg()).local_set(hn);
                i.local_get(hn).local_get(hs).i32_load(len_memarg()).i32_gt_u();
                i.if_(BlockType::Result(ValType::I32));
                i.i32_const(0);
                i.else_();
                i.i32_const(1).local_set(hr);
                i.i32_const(0).local_set(hk);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(hk).local_get(hn).i32_ge_u().br_if(1);
                i.local_get(hs).local_get(hk).i32_add().i32_load8_u(str_byte());
                i.local_get(hp).local_get(hk).i32_add().i32_load8_u(str_byte());
                i.i32_ne().if_(BlockType::Empty);
                i.i32_const(0).local_set(hr);
                i.br(2);
                i.end();
                i.local_get(hk).i32_const(1).i32_add().local_set(hk);
                i.br(0).end().end();
                i.local_get(hr);
                i.end();
                let _ = i;
                for _ in 0..5 {
                    self.release_i32();
                }
                Ok(Some(BOOL))
            }
            // string.join(xs, sep) is list.join with the module spelled
            // the other way — same F_LIST_JOIN, same List[String] demand.
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string" && func.as_str() == "join" && args.len() == 2 =>
            {
                match self.lower(&args[0], None)? {
                    SliceTy::List(h) if self.types.el(h) == STR => {}
                    other => return unsup(&format!("string-join-of:{other:?}")),
                }
                self.lower(&args[1], Some(STR))?;
                self.f.instructions().call(F_LIST_JOIN);
                Ok(Some(STR))
            }
            _ => return Ok(None),
        }
        .map(Some)
    }
}
