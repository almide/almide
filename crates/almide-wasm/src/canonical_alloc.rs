//! The canonical-allocator switch: the stdlib inspection meter's fixed
//! allocator (owner ruling 2026-10-15).
//!
//! Under it, `$alloc` serves every request from the bump head. It consults
//! neither the size-class free lists nor the large-block list, so the bytes a
//! program copies (a growing buffer's `memory.copy`) depend only on the
//! requests it makes, not on which freed blocks the heap happens to hold.
//! Reuse can only remove copies, so a series measured canonically is an
//! upper bound of the shipped module's. `$free` still files small blocks
//! into their class lists (an O(1) push; nothing takes them back) and
//! abandons blocks above the class ceiling: filing one walks the
//! address-ordered large list, and since every bump lands above every
//! filed block, that walk covered all blocks released so far — a call's
//! cost grew with how many calls came before it, which the meter's k-scaled
//! difference read as growth in n (net.tcp_read_exact's wasm series).
//!
//! Not measured under the switch: the free-list walks themselves. The
//! allocator's own cost is covered by its proofs (proofs/LargeTree.v
//! transcribes the large list's take and insert).
//!
//! OFF (the default) the module is byte-identical to a build without the
//! switch. The CLI arms it from `ALMIDE_WASM_CANONICAL_ALLOC`. A test arms it
//! on its own thread with the guard, as with `alloc_count`.

use std::cell::Cell;

use wasm_encoder::{InstructionSink, MemArg};

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

/// Is `$alloc` being emitted without its free-list paths?
pub fn armed() -> bool {
    ARMED.with(|c| c.get())
}

/// Turn the switch on for a scope and restore the previous state on drop.
#[must_use = "the guard restores the previous state when dropped; binding it to `_` restores immediately"]
pub struct CanonicalGuard(bool);

impl CanonicalGuard {
    pub fn set() -> Self {
        let prev = armed();
        ARMED.with(|c| c.set(true));
        Self(prev)
    }
}

impl Drop for CanonicalGuard {
    fn drop(&mut self) {
        ARMED.with(|c| c.set(self.0));
    }
}

/// `$alloc`'s class-slot head, the value its pop test reads: the slot's
/// first word, or under the switch a constant 0 (an empty list), so the
/// request takes the bump path. Under the checked-allocator switch
/// (checked_alloc.rs, #4009) a non-empty head's poison is checked first;
/// `(head, p, end)` are `$alloc`'s head local and two scratch locals.
pub(crate) fn emit_class_head<'a, 'b>(i: &'a mut InstructionSink<'b>, canonical: bool, slot: u32, (head, p, end): (u32, u32, u32)) -> &'a mut InstructionSink<'b> {
    if canonical {
        i.i32_const(0)
    } else if crate::checked_alloc::armed() {
        i.local_get(slot).i32_load(MemArg { offset: 0, align: 2, memory_index: 0 }).local_tee(head);
        i.if_(wasm_encoder::BlockType::Empty);
        crate::checked_alloc::emit_poison_check(i, head, p, end);
        i.end();
        i.local_get(head)
    } else {
        i.local_get(slot).i32_load(MemArg { offset: 0, align: 2, memory_index: 0 })
    }
}

/// `$free`'s release of a block above the class ceiling: filed into the
/// large list (`runtime_large::emit_lfree`), or under the switch nothing —
/// the block is abandoned, as nothing would take it back (see the module
/// doc for the walk this avoids). `(b, t)` are the block and its total,
/// `walk` the three scratch locals the filing uses.
pub(crate) fn emit_large_release(i: &mut InstructionSink<'_>, canonical: bool, (b, t): (u32, u32), walk: (u32, u32, u32)) {
    if !canonical {
        crate::runtime_large::emit_lfree(i, (b, t), walk);
    }
}
