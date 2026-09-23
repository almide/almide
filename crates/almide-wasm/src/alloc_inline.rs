//! The INLINE allocation fast path for fixed-size blocks (#2318 direction 2).
//!
//! `$alloc` (runtime_alloc.rs) takes a runtime length: it rounds to the
//! size class with `clz`, consults that class's free list, then bumps with
//! a grow test. A variant constructor's block has a compile-time size, so
//! the class, the class slot and the class capacity all fold to constants,
//! and what remains of the fast path is two loads and one compare:
//!
//!   free list of the class is empty  AND  the class capacity fits below
//!   the current memory size  ⇒  bump: three header stores, advance `$heap`.
//!
//! Anything else — a filed block to reuse, a grow, a wrap — takes the
//! generic `$alloc` call, which is the ONLY place those paths live. The
//! inline arm allocates exactly the block `$alloc`'s bump arm would (same
//! base, same class-rounded advance, same rc/len/cap header), so stdout,
//! the `__heap` watermark and the alloc-count ledger are unchanged by
//! construction; only the instruction count per allocation moves.
//!
//! Inside a region window (region.rs) the class heads are zero by
//! construction (`RegionSave` zeroes them), so a window-local constructor
//! is a pure bump — binarytrees' `Node` per node, no call. That is also
//! the only place the arm is emitted: in a REGION-PURE fn, the set a
//! window's producer is drawn from. Elsewhere the class lists refill as
//! blocks are freed, the empty test mostly fails, and the arm would be
//! bytes for nothing.
//!
//! The fit test is `heap + (cap - 1) < memory.size << 16`: with the `- 1`
//! a request whose end would wrap past 4 GiB cannot read as in range (the
//! sum wraps to 0xFFFF_FFFF and fails the compare), and a 4 GiB memory
//! shifts to 0 and refuses every inline bump — both land in `$alloc`,
//! whose wrap guard and C-197 grow path answer them.
//!
//! Counters (#2407): when the alloc-count switch is armed the inline arm
//! bumps `__alloc_count` and adds to `__alloc_bytes` exactly as `$alloc`
//! does, so the count ledger reads the same number of ALLOCATIONS whether
//! a site inlined or called.

use wasm_encoder::{BlockType, MemArg};

use crate::emitter::Emitter;
use crate::*;

fn word(offset: u32) -> MemArg {
    MemArg { offset: u64::from(offset), align: 2, memory_index: 0 }
}

/// The size class `$alloc` files a `len`-byte payload under, and that
/// class's total capacity (header + payload): `None` when the request is
/// above the class ceiling (never for a constructor, kept honest anyway).
pub(crate) fn fixed_class(len: u32) -> Option<(u32, u32)> {
    let want = ((almide_layout::PAYLOAD + len + 3) & !3).max(16);
    let class = 28 - (want - 1).leading_zeros();
    (class < FREELIST_CLASSES).then(|| (class, 16u32 << class))
}

impl Emitter<'_> {
    /// The first alloc-counter global when the switch is armed: the four
    /// counters are appended right after the top-let globals
    /// (assembly.rs), whose slots are `G_FIXED_COUNT..` contiguous — the
    /// map also carries use-site aliases, so count slots, not entries.
    fn alloc_counter_base(&self) -> Option<u32> {
        crate::alloc_count::armed()
            .then(|| self.globals.values().map(|&(slot, _)| slot + 1).max().unwrap_or(G_FIXED_COUNT))
    }

    /// Is the fn under emission REGION-PURE (region.rs) — one a window's
    /// producer can be? Only there is the inline arm worth its bytes: a
    /// window zeroes the class heads, so the empty-list test holds for
    /// every allocation inside it, while outside a window freed blocks
    /// refill the lists and the arm mostly falls through to `$alloc`
    /// anyway (the unrestricted form grew 82 corpus modules by 22.8 KB).
    fn in_region_pure_fn(&self) -> bool {
        let Some(me) = self.self_index else { return false };
        self.work.region_pure.borrow().iter().any(|&g| self.table.infos[g].wasm_index == me)
    }

    /// Leave the base of a fresh `len`-byte block on the stack, rc 1,
    /// through the inline bump when it applies and `$alloc` otherwise.
    /// `scratch` is a caller-held i32 local the fast arm may clobber.
    pub(crate) fn emit_alloc_fixed(&mut self, len: u32, scratch: u32) {
        let inline = if self.in_region_pure_fn() { fixed_class(len) } else { None };
        let Some((class, cap)) = inline else {
            self.f.instructions().i32_const(len as i32).call(F_ALLOC);
            return;
        };
        let counters = self.alloc_counter_base();
        let mut i = self.f.instructions();
        // free list of the class empty …
        i.i32_const((FREELIST_BASE + 4 * class) as i32).i32_load(word(0)).i32_eqz();
        // … and the class capacity fits below the memory size
        i.global_get(G_HEAP).i32_const((cap - 1) as i32).i32_add();
        i.memory_size(0).i32_const(16).i32_shl().i32_lt_u();
        i.i32_and().if_(BlockType::Result(wasm_encoder::ValType::I32));
        i.global_get(G_HEAP).local_set(scratch);
        i.local_get(scratch).i32_const(1).i32_store(word(almide_layout::RC.offset));
        i.local_get(scratch).i32_const(len as i32).i32_store(word(almide_layout::LEN.offset));
        i.local_get(scratch).i32_const((cap - almide_layout::PAYLOAD) as i32).i32_store(word(almide_layout::CAP.offset));
        i.local_get(scratch).i32_const(cap as i32).i32_add().global_set(G_HEAP);
        if let Some(c) = counters {
            let count = c + crate::alloc_count::COUNT;
            i.global_get(count).i64_const(1).i64_add().global_set(count);
            let bytes = c + crate::alloc_count::BYTES;
            i.global_get(bytes).i64_const(i64::from(len)).i64_add().global_set(bytes);
        }
        i.local_get(scratch);
        i.else_();
        i.i32_const(len as i32).call(F_ALLOC);
        i.end();
    }
}

#[cfg(test)]
mod tests {
    use super::fixed_class;

    /// The folded class must agree with `$alloc`'s runtime arithmetic
    /// (`want = max(16, round4(PAYLOAD + len))`, `class = 28 - clz(want - 1)`).
    #[test]
    fn folded_class_matches_the_runtime_arithmetic() {
        for len in 0..=4096u32 {
            let want = ((almide_layout::PAYLOAD + len + 3) & !3).max(16);
            let class = 28 - (want - 1).leading_zeros();
            assert_eq!(fixed_class(len), Some((class, 16 << class)), "len {len}");
            assert!(16 << class >= want, "class capacity covers the request at len {len}");
        }
        // binarytrees' `Node(Tree, Tree)`: 4-byte tag + two handles at 8/12.
        assert_eq!(fixed_class(16), Some((1, 32)));
        // Above the class ceiling nothing is inlined.
        assert_eq!(fixed_class(1 << 20), None);
    }
}
