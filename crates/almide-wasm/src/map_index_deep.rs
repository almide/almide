//! The deep-key lane of the keyed-lookup index (#4887): a Set element or Map
//! key of a tuple, record or variant type — and lists, options and results
//! of those — takes the hash index like an Int or String key instead of the
//! `$scan_deep` linear walk.
//!
//! The lane is decided per TYPE (`Emitter::idx_key`). A type qualifies when
//! every component is an Int, Bool, String or Bytes, `()`, or a list, option,
//! result, tuple, record or variant of qualifying components, and no named
//! type on the way is recursive. Everything else — a Float anywhere (IEEE
//! `==`: `0.0 == -0.0`), a Value, Map, Set, closure or matrix, a recursive
//! type — keeps the linear scan.
//!
//! A qualifying key carries two type-directed bodies, both `(a[, b]) -> i32`
//! helpers built on the emitter (display_helpers.rs):
//!
//! * the total order — `NamedOp::CmpTy`, the order `<` and `list.sort` use.
//!   Over these types a zero verdict is exactly `==` (no Float), so the
//!   probe confirms a match with it, and the flood fallback (map_ordered.rs,
//!   #4104) orders a flooded block by it;
//! * the hash — `NamedOp::HashTy`, below. It folds every component the
//!   equality compares, in the same order, into one 64-bit state, so two
//!   equal keys always hash alike (`List[Int]` / `List[Bool]`, which `==`
//!   compares as bytes, and String / Bytes hash through the String hash).
//!
//! A deleted deep key's handle slot is cleared like a String's, so the hole
//! family (map_holes.rs) reads a zero key as a hole for both.

use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::types_table::NamedDef;
use crate::*;

/// The multiplier each component is mixed with (the 64-bit Fibonacci
/// constant of the Int hash).
pub(crate) const DEEP_MUL: u64 = 0x9E37_79B9_7F4A_7C15;
/// The shift folded back in after each mix, so a later component's high
/// product bits reach the low bits (a shift and xor: the VM's instruction
/// set has no rotate).
pub(crate) const DEEP_SHIFT: i64 = 29;
/// The state a hash starts from.
pub(crate) const DEEP_SEED: u64 = 0x243F_6A88_85A3_08D3;

