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
