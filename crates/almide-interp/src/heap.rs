//! The interpreter's BLOCK HEAP (#1226 slice 1): a byte arena plus a handle
//! table, so a self-hosted stdlib body that manipulates linear memory can be
//! evaluated instead of abstained on.
//!
//! ## Why this is not shaped like `vfs.rs`
//!
//! The fs floor (#1218) is an add-on because an fs path is an opaque key with
//! no aliasing into `Value`. A handle is different: it is a LIVE ALIAS. The
//! majority class (`stdlib/base64_encode.almd`) does
//!
//! ```text
//! let out = prim.alloc_str(chunks * 4)                 // an EMPTY block
//! let _x = __b64_fill(prim.handle(out) + 12, ...)      // WRITES through it
//! out                                                   // returned AFTER
//! ```
//!
//! so an arena that materialized a snapshot on `prim.handle` and dropped it
//! would return an empty String and cast a WRONG THIRD VOTE — strictly worse
//! than the honest abstain it replaces, and the same failure mode as #1366
//! (`Hole`/`Todo` voting `Flow::Abort` against native's exit 101).
//!
//! ## The two directions, and the one sync point
//!
//! - **read** (`prim.handle(b)` on an argument): materialize `b` into the arena
//!   ONCE and remember the binding, so a second `prim.handle(b)` in the same
//!   body answers the same address. Keyed on `Rc::as_ptr`, which is stable for
//!   as long as the interpreter holds the value alive — i.e. the whole call.
//! - **write** (`prim.store*` into an allocated block): the arena is the source
//!   of truth from `prim.alloc_*` until the body returns.
//! - **sync**: a value the body RETURNS is rebuilt from its block. That is the
//!   single point where arena bytes become a `Value` again, which is what makes
//!   the round trip closed and testable.
//!
//! ## Layout
//!
//! The canonical String/Bytes block the two backends share, as documented by
//! `proofs/scalar-read-audit.toml` and rendered by the wasm renderer:
//!
//! ```text
//! [rc: i32 @0][len: i32 @4][cap: i32 @8][bytes @12 ..]
//! ```
//!
//! Slice 2 (#1226) adds the SLOT block the whole `alloc_list*` / `alloc_set*`
//! / `alloc_map*` / `alloc_value` family shares physically:
//!
//! ```text
//! [rc: i32 @0][len: i32 @4][cap: i32 @8][slots @12 .. : 8 bytes each]
//! ```
//!
//! `cap` counts SLOTS (the physical payload is `8 * cap` bytes); what `len`
//! MEANS varies by use (element count, entry count, or `alloc_value`'s
//! variant tag, patched in via `store32`) and is decided by the DECLARED type
//! at the return sync, never guessed from the block itself. A slot holds a
//! raw i64: a scalar, f64 bits, or a CHILD block's address — the MIR is
//! i64-uniform, so inside the pool tier a heap value IS its address and only
//! the typed return boundary rebuilds it into a `Value`.

use std::collections::HashMap;
use std::rc::Rc;

/// Offset of the payload inside a canonical String/Bytes block.
pub(crate) const PAYLOAD: u32 = 12;
/// Offset of the `len` field.
pub(crate) const LEN_OFF: u32 = 4;
/// Offset of the `cap` field.
const CAP_OFF: u32 = 8;

/// What a block's payload bytes mean, so a read-back cannot misinterpret one
/// family's bytes as another's. `Slots` covers the whole i64-slot family
/// (`alloc_list*`, `alloc_set*`, `alloc_map*`, `alloc_value`): their physical
/// shape is identical and the DECLARED type at the sync point decides how the
/// slots are read, so a finer split here would claim knowledge the block does
/// not carry.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum BlockKind {
    Str,
    Bytes,
    Slots,
}

#[derive(Default)]
pub(crate) struct Heap {
    /// The arena. Address 0 is never handed out, so a 0 handle stays a
    /// recognisable null the way it is on both backends.
    mem: Vec<u8>,
    /// `Rc::as_ptr` of a materialized value → the block base address, so
    /// `prim.handle(x)` is stable across repeated calls within one body.
    ///
    /// UNSOUND ON ITS OWN: an `Rc` address is recycled after the last handle
    /// drops, so a later, unrelated value can land on a freed pointer and
    /// inherit the earlier block. `keepalive` below is what makes the key
    /// honest — every bound value is held for the arena's lifetime, so no
    /// address it has ever answered for can be reused.
    bound: HashMap<usize, u32>,
    /// Holds every bound value alive so its `Rc` address cannot be recycled
    /// under `bound`. Type-erased: only liveness matters, never the contents.
    keepalive: Vec<Rc<dyn std::any::Any>>,
    /// Block base → what its payload means, for the return sync.
    kinds: HashMap<u32, BlockKind>,
}

