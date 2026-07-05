//! WasmBuilder — layout-safe WASM instruction builder.
//!
//! Every memory offset derives from `LayoutRegistry`.
//! Every alignment derives from `MemType::align_exp()`.
//! Zero magic numbers.

use wasm_encoder::{BlockType, Instruction, ValType};

use super::layout::*;
use crate::emit_wasm::TrackedFunction;

pub struct WasmBuilder<'a> {
    pub f: &'a mut TrackedFunction,
    reg: &'a LayoutRegistry,
}

fn ma(offset: u32, ty: MemType) -> wasm_encoder::MemArg {
    wasm_encoder::MemArg { offset: offset as u64, align: ty.align_exp(), memory_index: 0 }
}

impl<'a> WasmBuilder<'a> {
    pub fn new(f: &'a mut TrackedFunction, reg: &'a LayoutRegistry) -> Self {
        Self { f, reg }
    }

    pub fn registry(&self) -> &LayoutRegistry { self.reg }

    // ════════════════════════════════════════════════════════════════
    //  Layout-safe memory access
    // ════════════════════════════════════════════════════════════════

    /// Load a fixed field. Stack: `[base] → [value]`
    pub fn field_load(&mut self, layout: LayoutId, field: FieldId) -> &mut Self {
        let off = self.reg.fixed_offset(layout, field);
        let ty = self.reg.field(layout, field).ty;
        self.emit_load(off, ty)
    }

    /// Store a fixed field. Stack: `[base, value] → []`
    pub fn field_store(&mut self, layout: LayoutId, field: FieldId) -> &mut Self {
        let off = self.reg.fixed_offset(layout, field);
        let ty = self.reg.field(layout, field).ty;
        self.emit_store(off, ty)
    }

    /// Element address. Stack: `[base, index] → [addr]`
    pub fn elem_addr(&mut self, layout: LayoutId, field: FieldId, stride: u32) -> &mut Self {
        let off = self.reg.fixed_offset(layout, field);
        self.i32c(stride as i32).mul().add();
        if off != 0 { self.i32c(off as i32).add(); }
        self
    }

    /// Compute field address (not load value). Stack: `[base] → [base + offset]`
    pub fn field_addr(&mut self, layout: LayoutId, field: FieldId) -> &mut Self {
        let off = self.reg.fixed_offset(layout, field);
        if off != 0 { self.i32c(off as i32).add(); }
        self
    }

    /// Dynamic field address (Swiss Table entries). Stack: `[base] → [addr]`
    pub fn dyn_field_addr(&mut self, temp: u32, layout: LayoutId, field: FieldId) -> &mut Self {
        let mf = self.reg.field(layout, field);
        match &mf.offset {
            FieldOffset::Fixed(n) => {
                if *n != 0 { self.i32c(*n as i32).add(); }
            }
            FieldOffset::AfterDynamic { base, size_field } => {
                let size_off = self.reg.fixed_offset(layout, *size_field);
                let size_ty = self.reg.field(layout, *size_field).ty;
                self.tee(temp).emit_load(size_off, size_ty);
                self.get(temp).add();
                if *base != 0 { self.i32c(*base as i32).add(); }
            }
        }
        self
    }

    /// Load typed value at offset. Stack: `[addr] → [value]`
    pub fn emit_load(&mut self, offset: u32, ty: MemType) -> &mut Self {
        let a = ma(offset, ty);
        match ty {
            MemType::I32 => { self.f.instruction(&Instruction::I32Load(a)); }
            MemType::I64 => { self.f.instruction(&Instruction::I64Load(a)); }
            MemType::F32 => { self.f.instruction(&Instruction::F32Load(a)); }
            MemType::F64 => { self.f.instruction(&Instruction::F64Load(a)); }
            MemType::U8  => { self.f.instruction(&Instruction::I32Load8U(a)); }
        }
        self
    }

