//! ScratchAllocator: bump/reuse allocator for WASM temporary locals.
//!
//! Replaces the old match_i32_base/match_i64_base/match_depth system.
//! Each type (i32, i64, f64) has independent slots. alloc() returns a WASM local index,
//! free() marks it for reuse. High-water mark is tracked for local declaration.

use wasm_encoder::ValType;

/// Tracks allocation state for one WASM value type.
struct TypedSlots {
    /// true = in use
    slots: Vec<bool>,
    /// High-water mark: max simultaneous live slots
    hwm: usize,
    /// WASM local index where this type's slots start (set after all bind locals are allocated)
    base: u32,
    /// Max pre-allocated slots (overflow beyond this corrupts adjacent type regions)
    capacity: usize,
}

impl TypedSlots {
    fn new() -> Self {
        Self { slots: Vec::new(), hwm: 0, base: 0, capacity: 0 }
    }

    fn alloc(&mut self) -> u32 {
        // Find first free slot
        if let Some(idx) = self.slots.iter().position(|&used| !used) {
            self.slots[idx] = true;
            self.update_hwm();
            self.base + idx as u32
        } else {
            // All slots in use — add a new one
            let idx = self.slots.len();
            assert!(
                idx < self.capacity,
                "ScratchAllocator overflow: need {} slots but only {} pre-allocated (base={})",
                idx + 1, self.capacity, self.base,
            );
            self.slots.push(true);
            self.update_hwm();
            self.base + idx as u32
        }
    }

    fn free(&mut self, local_idx: u32) {
        let slot = (local_idx - self.base) as usize;
        debug_assert!(slot < self.slots.len(), "free: slot {} out of range (len={})", slot, self.slots.len());
        debug_assert!(self.slots[slot], "free: slot {} already free (double-free)", slot);
        self.slots[slot] = false;
    }

    fn update_hwm(&mut self) {
        let live = self.slots.iter().filter(|&&u| u).count();
        if live > self.hwm {
            self.hwm = live;
        }
    }

    fn live_count(&self) -> usize {
        self.slots.iter().filter(|&&u| u).count()
    }
}

/// Scratch local allocator for a single WASM function.
///
/// Usage:
/// 1. Create with `new()`
/// 2. Set bases with `set_bases(i32_base, i64_base, f64_base)` after bind locals are allocated
/// 3. During emit: `alloc_i32()`, `free_i32(idx)`, etc.
/// 4. After emit: `hwm_i32()`, `hwm_i64()`, `hwm_f64()` for local declarations
pub struct ScratchAllocator {
    i32: TypedSlots,
    i64: TypedSlots,
    f64: TypedSlots,
}

impl ScratchAllocator {
    pub fn new() -> Self {
        Self {
            i32: TypedSlots::new(),
            i64: TypedSlots::new(),
            f64: TypedSlots::new(),
        }
    }

    pub fn set_bases_with_capacity(&mut self, i32_base: u32, i32_cap: usize, i64_base: u32, i64_cap: usize, f64_base: u32, f64_cap: usize) {
        self.i32.base = i32_base;
        self.i32.capacity = i32_cap;
        self.i64.base = i64_base;
        self.i64.capacity = i64_cap;
        self.f64.base = f64_base;
        self.f64.capacity = f64_cap;
    }

    pub fn set_bases(&mut self, i32_base: u32, i64_base: u32, f64_base: u32) {
        self.i32.base = i32_base;
        self.i64.base = i64_base;
        self.f64.base = f64_base;
    }

    // ── Alloc ──

    pub fn alloc_i32(&mut self) -> u32 { self.i32.alloc() }
    pub fn alloc_i64(&mut self) -> u32 { self.i64.alloc() }
    pub fn alloc_f64(&mut self) -> u32 { self.f64.alloc() }

    /// Allocate a scratch local for the given ValType.
    pub fn alloc(&mut self, vt: ValType) -> u32 {
        match vt {
            ValType::I32 => self.alloc_i32(),
            ValType::I64 => self.alloc_i64(),
            ValType::F64 => self.alloc_f64(),
            _ => self.alloc_i32(), // fallback
        }
    }

    // ── Free ──

    pub fn free_i32(&mut self, idx: u32) { self.i32.free(idx); }
    pub fn free_i64(&mut self, idx: u32) { self.i64.free(idx); }
    pub fn free_f64(&mut self, idx: u32) { self.f64.free(idx); }

    /// Free a scratch local for the given ValType.
    pub fn free(&mut self, idx: u32, vt: ValType) {
        match vt {
            ValType::I32 => self.free_i32(idx),
            ValType::I64 => self.free_i64(idx),
            ValType::F64 => self.free_f64(idx),
            _ => self.free_i32(idx),
        }
    }

    // ── HWM queries (for local declaration) ──

    pub fn hwm_i32(&self) -> usize { self.i32.hwm }
    pub fn hwm_i64(&self) -> usize { self.i64.hwm }
    pub fn hwm_f64(&self) -> usize { self.f64.hwm }

    /// Assert no scratch locals are still live (call at function end).
    pub fn assert_all_freed(&self) {
        let i32_live = self.i32.live_count();
        let i64_live = self.i64.live_count();
        let f64_live = self.f64.live_count();
        debug_assert!(
            i32_live == 0 && i64_live == 0 && f64_live == 0,
            "Scratch leak: i32={} i64={} f64={} still live at function end",
            i32_live, i64_live, f64_live,
        );
    }
}
