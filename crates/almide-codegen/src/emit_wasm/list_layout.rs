//! Heap layout constants — single source of truth for collection headers.
//!
//! Legacy constants kept for backward compatibility during migration.
//! New code should use `engine::LayoutRegistry` via `WasmBuilder`.

use super::FuncCompiler;

// ── Legacy constants (used by existing code not yet migrated) ──

pub const DATA_OFFSET: i32 = 8;
pub const HEADER_SIZE: i32 = 8;
pub const STRING_DATA_OFFSET: i32 = 8;
pub const STRING_CAP_OFFSET: i32 = 4;
pub const STRING_HEADER_SIZE: i32 = 8;
pub const MAP_DATA_OFFSET: i32 = 8;
pub const MAP_TAGS_OFFSET: i32 = 8;
pub const MAP_CAP_OFFSET: i32 = 4;
pub const MAP_HEADER_SIZE: i32 = 8;
pub const MAP_SLOT_TAG_SIZE: i32 = 4;
pub const MAP_INITIAL_CAP: i32 = 16;
pub const MAP_TAG_EMPTY: i32 = 0;
pub const ALLOC_HEADER_SIZE: i32 = 8;
pub const RC_OFFSET: i32 = 4;
pub const SET_DATA_OFFSET: i32 = DATA_OFFSET;

// ── Migrated methods: offsets from LayoutRegistry, zero magic numbers ──

impl FuncCompiler<'_> {
    /// List data address. Stack: `[] → [data_ptr]`
    pub fn emit_list_data_addr(&mut self, list_local: u32) {
        use super::engine::{WasmBuilder, layout::{LIST, list}};
        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
        w.get(list_local).field_addr(LIST, list::DATA);
    }

    /// List element address. Stack: `[] → [elem_ptr]`
    pub fn emit_list_elem_addr(&mut self, list_local: u32, idx_local: u32, elem_size: u32) {
        use super::engine::{WasmBuilder, layout::{LIST, list}};
        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
        w.get(list_local).field_addr(LIST, list::DATA);
        w.get(idx_local).i32c(elem_size as i32).mul().add();
    }

    /// Allocate list for `len_local` elements. Returns scratch local with ptr.
    pub fn emit_list_alloc(&mut self, len_local: u32, elem_size: u32) -> u32 {
        use super::engine::{WasmBuilder, layout::LIST};
        let dst = self.scratch.alloc_i32();
        let alloc_fn = self.emitter.rt.alloc;
        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
        w.alloc_collection(LIST, len_local, elem_size, dst, alloc_fn);
        dst
    }

    /// Allocate empty list (len=0). Returns scratch local with ptr.
    pub fn emit_list_alloc_empty(&mut self) -> u32 {
        use super::engine::{WasmBuilder, layout::{LIST, list}};
        let dst = self.scratch.alloc_i32();
        let alloc_fn = self.emitter.rt.alloc;
        let hdr = self.emitter.layout_reg.header_size(LIST);
        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
        w.i32c(hdr as i32).call(alloc_fn).tee(dst);
        w.i32c(0).field_store(LIST, list::LEN);
        dst
    }

    /// List length. Stack: `[] → [len:i32]`
    pub fn emit_list_len(&mut self, list_local: u32) {
        use super::engine::{WasmBuilder, layout::{LIST, list}};
        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
        w.get(list_local).field_load(LIST, list::LEN);
    }

    // ── Compact-ordered-dict tag helpers (the tags array is unchanged from the old layout) ──

    /// Load the COD tag byte at slot index. Stack: `[] → [tag:i32]`
    pub fn emit_swiss_tag_load(&mut self, map: u32, idx: u32) {
        use super::engine::{WasmBuilder, layout::{SWISS_MAP, map as m, MemType}};
        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
        w.get(map).field_addr(SWISS_MAP, m::TAGS).get(idx).add();
        w.emit_load(0, MemType::U8);
    }

    /// Store Swiss Table tag at index. Stack: `[tag:i32] → []`
    pub fn emit_swiss_tag_store(&mut self, map: u32, idx: u32) {
        use super::engine::{WasmBuilder, layout::{SWISS_MAP, map as m, MemType}};
        let tmp = self.scratch.alloc_i32();
        wasm!(self.func, { local_set(tmp); }); // save tag from stack
        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
        w.get(map).field_addr(SWISS_MAP, m::TAGS).get(idx).add();
        w.get(tmp).emit_store(0, MemType::U8);
        self.scratch.free_i32(tmp);
    }

    /// Map length. Stack: `[] → [len:i32]`
    pub fn emit_map_len(&mut self, map: u32) {
        use super::engine::{WasmBuilder, layout::{SWISS_MAP, map as m}};
        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
        w.get(map).field_load(SWISS_MAP, m::LEN);
    }

    /// Store map length. Stack: `[] → []`. `len_local` has the value.
    pub fn emit_map_store_len(&mut self, map: u32, len_local: u32) {
        use super::engine::{WasmBuilder, layout::{SWISS_MAP, map as m}};
        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
        w.get(map).get(len_local).field_store(SWISS_MAP, m::LEN);
    }

    /// Allocate empty map (len=0, cap=0). Returns scratch local.
    pub fn emit_map_alloc_empty(&mut self) -> u32 {
        use super::engine::{WasmBuilder, layout::{SWISS_MAP, map as m}};
        let dst = self.scratch.alloc_i32();
        let hdr = self.emitter.layout_reg.header_size(SWISS_MAP);
        let alloc_fn = self.emitter.rt.alloc;
        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
        w.i32c(hdr as i32).call(alloc_fn).tee(dst);
        w.i32c(0).field_store(SWISS_MAP, m::LEN);
        w.get(dst).i32c(0).field_store(SWISS_MAP, m::CAP);
        dst
    }

    /// Allocate map with given capacity. Writes len=0, cap=cap_val.
    /// Total size = header + cap (tags) + cap * entry_stride (entries).
    /// Returns scratch local.
    pub fn emit_map_alloc(&mut self, cap_val: u32, entry_stride: u32) -> u32 {
        use super::engine::{WasmBuilder, layout::{SWISS_MAP, map as m}};
        let dst = self.scratch.alloc_i32();
        let hdr = self.emitter.layout_reg.header_size(SWISS_MAP);
        let alloc_fn = self.emitter.rt.alloc;
        // total = hdr + cap + cap * entry_stride
        let total = hdr + cap_val + cap_val * entry_stride;
        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
        w.i32c(total as i32).call(alloc_fn).tee(dst);
        w.i32c(0).field_store(SWISS_MAP, m::LEN);
        w.get(dst).i32c(cap_val as i32).field_store(SWISS_MAP, m::CAP);
        dst
    }
}
