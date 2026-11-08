//! #4006 A5: records with heap fields render as generated Rust structs (owner
//! ruling 2026-10-30: a struct per record layout, `Dup` = clone).
//!
//! The struct set is DERIVED from the program's declared record layouts
//! (`crate::lower::RecordLayouts`): one struct per non-generic record whose
//! every field has a native spelling, field `fK` holding declared field `K`
//! (the uniform slot `K` of the MIR block, so the slot-address algebra reads
//! and writes it by index). An all-scalar record keeps the existing `Vec<i64>`
//! convention, and a record that contains itself by value is left out (a
//! by-value Rust struct cannot), so it walls as before.
//!
//! The table is installed for the duration of one render (`install`), the
//! way the charge probe's budget flags are.

use crate::render_native::NTy;
use std::cell::RefCell;

/// One generated type: a declared record (a struct, `fields`) or a declared
/// variant (an enum, `cases`: each constructor's tag and field kinds). Field
/// kinds are owned spellings: `I64`, `Str`, `Vec`, `VecStr`, `Rec`; an enum
/// boxes its `Rec` fields, so a recursive variant has a size.
#[derive(Clone, Debug, Default)]
pub(crate) struct RecDecl {
    pub name: String,
    /// Source types prove Rust structural equality preserves language equality.
    pub eq_safe: bool,
    pub fields: Vec<NTy>,
    pub cases: Option<Vec<(u32, Vec<NTy>)>>,
    /// A list type of generated or nested-list elements: `Vec<elem>`.
    pub list_of: Option<NTy>,
    /// A builtin with a fixed runtime spelling (`Bytes` is `Vec<u8>`, the v0
    /// runtime's own representation, so floor bodies take it as is).
    pub alias: Option<&'static str>,
    /// An enum's Default constructor: one whose aggregate fields are all
    /// finite without it, so `Default` terminates.
    pub default_tag: Option<u32>,
    /// An `Option` of a heap payload: `Option<payload>` (a scalar payload
    /// rides the `NTy::Opt` carrier and is never registered here).
    pub option_of: Option<NTy>,
    /// A `Result` outside the `NTy::Res` / `NTy::ResStr` carriers:
    /// `Result<ok, err>`. It has no `Default`, so no struct or enum holds one.
    pub result_of: Option<(NTy, NTy)>,
    /// A `Map[K, V]`: v0's runtime `AlmideMap<key, value>`, so the declared
    /// floor's map bodies take it as is.
    pub map_of: Option<(NTy, NTy)>,
    /// A `Set[A]`: v0's runtime `AlmideSet<elem>`, so the declared floor's
    /// set bodies take it as is.
    pub set_of: Option<NTy>,
}

/// A type the MIR carries in one i64 lane, read off its `Repr` (the layout
/// decision `lower::repr_of` makes once): Int, Bool, the sized integers, and
/// the floats as their bits. A `RawPtr` is an address, not a value, so it has
/// no native spelling and stays out.
pub(crate) fn is_lane_scalar(t: &almide_lang::types::Ty) -> bool {
    !matches!(t, almide_lang::types::Ty::RawPtr)
        && matches!(crate::lower::repr_of(t), Ok(crate::Repr::Scalar { .. }))
}

/// A lane scalar the native render holds as a real `f64` (the 64-bit floats;
/// a `Float32` keeps its f32 bits in the lane, like every other lane value).
pub(crate) fn is_f64_scalar(t: &almide_lang::types::Ty) -> bool {
    matches!(t, almide_lang::types::Ty::Float | almide_lang::types::Ty::Float64)
}

/// The `Option`/`Result` types a generated carrier stands for: everything the
/// scalar carriers (`Option<i64>`, `Result<i64, String>`, `Result<String,
/// String>`) do not cover, read off the declared type.
fn is_heap_carrier(t: &almide_lang::types::Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    let scalar = |t: &Ty| is_lane_scalar(t) || matches!(t, Ty::Unit);
    match t {
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => !scalar(&a[0]) && !matches!(a[0], Ty::Unit),
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
            !(matches!(a[1], Ty::String) && (scalar(&a[0]) || matches!(a[0], Ty::String)))
        }
        _ => false,
    }
}

