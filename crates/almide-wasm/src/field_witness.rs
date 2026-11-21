//! The emitter's half of the FIELDS witness (#4013, #4279;
//! witness_fields.rs is the sink): the kind and layout of a record, a tuple
//! or a variant case, read from the one type table every emitted offset
//! comes from, and the recording calls each construction site and field
//! access makes right after its instruction.

use wasm_encoder::ValType;

use crate::emitter::Emitter;
use crate::types_table::NamedDef;
use crate::witness::fields::{self as fw, Kind, Layout};
use crate::SliceTy;

/// A field's width in bytes: the width of its slot's load and store.
fn width(t: SliceTy) -> u32 {
    match t.val_type() {
        ValType::I64 | ValType::F64 => 8,
        _ => 4,
    }
}

impl Emitter<'_> {
    /// Lay out `ty`'s kinds in this pass (every case of a variant) and
    /// answer the kind of `case` (a variant case's position; `None` for a
    /// record or a tuple) and whether it is a variant's. `None` when no
    /// sweep collects or `ty` is no record, tuple or variant.
    fn fw_kind(&self, ty: SliceTy, case: Option<usize>) -> Option<(Kind, bool)> {
        if !fw::armed() {
            return None;
        }
        match ty {
            SliceTy::Tuple(ti) => {
                let def = self.types.tuple_def(ti);
                let k = (3 * ti + 2, 0);
                fw::declare(k, &Layout { n: 1, z: def.size, fields: def.fields.iter().map(|&(t, o)| (o, width(t))).collect() });
                Some((k, false))
            }
            SliceTy::Named(ti) => match self.types.def(ti) {
                NamedDef::Record(r) => {
                    let k = (3 * ti, 0);
                    fw::declare(k, &Layout { n: 1, z: r.size, fields: r.fields.iter().map(|f| (f.offset, width(f.ty))).collect() });
                    Some((k, false))
                }
                NamedDef::Variant(v) => {
                    let n = v.cases.len() as u32;
                    for c in &v.cases {
                        let mut fields = vec![(almide_layout::SUM_TAG, 4)];
                        fields.extend(c.fields.iter().map(|f| (f.offset, width(f.ty))));
                        fw::declare((3 * ti + 1, c.tag), &Layout { n, z: c.size, fields });
                    }
                    let c = v.cases.get(case?)?;
                    Some(((3 * ti + 1, c.tag), true))
                }
                NamedDef::Excluded => None,
            },
            _ => None,
        }
    }

    /// The position of the case named `name` of variant `ty`.
    pub(crate) fn fw_case(&self, ty: SliceTy, name: &str) -> Option<usize> {
        let SliceTy::Named(ti) = ty else { return None };
        match self.types.def(ti) {
            NamedDef::Variant(v) => v.cases.iter().position(|c| c.name == name),
            _ => None,
        }
    }

    /// A construction site of `ty` (`case` for a variant): its block was
    /// just set or teed into its local, right after the allocation.
    pub(crate) fn fw_site(&mut self, ty: SliceTy, case: Option<usize>) -> Option<fw::Site> {
        let (k, _) = self.fw_kind(ty, case)?;
        fw::site(k, self.f)
    }

    /// Field `i` of `site` (its index in the kind's layout: a variant's tag
    /// is 0, its case fields from 1) was just stored.
    pub(crate) fn fw_store(&mut self, site: Option<fw::Site>, i: usize) {
        fw::store(site, i, self.f);
    }

    /// Field `pos` of `ty` (`case` for a variant; `pos` counts the record's,
    /// tuple's or case's own fields, the tag not included) was just loaded
    /// or stored.
    pub(crate) fn fw_field(&mut self, ty: SliceTy, case: Option<usize>, pos: usize) {
        if let Some((k, variant)) = self.fw_kind(ty, case) {
            fw::access(k, pos + usize::from(variant), self.f);
        }
    }

    /// The tag word of a block of variant `ty` was just loaded.
    pub(crate) fn fw_tag(&mut self, ty: SliceTy) {
        let SliceTy::Named(ti) = ty else { return };
        if !fw::armed() || !matches!(self.types.def(ti), NamedDef::Variant(_)) {
            return;
        }
        // Lays out every case: the checker holds the tag read to each.
        let _ = self.fw_kind(ty, None);
        fw::tag(3 * ti + 1, self.f);
    }
}

/// The representation numbers of the two built-in sums, clear of every
/// type-table kind (`3 * index + 0..2`): an Option block of a 4- or 8-byte
/// slot, and a Result block.
const SUM_BASE: u32 = 1 << 20;

/// Which built-in sum a block is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sum {
    Opt,
    Res,
}

