//! User-type table: records and variants resolved to packed layouts.

use std::cell::RefCell;
use std::collections::HashMap;

use almide_ir::{IrProgram, IrTypeDeclKind, IrVariantKind};

use crate::{slice_ty_of, ETy, SliceTy};

// ── user-type table ─────────────────────────────────────────────────────

pub(crate) struct FieldInfo {
    pub(crate) name: String,
    pub(crate) ty: SliceTy,
    /// Payload-relative offset (records) — variant case fields already
    /// include the SUM_FIELD shift.
    pub(crate) offset: u32,
}

pub(crate) struct RecordDef {
    pub(crate) fields: Vec<FieldInfo>,
    pub(crate) size: u32,
}

pub(crate) struct CaseDef {
    pub(crate) name: String,
    pub(crate) tag: u32,
    pub(crate) fields: Vec<FieldInfo>,
    pub(crate) size: u32,
}

pub(crate) struct VariantDef {
    pub(crate) cases: Vec<CaseDef>,
}

pub(crate) enum NamedDef {
    Record(RecordDef),
    Variant(VariantDef),
    /// Declared but outside the slice (generic, record-shaped case,
    /// unmappable field): NAME-resolvable so other layouts can hold a
    /// slot for it (every composite field is an i32 address — a slot
    /// never needs the pointee's layout), but every USE refuses.
    Excluded,
}

pub(crate) struct TypeTable {
    pub(crate) by_name: HashMap<String, u32>,
    pub(crate) defs: Vec<NamedDef>,
    /// Variant constructor name → (type index, case index).
    pub(crate) ctors: HashMap<String, (u32, u32)>,
    /// Element-type arena (interior mutability so every `&TypeTable`
    /// signature stays put): dedup on intern makes handles canonical, so
    /// `ETy` equality IS type equality.
    arena: RefCell<Vec<SliceTy>>,
    interned: RefCell<HashMap<SliceTy, ETy>>,
    /// Tuple SHAPES, interned by element list — `SliceTy::Tuple(i)`
    /// equality is shape equality. Layout from `pack_fields`.
    tuples: RefCell<Vec<TupleDef>>,
    tuple_ids: RefCell<HashMap<Vec<SliceTy>, u32>>,
}

#[derive(Clone)]
pub(crate) struct TupleDef {
    /// (element type, payload-relative offset) per position.
    pub(crate) fields: Vec<(SliceTy, u32)>,
    pub(crate) size: u32,
}

impl TypeTable {
    pub(crate) fn intern(&self, t: SliceTy) -> ETy {
        if let Some(&h) = self.interned.borrow().get(&t) {
            return h;
        }
        let mut arena = self.arena.borrow_mut();
        let h = ETy::from_index(arena.len());
        arena.push(t);
        self.interned.borrow_mut().insert(t, h);
        h
    }

    /// Resolve an element handle back to its type.
    pub(crate) fn el(&self, h: ETy) -> SliceTy {
        self.arena.borrow()[h.index()]
    }

    /// Intern a tuple shape; layout comes from `pack_fields`.
    pub(crate) fn tuple(&self, elems: Vec<SliceTy>) -> u32 {
        if let Some(&i) = self.tuple_ids.borrow().get(&elems) {
            return i;
        }
        let widths: Vec<u32> = elems.iter().map(|t| t.slot_size()).collect();
        let (offs, size) = almide_layout::pack_fields(&widths);
        let def = TupleDef { fields: elems.iter().copied().zip(offs).collect(), size };
        let mut ts = self.tuples.borrow_mut();
        let i = ts.len() as u32;
        ts.push(def);
        self.tuple_ids.borrow_mut().insert(elems, i);
        i
    }

    /// A tuple shape by id (cloned — defs are tiny).
    pub(crate) fn tuple_def(&self, i: u32) -> TupleDef {
        self.tuples.borrow()[i as usize].clone()
    }
}