/// The (key, value) kinds of a generated map type.
pub(crate) fn map_kinds(i: u16) -> Option<(NTy, NTy)> {
    RECS.with(|r| r.borrow().get(i as usize).and_then(|d| d.map_of))
}

/// Collection identities whose zero-element literal is their empty value.
/// A record/carrier with a Default is not an empty collection.
pub(crate) fn is_empty_collection(i: u16) -> bool {
    list_elem(i).is_some() || map_kinds(i).is_some() || set_elem(i).is_some()
}

/// The element kind of a generated set type.
pub(crate) fn set_elem(i: u16) -> Option<NTy> {
    RECS.with(|r| r.borrow().get(i as usize).and_then(|d| d.set_of))
}

/// A set whose elements retain the runtime's exact value representation.
/// Bool and Float lanes cannot substitute their bits for the runtime values.
/// The recursive type guard covers lists and sets without erasing their elements.
pub(crate) fn is_native_set(t: &almide_lang::types::Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    matches!(t, Ty::Applied(TypeConstructorId::Set, a) if a.len() == 1 && native_set_element_type(&a[0]))
}

/// The generated `AlmideSet<elem>` alias, appended on first use.
pub(crate) fn set_type_of(e: NTy) -> u16 {
    RECS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.iter().position(|d| d.set_of == Some(e)) {
            return i as u16;
        }
        r.push(RecDecl { name: format!("Set<{e:?}>"), set_of: Some(e), ..RecDecl::default() });
        (r.len() - 1) as u16
    })
}

/// The payload kind of a generated `Option` carrier.
pub(crate) fn option_payload(i: u16) -> Option<NTy> {
    RECS.with(|r| r.borrow().get(i as usize).and_then(|d| d.option_of))
}

/// The (ok, err) kinds of a generated `Result` carrier.
pub(crate) fn result_payloads(i: u16) -> Option<(NTy, NTy)> {
    RECS.with(|r| r.borrow().get(i as usize).and_then(|d| d.result_of))
}

// Default support follows the declared fields recursively.
include!("render_native_defaults.rs");

/// The table key of the builtin `Bytes` type.
pub(crate) const BYTES_KEY: &str = "Ty::Bytes";

/// `t` is a generic instance whose bound fields are all scalar.
pub(crate) fn is_scalar_instance(t: &almide_lang::types::Ty) -> bool {
    SCALAR_INSTANCES.with(|si| si.borrow().contains(&tuple_key(t)))
}

/// The index of `Bytes`, appended on first use. It is always the last entry
/// it joins, so no earlier index moves; the header is assembled after the
/// bodies render, so a body may be the first user.
pub(crate) fn bytes_index() -> u16 {
    RECS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.iter().position(|d| d.name == BYTES_KEY) {
            return i as u16;
        }
        r.push(RecDecl { name: BYTES_KEY.into(), fields: Vec::new(), cases: None, list_of: None, alias: Some("Vec<u8>"), default_tag: None, option_of: None, result_of: None, map_of: None, eq_safe: false, set_of: None });
        (r.len() - 1) as u16
    })
}

/// The table key of the stdlib's dynamic `Value` type.
pub(crate) const VALUE_KEY: &str = "Ty::Value";

/// The generated `Value` type: v0's own runtime enum (`AlmideValue`), so a
/// declared-floor body takes it as is. Appended on first use, like `Bytes`.
pub(crate) fn value_index() -> u16 {
    RECS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.iter().position(|d| d.name == VALUE_KEY) {
            return i as u16;
        }
        r.push(RecDecl { name: VALUE_KEY.into(), fields: Vec::new(), cases: None, list_of: None, alias: Some("AlmideValue"), default_tag: None, option_of: None, result_of: None, map_of: None, eq_safe: false, set_of: None });
        (r.len() - 1) as u16
    })
}

/// `t` is the generated `Value` type.
pub(crate) fn is_value(t: NTy) -> bool {
    matches!(t, NTy::Rec(i) | NTy::RecRef(i) if rec_index(VALUE_KEY) == Some(i))
}

/// The table key of the builtin `Matrix` type.
pub(crate) const MATRIX_KEY: &str = "Ty::Matrix";

