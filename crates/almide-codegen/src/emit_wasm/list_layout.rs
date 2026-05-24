//! Heap layout constants — single source of truth for collection headers.
//!
//! All list/string/map creation and data access in the WASM emitter should use
//! these constants instead of hardcoding offsets. This prevents layout changes
//! from requiring shotgun surgery across 20+ files.

use super::FuncCompiler;

// ── List: [len:i32 @ 0][cap:i32 @ 4][data @ 8...] ──

/// Byte offset from list pointer to first data element.
pub const DATA_OFFSET: i32 = 8;

/// Header size in bytes (len + cap fields).
pub const HEADER_SIZE: i32 = 8;

// ── String: [byte_len:i32 @ 0][data @ 4...] ──

/// String layout: [len:i32 @ 0][cap:i32 @ 4][data @ 8...]
/// len = used byte count, cap = allocated byte count (>= len).
/// Capacity enables amortized O(1) append for `var s; s = s + "x"`.

/// Byte offset from string pointer to UTF-8 data.
pub const STRING_DATA_OFFSET: i32 = 8;

/// Byte offset to capacity field.
pub const STRING_CAP_OFFSET: i32 = 4;

/// String header size in bytes (len + cap).
pub const STRING_HEADER_SIZE: i32 = 8;

// ── Map (hash table): [len:i32 @ 0][cap:i32 @ 4][slots @ 8...] ──
// Each slot: [tag:i32][key:K][val:V]  tag: 0=empty, 1=occupied

/// Byte offset from map pointer to first slot (legacy) / tag array (Swiss Table).
pub const MAP_DATA_OFFSET: i32 = 8;

/// Byte offset from map pointer to tag array (Swiss Table layout).
pub const MAP_TAGS_OFFSET: i32 = 8;

/// Byte offset to capacity field.
pub const MAP_CAP_OFFSET: i32 = 4;

/// Map header size in bytes (len + cap).
pub const MAP_HEADER_SIZE: i32 = 8;

/// Tag field size in each slot.
pub const MAP_SLOT_TAG_SIZE: i32 = 4;

/// Initial hash table capacity (must be power of 2).
pub const MAP_INITIAL_CAP: i32 = 16;

/// Tag value for empty slot (0x00, matches bump allocator zero-fill).
/// Full slots store h2 (0x01..0x7F, never 0x00).
pub const MAP_TAG_EMPTY: i32 = 0;

// ── Set: same layout as List (to_list returns identity) ──

/// Byte offset from set pointer to data. Same as list.
pub const SET_DATA_OFFSET: i32 = DATA_OFFSET;

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

    // ���─ List length ──

    /// Emit: load list length (i32) from list at local.
    /// Leaves len on the WASM stack.
    pub fn emit_list_len(&mut self, list_local: u32) {
        wasm!(self.func, {
            local_get(list_local);
            i32_load(0);
        });
    }
}
