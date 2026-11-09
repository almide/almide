//! ADR-0033 D1: allocation families. A family is one allocation holding
//! many INTERIOR blocks, each with its type's ordinary header and layout
//! except the rc word: bit 31 set, bits 0..30 the byte distance from the
//! interior block's base back to the family ROOT's base
//! (`word = 0x8000_0000 | (block - root)`). The root is an ordinary heap
//! block whose rc counts every reference into the family.
//!
//! Every routine that increments or decrements an rc word redirects an
//! interior word to the root:
//!
//! - `$inc` / `$dec_flat` RETARGET: after the static floor guard,
//!   `if (load(block) <s 0) block = block - (load(block) & 0x7fffffff);`
//!   then today's body runs on the root unchanged (double-free trap kept).
//!   One single-`local.set` `if` body, the shape proofs/StructuralRuntime.v
//!   decodes.
//! - every OTHER dec routine (`$drop_list`, `$drop_<shape>`, the deep
//!   walkers, `$drop_map`, `$drop_entries`, `$drop_http_call`) FORWARDS:
//!   `if (load(block) <s 0) { call $dec_flat(block); return }` — the family
//!   `$dec_flat` retargets itself, and the interior block's children are
//!   never walked (the family owns them).
//!
//! The redirect is emitted ONLY in a module that links a family producer: a
//! fn declared `@family_producer` at its definition (no name list). The
//! emitter arms the switch per pass from the fns that pass links
//! ([`producer_linked`]); a module linking none emits today's bytes exactly.
//! A test arms it on its own thread with [`FamilyGuard`].

use std::cell::Cell;

use wasm_encoder::{BlockType, InstructionSink, MemArg};

/// The attribute that declares a family producer at its definition.
pub const PRODUCER_ATTR: &str = "family_producer";

/// The INTERIOR bit of an rc word.
pub const INTERIOR: u32 = 0x8000_0000;
/// The distance mask of an INTERIOR rc word.
pub const DISTANCE_MASK: i32 = 0x7fff_ffff;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

/// Is the module under emission one that links a family producer?
pub fn armed() -> bool {
    ARMED.with(|c| c.get())
}

/// Set the switch for a scope and restore the previous state on drop.
#[must_use = "the guard restores the previous state when dropped; binding it to `_` restores immediately"]
pub struct FamilyGuard(bool);

impl FamilyGuard {
    pub fn set(on: bool) -> Self {
        let prev = armed();
        ARMED.with(|c| c.set(on));
        Self(prev)
    }
}

impl Drop for FamilyGuard {
    fn drop(&mut self) {
        let prev = self.0;
        ARMED.with(|c| c.set(prev));
    }
}

/// Does any of the linked fns (given by their attribute lists) carry
/// `@family_producer`?
pub(crate) fn producer_linked<'a>(fns: impl IntoIterator<Item = &'a [almide_lang::ast::Attribute]>) -> bool {
    fns.into_iter().any(|attrs| attrs.iter().any(|a| a.name.as_str() == PRODUCER_ATTR))
}

fn rc_word() -> MemArg {
    MemArg { offset: u64::from(almide_layout::RC.offset), align: 2, memory_index: 0 }
}

/// `$inc` / `$dec_flat`'s retarget (armed only): an INTERIOR word moves
/// `block` (a param local) to the family root.
pub(crate) fn emit_retarget(i: &mut InstructionSink<'_>, block: u32) {
    if !armed() {
        return;
    }
    i.local_get(block).i32_load(rc_word()).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.local_get(block);
    i.local_get(block).i32_load(rc_word()).i32_const(DISTANCE_MASK).i32_and();
    i.i32_sub().i32_const(4).i32_sub().local_set(block);
    i.end();
}

/// Every other dec routine's forward (armed only): an INTERIOR block is
/// released by the family `$dec_flat` (which credits the root) and its
/// children are not walked.
pub(crate) fn emit_forward_dec(i: &mut InstructionSink<'_>, block: u32) {
    if !armed() {
        return;
    }
    i.local_get(block).i32_load(rc_word()).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.local_get(block).call(crate::F_DEC_FLAT);
    i.return_();
    i.end();
}

#[cfg(test)]
#[path = "family_tests.rs"]
pub(crate) mod tests;