/// The generated `Matrix` type: v0's own runtime struct (`AlmideMatrix`), so a
/// declared-floor body takes it as is. Appended on first use, like `Bytes`.
pub(crate) fn matrix_index() -> u16 {
    RECS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.iter().position(|d| d.name == MATRIX_KEY) {
            return i as u16;
        }
        r.push(RecDecl { name: MATRIX_KEY.into(), fields: Vec::new(), cases: None, list_of: None, alias: Some("AlmideMatrix"), default_tag: None, option_of: None, result_of: None, map_of: None, eq_safe: false, set_of: None });
        (r.len() - 1) as u16
    })
}

/// `t` is the generated `Matrix` type.
pub(crate) fn is_matrix(t: NTy) -> bool {
    matches!(t, NTy::Rec(i) | NTy::RecRef(i) if rec_index(MATRIX_KEY) == Some(i))
}

/// The generated `Result<ok, err>` carrier, appended on first use (a
/// declared-floor call returns one the program's signatures never spell).
pub(crate) fn result_type_of(ok: NTy, err: NTy) -> u16 {
    RECS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.iter().position(|d| d.result_of == Some((ok, err))) {
            return i as u16;
        }
        r.push(RecDecl {
            name: format!("Result<{ok:?}, {err:?}>"),
            fields: Vec::new(),
            cases: None,
            list_of: None,
            alias: None,
            default_tag: None,
            option_of: None,
            result_of: Some((ok, err)),
            map_of: None, eq_safe: false, set_of: None,
        });
        (r.len() - 1) as u16
    })
}

thread_local! {
    static DECLARED_MAPS: std::cell::RefCell<std::collections::BTreeMap<crate::ValueId, u16>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// The generated map type each value of the function being rendered is
/// declared as (its signature's, its callees', through joins and rebinds).
pub(crate) fn set_declared(m: std::collections::BTreeMap<crate::ValueId, u16>) {
    DECLARED_MAPS.with(|d| *d.borrow_mut() = m);
}

/// The declared generated map type of `v` (`map.new()`'s result).
pub(crate) fn declared_map(v: crate::ValueId) -> Option<u16> {
    DECLARED_MAPS.with(|d| d.borrow().get(&v).copied())
}

/// The generated `AlmideMap<key, value>` alias, appended on first use.
pub(crate) fn map_type_of(key: NTy, value: NTy) -> u16 {
    RECS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.iter().position(|d| d.map_of == Some((key, value))) {
            return i as u16;
        }
        r.push(RecDecl {
            name: format!("Map<{key:?}, {value:?}>"),
            fields: Vec::new(),
            cases: None,
            list_of: None,
            alias: None,
            default_tag: None,
            option_of: None,
            result_of: None,
            map_of: Some((key, value)), eq_safe: false, set_of: None,
        });
        (r.len() - 1) as u16
    })
}

/// The generated `Option<payload>` carrier, appended on first use.
pub(crate) fn option_type_of(payload: NTy) -> u16 {
    RECS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.iter().position(|d| d.option_of == Some(payload)) {
            return i as u16;
        }
        r.push(RecDecl {
            name: format!("Option<{payload:?}>"),
            fields: Vec::new(),
            cases: None,
            list_of: None,
            alias: None,
            default_tag: None,
            option_of: Some(payload),
            result_of: None,
            map_of: None, eq_safe: false, set_of: None,
        });
        (r.len() - 1) as u16
    })
}

/// The generated list type of `e` elements, appended on first use (at the
/// end, like `Bytes`, so no earlier index moves).
pub(crate) fn list_type_of(e: NTy) -> u16 {
    RECS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.iter().position(|d| d.list_of == Some(e)) {
            return i as u16;
        }
        r.push(RecDecl {
            name: format!("List<{e:?}>"),
            fields: Vec::new(),
            cases: None,
            list_of: Some(e),
            alias: None,
            default_tag: None,
            option_of: None,
            result_of: None,
            map_of: None, eq_safe: false, set_of: None,
        });
        (r.len() - 1) as u16
    })
}

