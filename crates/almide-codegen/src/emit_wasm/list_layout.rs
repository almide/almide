//! List memory layout helpers — single source of truth for [len:i32][cap:i32][data...].
//!
//! All list creation and data access in the WASM emitter should use these helpers
//! instead of hardcoding offsets. This prevents layout changes from requiring
//! shotgun surgery across 20+ files.

use super::FuncCompiler;

/// List header: [len:i32 @ 0][cap:i32 @ 4], data starts at offset 8.
pub const DATA_OFFSET: i32 = 8;

/// Header size in bytes (same as DATA_OFFSET for lists).
pub const HEADER_SIZE: i32 = 8;

impl FuncCompiler<'_> {
    // ── List data access ──

    /// Emit: local_get(list) + DATA_OFFSET → address of first element.
    /// Leaves the data pointer on the WASM stack.
    pub fn emit_list_data_addr(&mut self, list_local: u32) {
        wasm!(self.func, {
            local_get(list_local);
            i32_const(DATA_OFFSET);
            i32_add;
        });
    }

    /// Emit: local_get(list) + DATA_OFFSET + idx_local * elem_size → element address.
    /// Leaves the element address on the WASM stack.
    pub fn emit_list_elem_addr(&mut self, list_local: u32, idx_local: u32, elem_size: u32) {
        wasm!(self.func, {
            local_get(list_local);
            i32_const(DATA_OFFSET);
            i32_add;
            local_get(idx_local);
            i32_const(elem_size as i32);
            i32_mul;
            i32_add;
        });
    }

    // ── List allocation ──

    /// Emit: allocate a list with space for `len_local` elements of `elem_size` bytes.
    /// Stores len in header. Cap is left as 0 (zero-initialized by bump allocator).
    /// Returns the scratch local holding the new list pointer.
    pub fn emit_list_alloc(&mut self, len_local: u32, elem_size: u32) -> u32 {
        let dst = self.scratch.alloc_i32();
        wasm!(self.func, {
            i32_const(HEADER_SIZE);
            local_get(len_local);
            i32_const(elem_size as i32);
            i32_mul;
            i32_add;
            call(self.emitter.rt.alloc);
            local_set(dst);
            // Store len
            local_get(dst);
            local_get(len_local);
            i32_store(0);
        });
        dst
    }

    /// Emit: allocate an empty list (len=0, no data space).
    /// Returns the scratch local holding the new list pointer.
    pub fn emit_list_alloc_empty(&mut self) -> u32 {
        let dst = self.scratch.alloc_i32();
        wasm!(self.func, {
            i32_const(HEADER_SIZE);
            call(self.emitter.rt.alloc);
            local_set(dst);
            local_get(dst);
            i32_const(0);
            i32_store(0);
        });
        dst
    }

    // ── List length ──

    /// Emit: load list length (i32) from list at local.
    /// Leaves len on the WASM stack.
    pub fn emit_list_len(&mut self, list_local: u32) {
        wasm!(self.func, {
            local_get(list_local);
            i32_load(0);
        });
    }
}
