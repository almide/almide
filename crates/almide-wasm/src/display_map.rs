// ── display.rs, the map arm ──
//
// include!-spliced into `display.rs` at module level (the 800-line file
// discipline): map display (`["k": v, …]`, `[:]` when empty), the display
// engine's one map-shaped arm. The imports are display.rs's own.

impl Emitter<'_> {
    /// Map display (`["k": v, …]`, `[:]` when empty) — split from
    /// `emit_display_at` for the complexity budget.
    fn emit_display_map(&mut self, kh: ETy, vh: ETy, ir: Option<&Ty>, path: &mut Vec<u32>) -> Result<(), EmitError> {
                let (k, v) = (self.types.el(kh), self.types.el(vh));
                let (koff, voff, esz) = crate::collections::entry_layout(k, v);
                let hb = self.hold_i32()?;
                let end = self.hold_i32()?;
                let cur = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                self.f.instructions().local_get(hb).i32_load(len_memarg()).i32_eqz();
                self.f.instructions().if_(BlockType::Empty);
                self.append_lit("[:]");
                self.f.instructions().else_();
                self.append_lit("[");
                {
                    let mut i = self.f.instructions();
                    i.local_get(hb)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_set(cur);
                    i.local_get(cur)
                        .local_get(hb)
                        .i32_load(len_memarg())
                        .i32_add()
                        .local_set(end);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
                    i.local_get(cur)
                        .local_get(hb)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .i32_ne()
                        .if_(BlockType::Empty);
                }
                self.append_lit(", ");
                self.f.instructions().end();
                self.f.instructions().local_get(cur).i32_const(koff as i32).i32_add();
                self.load_ty_slot_at(k);
                self.emit_display_at(k, true, ir_arg(ir, 0), path)?;
                self.append_lit(": ");
                self.f.instructions().local_get(cur).i32_const(voff as i32).i32_add();
                self.load_ty_slot_at(v);
                self.emit_display_at(v, true, ir_arg(ir, 1), path)?;
                {
                    let mut i = self.f.instructions();
                    i.local_get(cur).i32_const(esz as i32).i32_add().local_set(cur);
                    i.br(0);
                    i.end();
                    i.end();
                }
                self.append_lit("]");
                self.f.instructions().end();
                self.release_i32();
                self.release_i32();
                self.release_i32();
        Ok(())
    }
}