    /// Store typed value at offset. Stack: `[addr, value] → []`
    pub fn emit_store(&mut self, offset: u32, ty: MemType) -> &mut Self {
        let a = ma(offset, ty);
        match ty {
            MemType::I32 => { self.f.instruction(&Instruction::I32Store(a)); }
            MemType::I64 => { self.f.instruction(&Instruction::I64Store(a)); }
            MemType::F32 => { self.f.instruction(&Instruction::F32Store(a)); }
            MemType::F64 => { self.f.instruction(&Instruction::F64Store(a)); }
            MemType::U8  => { self.f.instruction(&Instruction::I32Store8(a)); }
        }
        self
    }

    // ════════════════════════════════════════════════════════════════
    //  Collection iteration
    // ════════════════════════════════════════════════════════════════

    /// Iterate `list[0..len]`.
    pub fn list_foreach(
        &mut self, list: u32, elem: u32, idx: u32, stride: u32,
        body: impl FnOnce(&mut Self),
    ) {
        let len_off = self.reg.fixed_offset(LIST, list::LEN);
        let data_off = self.reg.fixed_offset(LIST, list::DATA);
        let len_ty = self.reg.field(LIST, list::LEN).ty;

        self.i32c(0).set(idx);
        self.block(|w| { w.loop_(|w| {
            w.get(idx).get(list).emit_load(len_off, len_ty);
            w.ge_u().br_if(1);

            w.get(list).i32c(data_off as i32).add();
            w.get(idx).i32c(stride as i32).mul().add();
            w.set(elem);

            body(w);

            w.get(idx).i32c(1).add().set(idx);
            w.br(0);
        }); });
    }

    /// Iterate the dense entries of a compact-ordered-dict map in insertion order.
    /// `cap_l` is a scratch local (used transiently for the capacity, then the len
    /// bound). Walks `entries[0..len]` — every dense entry is occupied (no tag scan).
    pub fn map_foreach(
        &mut self, map: u32, entry: u32, cap_l: u32, eb: u32, idx: u32,
        entry_stride: u32, body: impl FnOnce(&mut Self),
    ) {
        let len_off = self.reg.fixed_offset(SWISS_MAP, map::LEN);
        let len_ty = self.reg.field(SWISS_MAP, map::LEN).ty;
        let cap_off = self.reg.fixed_offset(SWISS_MAP, map::CAP);
        let cap_ty = self.reg.field(SWISS_MAP, map::CAP).ty;
        let tags_off = self.reg.fixed_offset(SWISS_MAP, map::TAGS);

        // Dense entries base = map + header + cap + cap*INDEX_SLOT_SIZE (after tags + index).
        self.get(map).emit_load(cap_off, cap_ty).set(cap_l);
        self.get(map).i32c(tags_off as i32).add()
            .get(cap_l).add()
            .get(cap_l).i32c(map::INDEX_SLOT_SIZE as i32).mul().add()
            .set(eb);
        // Reuse cap_l to hold the len bound (cap only needed for the base above).
        self.get(map).emit_load(len_off, len_ty).set(cap_l);
        self.i32c(0).set(idx);

        self.block(|w| { w.loop_(|w| {
            w.get(idx).get(cap_l).ge_u().br_if(1);

            w.get(eb).get(idx).i32c(entry_stride as i32).mul().add().set(entry);

            body(w);

            w.get(idx).i32c(1).add().set(idx);
            w.br(0);
        }); });
    }

    // ════════════════════════════════════════════════════════════════
    //  Allocation
    // ════════════════════════════════════════════════════════════════

    /// Allocate collection: header + `len * stride`. Result in `out`.
    pub fn alloc_collection(
        &mut self, layout: LayoutId, len: u32, stride: u32,
        out: u32, alloc_fn: u32,
    ) {
        let hdr = self.reg.header_size(layout);
        let len_off = self.reg.fixed_offset(layout, FieldId(0)); // LEN is always field 0
        let len_ty = self.reg.field(layout, FieldId(0)).ty;
        let cap_off = self.reg.fixed_offset(layout, FieldId(1)); // CAP is always field 1
        let cap_ty = self.reg.field(layout, FieldId(1)).ty;

        self.get(len).i32c(stride as i32).mul().i32c(hdr as i32).add();
        self.call(alloc_fn).tee(out);
        self.get(len).emit_store(len_off, len_ty);
        self.get(out).get(len).emit_store(cap_off, cap_ty);
    }