impl Heap {
    pub(crate) fn new() -> Self {
        // Reserve address 0 so no live block is ever at the null address.
        Heap {
            mem: vec![0; PAYLOAD as usize],
            bound: HashMap::new(),
            keepalive: Vec::new(),
            kinds: HashMap::new(),
        }
    }

    /// Allocate a zeroed block with `cap` payload bytes and `len` = `cap`,
    /// returning its base address. `prim.alloc_str(n)` / `alloc_bytes(n)`.
    pub(crate) fn alloc(&mut self, cap: u32, kind: BlockKind) -> u32 {
        let base = self.mem.len() as u32;
        self.mem.resize(self.mem.len() + PAYLOAD as usize + cap as usize, 0);
        self.put_u32(base, 1); // rc
        self.put_u32(base + LEN_OFF, cap);
        self.put_u32(base + CAP_OFF, cap);
        self.kinds.insert(base, kind);
        base
    }

    /// Copy `bytes` into a fresh block and bind it to `key` (an `Rc::as_ptr`),
    /// so the same value answers the same handle next time.
    pub(crate) fn bind(&mut self, key: usize, bytes: &[u8], kind: BlockKind) -> u32 {
        if let Some(&a) = self.bound.get(&key) {
            return a;
        }
        let base = self.alloc(bytes.len() as u32, kind);
        let start = (base + PAYLOAD) as usize;
        self.mem[start..start + bytes.len()].copy_from_slice(bytes);
        self.bound.insert(key, base);
        base
    }

    /// Hold `rc` alive for the arena's lifetime, so the address `bind` keyed on
    /// cannot be recycled by a later allocation.
    pub(crate) fn keep<T: std::any::Any>(&mut self, rc: Rc<T>) {
        self.keepalive.push(rc);
    }

    /// Allocate a zeroed SLOT block with `n` i64 slots (`len` = `cap` = `n`,
    /// payload `8 * n` bytes), returning its base. The `alloc_list*` /
    /// `alloc_set*` / `alloc_map*` / `alloc_value` floor — builders patch the
    /// `len` field afterwards via `store32` exactly as they do on the backends.
    pub(crate) fn alloc_slots(&mut self, n: u32) -> u32 {
        let base = self.mem.len() as u32;
        self.mem.resize(self.mem.len() + PAYLOAD as usize + 8 * n as usize, 0);
        self.put_u32(base, 1); // rc
        self.put_u32(base + LEN_OFF, n);
        self.put_u32(base + CAP_OFF, n);
        self.kinds.insert(base, BlockKind::Slots);
        base
    }

    /// Copy `slots` into a fresh slot block with an explicit `len` field
    /// (SLOT count, ENTRY count, or tag — the caller's layout decides) and
    /// bind it to `key`, so the same container answers the same handle.
    pub(crate) fn bind_slots(&mut self, key: usize, slots: &[i64], len_field: u32) -> u32 {
        if let Some(&a) = self.bound.get(&key) {
            return a;
        }
        let base = self.alloc_slots(slots.len() as u32);
        self.put_u32(base + LEN_OFF, len_field);
        for (i, s) in slots.iter().enumerate() {
            let a = (base + PAYLOAD) as usize + 8 * i;
            self.mem[a..a + 8].copy_from_slice(&s.to_le_bytes());
        }
        self.bound.insert(key, base);
        base
    }

    /// The kind of the block at `addr` — `None` when `addr` is not a base this
    /// heap handed out.
    pub(crate) fn kind(&self, addr: u32) -> Option<BlockKind> {
        self.kinds.get(&addr).copied()
    }

    /// Bind an EXISTING block to `key` without copying — the aliasing side of
    /// `load_str`/`load_handle`: the `Value` rebuilt from a child block is a
    /// borrow, so a later `prim.handle` on it must answer the child's OWN
    /// address, not a fresh copy.
    pub(crate) fn adopt(&mut self, key: usize, addr: u32) {
        self.bound.entry(key).or_insert(addr);
    }

