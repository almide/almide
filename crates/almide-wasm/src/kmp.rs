//! Knuth–Morris–Pratt substring search, emitted inline by every kernel that
//! looks for a byte needle in a String or Bytes payload (#4206): `$str_replace`
//! (string.replace / replace_first), the `$split` helper (string.split), the
//! `bytes.split` arm and `split_once#at`. Each of them used to compare the
//! needle at every haystack position, O(n·m): a haystack of `a`s and a needle
//! of `a`s ending in `b` took seconds at 100,000 bytes. With the failure table
//! the haystack cursor only moves forward, so a search is O(n + m), and a
//! non-overlapping scan that resumes each search where the last match ended
//! (count, split, replace) is one forward walk with one table. The native leg
//! (runtime/rs/src/bytes.rs) and the self-hosted bodies (stdlib/bytes_core.almd,
//! stdlib/string_search.almd) run the same algorithm.

use wasm_encoder::{BlockType, InstructionSink, MemArg};

use crate::{F_ALLOC, F_FREE};

/// The locals one search uses: the haystack and needle blocks with their
/// byte lengths, the table block, and three scratch registers (`k` the
/// matched length, `c` the byte just read, `q` the cursor).
#[derive(Clone, Copy)]
pub(crate) struct Kmp {
    pub(crate) hay: u32,
    pub(crate) n: u32,
    pub(crate) pat: u32,
    pub(crate) m: u32,
    pub(crate) tbl: u32,
    pub(crate) k: u32,
    pub(crate) c: u32,
    pub(crate) q: u32,
}

fn byte() -> MemArg {
    MemArg {
        offset: u64::from(almide_layout::PAYLOAD),
        align: 0,
        memory_index: 0,
    }
}

fn word() -> MemArg {
    MemArg {
        offset: u64::from(almide_layout::PAYLOAD),
        align: 2,
        memory_index: 0,
    }
}

impl Kmp {
    /// `k` after reading byte `c`: fall back through the table while the
    /// next needle byte differs, then extend by one on a match.
    fn emit_step(&self, i: &mut InstructionSink) {
        let Kmp { pat, tbl, k, c, .. } = *self;
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(k).i32_eqz().br_if(1);
        i.local_get(pat).local_get(k).i32_add().i32_load8_u(byte());
        i.local_get(c).i32_eq().br_if(1);
        // slot 0 is always 0: one matched byte falls back to none without
        // reading the table (a one- or two-byte needle has none)
        i.local_get(k).i32_const(1).i32_eq().if_(BlockType::Empty);
        i.i32_const(0).local_set(k);
        i.br(2);
        i.end();
        i.local_get(tbl)
            .local_get(k)
            .i32_const(1)
            .i32_sub()
            .i32_const(4)
            .i32_mul()
            .i32_add();
        i.i32_load(word()).local_set(k);
        i.br(0).end().end();
        i.local_get(pat).local_get(k).i32_add().i32_load8_u(byte());
        i.local_get(c).i32_eq().if_(BlockType::Empty);
        i.local_get(k).i32_const(1).i32_add().local_set(k);
        i.end();
    }

    /// `tbl` = the failure table of the needle (`m >= 1`): slot `x` holds
    /// the length of the longest proper prefix of `pat[..=x]` that is also
    /// its suffix. Release it with [`Kmp::emit_free`].
    ///
    /// A search reads slot `k - 1` only after `k >= 2` bytes matched and the
    /// next one differs, and slot 0 is always 0 — so a needle of one or two
    /// bytes (`;`, `::`, `\r\n`) never reads the table, and `tbl` stays 0:
    /// the common separators search without allocating.
    pub(crate) fn emit_table(&self, i: &mut InstructionSink) {
        let Kmp {
            pat,
            m,
            tbl,
            k,
            c,
            q,
            ..
        } = *self;
        i.i32_const(0).local_set(tbl);
        i.local_get(m).i32_const(3).i32_ge_u().if_(BlockType::Empty);
        i.local_get(m)
            .i32_const(4)
            .i32_mul()
            .call(F_ALLOC)
            .local_set(tbl);
        i.local_get(tbl).i32_const(0).i32_store(word());
        i.i32_const(0).local_set(k);
        i.i32_const(1).local_set(q);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(q).local_get(m).i32_ge_u().br_if(1);
        i.local_get(pat)
            .local_get(q)
            .i32_add()
            .i32_load8_u(byte())
            .local_set(c);
        self.emit_step(i);
        i.local_get(tbl)
            .local_get(q)
            .i32_const(4)
            .i32_mul()
            .i32_add();
        i.local_get(k).i32_store(word());
        i.local_get(q).i32_const(1).i32_add().local_set(q);
        i.br(0).end().end();
        i.end();
    }

    /// `res` = the first match start at or after `q` (the needle non-empty,
    /// the table built), or -1. `q` is clobbered.
    pub(crate) fn emit_find(&self, i: &mut InstructionSink, res: u32) {
        let Kmp {
            hay, n, m, k, c, q, ..
        } = *self;
        i.i32_const(0).local_set(k);
        i.i32_const(-1).local_set(res);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        // the rest of the haystack cannot complete a match
        i.local_get(n).local_get(q).i32_sub();
        i.local_get(m).local_get(k).i32_sub();
        i.i32_lt_s().br_if(1);
        i.local_get(hay)
            .local_get(q)
            .i32_add()
            .i32_load8_u(byte())
            .local_set(c);
        self.emit_step(i);
        i.local_get(q).i32_const(1).i32_add().local_set(q);
        i.local_get(k).local_get(m).i32_eq().if_(BlockType::Empty);
        i.local_get(q).local_get(m).i32_sub().local_set(res);
        i.br(2);
        i.end();
        i.br(0).end().end();
    }

    /// Release the table block (owned outright by the kernel), if the
    /// needle needed one.
    pub(crate) fn emit_free(&self, i: &mut InstructionSink) {
        i.local_get(self.tbl).if_(BlockType::Empty);
        i.local_get(self.tbl).call(F_FREE);
        i.end();
    }
}
