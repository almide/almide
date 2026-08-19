//! User-type table: records and variants resolved to packed layouts.

use std::cell::RefCell;
use std::collections::HashMap;

use almide_ir::{IrProgram, IrTypeDecl, IrTypeDeclKind, IrVariantKind};
use almide_base::intern::Sym;
use almide_types::types::Ty;

use crate::{slice_ty_of, ETy, SliceTy};

// ── user-type table ─────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct FieldInfo {
    pub(crate) name: String,
    pub(crate) ty: SliceTy,
    /// Payload-relative offset (records) — variant case fields already
    /// include the SUM_FIELD shift.
    pub(crate) offset: u32,
}

#[derive(Clone)]
pub(crate) struct RecordDef {
    pub(crate) fields: Vec<FieldInfo>,
    pub(crate) size: u32,
}

#[derive(Clone)]
pub(crate) struct CaseDef {
    pub(crate) name: String,
    pub(crate) tag: u32,
    pub(crate) fields: Vec<FieldInfo>,
    pub(crate) size: u32,
}

#[derive(Clone)]
pub(crate) struct VariantDef {
    pub(crate) cases: Vec<CaseDef>,
}

#[derive(Clone)]
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
    pub(crate) defs: RefCell<Vec<NamedDef>>,
    /// Generic declarations kept whole for on-demand monomorph
    /// instantiation; instances are indexed by (name, resolved args).
    generic_decls: HashMap<String, IrTypeDecl>,
    instances: RefCell<HashMap<(String, Vec<SliceTy>), u32>>,
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

impl TypeTable {
    /// A definition by index (cloned — defs are small).
    pub(crate) fn def(&self, i: u32) -> NamedDef {
        self.defs.borrow()[i as usize].clone()
    }

    /// Monomorph instance of a generic declaration, built on demand.
    /// The index is RESERVED before fields build, so mutually recursive
    /// generic types (Tree[A] / Forest[A]) resolve like concrete ones.
    pub(crate) fn instance(&self, name: &str, args: &[Ty]) -> Option<u32> {
        let mut resolved = Vec::new();
        for a in args {
            resolved.push(slice_ty_of(a, self)?);
        }
        let key = (name.to_string(), resolved);
        if let Some(&i) = self.instances.borrow().get(&key) {
            return Some(i);
        }
        let decl = self.generic_decls.get(name)?.clone();
        let params = decl.generics.as_ref()?;
        if params.len() != args.len() {
            return None;
        }
        let env: HashMap<Sym, &Ty> =
            params.iter().map(|p| p.name).zip(args.iter()).collect();
        let i = {
            let mut defs = self.defs.borrow_mut();
            let i = defs.len() as u32;
            defs.push(NamedDef::Excluded);
            i
        };
        self.instances.borrow_mut().insert(key, i);
        let built = match &decl.kind {
            IrTypeDeclKind::Record { fields } => build_record_def(self, fields, &env),
            IrTypeDeclKind::Variant { cases, .. } => build_variant_def(self, cases, &env),
            IrTypeDeclKind::Alias { .. } => None,
        };
        if let Some(def) = built {
            self.defs.borrow_mut()[i as usize] = def;
        }
        Some(i)
    }
}

/// Substitute type variables per the instantiation environment.
fn subst(ty: &Ty, env: &HashMap<Sym, &Ty>) -> Ty {
    match ty {
        Ty::TypeVar(s) => env.get(s).map(|t| (*t).clone()).unwrap_or_else(|| ty.clone()),
        // Generic params arrive from the decl lowerer as bare Named
        // references (`Named("T", [])`), not TypeVar — the param SHADOWS
        // any like-named type inside its declaration's scope.
        Ty::Named(n, args) if args.is_empty() && env.contains_key(n) => {
            (*env[n]).clone()
        }
        Ty::Applied(c, args) => Ty::Applied(c.clone(), args.iter().map(|a| subst(a, env)).collect()),
        Ty::Named(n, args) => Ty::Named(*n, args.iter().map(|a| subst(a, env)).collect()),
        Ty::Tuple(args) => Ty::Tuple(args.iter().map(|a| subst(a, env)).collect()),
        other => other.clone(),
    }
}