impl TypeTable {
    /// Build the table from the program's declarations — TWO PHASES so
    /// forward references and (mutually) recursive types resolve: every
    /// name is REGISTERED first (a slot for a composite field is an i32
    /// address, never the pointee's layout), then definitions are built;
    /// a declaration outside the slice becomes `Excluded` (uses refuse,
    /// but other layouts holding slots of it stay valid — constructing a
    /// value of an excluded type is impossible, so those slots are
    /// unreachable).
    pub(crate) fn build(ir: &IrProgram) -> TypeTable {
        let mut table = TypeTable {
            by_name: HashMap::new(),
            defs: Vec::new(),
            ctors: HashMap::new(),
            arena: RefCell::new(Vec::new()),
            interned: RefCell::new(HashMap::new()),
            tuples: RefCell::new(Vec::new()),
            tuple_ids: RefCell::new(HashMap::new()),
        };
        // Phase 1: every declaration gets an index (Excluded placeholder).
        for decl in &ir.type_decls {
            if matches!(decl.kind, IrTypeDeclKind::Alias { .. }) {
                continue;
            }
            let idx = table.defs.len() as u32;
            table.defs.push(NamedDef::Excluded);
            table.by_name.insert(decl.name.as_str().to_string(), idx);
        }
        // Phase 2: build definitions in place.
        for decl in &ir.type_decls {
            if decl.generics.is_some() {
                continue;
            }
            match &decl.kind {
                IrTypeDeclKind::Record { fields } => {
                    add_record(&mut table, decl.name.as_str(), fields);
                }
                IrTypeDeclKind::Variant { cases, boxed_args, boxed_record_fields, .. } => {
                    add_variant(&mut table, decl.name.as_str(), cases, boxed_args, boxed_record_fields);
                }
                IrTypeDeclKind::Alias { .. } => {}
            }
        }
        table
    }
}

/// One record declaration → packed field layout (or silently excluded
/// when a field is outside the slice).
fn add_record(table: &mut TypeTable, name: &str, fields: &[almide_ir::IrFieldDecl]) {

    let mut infos = Vec::new();
    let mut ok = true;
    for f in fields {
        match slice_ty_of(&f.ty, table) {
            Some(t) => infos.push((f.name.as_str().to_string(), t)),
            None => {
                ok = false;
                break;
            }
        }
    }
    if !ok {
        return;
    }
    let widths: Vec<u32> = infos.iter().map(|(_, t)| t.slot_size()).collect();
    let (offsets, size) = almide_layout::pack_fields(&widths);
    let fields = infos
        .into_iter()
        .zip(offsets)
        .map(|((name, ty), offset)| FieldInfo { name, ty, offset })
        .collect();
    let idx = table.by_name[name];
    table.defs[idx as usize] = NamedDef::Record(RecordDef { fields, size });
                }

/// One variant declaration → tagged-case layouts (excluded when generic,
/// recursive/boxed, record-shaped, or any field is outside the slice).
fn add_variant(
    table: &mut TypeTable,
    name: &str,
    cases: &[almide_ir::IrVariantDecl],
    boxed_args: &std::collections::HashSet<(String, usize)>,
    boxed_record_fields: &std::collections::HashSet<(String, String)>,
) {

    // Recursive variants are fine here: every case field is a SLOT
    // (composites are i32 addresses), so "boxing" is a Rust-target
    // concern the wasm layout never sees.
    let _ = (boxed_args, boxed_record_fields);
    let mut defs = Vec::new();
    let mut ok = true;
    for (tag, c) in cases.iter().enumerate() {
        let tys: Vec<SliceTy> = match &c.kind {
            IrVariantKind::Unit => Vec::new(),
            IrVariantKind::Tuple { fields } => {
                let mut v = Vec::new();
                for t in fields {
                    match slice_ty_of(t, table) {
        Some(st) => v.push(st),
        None => {
            ok = false;
            break;
        }
                    }
                }
                if !ok {
                    break;
                }
                v
            }
            IrVariantKind::Record { .. } => {
                ok = false;
                break;
            }
        };
        let widths: Vec<u32> = tys.iter().map(|t| t.slot_size()).collect();
        let (offsets, fsize) = almide_layout::pack_fields(&widths);
        let fields = tys
            .into_iter()
            .zip(offsets)
            .enumerate()
            .map(|(i, (ty, off))| FieldInfo {
                name: format!("{i}"),
                ty,
                offset: almide_layout::SUM_FIELD + off,
            })
            .collect();
        defs.push(CaseDef {
            name: c.name.as_str().to_string(),
            tag: tag as u32,
            fields,
            size: almide_layout::SUM_FIELD + fsize,
        });
    }
    if !ok {
        return;
    }
    let idx = table.by_name[name];
    for (ci, c) in defs.iter().enumerate() {
        table.ctors.insert(c.name.clone(), (idx, ci as u32));
    }
    table.defs[idx as usize] = NamedDef::Variant(VariantDef { cases: defs });
                }
