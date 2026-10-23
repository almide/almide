//! The checked-allocator switch (#4009 item 2): the structural wasm leg's
//! answer to running native code under ASan.
//!
//! A wasm out-of-bounds or use-after-free write inside linear memory is not
//! a trap — the module keeps running on corrupted data and can exit 0. Under
//! the switch:
//!
//! - `$free` fills a size-class block's payload past the free-list link with
//!   the poison byte 0xA5 after filing it;
//! - `$alloc`, before handing a filed block back out, checks every poisoned
//!   word is still 0xA5A5A5A5 and, when one is not, prints [`TEXT`] and
//!   exits [`EXIT`] — a
//!   write landed in the block while it was free (a use-after-free write, or
//!   an out-of-bounds write from a neighbour);
//! - a read after free sees poison instead of the old value, which the
//!   differential run (scripts/wasm-checked-alloc.sh: checked build against
//!   the ordinary build) shows as a changed output.
//!
//! - every block `$alloc` hands out is [`CANARY`] bytes larger than the
//!   request, its header's `cap` stops short of them, and they hold the
//!   canary word; `$free` checks the canary before filing the block and,
//!   when it is broken, prints [`OVERRUN_TEXT`] and exits [`EXIT`] — a
//!   write ran past the end of the block's usable capacity (a heap
//!   overflow).
//!
//! Not covered: poison in blocks above the class ceiling (the large list
//! keeps its own links in the payload) and in blocks too small to file; an
//! overrun of a block that is never freed (a top-level value, a region the
//! window discards wholesale) or one that skips the canary and lands
//! further on; and an out-of-bounds write INSIDE the usable capacity — its
//! corruption is only seen if it changes the output. Inside a region-pure
//! fn the inline allocation arm (alloc_inline.rs) is off under the switch,
//! so every block takes `$alloc`. The shipped module never carries this code: OFF (the default) the
//! module is byte-identical to a build without the switch, so the decoded
//! `$alloc` / `$free` bytes the proofs read (StructuralDecode.v) are
//! unchanged. The CLI arms it from `ALMIDE_WASM_CHECKED_ALLOC`; a test arms
//! it on its own thread with the guard, as with `canonical_alloc`.

use std::cell::Cell;

use wasm_encoder::{BlockType, InstructionSink, MemArg};

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static MESSAGE: Cell<u32> = const { Cell::new(0) };
    static OVERRUN_MESSAGE: Cell<u32> = const { Cell::new(0) };
}

/// The exit status of a failed check — distinct from a program's own
/// `exit 1`, so a gate cannot mistake one for the other.
pub const EXIT: i32 = 70;

/// The failed check's message.
pub const TEXT: &str = "Error: heap check: a freed block was written (a use after free, or a stray write)";

/// The broken canary's message.
pub const OVERRUN_TEXT: &str = "Error: heap check: a write ran past the end of a block (a heap overflow)";

/// Record the pooled address of [`TEXT`] for `$alloc` (assembly.rs interns
/// it only while armed, so the shipped data segment is unchanged).
pub(crate) fn set_message(addr: u32) {
    MESSAGE.with(|c| c.set(addr));
}

/// Record the pooled address of [`OVERRUN_TEXT`] for `$free`.
pub(crate) fn set_overrun_message(addr: u32) {
    OVERRUN_MESSAGE.with(|c| c.set(addr));
}

/// The canary: this many bytes past a block's usable capacity, two words of
/// [`CANARY_WORD`].
pub const CANARY: i32 = 8;
const CANARY_WORD: i32 = 0xC3C3_C3C3_u32 as i32;

/// The poison byte, and the word four of them make.
pub const POISON: u8 = 0xA5;
const POISON_WORD: i32 = 0xA5A5_A5A5_u32 as i32;

/// Is the allocator being emitted with its poison and check?
pub fn armed() -> bool {
    ARMED.with(|c| c.get())
}

/// Turn the switch on for a scope and restore the previous state on drop.
#[must_use = "the guard restores the previous state when dropped; binding it to `_` restores immediately"]
pub struct CheckedGuard(bool);

impl CheckedGuard {
    pub fn set() -> Self {
        let prev = armed();
        ARMED.with(|c| c.set(true));
        Self(prev)
    }
}

impl Drop for CheckedGuard {
    fn drop(&mut self) {
        ARMED.with(|c| c.set(self.0));
    }
}