    /// The `len` field of the block at `addr`. What it MEANS (bytes, elements,
    /// entries, or a tag) is the caller's to decide from the declared type.
    pub(crate) fn block_len(&self, addr: u32) -> Option<u32> {
        self.kinds.contains_key(&addr).then(|| self.get_u32(addr + LEN_OFF)).flatten()
    }

    /// Slot `i` of the slot block at `addr`, bounds-checked against the
    /// block's physical `cap` — `None` (an abstain upstream) rather than a
    /// guess for anything out of range or not a slot block.
    pub(crate) fn slot(&self, addr: u32, i: u32) -> Option<i64> {
        if self.kind(addr)? != BlockKind::Slots {
            return None;
        }
        let cap = self.get_u32(addr + CAP_OFF)?;
        if i >= cap {
            return None;
        }
        self.load(addr + PAYLOAD + 8 * i, 8)
    }

    /// The block whose base is `addr`, as its payload bytes — `None` when
    /// `addr` is not a base this heap handed out (so the caller abstains
    /// rather than inventing a value).
    pub(crate) fn block_bytes(&self, addr: u32) -> Option<(Vec<u8>, BlockKind)> {
        let kind = *self.kinds.get(&addr)?;
        let len = self.get_u32(addr + LEN_OFF)? as usize;
        let start = (addr + PAYLOAD) as usize;
        let end = start.checked_add(len)?;
        if end > self.mem.len() {
            return None;
        }
        Some((self.mem[start..end].to_vec(), kind))
    }

    /// `prim.load8` / `load32` / `load64`. `None` on an out-of-range address —
    /// the two backends read real memory there, so guessing would be a wrong
    /// vote; the caller turns this into an abstain.
    pub(crate) fn load(&self, addr: u32, width: u32) -> Option<i64> {
        let a = addr as usize;
        let w = width as usize;
        if a.checked_add(w)? > self.mem.len() {
            return None;
        }
        let mut v: u64 = 0;
        for i in 0..w {
            v |= (self.mem[a + i] as u64) << (8 * i);
        }
        Some(match width {
            1 => v as u8 as i64,
            4 => v as u32 as i64,
            _ => v as i64,
        })
    }

    /// `prim.store8` / `store32` / `store64`, little-endian like both backends.
    pub(crate) fn store(&mut self, addr: u32, width: u32, value: i64) -> Option<()> {
        let a = addr as usize;
        let w = width as usize;
        if a.checked_add(w)? > self.mem.len() {
            return None;
        }
        let v = value as u64;
        for i in 0..w {
            self.mem[a + i] = (v >> (8 * i)) as u8;
        }
        Some(())
    }