/// The generated struct of an untyped block whose slot kinds its stores give
/// (`render_native_slots`): field `fK` holds slot `K`. Appended on first use
/// at the end, like a list type, so no earlier index moves.
pub(crate) fn slots_type_of(fields: &[NTy]) -> u16 {
    RECS.with(|r| {
        let mut r = r.borrow_mut();
        let name = format!("slots {fields:?}");
        if let Some(i) = r.iter().position(|d| d.name == name) {
            return i as u16;
        }
        r.push(RecDecl { name, fields: fields.to_vec(), cases: None, list_of: None, option_of: None, result_of: None, map_of: None, eq_safe: false, set_of: None, alias: None, default_tag: None });
        (r.len() - 1) as u16
    })
}

/// `t` is the generated `Bytes` type.
pub(crate) fn is_bytes(t: NTy) -> bool {
    matches!(t, NTy::Rec(i) | NTy::RecRef(i) if rec_index(BYTES_KEY) == Some(i))
}

/// The declarations one render derives from: (record layouts, variant
/// layouts, signature tuple and list types).
type Inputs = (crate::lower::RecordLayouts, crate::lower::VariantLayouts, Vec<almide_lang::types::Ty>);

thread_local! {
    /// Generic instances whose bound fields are all scalar: they keep the
    /// `Vec<i64>` slot convention, like a non-generic scalar record.
    static SCALAR_INSTANCES: RefCell<std::collections::BTreeSet<String>> = const { RefCell::new(std::collections::BTreeSet::new()) };
    static RECS: RefCell<Vec<RecDecl>> = const { RefCell::new(Vec::new()) };
    static INPUTS: RefCell<Option<Inputs>> = const { RefCell::new(None) };
}


/// Derive and install the table for one render.
pub(crate) fn install_derived(
    layouts: &crate::lower::RecordLayouts,
    variants: &crate::lower::VariantLayouts,
    tuples: Vec<almide_lang::types::Ty>,
    region_names: std::collections::BTreeSet<String>,
) -> RecGuard {
    REGION_NAMES.with(|n| *n.borrow_mut() = region_names);
    let recs = derive_records(layouts, variants, &tuples);
    INPUTS.with(|i| *i.borrow_mut() = Some((layouts.clone(), variants.clone(), tuples)));
    install(recs)
}

/// Re-derive with the tuple and list types of more signatures (a linked
/// self-host body's). Indices may move, so this runs before any signature
/// table or render reads them.
pub(crate) fn extend_with(more: &[almide_lang::types::Ty]) {
    let recs = INPUTS.with(|i| {
        let mut i = i.borrow_mut();
        let (l, v, ts) = i.as_mut()?;
        let mut grew = false;
        for t in more {
            let mut found = Vec::new();
            collect_tuples(t, &mut found);
            for f in found {
                if !ts.contains(&f) {
                    ts.push(f);
                    grew = true;
                }
            }
        }
        grew.then(|| derive_records(l, v, ts))
    });
    if let Some(recs) = recs {
        RECS.with(|r| *r.borrow_mut() = recs);
    }
}

/// Clears the installed table when the render ends.
pub(crate) struct RecGuard;
impl Drop for RecGuard {
    fn drop(&mut self) {
        RECS.with(|r| r.borrow_mut().clear());
        INPUTS.with(|i| *i.borrow_mut() = None);
        SCALAR_INSTANCES.with(|si| si.borrow_mut().clear());
        REGION_NAMES.with(|n| n.borrow_mut().clear());
    }
}

pub(crate) fn install(recs: Vec<RecDecl>) -> RecGuard {
    RECS.with(|r| *r.borrow_mut() = recs);
    RecGuard
}

pub(crate) fn rec_index(name: &str) -> Option<u16> {
    RECS.with(|r| r.borrow().iter().position(|d| d.name == name).map(|i| i as u16))
}

pub(crate) fn rec_field(i: u16, k: usize) -> Option<NTy> {
    RECS.with(|r| r.borrow().get(i as usize).filter(|d| d.cases.is_none() && d.list_of.is_none() && d.alias.is_none()).and_then(|d| d.fields.get(k).copied()))
}