/// The first poisoned byte: past the header and the free-list link.
const FIRST: i32 = almide_layout::PAYLOAD as i32 + 4;

/// `$free`, after filing `block` (whose total size is in local `total`):
/// poison `[block + FIRST, block + total)`. Nothing when the switch is off.
pub(crate) fn emit_poison(i: &mut InstructionSink<'_>, block: u32, total: u32) {
    if !armed() {
        return;
    }
    i.local_get(block).i32_const(FIRST).i32_add();
    i.i32_const(i32::from(POISON));
    i.local_get(total).i32_const(FIRST).i32_sub();
    i.memory_fill(0);
}

/// `$alloc`, about to pop `head` off a class list (its header still the
/// freed block's): trap unless every word of `[head + FIRST, head + total)`
/// is poison, `total` re-derived from the header's `cap` exactly as `$free`
/// derived it. `p` and `end` are two scratch i32 locals.
pub(crate) fn emit_poison_check(i: &mut InstructionSink<'_>, head: u32, p: u32, end: u32) {
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    // end = head + ((cap + PAYLOAD + 3) & !3)
    i.local_get(head)
        .local_get(head)
        .i32_load(word(almide_layout::CAP.offset))
        .i32_const(almide_layout::PAYLOAD as i32 + 3)
        .i32_add()
        .i32_const(-4)
        .i32_and()
        .i32_add()
        .local_set(end);
    i.local_get(head).i32_const(FIRST).i32_add().local_set(p);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p).i32_load(word(0)).i32_const(POISON_WORD).i32_ne().if_(BlockType::Empty);
    i.i32_const(MESSAGE.with(|c| c.get()) as i32).call(crate::F_EPRINTLN_BLOCK);
    i.i32_const(EXIT).call(crate::F_EXIT_IMPORT).unreachable();
    i.end();
    i.local_get(p).i32_const(4).i32_add().local_set(p);
    i.br(0);
    i.end().end();
}

fn word(offset: u32) -> MemArg {
    MemArg { offset: u64::from(offset), align: 2, memory_index: 0 }
}

/// `$alloc`, on entry: size the block [`CANARY`] bytes past the request
/// (`len` is the request parameter). Nothing when the switch is off.
pub(crate) fn emit_canary_reserve(i: &mut InstructionSink<'_>, len: u32) {
    if !armed() {
        return;
    }
    i.local_get(len).i32_const(CANARY).i32_add().local_set(len);
}

/// `$alloc`, at each return with the block in `b`: the header's `len` back
/// to the request, `cap` short of the canary, and the canary written right
/// after the usable capacity. Nothing when the switch is off.
pub(crate) fn emit_canary_set(i: &mut InstructionSink<'_>, b: u32, len: u32) {
    if !armed() {
        return;
    }
    let (lenf, capf) = (almide_layout::LEN.offset, almide_layout::CAP.offset);
    i.local_get(b).local_get(len).i32_const(CANARY).i32_sub().i32_store(word(lenf));
    i.local_get(b).local_get(b).i32_load(word(capf)).i32_const(CANARY).i32_sub().i32_store(word(capf));
    for k in 0..2 {
        i.local_get(b).local_get(b).i32_load(word(capf)).i32_add();
        i.i32_const(CANARY_WORD).i32_store(word(almide_layout::PAYLOAD + 4 * k));
    }
}

/// `$free`, on entry: trap unless the canary after the usable capacity is
/// intact, then hand the block its full capacity back so the filing below
/// derives the class `$alloc` took it from. Nothing when the switch is off.
pub(crate) fn emit_canary_check(i: &mut InstructionSink<'_>, block: u32) {
    if !armed() {
        return;
    }
    let capf = almide_layout::CAP.offset;
    for k in 0..2 {
        i.local_get(block).local_get(block).i32_load(word(capf)).i32_add();
        i.i32_load(word(almide_layout::PAYLOAD + 4 * k)).i32_const(CANARY_WORD).i32_ne().if_(BlockType::Empty);
        i.i32_const(OVERRUN_MESSAGE.with(|c| c.get()) as i32).call(crate::F_EPRINTLN_BLOCK);
        i.i32_const(EXIT).call(crate::F_EXIT_IMPORT).unreachable();
        i.end();
    }
    i.local_get(block).local_get(block).i32_load(word(capf)).i32_const(CANARY).i32_add().i32_store(word(capf));
}