impl Emitter<'_> {
    /// Field `i` of the built-in sum kind `r` (laid out as `l`) was just
    /// loaded or stored.
    fn fw_sum(&mut self, r: u32, l: Layout, i: usize) {
        if !fw::armed() {
            return;
        }
        let k = (3 * (SUM_BASE + r), 0);
        fw::declare(k, &l);
        fw::access(k, i, self.f);
    }

    /// The value slot of a `some` block holding a `t` was just loaded or
    /// stored: a block of exactly that slot.
    pub(crate) fn fw_opt(&mut self, t: SliceTy) {
        let w = width(t);
        self.fw_sum(u32::from(w == 8), Layout { n: 1, z: w, fields: vec![(almide_layout::OPTION_FIELD, w)] }, 0);
    }

    /// A `some` block holding a `t` was just set into its local, right after
    /// its allocation of exactly that slot: a construction site, whose one
    /// store [`Self::fw_store`] records as field 0.
    pub(crate) fn fw_opt_site(&mut self, t: SliceTy) -> Option<fw::Site> {
        if !fw::armed() {
            return None;
        }
        let w = width(t);
        let k = (3 * (SUM_BASE + u32::from(w == 8)), 0);
        fw::declare(k, &Layout { n: 1, z: w, fields: vec![(almide_layout::OPTION_FIELD, w)] });
        fw::site(k, self.f)
    }

    /// As [`Self::fw_opt`], by the slot's width, for the access that ended
    /// `back` bytes of instructions ago.
    pub(crate) fn fw_opt_w(&mut self, w: u32, back: usize) {
        if !fw::armed() {
            return;
        }
        let k = (3 * (SUM_BASE + u32::from(w == 8)), 0);
        fw::declare(k, &Layout { n: 1, z: w, fields: vec![(almide_layout::OPTION_FIELD, w)] });
        fw::access_back(k, 0, self.f, back);
    }

    /// As [`Self::fw_res`], by the slot's width, for the access that ended
    /// `back` bytes of instructions ago.
    pub(crate) fn fw_res_w(&mut self, w: u32, back: usize) {
        if !fw::armed() {
            return;
        }
        let k = (3 * (SUM_BASE + 2), 0);
        fw::declare(k, &Self::res_layout());
        fw::access_back(k, if w == 8 { 2 } else { 1 }, self.f, back);
    }

    /// A Result block holding a `t` was just set (or teed) into its local,
    /// right after its allocation of 16 bytes: a construction site of the
    /// Result kind of that slot width, whose tag store is field 0 and whose
    /// value store is field 1. (Reads name the one Result kind whose layout
    /// carries the slot at both widths; a site needs a kind it fills.)
    pub(crate) fn fw_res_site(&mut self, t: SliceTy) -> Option<fw::Site> {
        if !fw::armed() {
            return None;
        }
        let w = width(t);
        let k = (3 * (SUM_BASE + 3 + u32::from(w == 8)), 0);
        fw::declare(k, &Layout { n: 1, z: 16, fields: vec![(almide_layout::SUM_TAG, 4), (almide_layout::SUM_FIELD, w)] });
        fw::site(k, self.f)
    }

    /// A Result block: its tag word, then one 8-byte value slot read at the
    /// width of the side it holds (fields 1 and 2 name the same slot).
    fn res_layout() -> Layout {
        Layout { n: 1, z: 16, fields: vec![(almide_layout::SUM_TAG, 4), (almide_layout::SUM_FIELD, 4), (almide_layout::SUM_FIELD, 8)] }
    }

    /// The tag word of a Result block was just loaded or stored.
    pub(crate) fn fw_res_tag(&mut self) {
        self.fw_res_tag_back(0);
    }

    /// The tag word of a Result block was loaded or stored `back` bytes of
    /// instructions ago (the fixed-size tail of the chain it was emitted in).
    pub(crate) fn fw_res_tag_back(&mut self, back: usize) {
        if !fw::armed() {
            return;
        }
        let k = (3 * (SUM_BASE + 2), 0);
        fw::declare(k, &Self::res_layout());
        fw::access_back(k, 0, self.f, back);
    }

    /// The value slot of a Result block holding a `t` was just loaded or
    /// stored.
    pub(crate) fn fw_res(&mut self, t: SliceTy) {
        let i = if width(t) == 8 { 2 } else { 1 };
        self.fw_sum(2, Self::res_layout(), i);
    }

    pub(crate) fn load_opt_slot(&mut self, t: SliceTy) {
        self.load_ty_slot(t, almide_layout::OPTION_FIELD);
        self.fw_opt(t);
    }

    pub(crate) fn store_opt_slot(&mut self, t: SliceTy) {
        self.store_ty_slot(t, almide_layout::OPTION_FIELD);
        self.fw_opt(t);
    }

    pub(crate) fn load_res_slot(&mut self, t: SliceTy) {
        self.load_ty_slot(t, almide_layout::SUM_FIELD);
        self.fw_res(t);
    }

    pub(crate) fn store_res_slot(&mut self, t: SliceTy) {
        self.store_ty_slot(t, almide_layout::SUM_FIELD);
        self.fw_res(t);
    }

    /// Load the value slot at `field` of a block that is `sum` (or of no
    /// built-in sum: a plain slot load).
    pub(crate) fn load_slot_of(&mut self, sum: Option<Sum>, t: SliceTy, field: u32) {
        match sum {
            Some(Sum::Opt) => self.load_opt_slot(t),
            Some(Sum::Res) => self.load_res_slot(t),
            None => self.load_ty_slot(t, field),
        }
    }
}