    fn put_u32(&mut self, addr: u32, v: u32) {
        let a = addr as usize;
        self.mem[a..a + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn get_u32(&self, addr: u32) -> Option<u32> {
        let a = addr as usize;
        if a + 4 > self.mem.len() {
            return None;
        }
        Some(u32::from_le_bytes([self.mem[a], self.mem[a + 1], self.mem[a + 2], self.mem[a + 3]]))
    }

}

/// The identity a value is bound under. `Rc::as_ptr` is stable while the value
/// is alive, and the interpreter holds it for the duration of the call, which
/// is exactly the window a handle must stay valid for.
pub(crate) fn rc_key<T>(rc: &Rc<T>) -> usize {
    Rc::as_ptr(rc) as *const u8 as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_round_trips_through_stores() {
        // The base64_encode shape: allocate empty, write through the payload
        // address, read the block back. This is the round trip the whole slice
        // exists to make evaluable.
        let mut h = Heap::new();
        let base = h.alloc(3, BlockKind::Str);
        assert_ne!(base, 0, "no live block may sit at the null address");
        assert_eq!(h.load(base + LEN_OFF, 4), Some(3));
        for (i, b) in [b'a', b'b', b'c'].iter().enumerate() {
            h.store(base + PAYLOAD + i as u32, 1, *b as i64).expect("in range");
        }
        let (bytes, kind) = h.block_bytes(base).expect("a base this heap handed out");
        assert_eq!(bytes, b"abc");
        assert_eq!(kind, BlockKind::Str);
    }

    #[test]
    fn the_same_value_answers_the_same_handle() {
        // `prim.handle(b) + 4` and `prim.handle(b) + 12` in one body must read
        // the SAME block, or a length read and a payload read disagree.
        let mut h = Heap::new();
        let a1 = h.bind(0xbeef, b"hello", BlockKind::Bytes);
        let a2 = h.bind(0xbeef, b"hello", BlockKind::Bytes);
        assert_eq!(a1, a2);
        assert_eq!(h.load(a1 + LEN_OFF, 4), Some(5));
    }

    #[test]
    fn an_out_of_range_access_abstains_instead_of_guessing() {
        let mut h = Heap::new();
        let base = h.alloc(2, BlockKind::Bytes);
        assert_eq!(h.load(base + PAYLOAD + 8, 1), None);
        assert_eq!(h.store(base + PAYLOAD + 8, 1, 1), None);
        assert_eq!(h.block_bytes(base + 4), None, "only a real base resolves");
    }

    #[test]
    fn load_widths_are_unsigned_little_endian() {
        let mut h = Heap::new();
        let base = h.alloc(8, BlockKind::Bytes);
        h.store(base + PAYLOAD, 4, 0xDEAD_BEEFu32 as i64).expect("in range");
        assert_eq!(h.load(base + PAYLOAD, 4), Some(0xDEAD_BEEF));
        assert_eq!(h.load(base + PAYLOAD, 1), Some(0xEF));
    }

    #[test]
    fn a_slot_block_reads_by_slot_and_patches_len() {
        // The set_union shape: over-alloc, fill, patch the len field down.
        let mut h = Heap::new();
        let base = h.alloc_slots(4);
        assert_eq!(h.block_len(base), Some(4));
        h.store(base + PAYLOAD, 8, -7).expect("slot 0");
        h.store(base + PAYLOAD + 8, 8, i64::MAX).expect("slot 1");
        h.store(base + LEN_OFF, 4, 2).expect("len patch");
        assert_eq!(h.block_len(base), Some(2));
        assert_eq!(h.slot(base, 0), Some(-7));
        assert_eq!(h.slot(base, 1), Some(i64::MAX));
        // Bounds are the PHYSICAL cap, not the patched len (the skv value
        // region lives above len) — but past cap abstains.
        assert_eq!(h.slot(base, 3), Some(0));
        assert_eq!(h.slot(base, 4), None);
        assert_eq!(h.kind(base), Some(BlockKind::Slots));
    }

    #[test]
    fn bind_slots_dedups_and_keeps_its_len_field() {
        let mut h = Heap::new();
        let a1 = h.bind_slots(0xfeed, &[1, 10, 2, 20], 2); // paired map, len = entries
        let a2 = h.bind_slots(0xfeed, &[9, 9], 9);
        assert_eq!(a1, a2, "same key answers the same block");
        assert_eq!(h.block_len(a1), Some(2));
        assert_eq!(h.slot(a1, 3), Some(20));
    }

    #[test]
    fn adopt_aliases_without_copying() {
        // The load_str shape: a Value rebuilt from a child block must answer
        // the child's OWN address on a later bind, not a fresh copy.
        let mut h = Heap::new();
        let child = h.alloc(2, BlockKind::Str);
        h.adopt(0xabc, child);
        assert_eq!(h.bind(0xabc, b"ignored", BlockKind::Str), child);
    }

    #[test]
    fn a_fresh_heap_shares_nothing_with_an_earlier_one() {
        // Per-run isolation comes from CONSTRUCTION, not from a reset call:
        // every `Interpreter` builds its own `Heap`, and `cargo test` runs the
        // gates in parallel threads, so one fixture's arena must be unreachable
        // from the next. (An explicit `reset` was written first and deleted —
        // the rustc-warning ratchet flagged it as never called, which was the
        // correct read: construction already provides the property.)
        let mut h1 = Heap::new();
        let a = h1.bind(1, b"x", BlockKind::Str);
        let mut h2 = Heap::new();
        let b = h2.bind(1, b"yy", BlockKind::Str);
        assert_eq!(a, b, "each arena numbers its blocks from the same origin");
        assert_eq!(h1.block_bytes(a).map(|(v, _)| v), Some(b"x".to_vec()));
        assert_eq!(h2.block_bytes(b).map(|(v, _)| v), Some(b"yy".to_vec()));
    }
}
