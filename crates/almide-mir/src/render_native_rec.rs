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

/// One generated struct: the declared record's name and its field kinds
/// (owned spellings: `I64`, `Str`, `Vec`, `VecStr`, `Rec`).
#[derive(Clone, Debug)]
pub(crate) struct RecDecl {
    pub name: String,
    pub fields: Vec<NTy>,
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
    RECS.with(|r| r.borrow().get(i as usize).and_then(|d| d.fields.get(k).copied()))
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
            out.push_str(&format!("/// record `{}`\n#[derive(Clone, Default)]\nstruct {} {{\n", d.name, struct_name(i as u16)));
            for (k, f) in d.fields.iter().enumerate() {
                out.push_str(&format!("    f{k}: {},\n", owned_ty(*f)));
            }
            out.push_str("}\n\n");
        }
        out
    })
}

/// Derive the struct table from the declared record layouts.
pub(crate) fn derive_records(layouts: &crate::lower::RecordLayouts) -> Vec<RecDecl> {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    use std::collections::{BTreeMap, BTreeSet};
    let all_scalar = |fs: &[(almide_lang::intern::Sym, Ty)]| fs.iter().all(|(_, t)| !crate::lower::is_heap_ty(t));
    // Candidates: non-generic records with at least one heap field.
    let mut set: BTreeSet<String> = layouts
        .iter()
        .filter(|(_, (gs, fs))| gs.is_empty() && !all_scalar(fs))
        .map(|(n, _)| n.clone())
        .collect();
    let kind = |t: &Ty, set: &BTreeSet<String>| -> Option<Result<NTy, String>> {
        Some(Ok(match t {
            Ty::Int | Ty::Bool | Ty::Float => NTy::I64,
            Ty::String => NTy::Str,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Bool) => NTy::Vec,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String) => NTy::VecStr,
            Ty::Named(n, args) if args.is_empty() => {
                let (gs, fs) = layouts.get(n.as_str())?;
                if gs.is_empty() && all_scalar(fs) {
                    NTy::Vec
                } else if set.contains(n.as_str()) {
                    return Some(Err(n.as_str().to_string()));
                } else {
                    return None;
                }
            }
            _ => return None,
        }))
    };
    // Fixpoint: drop a record with a field that has no spelling.
    loop {
        let before = set.len();
        let keep: BTreeSet<String> = set
            .iter()
            .filter(|n| layouts[n.as_str()].1.iter().all(|(_, t)| kind(t, &set).is_some()))
            .cloned()
            .collect();
        set = keep;
        if set.len() == before {
            break;
        }
    }
    // A record that reaches itself through by-value fields has no Rust size.
    let edges: BTreeMap<String, Vec<String>> = set
        .iter()
        .map(|n| {
            let e = layouts[n.as_str()].1.iter().filter_map(|(_, t)| kind(t, &set)?.err()).collect();
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
    let cyclic: BTreeSet<String> = set.iter().filter(|n| reaches_self(n)).cloned().collect();
    loop {
        let before = set.len();
        set = set
            .iter()
            .filter(|n| !cyclic.contains(*n) && edges[*n].iter().all(|m| set.contains(m) && !cyclic.contains(m)))
            .cloned()
            .collect();
        if set.len() == before {
            break;
        }
    }
    let names: Vec<String> = set.iter().cloned().collect();
    names
        .iter()
        .map(|n| RecDecl {
            name: n.clone(),
            fields: layouts[n.as_str()]
                .1
                .iter()
                .map(|(_, t)| match kind(t, &set) {
                    Some(Ok(k)) => k,
                    Some(Err(m)) => NTy::Rec(names.iter().position(|x| *x == m).expect("kept") as u16),
                    None => unreachable!("pruned above"),
                })
                .collect(),
        })
        .collect()
}