/// Generated type `i` is a struct whose every field is an i64 lane value.
pub(crate) fn all_scalar_struct(i: u16) -> bool {
    RECS.with(|r| {
        r.borrow().get(i as usize).is_some_and(|d| {
            d.cases.is_none() && d.list_of.is_none() && d.alias.is_none() && d.option_of.is_none() && d.result_of.is_none()
                && !d.fields.is_empty() && d.fields.iter().all(|f| *f == NTy::I64)
        })
    })
}

/// The number of fields of a generated struct.
pub(crate) fn field_count(i: u16) -> usize {
    RECS.with(|r| r.borrow().get(i as usize).map_or(0, |d| d.fields.len()))
}

/// The element kind of a generated list type (`None` for a struct or enum).
pub(crate) fn list_elem(i: u16) -> Option<NTy> {
    RECS.with(|r| r.borrow().get(i as usize).and_then(|d| d.list_of))
}

/// The cases of a generated enum (`None` for a struct).
pub(crate) fn enum_cases(i: u16) -> Option<Vec<(u32, Vec<NTy>)>> {
    RECS.with(|r| r.borrow().get(i as usize).and_then(|d| d.cases.clone()))
}

/// The suffix naming one field kind in an enum's slot accessors.
pub(crate) fn kind_suffix(t: NTy) -> String {
    match t {
        NTy::I64 => "i".into(),
        NTy::Str => "s".into(),
        NTy::Vec => "v".into(),
        NTy::VecStr => "vs".into(),
        NTy::Rec(j) => format!("r{j}"),
        _ => "x".into(),
    }
}

/// The distinct kinds an enum holds at payload slot `k` (slot 0 is the tag).
pub(crate) fn enum_slot_kinds(i: u16, k: usize) -> Vec<NTy> {
    let mut out: Vec<NTy> = Vec::new();
    for (_, fs) in enum_cases(i).unwrap_or_default() {
        if let Some(f) = k.checked_sub(1).and_then(|j| fs.get(j)) {
            if !out.contains(f) {
                out.push(*f);
            }
        }
    }
    out
}

/// A record field of enum `i` is boxed only where the field's type holds `i`
/// again inline (through struct, enum, Option and Result payloads; a list or
/// map already holds its elements behind a pointer): a recursive variant then
/// has a size, and a non-recursive one keeps its payload in place, as v0's
/// enums do.
pub(crate) fn enum_field_boxed(i: u16, f: NTy) -> bool {
    let NTy::Rec(j) = f else { return false };
    RECS.with(|r| {
        let r = r.borrow();
        let inline = |k: u16| -> Vec<u16> {
            let Some(d) = r.get(k as usize) else { return Vec::new() };
            let mut ks: Vec<NTy> = d.fields.clone();
            if let Some(cs) = &d.cases {
                ks.extend(cs.iter().flat_map(|(_, fs)| fs.iter().copied()));
            }
            ks.extend(d.option_of);
            if let Some((o, e)) = d.result_of {
                ks.extend([o, e]);
            }
            ks.into_iter().filter_map(|t| match t {
                NTy::Rec(x) => Some(x),
                _ => None,
            }).collect()
        };
        let mut stack = vec![j];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(k) = stack.pop() {
            if k == i {
                return true;
            }
            if seen.insert(k) {
                stack.extend(inline(k));
            }
        }
        false
    })
}

/// An enum payload field's spelling: boxed where [`enum_field_boxed`] says so
/// — in a region twin (#4006 C2) the box is an `AlmideRgn` arena handle.
fn enum_field_ty(i: u16, f: &NTy) -> String {
    let boxed = if is_region(i) { "AlmideRgn" } else { "Box" };
    if enum_field_boxed(i, *f) { format!("{boxed}<{}>", owned_ty(*f)) } else { owned_ty(*f) }
}