impl Emitter<'_> {
    /// Whether a key of type `t` takes the deep hash lane: every component
    /// is hashable consistently with `==`, no named type on the way is
    /// recursive, and the hash walk fits the hold pool.
    pub(crate) fn deep_key_ok(&self, t: SliceTy) -> bool {
        self.deep_component_ok(t) && self.hash_hold_need(t) < crate::emitter::HOLD_I32_POOL
    }

    fn deep_component_ok(&self, t: SliceTy) -> bool {
        match t {
            INT | BOOL | STR | SliceTy::Scalar(Scalar::Bytes) | SliceTy::Unit => true,
            SliceTy::List(h) | SliceTy::Option(h) => self.deep_component_ok(self.types.el(h)),
            SliceTy::Result(o, e) => self.deep_component_ok(self.types.el(o)) && self.deep_component_ok(self.types.el(e)),
            SliceTy::Tuple(id) => self.types.tuple_def(id).fields.iter().all(|(f, _)| self.deep_component_ok(*f)),
            SliceTy::Named(ti) => {
                !self.types.named_on_cycle(ti)
                    && match self.types.def(ti) {
                        NamedDef::Record(r) => r.fields.iter().all(|f| self.deep_component_ok(f.ty)),
                        NamedDef::Variant(v) => v.cases.iter().all(|c| c.fields.iter().all(|f| self.deep_component_ok(f.ty))),
                        NamedDef::Excluded => false,
                    }
            }
            // A Float (IEEE `==`), a Value, a Map or Set, a closure, a matrix.
            _ => false,
        }
    }

    /// The i32 holds the hash walk of `t` keeps live at its deepest point
    /// (mirrors `emit_val_hash`). Only called on a qualifying (acyclic) type.
    fn hash_hold_need(&self, t: SliceTy) -> u32 {
        match t {
            SliceTy::List(h) => match self.types.el(h) {
                INT | BOOL => 0,
                el => 3 + self.hash_hold_need(el),
            },
            SliceTy::Option(h) => 1 + self.hash_hold_need(self.types.el(h)),
            SliceTy::Result(o, e) => 1 + self.hash_hold_need(self.types.el(o)).max(self.hash_hold_need(self.types.el(e))),
            SliceTy::Tuple(id) => 1 + self.types.tuple_def(id).fields.iter().map(|(f, _)| self.hash_hold_need(*f)).max().unwrap_or(0),
            SliceTy::Named(ti) => {
                let fields: Vec<SliceTy> = match self.types.def(ti) {
                    NamedDef::Record(r) => r.fields.iter().map(|f| f.ty).collect(),
                    NamedDef::Variant(v) => v.cases.iter().flat_map(|c| c.fields.iter().map(|f| f.ty)).collect(),
                    NamedDef::Excluded => Vec::new(),
                };
                1 + fields.into_iter().map(|f| self.hash_hold_need(f)).max().unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// `[x: i64]` -> mixed into `acc`.
    fn mix(&mut self, acc: u32) {
        let mut i = self.f.instructions();
        i.local_get(acc).i64_xor().i64_const(DEEP_MUL as i64).i64_mul().local_tee(acc);
        i.local_get(acc).i64_const(DEEP_SHIFT).i64_shr_u().i64_xor().local_set(acc);
    }

    /// `[x: i32]` -> mixed into `acc` (zero-extended).
    fn mix32(&mut self, acc: u32) {
        self.f.instructions().i64_extend_i32_u();
        self.mix(acc);
    }

    /// `[block]` -> the String hash of its bytes, mixed into `acc`.
    fn mix_bytes(&mut self, acc: u32) {
        let hash_str = self.work.helper(Helper::MapIdxHash { key: crate::map_index::IdxKey::Str });
        self.f.instructions().call(hash_str);
        self.mix32(acc);
    }

    /// Consume a value of qualifying type `t` from the stack and mix every
    /// component `==` compares into the i64 local `acc`.
    pub(crate) fn emit_val_hash(&mut self, t: SliceTy, acc: u32) -> Result<(), EmitError> {
        match t {
            INT => self.mix(acc),
            BOOL => self.mix32(acc),
            STR | SliceTy::Scalar(Scalar::Bytes) => self.mix_bytes(acc),
            SliceTy::Unit => {
                self.f.instructions().drop();
            }
            // `==` compares a list of Int or Bool as its bytes.
            SliceTy::List(h) if matches!(self.types.el(h), INT | BOOL) => self.mix_bytes(acc),
            SliceTy::List(h) => self.emit_list_hash(self.types.el(h), acc)?,
            SliceTy::Option(h) => {
                let et = self.types.el(h);
                let ha = self.hold_i32()?;
                self.f.instructions().local_tee(ha).i32_eqz().i32_eqz();
                self.mix32(acc);
                self.f.instructions().local_get(ha).if_(BlockType::Empty);
                self.f.instructions().local_get(ha);
                self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                self.emit_val_hash(et, acc)?;
                self.f.instructions().end();
                self.release_i32();
            }
            SliceTy::Result(o, e) => {
                let (ot, et) = (self.types.el(o), self.types.el(e));
                let tag = slot_memarg(almide_layout::SUM_TAG);
                let ha = self.hold_i32()?;
                self.f.instructions().local_tee(ha).i32_load(tag);
                self.mix32(acc);
                self.f.instructions().local_get(ha).i32_load(tag).i32_eqz().if_(BlockType::Empty);
                self.f.instructions().local_get(ha);
                self.load_ty_slot(ot, almide_layout::SUM_FIELD);
                self.emit_val_hash(ot, acc)?;
                self.f.instructions().else_();
                self.f.instructions().local_get(ha);
                self.load_ty_slot(et, almide_layout::SUM_FIELD);
                self.emit_val_hash(et, acc)?;
                self.f.instructions().end();
                self.release_i32();
            }
            SliceTy::Tuple(id) => {
                let fields = self.types.tuple_def(id).fields;
                self.emit_fields_hash(&fields, acc)?;
            }
            SliceTy::Named(ti) => match self.types.def(ti) {
                NamedDef::Record(r) => {
                    let fields: Vec<(SliceTy, u32)> = r.fields.iter().map(|f| (f.ty, f.offset)).collect();
                    self.emit_fields_hash(&fields, acc)?;
                }
                NamedDef::Variant(v) => {
                    let cases: Vec<(u32, Vec<(SliceTy, u32)>)> =
                        v.cases.iter().map(|c| (c.tag, c.fields.iter().map(|f| (f.ty, f.offset)).collect())).collect();
                    let tag = slot_memarg(almide_layout::SUM_TAG);
                    let ha = self.hold_i32()?;
                    self.f.instructions().local_tee(ha).i32_load(tag);
                    self.mix32(acc);
                    for (t, fields) in cases.iter().filter(|(_, fs)| !fs.is_empty()) {
                        self.f.instructions().local_get(ha).i32_load(tag).i32_const(*t as i32).i32_eq().if_(BlockType::Empty);
                        for (fty, off) in fields {
                            self.f.instructions().local_get(ha);
                            self.load_ty_slot(*fty, *off);
                            self.emit_val_hash(*fty, acc)?;
                        }
                        self.f.instructions().end();
                    }
                    self.release_i32();
                }
                NamedDef::Excluded => return unsup("deep-hash-excluded"),
            },
            other => return unsup(&format!("deep-hash:{other:?}")),
        }
        Ok(())
    }

    /// `[block]` -> each field of the block, in order, mixed into `acc`.
    fn emit_fields_hash(&mut self, fields: &[(SliceTy, u32)], acc: u32) -> Result<(), EmitError> {
        let ha = self.hold_i32()?;
        self.f.instructions().local_set(ha);
        for (fty, off) in fields {
            self.f.instructions().local_get(ha);
            self.load_ty_slot(*fty, *off);
            self.emit_val_hash(*fty, acc)?;
        }
        self.release_i32();
        Ok(())
    }

    /// `[list]` -> its byte length, then every element, mixed into `acc`.
    fn emit_list_hash(&mut self, el: SliceTy, acc: u32) -> Result<(), EmitError> {
        let stride = el.slot_size() as i32;
        let ha = self.hold_i32()?;
        let cur = self.hold_i32()?;
        let end = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_tee(ha).i32_load(len_memarg());
        }
        self.mix32(acc);
        {
            let mut i = self.f.instructions();
            i.local_get(ha).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_tee(cur);
            i.local_get(ha).i32_load(len_memarg()).i32_add().local_set(end);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
            i.local_get(cur);
        }
        self.load_ty_slot_at(el);
        self.emit_val_hash(el, acc)?;
        {
            let mut i = self.f.instructions();
            i.local_get(cur).i32_const(stride).i32_add().local_set(cur);
            i.br(0).end().end();
        }
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    /// The body of `$named_hashty_<ety>(key) -> i32`: the deep hash of the
    /// key in param 0, folded to 32 bits the way the String hash folds.
    pub(crate) fn emit_deep_hash_body(&mut self, t: SliceTy) -> Result<(), EmitError> {
        let acc = self.hold_i64()?;
        self.f.instructions().i64_const(DEEP_SEED as i64).local_set(acc).local_get(0);
        self.emit_val_hash(t, acc)?;
        {
            let mut i = self.f.instructions();
            i.local_get(acc).i64_const(32).i64_shr_u().local_get(acc).i64_xor();
            i.i64_const(crate::map_index::HASH_STR_MUL as i64).i64_mul().i64_const(32).i64_shr_u().i32_wrap_i64();
        }
        self.release_i64();
        Ok(())
    }
}