    // ════════════════════════════════════════════════════════════════
    //  Perceus RC
    // ════════════════════════════════════════════════════════════════

    pub fn rc_inc(&mut self, ptr: u32, fn_idx: u32) -> &mut Self {
        self.get(ptr).call(fn_idx)
    }

    pub fn rc_dec(&mut self, ptr: u32, fn_idx: u32) -> &mut Self {
        self.get(ptr).call(fn_idx)
    }

    /// COW check: if rc > 1, clone.
    pub fn cow_check(&mut self, ptr: u32, clone: impl FnOnce(&mut Self)) {
        let neg = self.reg.alloc_header_neg_offset(alloc::RC);
        let rc_ty = self.reg.field(ALLOC_HEADER, alloc::RC).ty;
        self.get(ptr).i32c(neg as i32).sub().emit_load(0, rc_ty);
        self.i32c(1).gt_u();
        self.if_void(clone, |_| {});
    }

    // ════════════════════════════════════════════════════════════════
    //  Structured control flow
    // ════════════════════════════════════════════════════════════════

    pub fn block(&mut self, body: impl FnOnce(&mut Self)) {
        self.raw(Instruction::Block(BlockType::Empty)); body(self); self.raw(Instruction::End);
    }
    pub fn loop_(&mut self, body: impl FnOnce(&mut Self)) {
        self.raw(Instruction::Loop(BlockType::Empty)); body(self); self.raw(Instruction::End);
    }
    pub fn if_void(&mut self, t: impl FnOnce(&mut Self), e: impl FnOnce(&mut Self)) {
        self.raw(Instruction::If(BlockType::Empty));
        t(self); self.raw(Instruction::Else); e(self); self.raw(Instruction::End);
    }
    pub fn if_typed(&mut self, bt: BlockType, t: impl FnOnce(&mut Self), e: impl FnOnce(&mut Self)) {
        self.raw(Instruction::If(bt));
        t(self); self.raw(Instruction::Else); e(self); self.raw(Instruction::End);
    }
    pub fn if_i32(&mut self, t: impl FnOnce(&mut Self), e: impl FnOnce(&mut Self)) {
        self.if_typed(BlockType::Result(ValType::I32), t, e);
    }
    pub fn if_i64(&mut self, t: impl FnOnce(&mut Self), e: impl FnOnce(&mut Self)) {
        self.if_typed(BlockType::Result(ValType::I64), t, e);
    }

    // ════════════════════════════════════════════════════════════════
    //  Chainable primitives — no magic numbers, just instruction names
    // ════════════════════════════════════════════════════════════════