fn enum_def(i: u16, d: &RecDecl, cases: &[(u32, Vec<NTy>)]) -> String {
    let n = struct_name(i);
    // A region twin is `Copy` (its boxed fields are arena handles), so a
    // rewound window block owns nothing to drop.
    let derive = if is_region(i) { "Clone, Copy" } else if d.eq_safe { "Clone, PartialEq" } else { "Clone" };
    let mut out = format!("/// variant `{}`\n#[derive({derive})]\nenum {n} {{\n", d.name);
    for (t, fs) in cases {
        if fs.is_empty() {
            out.push_str(&format!("    C{t},\n"));
        } else {
            out.push_str(&format!("    C{t}({}),\n", fs.iter().map(|f| enum_field_ty(i, f)).collect::<Vec<_>>().join(", ")));
        }
    }
    out.push_str("}\n");
    // Default is a case with no generated-type field, so a recursive enum's
    // default terminates (derive_records keeps only enums that have one).
    let leaf = d.default_tag.unwrap_or(cases[0].0);
    out.push_str(&format!("impl Default for {n} {{\n    fn default() -> Self {{ Self::with_tag({leaf}) }}\n}}\n"));
    out.push_str(&format!("#[allow(unused_variables, unreachable_patterns)]\nimpl {n} {{\n"));
    out.push_str(&enum_tag_fns(i, cases));
    let width = cases.iter().map(|(_, fs)| fs.len()).max().unwrap_or(0);
    for k in 1..=width {
        for kind in enum_slot_kinds(i, k) {
            out.push_str(&enum_slot_fns(i, k, kind, cases));
        }
    }
    // from_slots: a scalar-only constructor built as an i64 slot list.
    out.push_str("    fn from_slots(s: &[i64]) -> Self {\n        let mut v = Self::with_tag(s[0]);\n");
    for k in 1..=width {
        if enum_slot_kinds(i, k).contains(&NTy::I64) {
            out.push_str(&format!("        if s.len() > {k} {{ v.set_s{k}_i(s[{k}]); }}\n"));
        }
    }
    out.push_str("        v\n    }\n}\n\n");
    out
}

/// An enum's `with_tag` (the constructor at `t` with default payloads) and
/// `tag` methods.
fn enum_tag_fns(i: u16, cases: &[(u32, Vec<NTy>)]) -> String {
    let mut out = String::new();
    // with_tag: the constructor at `t` with default payloads. A region twin's
    // arena handle has no `Default`: its default is a default node in the arena.
    let region = is_region(i);
    out.push_str("    fn with_tag(t: i64) -> Self {\n        match t {\n");
    for (t, fs) in cases {
        if fs.is_empty() {
            out.push_str(&format!("            {t} => Self::C{t},\n"));
        } else {
            let defs = fs
                .iter()
                .map(|f| if region && enum_field_boxed(i, *f) { "almide_rgn_alloc(Default::default())" } else { "Default::default()" })
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("            {t} => Self::C{t}({defs}),\n"));
        }
    }
    out.push_str("            _ => { eprintln!(\"Error: variant tag out of range\"); std::process::exit(1) }\n        }\n    }\n");
    out.push_str("    fn tag(&self) -> i64 {\n        match self {\n");
    for (t, fs) in cases {
        let pat = if fs.is_empty() { String::new() } else { "(..)".into() };
        out.push_str(&format!("            Self::C{t}{pat} => {t},\n"));
    }
    out.push_str("        }\n    }\n");
    out
}

/// A case pattern's binders that name payload slot `k` (1-based) `f`.
fn slot_binds(len: usize, k: usize) -> String {
    let binds: Vec<String> = (0..len).map(|j| if j == k - 1 { "f".into() } else { "_".into() }).collect();
    binds.join(", ")
}

