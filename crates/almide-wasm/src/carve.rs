//! ADR-0033 D7: CARVING an allocation family — the emitter-owned ops that
//! build a family's interior blocks, each lowered to ONE fixed shape and
//! recorded in the init witness (witness_init.rs `family` / `carve` /
//! `seal`), so `almide-anchor-check` holds the shipped bytes to it and
//! `InitCert.check_init_cert` judges the record.
//!
//! - `prim.family(n) -> Bytes`: the ROOT, `$alloc(4 + n)` behind the alloc
//!   size guard, then the CARVE CURSOR word `root[12] := 16` — the byte
//!   offset, from the root's base, where the next interior block goes. The
//!   cursor lives in the root, so the carve ops find it there and nothing
//!   else has to carry it; the root's len / cap stay `$alloc`'s.
//! - `prim.carve(fam, len) -> Int`: one interior block of `len` payload
//!   bytes at the cursor, its base address answered. `cap = max(4,
//!   round4(len))`, `size = 12 + cap`. The ROOM TEST `cur + size >u 12 +
//!   len(root)` (i64, so nothing wraps) aborts before anything is written;
//!   then the cursor advances by `size`, and the block is stored in full:
//!   its header (rc = `0x8000_0000 | cur`, the interior word; len; cap) and
//!   EVERY payload word, zeroed (unrolled when `len` is a literal, else the
//!   countdown fill). A carved block is initialized over its whole extent
//!   by the op itself; what the caller stores into it afterwards is an
//!   ordinary write into initialized memory.
//! - `prim.family_seal(fam)`: aborts unless the cursor reached the root's
//!   end exactly — the carves tile the root, so with the root's header and
//!   cursor word every byte of it is initialized (InitCert.v
//!   `sealed_family_ok`).
//!
//! The ops lower only in a module whose family switch is armed (a linked
//! `@family_producer`, family.rs): an interior rc word anywhere else would
//! meet an `$inc` / `$dec` that does not redirect it.

use almide_ir::{IrExpr, IrExprKind};
use wasm_encoder::{BlockType, MemArg};

use crate::emitter::Emitter;
use crate::*;

/// The cursor word's offset in the root (its first payload word).
pub(crate) const CURSOR: u64 = almide_layout::PAYLOAD as u64;
/// Where the first interior block of a family goes: after the root's header
/// and its cursor word.
pub(crate) const FIRST: i32 = almide_layout::PAYLOAD as i32 + 4;
/// The largest payload one carve takes (the alloc size guard's bound, so
/// `len + 3` and `12 + 4 * words` stay inside i32).
pub(crate) const MAX_LEN: i64 = 0x7FFF_0000;
/// A literal carve unrolls its zero stores up to this many payload words.
pub(crate) const UNROLL_WORDS: i64 = 8;

fn word(offset: u64) -> MemArg {
    MemArg { offset, align: 2, memory_index: 0 }
}

/// The abort every carve check takes (a defined abort, exit 1).
pub(crate) const FULL_TEXT: &str = "Error: allocation family out of room";