    pub fn raw(&mut self, i: Instruction<'_>) -> &mut Self { self.f.instruction(&i); self }

    // locals / globals
    pub fn get(&mut self, l: u32) -> &mut Self { self.f.instruction(&Instruction::LocalGet(l)); self }
    pub fn set(&mut self, l: u32) -> &mut Self { self.f.instruction(&Instruction::LocalSet(l)); self }
    pub fn tee(&mut self, l: u32) -> &mut Self { self.f.instruction(&Instruction::LocalTee(l)); self }
    pub fn gget(&mut self, g: u32) -> &mut Self { self.f.instruction(&Instruction::GlobalGet(g)); self }
    pub fn gset(&mut self, g: u32) -> &mut Self { self.f.instruction(&Instruction::GlobalSet(g)); self }

    // constants
    pub fn i32c(&mut self, v: i32) -> &mut Self { self.f.instruction(&Instruction::I32Const(v)); self }
    pub fn i64c(&mut self, v: i64) -> &mut Self { self.f.instruction(&Instruction::I64Const(v)); self }
    pub fn f64c(&mut self, v: f64) -> &mut Self { self.f.instruction(&Instruction::F64Const(v.into())); self }

    // i32 ops
    pub fn add(&mut self) -> &mut Self { self.f.instruction(&Instruction::I32Add); self }
    pub fn sub(&mut self) -> &mut Self { self.f.instruction(&Instruction::I32Sub); self }
    pub fn mul(&mut self) -> &mut Self { self.f.instruction(&Instruction::I32Mul); self }
    pub fn and(&mut self) -> &mut Self { self.f.instruction(&Instruction::I32And); self }
    pub fn or(&mut self) -> &mut Self  { self.f.instruction(&Instruction::I32Or); self }
    pub fn shl(&mut self) -> &mut Self { self.f.instruction(&Instruction::I32Shl); self }
    pub fn shr_u(&mut self) -> &mut Self { self.f.instruction(&Instruction::I32ShrU); self }
    pub fn eqz(&mut self) -> &mut Self { self.f.instruction(&Instruction::I32Eqz); self }
    pub fn eq(&mut self) -> &mut Self  { self.f.instruction(&Instruction::I32Eq); self }
    pub fn ne(&mut self) -> &mut Self  { self.f.instruction(&Instruction::I32Ne); self }
    pub fn lt_u(&mut self) -> &mut Self { self.f.instruction(&Instruction::I32LtU); self }
    pub fn ge_u(&mut self) -> &mut Self { self.f.instruction(&Instruction::I32GeU); self }
    pub fn gt_u(&mut self) -> &mut Self { self.f.instruction(&Instruction::I32GtU); self }

    // control
    pub fn br(&mut self, d: u32) -> &mut Self { self.f.instruction(&Instruction::Br(d)); self }
    pub fn br_if(&mut self, d: u32) -> &mut Self { self.f.instruction(&Instruction::BrIf(d)); self }
    pub fn call(&mut self, i: u32) -> &mut Self { self.f.instruction(&Instruction::Call(i)); self }
    pub fn call_indirect(&mut self, sig: u32) -> &mut Self {
        self.f.instruction(&Instruction::CallIndirect { type_index: sig, table_index: 0 }); self
    }
    pub fn ret(&mut self) -> &mut Self { self.f.instruction(&Instruction::Return); self }
    pub fn drop_(&mut self) -> &mut Self { self.f.instruction(&Instruction::Drop); self }
    pub fn unreachable_(&mut self) -> &mut Self { self.f.instruction(&Instruction::Unreachable); self }

    // memory
    pub fn memory_copy(&mut self) -> &mut Self {
        self.f.instruction(&Instruction::MemoryCopy { src_mem: 0, dst_mem: 0 }); self
    }
    pub fn memory_size(&mut self) -> &mut Self { self.f.instruction(&Instruction::MemorySize(0)); self }
    pub fn memory_grow(&mut self) -> &mut Self { self.f.instruction(&Instruction::MemoryGrow(0)); self }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit_wasm::TrackedFunction;

    fn func() -> TrackedFunction {
        TrackedFunction::new(vec![(6, ValType::I32)])
    }

    #[test]
    fn field_load_derives_offset_and_alignment() {
        let reg = LayoutRegistry::new();
        let mut f = func();
        let mut w = WasmBuilder::new(&mut f, &reg);
        // String LEN @ 0, align=2; CAP @ 4, align=2; DATA @ 8, align=0
        w.get(0).field_load(STRING, string::LEN);
        w.get(0).field_load(STRING, string::CAP);
        w.get(0).field_load(STRING, string::DATA);
    }

    #[test]
    fn alloc_collection_uses_layout_fields() {
        let reg = LayoutRegistry::new();
        let mut f = func();
        let mut w = WasmBuilder::new(&mut f, &reg);
        // This must NOT contain any hardcoded 0/4 offsets — all from LayoutRegistry
        w.alloc_collection(LIST, 0, 4, 1, 10);
    }

    #[test]
    fn cow_check_uses_alloc_header_layout() {
        let reg = LayoutRegistry::new();
        let mut f = func();
        let mut w = WasmBuilder::new(&mut f, &reg);
        w.cow_check(0, |w| { w.unreachable_(); });
    }

    #[test]
    fn list_foreach_uses_layout() {
        let reg = LayoutRegistry::new();
        let mut f = func();
        let mut w = WasmBuilder::new(&mut f, &reg);
        w.list_foreach(0, 1, 2, 4, |w| { w.get(1).emit_load(0, MemType::I32).drop_(); });
    }

    #[test]
    fn map_foreach_uses_layout() {
        let reg = LayoutRegistry::new();
        let mut f = func();
        let mut w = WasmBuilder::new(&mut f, &reg);
        w.map_foreach(0, 1, 2, 3, 4, 8, |w| { w.get(1).emit_load(0, MemType::I32).drop_(); });
    }

    #[test]
    fn dyn_field_addr_from_layout() {
        let reg = LayoutRegistry::new();
        let mut f = func();
        let mut w = WasmBuilder::new(&mut f, &reg);
        w.get(0).dyn_field_addr(1, SWISS_MAP, map::ENTRIES);
    }

    // ── Perceus integration tests ──

    #[test]
    fn perceus_list_child_dec() {
        let reg = LayoutRegistry::new();
        let mut f = func();
        let mut w = WasmBuilder::new(&mut f, &reg);
        let rc_dec_fn = 12;
        // Typed RcDec for List[String]: iterate elements, rc_dec each
        w.list_foreach(0, 1, 2, MemType::I32.byte_size(), |w| {
            w.get(1).emit_load(0, MemType::I32); // load element ptr
            w.call(rc_dec_fn);
        });
        w.rc_dec(0, rc_dec_fn);
    }

    #[test]
    fn perceus_closure_env_dec() {
        let reg = LayoutRegistry::new();
        let mut f = func();
        let mut w = WasmBuilder::new(&mut f, &reg);
        let rc_dec_fn = 12;
        // Load env_ptr from closure pair, then rc_dec it
        w.get(0).field_load(CLOSURE_PAIR, closure::ENV_PTR);
        w.call(rc_dec_fn);
        w.rc_dec(0, rc_dec_fn);
    }

    #[test]
    fn perceus_option_child_dec() {
        let reg = LayoutRegistry::new();
        let mut f = func();
        let mut w = WasmBuilder::new(&mut f, &reg);
        let rc_dec_fn = 12;
        // Option[String]: if tag == Some(1), dec the payload
        w.get(0).field_load(OPTION, tagged::TAG);
        w.if_void(|w| {
            w.get(0).field_load(OPTION, tagged::PAYLOAD);
            w.call(rc_dec_fn);
        }, |_| {});
        w.rc_dec(0, rc_dec_fn);
    }

    #[test]
    fn perceus_variant_tag_dispatch() {
        let reg = LayoutRegistry::new();
        let mut f = func();
        let mut w = WasmBuilder::new(&mut f, &reg);
        // Load variant tag via layout, no hardcoded 0 offset
        w.get(0).field_load(VARIANT, tagged::TAG);
        w.i32c(1).eq(); // check if tag == 1
        w.drop_();
    }

    #[test]
    fn perceus_map_child_dec() {
        let reg = LayoutRegistry::new();
        let mut f = func();
        let mut w = WasmBuilder::new(&mut f, &reg);
        let rc_dec_fn = 12;
        let entry_stride = 8; // key:i32 + val:i32
        // Map[String, String]: iterate entries, dec both key and val
        w.map_foreach(0, 1, 2, 3, 4, entry_stride, |w| {
            w.get(1).emit_load(0, MemType::I32).call(rc_dec_fn); // key
            let val_off = MemType::I32.byte_size();
            w.get(1).emit_load(val_off, MemType::I32).call(rc_dec_fn); // val
        });
        w.rc_dec(0, rc_dec_fn);
    }
}
