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
#[derive(Clone, Debug)]
pub(crate) struct RecDecl {
    pub name: String,
    pub fields: Vec<NTy>,
    pub cases: Option<Vec<(u32, Vec<NTy>)>>,
    /// A list type of generated or nested-list elements: `Vec<elem>`.
    pub list_of: Option<NTy>,
}

thread_local! {
    static RECS: RefCell<Vec<RecDecl>> = const { RefCell::new(Vec::new()) };
}

/// Clears the installed table when the render ends.
pub(crate) struct RecGuard;
impl Drop for RecGuard {
    fn drop(&mut self) {
        RECS.with(|r| r.borrow_mut().clear());
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
    RECS.with(|r| r.borrow().get(i as usize).filter(|d| d.cases.is_none() && d.list_of.is_none()).and_then(|d| d.fields.get(k).copied()))
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

fn enum_def(i: u16, d: &RecDecl, cases: &[(u32, Vec<NTy>)]) -> String {
    let n = struct_name(i);
    let fty = |f: &NTy| match f {
        NTy::Rec(_) => format!("Box<{}>", owned_ty(*f)),
        _ => owned_ty(*f),
    };
    let mut out = format!("/// variant `{}`\n#[derive(Clone)]\nenum {n} {{\n", d.name);
    for (t, fs) in cases {
        if fs.is_empty() {
            out.push_str(&format!("    C{t},\n"));
        } else {
            out.push_str(&format!("    C{t}({}),\n", fs.iter().map(fty).collect::<Vec<_>>().join(", ")));
        }
    }
    out.push_str("}\n");
    // Default is a case with no generated-type field, so a recursive enum's
    // default terminates (derive_records keeps only enums that have one).
    let leaf = cases.iter().find(|(_, fs)| !fs.iter().any(|f| matches!(f, NTy::Rec(_)))).map_or(cases[0].0, |c| c.0);
    out.push_str(&format!("impl Default for {n} {{\n    fn default() -> Self {{ Self::with_tag({leaf}) }}\n}}\n"));
    out.push_str(&format!("#[allow(unused_variables, unreachable_patterns)]\nimpl {n} {{\n"));
    // with_tag: the constructor at `t` with default payloads.
    out.push_str("    fn with_tag(t: i64) -> Self {\n        match t {\n");
    for (t, fs) in cases {
        if fs.is_empty() {
            out.push_str(&format!("            {t} => Self::C{t},\n"));
        } else {
            let defs = vec!["Default::default()"; fs.len()].join(", ");
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
    let width = cases.iter().map(|(_, fs)| fs.len()).max().unwrap_or(0);
    for k in 1..=width {
        for kind in enum_slot_kinds(i, k) {
            let suf = kind_suffix(kind);
            let ty = owned_ty(kind);
            let mut get = format!("    fn s{k}_{suf}(&self) -> {ty} {{\n        match self {{\n");
            let mut set = format!("    fn set_s{k}_{suf}(&mut self, x: {ty}) {{\n        match self {{\n");
            for (t, fs) in cases {
                if fs.get(k - 1) != Some(&kind) {
                    continue;
                }
                let binds: Vec<String> = (0..fs.len()).map(|j| if j == k - 1 { "f".into() } else { "_".into() }).collect();
                let pat = format!("Self::C{t}({})", binds.join(", "));
                let (rd, wr) = match kind {
                    NTy::I64 => ("*f".to_string(), "*f = x".to_string()),
                    NTy::Rec(_) => ("(**f).clone()".to_string(), "*f = Box::new(x)".to_string()),
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
        NTy::Rec(i) | NTy::RecRef(i) => struct_name(i),
    }
}

/// The struct definitions, or "" when the program declares none (so every
/// render without a heap record is byte-identical to before).
pub(crate) fn struct_defs() -> String {
    RECS.with(|r| {
        let mut out = String::new();
        for (i, d) in r.borrow().iter().enumerate() {
            if let Some(e) = d.list_of {
                out.push_str(&format!("/// list `{}`\ntype {} = Vec<{}>;\n\n", d.name, struct_name(i as u16), owned_ty(e)));
                continue;
            }
            if let Some(cases) = &d.cases {
                out.push_str(&enum_def(i as u16, d, cases));
                continue;
            }
            out.push_str(&format!("/// record `{}`\n#[derive(Clone, Default)]\nstruct {} {{\n", d.name, struct_name(i as u16)));
            for (k, f) in d.fields.iter().enumerate() {
                out.push_str(&format!("    f{k}: {},\n", owned_ty(*f)));
            }
            out.push_str("}\n\n");
        }
        out
    })
}

/// The table key of a tuple type (its spelled form).
pub(crate) fn tuple_key(t: &almide_lang::types::Ty) -> String {
    format!("{t:?}")
}

/// Every tuple type and every list type of non-scalar, non-string elements
/// in `t`, outermost first.
pub(crate) fn collect_tuples(t: &almide_lang::types::Ty, out: &mut Vec<almide_lang::types::Ty>) {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    match t {
        Ty::Tuple(es) => {
            if !out.contains(t) {
                out.push(t.clone());
            }
            es.iter().for_each(|e| collect_tuples(e, out));
        }
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
            if !matches!(a[0], Ty::Int | Ty::Bool | Ty::String) && !out.contains(t) {
                out.push(t.clone());
            }
            a.iter().for_each(|e| collect_tuples(e, out));
        }
        Ty::Applied(_, a) => a.iter().for_each(|e| collect_tuples(e, out)),
        _ => {}
    }
}

/// Derive the generated types from the declared record and variant layouts.
pub(crate) fn derive_records(
    layouts: &crate::lower::RecordLayouts,
    variants: &crate::lower::VariantLayouts,
    tuples: &[almide_lang::types::Ty],
) -> Vec<RecDecl> {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    use std::collections::{BTreeMap, BTreeSet};
    let heap = |t: &Ty| crate::lower::is_heap_ty(t);
    // Every declared aggregate: name -> (is_enum, cases (tag, field types)).
    let mut decls: BTreeMap<String, (bool, Vec<(u32, Vec<Ty>)>)> = BTreeMap::new();
    for (n, (gs, fs)) in layouts {
        if gs.is_empty() {
            decls.insert(n.clone(), (false, vec![(0, fs.iter().map(|(_, t)| t.clone()).collect())]));
        }
    }
    for (n, l) in &variants.by_type {
        if l.generics.is_empty() && !decls.contains_key(n) {
            let cases = l.cases.iter().map(|c| (c.tag, c.fields.iter().map(|(_, t)| t.clone()).collect())).collect();
            decls.insert(n.clone(), (true, cases));
        }
    }
    // A tuple type the program declares in a signature is an anonymous record:
    // the same uniform slots, keyed by its spelled type.
    let mut lists: BTreeSet<String> = BTreeSet::new();
    for t in tuples {
        if let Ty::Tuple(es) = t {
            decls.entry(tuple_key(t)).or_insert((false, vec![(0, es.iter().cloned().collect())]));
        }
        if let Ty::Applied(TypeConstructorId::List, a) = t {
            decls.entry(tuple_key(t)).or_insert((false, vec![(0, a.iter().cloned().collect())]));
            lists.insert(tuple_key(t));
        }
    }
    // An all-scalar aggregate keeps the `Vec<i64>` slot convention.
    let scalar = |n: &str| decls.get(n).is_some_and(|(_, cs)| cs.iter().all(|(_, fs)| fs.iter().all(|t| !heap(t))));
    let mut set: BTreeSet<String> = decls.keys().filter(|n| !scalar(n)).cloned().collect();
    // Some(Ok(kind)) | Some(Err(name of a generated type)) | None (no spelling).
    let kind = |t: &Ty, set: &BTreeSet<String>| -> Option<Result<NTy, String>> {
        Some(Ok(match t {
            Ty::Int | Ty::Bool | Ty::Float => NTy::I64,
            Ty::String => NTy::Str,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Bool) => NTy::Vec,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String) => NTy::VecStr,
            Ty::Named(n, args) if args.is_empty() && scalar(n.as_str()) => NTy::Vec,
            Ty::Named(n, args) if args.is_empty() && set.contains(n.as_str()) => return Some(Err(n.as_str().to_string())),
            Ty::Tuple(_) | Ty::Applied(TypeConstructorId::List, _) if set.contains(&tuple_key(t)) => return Some(Err(tuple_key(t))),
            _ => return None,
        }))
    };
    let fields_of = |n: &str| decls[n].1.iter().flat_map(|(_, fs)| fs.iter()).cloned().collect::<Vec<Ty>>();
    loop {
        let before = set.len();
        set = set.iter().filter(|n| fields_of(n).iter().all(|t| kind(t, &set).is_some())).cloned().collect();
        if set.len() == before {
            break;
        }
    }
    // By-value edges: a struct holds its record fields inline; an enum boxes all.
    let edges: BTreeMap<String, Vec<String>> = set
        .iter()
        .map(|n| {
            let e = if decls[n].0 || lists.contains(n) { Vec::new() } else { fields_of(n).iter().filter_map(|t| kind(t, &set)?.err()).collect() };
            (n.clone(), e)
        })
        .collect();
    let reaches_self = |start: &String| {
        let mut stack = edges[start].clone();
        let mut seen = BTreeSet::new();
        while let Some(n) = stack.pop() {
            if &n == start {
                return true;
            }
            if seen.insert(n.clone()) {
                stack.extend(edges.get(&n).cloned().unwrap_or_default());
            }
        }
        false
    };
    // An enum needs a case with no aggregate field to be its finite Default.
    let no_leaf = |n: &String| {
        decls[n].0 && !decls[n].1.iter().any(|(_, fs)| fs.iter().all(|t| matches!(kind(t, &set), Some(Ok(_)))))
    };
    let cyclic: BTreeSet<String> = set.iter().filter(|n| reaches_self(n) || no_leaf(n)).cloned().collect();
    set.retain(|n| !cyclic.contains(n));
    loop {
        let before = set.len();
        set = set.iter().filter(|n| fields_of(n).iter().all(|t| kind(t, &set).is_some())).cloned().collect();
        if set.len() == before {
            break;
        }
    }
    let names: Vec<String> = set.iter().cloned().collect();
    let spell = |t: &Ty| match kind(t, &set) {
        Some(Ok(k)) => k,
        Some(Err(m)) => NTy::Rec(names.iter().position(|x| *x == m).expect("kept") as u16),
        None => unreachable!("pruned above"),
    };
    names
        .iter()
        .map(|n| {
            let (is_enum, cases) = &decls[n];
            if lists.contains(n) {
                return RecDecl { name: n.clone(), fields: Vec::new(), cases: None, list_of: Some(spell(&cases[0].1[0])) };
            }
            if *is_enum {
                RecDecl {
                    name: n.clone(),
                    fields: Vec::new(),
                    cases: Some(cases.iter().map(|(t, fs)| (*t, fs.iter().map(spell).collect())).collect()),
                    list_of: None,
                }
            } else {
                RecDecl { name: n.clone(), fields: cases[0].1.iter().map(spell).collect(), cases: None, list_of: None }
            }
        })
        .collect()
}