impl Emitter<'_> {
    pub(crate) fn lower_carve_op(&mut self, func: &str, args: &[IrExpr]) -> Option<ArmResult> {
        let r = match (func, args) {
            ("family", [n]) => self.lower_family(n),
            ("carve", [fam, len]) => self.lower_carve(fam, len),
            ("family_seal", [fam]) => self.lower_family_seal(fam),
            _ => return None,
        };
        Some(r)
    }

    /// The anchor of the instruction just emitted (witness_anchor.rs).
    fn anchor_here(&self) -> String {
        crate::witness::anchor::at(self.f)
    }

    /// The family root argument: a Bytes block, its address on the stack.
    fn lower_root(&mut self, fam: &IrExpr, op: &str) -> Result<(), EmitError> {
        match self.lower_arg(fam, None, ArgMode::Raw)? {
            SliceTy::Scalar(Scalar::Bytes) => Ok(()),
            other => Err(EmitError::Unsupported(format!("prim.{op}: a family root is Bytes, not {other:?}"))),
        }
    }

    fn carve_armed(&self, op: &str) -> Result<(), EmitError> {
        if crate::family::armed() {
            Ok(())
        } else {
            Err(EmitError::Unsupported(format!("prim.{op}: outside a module linking a @family_producer")))
        }
    }

    /// `if <i32 cond on the stack> { "out of room"; exit 1 }`.
    fn carve_abort_if(&mut self) {
        let msg = self.pool.intern(FULL_TEXT);
        let mut i = self.f.instructions();
        i.if_(BlockType::Empty);
        i.i32_const(msg as i32).call(F_EPRINTLN_BLOCK);
        i.i32_const(1).call(F_EXIT_IMPORT).unreachable();
        i.end();
    }

    fn lower_family(&mut self, n: &IrExpr) -> ArmResult {
        self.carve_armed("family")?;
        self.lower_arg(n, Some(INT), ArgMode::Raw)?;
        self.emit_alloc_size_guard(1)?;
        let root = self.hold_i32()?;
        self.f.instructions().i32_wrap_i64().i32_const(4).i32_add().call(F_ALLOC).local_set(root);
        let a_site = self.anchor_here();
        self.f.instructions().local_get(root).i32_const(FIRST).i32_store(word(CURSOR));
        let a_cursor = self.anchor_here();
        self.f.instructions().local_get(root);
        crate::witness::init::family(FIRST as u32, &[a_site, a_cursor]);
        self.release_i32();
        Ok(Some(Lowered::owned(SliceTy::Scalar(Scalar::Bytes))))
    }

    fn lower_family_seal(&mut self, fam: &IrExpr) -> ArmResult {
        self.carve_armed("family_seal")?;
        self.lower_root(fam, "family_seal")?;
        let root = self.hold_i32()?;
        let mut anchors = Vec::new();
        self.f.instructions().local_set(root).local_get(root).i32_load(word(CURSOR));
        anchors.push(self.anchor_here());
        self.f.instructions().local_get(root).i32_load(len_memarg());
        anchors.push(self.anchor_here());
        self.f.instructions().i32_const(almide_layout::PAYLOAD as i32).i32_add().i32_ne();
        self.carve_abort_if();
        anchors.push(self.anchor_here());
        crate::witness::init::seal(almide_layout::PAYLOAD, &anchors);
        self.release_i32();
        Ok(None)
    }

    fn lower_carve(&mut self, fam: &IrExpr, len: &IrExpr) -> ArmResult {
        self.carve_armed("carve")?;
        let literal = match &len.kind {
            IrExprKind::LitInt { value } if (0..=UNROLL_WORDS * 4).contains(value) => Some(*value),
            _ => None,
        };
        self.lower_root(fam, "carve")?;
        let root = self.hold_i32()?;
        self.f.instructions().local_set(root);
        let (cur, base) = (self.hold_i32()?, self.hold_i32()?);
        let mut anchors = Vec::new();
        match literal {
            Some(n) => {
                let words = ((n + 3) / 4).max(1);
                let size = 12 + 4 * words;
                self.carve_room(root, cur, base, CarveSize::Const(size as i32), &mut anchors);
                let mut i = self.f.instructions();
                i.local_get(base).i32_const(n as i32).i32_store(len_memarg());
                anchors.push(self.anchor_here());
                let mut i = self.f.instructions();
                i.local_get(base).i32_const((4 * words) as i32).i32_store(word(almide_layout::CAP.offset.into()));
                anchors.push(self.anchor_here());
                // Each payload word's record is taken where its store is
                // emitted: a lost store is a lost `s i`, which the proven
                // checker rejects.
                let mut stores = Vec::new();
                for k in 0..words {
                    self.f.instructions().local_get(base).i32_const(0).i32_store(word(12 + 4 * k as u64));
                    anchors.push(self.anchor_here());
                    stores.push(k as usize);
                }
                self.f.instructions().local_get(base);
                crate::witness::init::carve_fixed(almide_layout::PAYLOAD, 4, words as usize, &stores, &anchors);
            }
            None => {
                let (n, k, s, p) = (self.hold_i32()?, self.hold_i32()?, self.hold_i32()?, self.hold_i32()?);
                self.lower_arg(len, Some(INT), ArgMode::Raw)?;
                let t = self.hold_i64()?;
                self.f.instructions().local_tee(t).i64_const(MAX_LEN).i64_gt_u();
                self.carve_abort_if();
                anchors.push(self.anchor_here());
                let mut i = self.f.instructions();
                i.local_get(t).i32_wrap_i64().local_set(n);
                // words = max(1, (len + 3) >> 2); size = 12 + 4 * words
                i.local_get(n).i32_const(3).i32_add().i32_const(2).i32_shr_u().local_set(k);
                i.local_get(k).i32_const(1).local_get(k).select().local_set(k);
                i.local_get(k).i32_const(2).i32_shl().i32_const(12).i32_add().local_set(s);
                self.carve_room(root, cur, base, CarveSize::Local(s), &mut anchors);
                let mut i = self.f.instructions();
                i.local_get(base).local_get(n).i32_store(len_memarg());
                anchors.push(self.anchor_here());
                let mut i = self.f.instructions();
                i.local_get(base).local_get(k).i32_const(2).i32_shl().i32_store(word(almide_layout::CAP.offset.into()));
                anchors.push(self.anchor_here());
                // The countdown fill: one zero word per step, `k` steps.
                let mut i = self.f.instructions();
                i.local_get(base).i32_const(12).i32_add().local_set(p);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(k).i32_eqz().br_if(1);
                i.local_get(p).i32_const(0).i32_store(word(0));
                anchors.push(self.anchor_here());
                let mut i = self.f.instructions();
                i.local_get(p).i32_const(4).i32_add().local_set(p);
                i.local_get(k).i32_const(1).i32_sub().local_set(k);
                i.br(0).end().end();
                i.local_get(base);
                anchors.push(self.anchor_here());
                crate::witness::init::carve_fill(almide_layout::PAYLOAD, 4, k, &anchors);
                self.release_i64();
                for _ in 0..4 {
                    self.release_i32();
                }
            }
        }
        self.f.instructions().i64_extend_i32_u();
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(Lowered::scalar(INT)))
    }

    /// The carve's common head: read the cursor, the room test, advance the
    /// cursor, set the base, store the interior rc word. Anchors: the cursor
    /// load, the root len load, the room test's `if`, the cursor store, the
    /// base's `local.set`, the rc word store.
    fn carve_room(&mut self, root: u32, cur: u32, base: u32, size: CarveSize, anchors: &mut Vec<String>) {
        self.f.instructions().local_get(root).i32_load(word(CURSOR));
        anchors.push(self.anchor_here());
        let mut i = self.f.instructions();
        i.local_set(cur).local_get(cur).i64_extend_i32_u();
        match size {
            CarveSize::Const(s) => i.i64_const(i64::from(s)),
            CarveSize::Local(s) => i.local_get(s).i64_extend_i32_u(),
        };
        i.i64_add().local_get(root).i32_load(len_memarg());
        anchors.push(self.anchor_here());
        self.f.instructions().i64_extend_i32_u().i64_const(almide_layout::PAYLOAD as i64).i64_add().i64_gt_u();
        self.carve_abort_if();
        anchors.push(self.anchor_here());
        let mut i = self.f.instructions();
        i.local_get(root).local_get(cur);
        match size {
            CarveSize::Const(s) => i.i32_const(s),
            CarveSize::Local(s) => i.local_get(s),
        };
        i.i32_add().i32_store(word(CURSOR));
        anchors.push(self.anchor_here());
        self.f.instructions().local_get(root).local_get(cur).i32_add().local_set(base);
        anchors.push(self.anchor_here());
        let mut i = self.f.instructions();
        i.local_get(base)
            .local_get(cur)
            .i32_const(crate::family::INTERIOR as i32)
            .i32_or()
            .i32_store(word(almide_layout::RC.offset.into()));
        anchors.push(self.anchor_here());
    }
}

#[derive(Clone, Copy)]
enum CarveSize {
    Const(i32),
    Local(u32),
}