/// The accessors of payload slot `k` at `kind`: the owned read and the
/// write, and a borrowed read for a list or record slot.
fn enum_slot_fns(i: u16, k: usize, kind: NTy, cases: &[(u32, Vec<NTy>)]) -> String {
    let mut out = String::new();
    let suf = kind_suffix(kind);
    let ty = owned_ty(kind);
    let mut get = format!("    fn s{k}_{suf}(&self) -> {ty} {{\n        match self {{\n");
    let mut set = format!("    fn set_s{k}_{suf}(&mut self, x: {ty}) {{\n        match self {{\n");
    for (t, fs) in cases {
        if fs.get(k - 1) != Some(&kind) {
            continue;
        }
        let pat = format!("Self::C{t}({})", slot_binds(fs.len(), k));
        let (rd, wr) = match kind {
            NTy::I64 => ("*f".to_string(), "*f = x".to_string()),
            NTy::Rec(_) if enum_field_boxed(i, kind) => ("(**f).clone()".to_string(), format!("*f = {}(x)", box_new(i))),
            _ => ("f.clone()".to_string(), "*f = x".to_string()),
        };
        get.push_str(&format!("            {pat} => {rd},\n"));
        set.push_str(&format!("            {pat} => {wr},\n"));
    }
    // A slot past the case's payload reads the block's zero padding.
    get.push_str("            _ => Default::default(),\n        }\n    }\n");
    set.push_str("            _ => {}\n        }\n    }\n");
    out.push_str(&get);
    out.push_str(&set);
    // A borrowed read of a list slot: a view of the slot,
    // `None` past the case's payload (#4006, no-perf-loss: the owned
    // read copies the slot's buffer only for the read to look at it).
    if let Some(view) = match kind {
        NTy::Vec => Some("[i64]"),
        NTy::VecStr => Some("[AlmideString]"),
        _ => None,
    } {
        let mut get_ref = format!("    fn s{k}_{suf}_ref(&self) -> Option<&{view}> {{\n        match self {{\n");
        for (t, fs) in cases {
            if fs.get(k - 1) != Some(&kind) {
                continue;
            }
            get_ref.push_str(&format!("            Self::C{t}({}) => Some(&f[..]),\n", slot_binds(fs.len(), k)));
        }
        get_ref.push_str("            _ => None,\n        }\n    }\n");
        out.push_str(&get_ref);
    }
    // A borrowed read of a boxed record slot (crate::render_native_borrow_read):
    // `None` past the case's payload, where the owned read gives the default.
    if let NTy::Rec(_) = kind {
        let mut get_ref = format!("    fn s{k}_{suf}_ref(&self) -> Option<&{ty}> {{\n        match self {{\n");
        for (t, fs) in cases {
            if fs.get(k - 1) != Some(&kind) {
                continue;
            }
            let r = if enum_field_boxed(i, kind) { "&**f" } else { "f" };
            get_ref.push_str(&format!("            Self::C{t}({}) => Some({r}),\n", slot_binds(fs.len(), k)));
        }
        get_ref.push_str("            _ => None,\n        }\n    }\n");
        out.push_str(&get_ref);
    }
    out
}

pub(crate) fn struct_name(i: u16) -> String {
    format!("AlmdRec{i}")
}

/// The owned Rust spelling of a value kind.
pub(crate) fn owned_ty(t: NTy) -> String {
    match t {
        NTy::I64 => "i64".into(),
        NTy::F64 => "f64".into(),
        NTy::Str | NTy::StrRef => "AlmideString".into(),
        NTy::Vec | NTy::VecRef => "Vec<i64>".into(),
        NTy::VecStr | NTy::VecStrRef => "Vec<AlmideString>".into(),
        NTy::Res => "Result<i64, AlmideString>".into(),
        NTy::Opt => "Option<i64>".into(),
        NTy::ResStr => "Result<AlmideString, AlmideString>".into(),
        NTy::Rec(i) | NTy::RecRef(i) => struct_name(i),
    }
}

/// The `Default` of each runtime type the render carries that has none of its
/// own: `Value` (the JSON null) and `Matrix` (the 0×0 one). A struct or enum
/// starts from `Default`, a moved-out value leaves one behind and a carrier's
/// absent payload reads as one, each overwritten or never observed, as a
/// String's empty default is. Empty when the render carries neither. It goes
/// inside the runtime module, the crate that defines the types.
pub(crate) fn runtime_default_impls() -> String {
    const IMPLS: [(&str, &str); 2] = [
        (VALUE_KEY, "impl Default for AlmideValue {\n    fn default() -> Self {\n        AlmideValue::Null\n    }\n}\n"),
        (MATRIX_KEY, "impl Default for AlmideMatrix {\n    fn default() -> Self {\n        AlmideMatrix { rows: 0, cols: 0, data: Vec::new() }\n    }\n}\n"),
    ];
    RECS.with(|r| {
        let r = r.borrow();
        IMPLS.iter().filter(|(k, _)| r.iter().any(|d| d.name == *k)).map(|(_, i)| *i).collect()
    })
}

include!("render_native_defs.rs");