/// One variant case → CaseDef. Tuple cases get positional names
/// ("0", "1"); RECORD-shaped cases keep their declared field names —
/// one CaseDef shape serves construction and patterns for both.
fn build_case(
    table: &TypeTable,
    c: &almide_ir::IrVariantDecl,
    tag: u32,
    env: Option<&HashMap<Sym, &Ty>>,
) -> Option<CaseDef> {
    let named: Vec<(String, Ty)> = match &c.kind {
        IrVariantKind::Unit => Vec::new(),
        IrVariantKind::Tuple { fields } => fields
            .iter()
            .enumerate()
            .map(|(i, t)| (format!("{i}"), t.clone()))
            .collect(),
        IrVariantKind::Record { fields } => fields
            .iter()
            .map(|f| (f.name.as_str().to_string(), f.ty.clone()))
            .collect(),
    };
    let mut tys: Vec<(String, SliceTy)> = Vec::new();
    for (fname, t) in &named {
        let resolved = match env {
            Some(env) => slice_ty_of(&subst(t, env), table)?,
            None => slice_ty_of(t, table)?,
        };
        tys.push((fname.clone(), resolved));
    }
    let widths: Vec<u32> = tys.iter().map(|(_, t)| t.slot_size()).collect();
    let (offsets, fsize) = almide_layout::pack_fields(&widths);
    let fields = tys
        .into_iter()
        .zip(offsets)
        .map(|((name, ty), off)| FieldInfo {
            name,
            ty,
            offset: almide_layout::SUM_FIELD + off,
        })
        .collect();
    Some(CaseDef {
        name: c.name.as_str().to_string(),
        tag,
        fields,
        size: almide_layout::SUM_FIELD + fsize,
    })
}

fn build_record_def(
    table: &TypeTable,
    fields: &[almide_ir::IrFieldDecl],
    env: &HashMap<Sym, &Ty>,
) -> Option<NamedDef> {
    let mut infos = Vec::new();
    for f in fields {
        let t = slice_ty_of(&subst(&f.ty, env), table)?;
        infos.push((f.name.as_str().to_string(), t));
    }
    let widths: Vec<u32> = infos.iter().map(|(_, t)| t.slot_size()).collect();
    let (offsets, size) = almide_layout::pack_fields(&widths);
    let fields = infos
        .into_iter()
        .zip(offsets)
        .map(|((name, ty), offset)| FieldInfo { name, ty, offset })
        .collect();
    Some(NamedDef::Record(RecordDef { fields, size }))
}

fn build_variant_def(
    table: &TypeTable,
    cases: &[almide_ir::IrVariantDecl],
    env: &HashMap<Sym, &Ty>,
) -> Option<NamedDef> {
    let mut defs = Vec::new();
    for (tag, c) in cases.iter().enumerate() {
        defs.push(build_case(table, c, tag as u32, Some(env))?);
    }
    Some(NamedDef::Variant(VariantDef { cases: defs }))
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
            defs: RefCell::new(Vec::new()),
            generic_decls: HashMap::new(),
            instances: RefCell::new(HashMap::new()),
            ctors: HashMap::new(),
            arena: RefCell::new(Vec::new()),
            interned: RefCell::new(HashMap::new()),
            tuples: RefCell::new(Vec::new()),
            tuple_ids: RefCell::new(HashMap::new()),
        };
        // Phase 1: every CONCRETE declaration gets an index (Excluded
        // placeholder); generic declarations are kept whole for on-demand
        // instantiation. Module-owned declarations join the same table.
        let all_decls: Vec<&IrTypeDecl> = ir
            .type_decls
            .iter()
            .chain(ir.modules.iter().flat_map(|m| m.type_decls.iter()))
            .collect();
        for decl in &all_decls {
            if matches!(decl.kind, IrTypeDeclKind::Alias { .. }) {
                continue;
            }
            if decl.generics.is_some() {
                table.generic_decls.insert(decl.name.as_str().to_string(), (*decl).clone());
                continue;
            }
            let idx = table.defs.borrow().len() as u32;
            table.defs.borrow_mut().push(NamedDef::Excluded);
            table.by_name.insert(decl.name.as_str().to_string(), idx);
        }
        // Phase 2: build definitions in place.
        for decl in &all_decls {
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
    table.defs.borrow_mut()[idx as usize] = NamedDef::Record(RecordDef { fields, size });
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
    for (tag, c) in cases.iter().enumerate() {
        match build_case(table, c, tag as u32, None) {
            Some(d) => defs.push(d),
            None => return, // stays Excluded — honest refusal at uses
        }
    }
    let idx = table.by_name[name];
    for (ci, c) in defs.iter().enumerate() {
        table.ctors.insert(c.name.clone(), (idx, ci as u32));
    }
    table.defs.borrow_mut()[idx as usize] = NamedDef::Variant(VariantDef { cases: defs });
                }
