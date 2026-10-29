//! The per-type RC routine choice — which `$dec` / `$inc` / copy / cow
//! helper a slot type takes, its handle slots, and the droppable set —
//! split from rc_ownership.rs for the file budget.

use wasm_encoder::ValType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// A slot whose value is a heap HANDLE the holder owns one credit of
    /// (#2010 stage 2b/2c): Str / Bytes, a List, an Option / Result /
    /// tuple block, a record / variant (2c-ii), a Map / Set (Map stage
    /// b), a Value (its tagged payload glue), a Fn (the env's own drop
    /// glue, ruling B) and a Matrix (one flat block) — every heap shape
    /// (#2010 closed the graveyard).
    pub(crate) fn elem_is_handle(&self, elem: SliceTy) -> bool {
        match elem {
            SliceTy::Scalar(Scalar::Str | Scalar::Bytes)
            | SliceTy::List(_)
            | SliceTy::Option(_)
            | SliceTy::Result(..)
            | SliceTy::Tuple(_) => true,
            // Stage 2c-ii: records and variants with a layout.
            SliceTy::Named(ti) => self.named_has_layout(ti),
            // Map stage b: the entries array and its entries are credits.
            SliceTy::Map(..) | SliceTy::Set(_) => true,
            // Closures (#2010 ruling B): the env block is released through
            // the drop glue its own payload names (`$drop_fn`).
            SliceTy::Fn(_) => true,
            // #2010 item 5: a Value block releases its Str / Array / Object
            // payload through its tagged glue.
            SliceTy::Value => true,
            // A flat matrix block: `$dec_flat` (dec_fn_of's default).
            SliceTy::Matrix => true,
            _ => false,
        }
    }

    /// The `HttpCall` block (#2633): the one record whose single field is
    /// the unspellable `ty::HTTP_CALL_FIELD`.
    pub(crate) fn is_http_call(&self, t: SliceTy) -> bool {
        let SliceTy::Named(ti) = t else { return false };
        matches!(
            self.types.def(ti),
            crate::types_table::NamedDef::Record(r)
                if r.fields.len() == 1 && r.fields[0].name == crate::ty::HTTP_CALL_FIELD
        )
    }

    /// Does the type table hold a layout for this Named type (a record or
    /// a variant — `Excluded` names have slots but no layout)?
    fn named_has_layout(&self, ti: u32) -> bool {
        matches!(self.types.def(ti), crate::types_table::NamedDef::Record(_) | crate::types_table::NamedDef::Variant(_))
    }

    /// The handle slots of a fixed-slot block: `(payload offset, dec fn)`
    /// per slot, and for a tagged block (Result, variant) the per-tag
    /// tables under the tag word. Registers the nested glue on the way.
    fn shape_slots(&self, t: SliceTy) -> (Vec<(u32, u32)>, Option<(u32, Vec<(u32, Vec<(u32, u32)>)>)>) {
        let slot = |ft: SliceTy, off: u32| self.elem_is_handle(ft).then(|| (off, self.dec_fn_of(ft)));
        match t {
            SliceTy::Option(h) => (slot(self.types.el(h), almide_layout::OPTION_FIELD).into_iter().collect(), None),
            SliceTy::Result(a, b) => {
                let cases = [(0u32, self.types.el(a)), (1u32, self.types.el(b))]
                    .into_iter()
                    .filter_map(|(tag, pt)| slot(pt, almide_layout::SUM_FIELD).map(|s| (tag, vec![s])))
                    .collect();
                (Vec::new(), Some((almide_layout::SUM_TAG, cases)))
            }
            SliceTy::Tuple(h) => {
                let fields = self.types.tuple_def(h).fields.clone();
                (fields.iter().filter_map(|&(ft, off)| slot(ft, off)).collect(), None)
            }
            SliceTy::Named(ti) => match self.types.def(ti) {
                crate::types_table::NamedDef::Record(def) => {
                    (def.fields.iter().filter_map(|fi| slot(fi.ty, fi.offset)).collect(), None)
                }
                crate::types_table::NamedDef::Variant(def) => {
                    let cases = def
                        .cases
                        .iter()
                        .map(|c| (c.tag, c.fields.iter().filter_map(|fi| slot(fi.ty, fi.offset)).collect::<Vec<_>>()))
                        .filter(|(_, v)| !v.is_empty())
                        .collect();
                    (Vec::new(), Some((almide_layout::SUM_TAG, cases)))
                }
                crate::types_table::NamedDef::Excluded => (Vec::new(), None),
            },
            // #2010 item 5: a Value's payload by its tag — a Str block, an
            // Array's `List[Value]`, an Object's `List[(String, Value)]`.
            SliceTy::Value => {
                let items = SliceTy::List(self.types.intern(SliceTy::Value));
                let pair = self.types.tuple(vec![STR, SliceTy::Value]);
                let pairs = SliceTy::List(self.types.intern(SliceTy::Tuple(pair)));
                let cases = vec![
                    (crate::value::VT_STR as u32, vec![(almide_layout::SUM_FIELD, F_DEC_FLAT)]),
                    (crate::value::VT_ARRAY as u32, vec![(almide_layout::SUM_FIELD, self.dec_fn_of(items))]),
                    (crate::value::VT_OBJECT as u32, vec![(almide_layout::SUM_FIELD, self.dec_fn_of(pairs))]),
                ];
                (Vec::new(), Some((almide_layout::SUM_TAG, cases)))
            }
            _ => (Vec::new(), None),
        }
    }

    /// Does a fixed-slot block of type `t` hold any handle slot at all?
    fn shape_has_handles(&self, t: SliceTy) -> bool {
        let has = |ft: SliceTy| self.elem_is_handle(ft);
        match t {
            SliceTy::Option(h) => has(self.types.el(h)),
            SliceTy::Result(a, b) => has(self.types.el(a)) || has(self.types.el(b)),
            SliceTy::Tuple(h) => self.types.tuple_def(h).fields.iter().any(|&(ft, _)| has(ft)),
            SliceTy::Named(ti) => match self.types.def(ti) {
                crate::types_table::NamedDef::Record(def) => def.fields.iter().any(|fi| has(fi.ty)),
                crate::types_table::NamedDef::Variant(def) => {
                    def.cases.iter().any(|c| c.fields.iter().any(|fi| has(fi.ty)))
                }
                crate::types_table::NamedDef::Excluded => false,
            },
            SliceTy::Value => true,
            _ => false,
        }
    }

    /// Register a per-shape glue (`DropShape` / `IncShape`) and build its
    /// body once. A RECURSIVE shape (a tree variant whose case holds the
    /// tree) reaches its own registration while its slots are computed:
    /// the placeholder inserted first makes that inner call return the
    /// promised index instead of recursing.
    fn shape_helper(&self, h: crate::work::Helper, ty: SliceTy) -> u32 {
        let idx = self.work.helper(h.clone());
        if self.work.drop_bodies.borrow().iter().any(|(k, _)| *k == h) {
            return idx;
        }
        self.work.drop_bodies.borrow_mut().push((h.clone(), None));
        let (slots, tagged) = self.shape_slots(ty);
        let body = match (h.clone(), ty) {
            (crate::work::Helper::IncShape { .. }, _) => crate::runtime_alloc::emit_inc_shape(&slots, tagged),
            // #4252: a type on a cycle drops a deep value without one frame
            // per level — past the limit its releases are queued by table slot.
            (_, SliceTy::Named(ti)) if self.types.named_on_cycle(ti) => {
                let slot_of = |dec: u32| if dec == F_DEC_FLAT { 0 } else { self.work.slot(TableEntry::Direct(dec)) };
                let deep = |s: &[(u32, u32)]| s.iter().map(|&(off, dec)| (off, dec, slot_of(dec))).collect::<Vec<_>>();
                let slots = deep(&slots);
                let tagged = tagged.map(|(off, cases)| (off, cases.into_iter().map(|(tag, cs)| (tag, deep(&cs))).collect()));
                let globals = (
                    crate::deep_walk::global(self.work, crate::deep_walk::DROP_Q),
                    crate::deep_walk::global(self.work, crate::deep_walk::DROP_DEPTH),
                );
                let drop_ty = self.work.itype(vec![ValType::I32], None);
                crate::deep_walk::emit_drop_shape_deep(&slots, tagged, globals, drop_ty)
            }
            _ => crate::runtime_alloc::emit_drop_shape(&slots, tagged),
        };
        let mut bodies = self.work.drop_bodies.borrow_mut();
        if let Some(entry) = bodies.iter_mut().find(|(k, _)| *k == h) {
            entry.1 = Some(body);
        }
        idx
    }

    /// The release routine of a `t` value (#4277: every one handed out is
    /// recorded as a release for the RC byte check).
    pub(crate) fn dec_fn_of(&self, t: SliceTy) -> u32 {
        let f = self.dec_fn_of_ty(t);
        crate::witness::rc::release_fn(f);
        f
    }

    fn dec_fn_of_ty(&self, t: SliceTy) -> u32 {
        if self.is_http_call(t) {
            // #2633: the host call id's own drop — cancel and forget.
            self.note_host_op(crate::fs_meta::OP_HTTP_CALL_DROP);
            return self.work.helper(crate::work::Helper::DropHttpCall);
        }
        match t {
            SliceTy::List(h) => {
                let elem = self.types.el(h);
                if self.elem_is_handle(elem) {
                    let elem_dec = self.dec_fn_of(elem);
                    self.work.helper(crate::work::Helper::DropList { elem_dec })
                } else {
                    F_DEC_FLAT
                }
            }
            SliceTy::Option(_) | SliceTy::Result(..) | SliceTy::Tuple(_) | SliceTy::Named(_) | SliceTy::Value => {
                if self.shape_has_handles(t) {
                    self.shape_helper(crate::work::Helper::DropShape { ty: t }, t)
                } else {
                    F_DEC_FLAT
                }
            }
            SliceTy::Map(..) | SliceTy::Set(_) => {
                let raw = self.work.helper(crate::work::Helper::MapIdxSideRaw);
                let side_set = self.work.helper(crate::work::Helper::MapIdxSideSet { raw });
                let side_get = self.work.helper(crate::work::Helper::MapIdxSideGet);
                let side_clear = self.work.helper(crate::work::Helper::MapIdxForget { side_get, side_set });
                let (stride, decs) = self.entry_slots(t);
                let slots: [Option<(u32, u32)>; 2] =
                    [decs[0].map(|(off, ft)| (off, self.dec_fn_of(ft))), decs[1].map(|(off, ft)| (off, self.dec_fn_of(ft)))];
                if slots.iter().all(Option::is_none) {
                    self.work.helper(crate::work::Helper::DropMapSpine { side_clear })
                } else {
                    self.work.helper(crate::work::Helper::DropEntries { stride, slots, side_clear })
                }
            }
            SliceTy::Fn(_) => {
                let ti = self.work.itype(vec![ValType::I32], None);
                self.work.helper(crate::work::Helper::DropFn { ti })
            }
            _ => F_DEC_FLAT,
        }
    }

    /// The release of a C-319 cell holding a `t` (#2010): the cell's
    /// credit down; at zero its occupant released by `t`'s own drop (a
    /// flat occupant needs none), then the cell freed. Credits: one for
    /// the binding frame (released at its exits and at a loop rebind),
    /// one per capturing env (released by the env's glue).
    pub(crate) fn dec_cell_fn(&self, t: SliceTy) -> u32 {
        let elem_dec = self.elem_is_handle(t).then(|| self.dec_fn_of(t));
        self.work.helper(crate::work::Helper::DropCell { elem_dec })
    }

    /// The entry layout of a Map / Set: `(stride, [key slot, value slot])`
    /// where a slot is `Some((offset, type))` only when the type is a heap
    /// handle the entries array holds a credit of (a Set has one slot at
    /// offset 0; a flat key or value is `None`).
    fn entry_slots(&self, t: SliceTy) -> (u32, [Option<(u32, SliceTy)>; 2]) {
        let handle = |ft: SliceTy, off: u32| self.elem_is_handle(ft).then_some((off, ft));
        match t {
            SliceTy::Map(kh, vh) => {
                let (k, v) = (self.types.el(kh), self.types.el(vh));
                let (koff, voff, stride) = crate::collections::entry_layout(k, v);
                (stride, [handle(k, koff), handle(v, voff)])
            }
            SliceTy::Set(h) => {
                let e = self.types.el(h);
                (e.slot_size(), [handle(e, 0), None])
            }
            _ => (0, [None, None]),
        }
    }

    /// `Some($inc_entries)` when the entries of a Map / Set of type `t`
    /// hold handle slots a copied entries array must take credits on.
    pub(crate) fn inc_entries_fn(&self, t: SliceTy) -> Option<u32> {
        let (stride, decs) = self.entry_slots(t);
        let slots = [decs[0].map(|(off, _)| off), decs[1].map(|(off, _)| off)];
        (!slots.iter().all(Option::is_none)).then(|| self.work.helper(crate::work::Helper::IncEntries { stride, slots }))
    }

    /// +1 on every handle slot of the entries in local `h` (the whole
    /// LEN, or the first `nbytes` from local `nbytes_local`) — after a
    /// native arm copied entries out of another Map / Set.
    pub(crate) fn emit_inc_entries(&mut self, h: u32, t: SliceTy, nbytes_local: Option<u32>) {
        if let Some(f) = self.inc_entries_fn(t) {
            let mut i = self.f.instructions();
            i.local_get(h);
            match nbytes_local {
                Some(n) => i.local_get(n),
                None => i.local_get(h).i32_load(len_memarg()),
            };
            i.call(f);
        }
    }

    /// The release fn of an owned LOCAL, by the type `rc_own` recorded
    /// for it (a param is recorded at frame entry).
    pub(crate) fn dec_fn_of_local(&self, idx: u32) -> u32 {
        if let Some(t) = self.arm_rests.ty_of(idx) {
            return self.dec_fn_of(t);
        }
        let Some(&t) = self.owned_ty.get(&idx) else {
            crate::witness::rc::release_fn(F_DEC_FLAT);
            return F_DEC_FLAT;
        };
        // A C-319 cell local owns the CELL, not the occupant (#2010).
        let is_cell = self.locals.iter().any(|(v, &(i, _))| i == idx && self.cells.contains(v));
        if is_cell {
            let f = self.dec_cell_fn(t);
            crate::witness::rc::release_fn(f);
            f
        } else {
            self.dec_fn_of(t)
        }
    }

    /// `Some($inc_elems)` when a spine of `elem` slots copied from another
    /// spine must take its own element credits.
    pub(crate) fn inc_elems_fn(&self, elem: SliceTy) -> Option<u32> {
        self.elem_is_handle(elem).then(|| self.work.helper(crate::work::Helper::IncElems))
    }

    /// +1 on every element of the spine in local `h` (nothing for scalar
    /// elements) — after a native arm's `memory_copy` of its slots.
    pub(crate) fn emit_inc_elems(&mut self, h: u32, elem: SliceTy) {
        if let Some(f) = self.inc_elems_fn(elem) {
            self.f.instructions().local_get(h).call(f);
        }
    }

    /// The whole-block copy of a value of type `t`: `$block_copy`, or for
    /// a List of handles the variant whose copy takes its element credits.
    pub(crate) fn copy_fn_of(&self, t: SliceTy) -> u32 {
        match t {
            SliceTy::List(h) => match self.inc_elems_fn(self.types.el(h)) {
                Some(inc_elems) => self.work.helper(crate::work::Helper::CopyElems { inc_elems }),
                None => F_BLOCK_COPY,
            },
            SliceTy::Option(_) | SliceTy::Result(..) | SliceTy::Tuple(_) | SliceTy::Named(_) | SliceTy::Value
                if self.shape_has_handles(t) =>
            {
                let inc_elems = self.shape_helper(crate::work::Helper::IncShape { ty: t }, t);
                self.work.helper(crate::work::Helper::CopyElems { inc_elems })
            }
            // Map stage b: a copied entries array takes its entry credits.
            SliceTy::Map(..) | SliceTy::Set(_) => match self.inc_entries_fn(t) {
                Some(inc_entries) => self.work.helper(crate::work::Helper::CopyEntries { inc_entries }),
                None => F_BLOCK_COPY,
            },
            _ => F_BLOCK_COPY,
        }
    }

    /// The copy-on-write judge for a value of type `t`: `$cow`, or for a
    /// List of handles — or a fixed-slot shape holding one (a record whose
    /// field a set writes through, bytes_recv.rs) — the variant whose copy
    /// takes its slot credits, as `copy_fn_of` picks for the plain copy.
    pub(crate) fn cow_fn_of(&self, t: SliceTy) -> u32 {
        match t {
            SliceTy::List(h) => match self.inc_elems_fn(self.types.el(h)) {
                Some(inc_elems) => self.work.helper(crate::work::Helper::CowElems { inc_elems }),
                None => F_COW,
            },
            SliceTy::Option(_) | SliceTy::Result(..) | SliceTy::Tuple(_) | SliceTy::Named(_) | SliceTy::Value
                if self.shape_has_handles(t) =>
            {
                let inc_elems = self.shape_helper(crate::work::Helper::IncShape { ty: t }, t);
                self.work.helper(crate::work::Helper::CowElems { inc_elems })
            }
            _ => F_COW,
        }
    }

    pub(crate) fn rc_droppable(&self, t: SliceTy) -> bool {
        rc_droppable_ty(self.types, t)
    }
}