/// The table key of a tuple type (its spelled form).
pub(crate) fn tuple_key(t: &almide_lang::types::Ty) -> String {
    format!("{t:?}")
}

/// The generated type's table name for a value of type `t`: a declared
/// non-generic aggregate by its name, any other by its spelled type.
pub(crate) fn block_key(t: &almide_lang::types::Ty) -> String {
    match t {
        t if crate::lower::is_value_ty(t) => VALUE_KEY.to_string(),
        almide_lang::types::Ty::Named(n, args) if args.is_empty() => n.as_str().to_string(),
        t => tuple_key(t),
    }
}

/// `t` onto `out` unless it is there already.
fn push_new(out: &mut Vec<almide_lang::types::Ty>, t: &almide_lang::types::Ty) {
    if !out.contains(t) {
        out.push(t.clone());
    }
}

/// Every tuple type and every list type of non-scalar, non-string elements
/// in `t`, outermost first.
pub(crate) fn collect_tuples(t: &almide_lang::types::Ty, out: &mut Vec<almide_lang::types::Ty>) {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    match t {
        Ty::Tuple(es) => {
            push_new(out, t);
            es.iter().for_each(|e| collect_tuples(e, out));
        }
        // An anonymous record: the same uniform slots as a tuple, in its
        // type's field order (the order the lowering stores them in).
        Ty::Record { fields } => {
            push_new(out, t);
            fields.iter().for_each(|(_, e)| collect_tuples(e, out));
        }
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
            if !(is_lane_scalar(&a[0]) || matches!(a[0], Ty::String)) && !out.contains(t) {
                out.push(t.clone());
            }
            a.iter().for_each(|e| collect_tuples(e, out));
        }
        // A map of non-Float keys: v0's runtime `AlmideMap` (a Float key would
        // compare by its i64 bits, not as a float).
        Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2 && !matches!(a[0], Ty::Float) => {
            push_new(out, t);
            a.iter().for_each(|e| collect_tuples(e, out));
        }
        // A set and its recursively typed runtime-compatible elements.
        t if is_native_set(t) => {
            push_new(out, t);
            if let Ty::Applied(_, a) = t { a.iter().for_each(|e| collect_tuples(e, out)); }
        }
        // A heap-payload Option or a Result outside the scalar carriers: its
        // own generated `Option<T>` / `Result<T, E>`.
        Ty::Applied(_, a) if is_heap_carrier(t) => {
            push_new(out, t);
            a.iter().for_each(|e| collect_tuples(e, out));
        }
        Ty::Applied(_, a) => a.iter().for_each(|e| collect_tuples(e, out)),
        Ty::Fn { params, ret, .. } => {
            params.iter().for_each(|p| collect_tuples(p, out));
            collect_tuples(ret, out);
        }
        Ty::Bytes | Ty::Matrix if !out.contains(t) => out.push(t.clone()),
        t if crate::lower::is_value_ty(t) => {
            push_new(out, t);
        }
        // An instantiated generic record or variant: its own generated type.
        Ty::Named(_, args) if !args.is_empty() => {
            push_new(out, t);
            args.iter().for_each(|e| collect_tuples(e, out));
        }
        _ => {}
    }
}

/// `t` with each declared type parameter bound: the layouts spell a parameter
/// as `TypeVar(T)` or as a bare `Named(T, [])`.
fn bind_type_params(
    t: &almide_lang::types::Ty,
    b: &std::collections::HashMap<almide_lang::intern::Sym, almide_lang::types::Ty>,
) -> almide_lang::types::Ty {
    use almide_lang::types::Ty;
    match t {
        Ty::Named(n, a) if a.is_empty() && b.contains_key(n) => b[n].clone(),
        Ty::Named(n, a) => Ty::Named(*n, a.iter().map(|x| bind_type_params(x, b)).collect()),
        Ty::Applied(c, a) => Ty::Applied(c.clone(), a.iter().map(|x| bind_type_params(x, b)).collect()),
        Ty::Tuple(es) => Ty::Tuple(es.iter().map(|x| bind_type_params(x, b)).collect()),
        _ => almide_lang::types::substitute(t, b),
    }
}

include!("render_native_rec_b.rs");

include!("render_native_rec_checkpoint.rs");

include!("render_native_eq.rs");