/// `rc_droppable` as a free function: the param borrow inference
/// (param_borrow.rs) runs before any Emitter exists.
pub(crate) fn rc_droppable_ty(types: &crate::types_table::TypeTable, t: SliceTy) -> bool {
    {
        match t {
            SliceTy::Scalar(Scalar::Str | Scalar::Bytes) => true,
            // A List of ANY element (stage 2a): `$dec_flat` frees the SPINE
            // only; the elements keep the credits they hold today. The
            // shallow-drop experiment first diverged 40 corpus fixtures, and
            // every one was one of two defects: `value.object` borrowed the
            // spine it keeps (now Retain), and the matrix byte loaders trusted
            // fresh memory to be zero (now filled). Releasing the elements is
            // stage 2b: typed glue (#2010).
            SliceTy::List(_) => true,
            // Stage 2c: an Option / Result / tuple block of ANY payload —
            // a handle slot is released by the typed drop (`dec_fn_of`),
            // a flat slot needs nothing, and a slot of a shape with no
            // typed drop yet (a record, a Map) keeps its credit (leak,
            // never a dangle).
            SliceTy::Option(_) | SliceTy::Result(..) | SliceTy::Tuple(_) => true,
            // Records and variants wait for stage 1b: the `mut` param
            // move-mode rewrite (C-132) carries a record through a
            // (result, buffer) tuple and writes it back through the Assign
            // route, whose ownership is not yet audited for a droppable
            // record (mut_param_effect_never_err double-freed the Tally).
            SliceTy::Named(ti) => matches!(
                types.def(ti),
                crate::types_table::NamedDef::Record(_) | crate::types_table::NamedDef::Variant(_)
            ),
            // Map stage b (#2010): the entries array is released with its
            // index side-table entry, and every handle key / value / member
            // through the typed entry walk (`DropEntries`).
            SliceTy::Map(..) | SliceTy::Set(_) => true,
            // Closures (#2010 ruling B): a Fn value is an env block that
            // names its own drop glue; pool-static Fn blocks are immortal.
            SliceTy::Fn(_) => true,
            // #2010 item 5: a Value through its tagged payload glue.
            SliceTy::Value => true,
            // A matrix is ONE flat block (`[rows][cols][f64…]`, no interior
            // handles): `$dec_flat` releases it whole.
            SliceTy::Matrix => true,
            _ => false,
        }
    }
}
