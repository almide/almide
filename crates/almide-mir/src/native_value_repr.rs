//! Representation compatibility at native value joins and field boundaries.
//! Generated aliases keep their declaration identity; compatibility follows
//! their Rust value representation, never a function's name or a slot's bytes.
use crate::render_native::{owned_nty, NTy};
use crate::render_native_rec as rec;

fn canonical(t: NTy) -> NTy {
    let t = owned_nty(t);
    let NTy::Rec(i) = t else { return t };
    match (
        rec::list_elem(i),
        rec::option_payload(i),
        rec::result_payloads(i),
    ) {
        (Some(NTy::I64), _, _) => NTy::Vec,
        (Some(NTy::Str), _, _) => NTy::VecStr,
        (_, Some(NTy::I64), _) => NTy::Opt,
        (_, _, Some((NTy::I64, NTy::Str))) => NTy::Res,
        (_, _, Some((NTy::Str, NTy::Str))) => NTy::ResStr,
        _ => t,
    }
}

pub(crate) fn same_owned(a: NTy, b: NTy) -> bool {
    fn same(a: NTy, b: NTy, seen: &mut Vec<(u16, u16)>) -> bool {
        let (a, b) = (canonical(a), canonical(b));
        if a == b {
            return true;
        }
        let (NTy::Rec(i), NTy::Rec(j)) = (a, b) else {
            return false;
        };
        if seen.contains(&(i, j)) {
            return false;
        }
        seen.push((i, j));
        if let (Some((ap, ar)), Some((bp, br))) = (rec::closure_sig(i), rec::closure_sig(j)) {
            let result = ap.len() == bp.len() && ap.iter().zip(&bp).all(|(a, b)| same(*a, *b, seen))
                && match (ar, br) { (Some(a), Some(b)) => same(a, b, seen), (None, None) => true, _ => false };
            seen.pop();
            return result;
        }
        let answer = match (rec::list_elem(i), rec::list_elem(j)) {
            (Some(a), Some(b)) => same(a, b, seen),
            _ => match (rec::option_payload(i), rec::option_payload(j)) {
                (Some(a), Some(b)) => same(a, b, seen),
                _ => match (rec::result_payloads(i), rec::result_payloads(j)) {
                    (Some((a, e)), Some((b, f))) => same(a, b, seen) && same(e, f, seen),
                    _ => match (rec::map_kinds(i), rec::map_kinds(j)) {
                        (Some((k, v)), Some((l, w))) => same(k, l, seen) && same(v, w, seen),
                        _ => match (rec::set_elem(i), rec::set_elem(j)) {
                            (Some(a), Some(b)) => same(a, b, seen),
                            _ => false,
                        },
                    },
                },
            },
        };
        seen.pop();
        answer
    }
    same(a, b, &mut Vec::new())
}

/// MIR's i64 Float bits and an actual native f64 denote the same checked
/// scalar at a join. These are bit casts, never numeric conversions.
pub(crate) fn scalar_recode(code: &str, from: NTy, to: NTy) -> Option<String> {
    match (from, to) {
        (NTy::I64, NTy::F32) => Some(format!("f32::from_bits({code} as u32)")),
        (NTy::F32, NTy::I64) => Some(format!("({code}).to_bits() as i64")),
        (NTy::I64, NTy::F64) => Some(format!("f64::from_bits({code} as u64)")),
        (NTy::F64, NTy::I64) => Some(format!("{code}.to_bits() as i64")),
        _ => None,
    }
}

/// Runtime tuples choose a structural identity from their native field kinds.
/// Several checked tuples can share those kinds (Float and Int use i64 lanes)
/// while still having distinct Rust structs. Rebuild the requested struct;
/// never declare those structs interchangeable. Checked structural records
/// also rebuild by field name, including conversion to or from a named record;
/// two distinct nominal records are never implicitly converted.
pub(crate) fn tuple_copy(code: &str, from: NTy, to: NTy) -> Option<String> {
    fn copy(code: &str, from: NTy, to: NTy, seen: &mut Vec<(u16, u16)>) -> Option<String> {
        if same_owned(from, to) { return Some(format!("({code}).clone()")); }
        let (NTy::Rec(i), NTy::Rec(j)) = (owned_nty(from), owned_nty(to)) else { return None };
        if seen.contains(&(i, j)) { return None; }
        seen.push((i, j));
        let result = if let (Some(a), Some(b)) = (rec::tuple_fields(i), rec::tuple_fields(j)) {
            if a.len() != b.len() { return None; }
            let fields = a.iter().zip(&b).enumerate().map(|(k, (a, b))| {
                copy(&format!("__tuple.f{k}"), *a, *b, seen).map(|v| format!("f{k}: {v}"))
            }).collect::<Option<Vec<_>>>()?.join(", ");
            Some(format!("{{ let __tuple = &({code}); {} {{ {fields} }} }}", rec::struct_name(j)))
        } else if let (Some((sa, a)), Some((sb, b))) = (rec::record_fields(i), rec::record_fields(j)) {
            if (!sa && !sb) || a.len() != b.len() { return None; }
            let fields = b.iter().enumerate().map(|(dst, (label, to))| {
                let (src, (_, from)) = a.iter().enumerate().find(|(_, (n, _))| n == label)?;
                copy(&format!("__tuple.f{src}"), *from, *to, seen).map(|v| format!("f{dst}: {v}"))
            }).collect::<Option<Vec<_>>>()?.join(", ");
            Some(format!("{{ let __tuple = &({code}); {} {{ {fields} }} }}", rec::struct_name(j)))
        } else if let (Some(a), Some(b)) = (rec::list_elem(i), rec::list_elem(j)) {
            let value = copy("*__item", a, b, seen)?;
            Some(format!("({code}).iter().map(|__item| {value}).collect::<{}>()", rec::struct_name(j)))
        } else if let (Some(a), Some(b)) = (rec::option_payload(i), rec::option_payload(j)) {
            let value = copy("*__item", a, b, seen)?;
            Some(format!("({code}).as_ref().map(|__item| {value})"))
        } else if let (Some((a, e)), Some((b, f))) = (rec::result_payloads(i), rec::result_payloads(j)) {
            let ok = copy("*__item", a, b, seen)?;
            let err = copy("*__item", e, f, seen)?;
            Some(format!("({code}).as_ref().map(|__item| {ok}).map_err(|__item| {err})"))
        } else { None };
        seen.pop();
        result
    }
    // Identical values use the caller's existing move/borrow rule. This
    // adapter rebuilds checked structural aggregates and their owning containers.
    if same_owned(from, to) { return None; }
    copy(code, from, to, &mut Vec::new())
}

/// A value used ONLY on the unobserved side of a hoisted carrier read.
/// The caller must prove every use is under the opposite tag arm. This
/// does not give Result a Default or permit uninitialized container cells.
pub(crate) fn unused_payload(t: NTy) -> Option<String> {
    fn value(t: NTy, seen: &mut Vec<u16>) -> Option<String> {
        if rec::has_default(t) {
            return Some("Default::default()".into());
        }
        match t {
            NTy::Res | NTy::ResStr => Some("Err(AlmideString::new())".into()),
            NTy::Rec(i) if !seen.contains(&i) => {
                seen.push(i);
                let (ok, err) = rec::result_payloads(i)?;
                let v = value(err, seen)
                    .map(|e| format!("Err({e})"))
                    .or_else(|| value(ok, seen).map(|o| format!("Ok({o})")));
                seen.pop();
                v
            }
            _ => None,
        }
    }
    value(t, &mut Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structural_record_conversion_uses_labels_and_preserves_nominal_identity() {
        let decl = |name: &str, anonymous, fields, labels: &[&str]| rec::RecDecl {
            name: name.into(), fields,
            record_labels: Some((anonymous, labels.iter().map(|s| (*s).into()).collect())),
            ..Default::default()
        };
        let _guard = rec::install(vec![
            decl("named", false, vec![NTy::I64, NTy::Str], &["id", "name"]),
            decl("structural", true, vec![NTy::Str, NTy::I64], &["name", "id"]),
            decl("other-named", false, vec![NTy::I64, NTy::Str], &["id", "name"]),
            decl("other-label", true, vec![NTy::I64, NTy::Str], &["id", "title"]),
            decl("other-type", true, vec![NTy::Str, NTy::Str], &["id", "name"]),
        ]);
        let code = tuple_copy("value", NTy::Rec(0), NTy::Rec(1)).unwrap();
        assert!(code.contains("f0: (__tuple.f1).clone()"));
        assert!(code.contains("f1: (__tuple.f0).clone()"));
        assert!(tuple_copy("value", NTy::Rec(1), NTy::Rec(0)).is_some());
        for target in [2, 3, 4] {
            assert!(tuple_copy("value", NTy::Rec(0), NTy::Rec(target)).is_none());
        }
        assert!(!same_owned(NTy::Rec(0), NTy::Rec(1)));
        // Assignment uses the same checked structural adapter. Distinct
        // nominal records, missing labels/types and borrowed destinations
        // remain errors; a local's representation never changes at the join.
        for (target, source, accepted) in [
            (NTy::Rec(0), NTy::Rec(1), true),
            (NTy::Rec(1), NTy::Rec(0), true),
            (NTy::Rec(0), NTy::Rec(2), false),
            (NTy::Rec(0), NTy::Rec(3), false),
            (NTy::Rec(0), NTy::Rec(4), false),
            (NTy::RecRef(0), NTy::Rec(1), false),
        ] {
            let local = crate::ValueId(0);
            let src = crate::ValueId(1);
            let mut tys = std::collections::BTreeMap::from([(local, target), (src, source)]);
            let mut out = String::new();
            let result = crate::render_native::render_set_local(&local, &src, &mut tys, &mut out, 0);
            assert_eq!(result.is_ok(), accepted, "{target:?} <- {source:?}");
            assert_eq!(tys[&local], target);
            if accepted { assert!(out.contains("f0: (__tuple.f1).clone()")); }
        }
    }
    #[test]
    fn nested_aliases_agree_but_nominal_records_and_different_payloads_do_not() {
        let _guard = rec::install(vec![
            rec::RecDecl {
                name: "list-int".into(),
                list_of: Some(NTy::I64),
                ..Default::default()
            },
            rec::RecDecl {
                name: "option-int".into(),
                option_of: Some(NTy::I64),
                ..Default::default()
            },
            rec::RecDecl {
                name: "list-opt-alias".into(),
                list_of: Some(NTy::Rec(1)),
                ..Default::default()
            },
            rec::RecDecl {
                name: "list-opt".into(),
                list_of: Some(NTy::Opt),
                ..Default::default()
            },
            rec::RecDecl {
                name: "record-a".into(),
                fields: vec![NTy::I64],
                ..Default::default()
            },
            rec::RecDecl {
                name: "record-b".into(),
                fields: vec![NTy::I64],
                ..Default::default()
            },
            rec::RecDecl {
                name: "result".into(),
                result_of: Some((NTy::I64, NTy::Str)),
                ..Default::default()
            },
            rec::RecDecl {
                name: "list-str".into(),
                list_of: Some(NTy::Str),
                ..Default::default()
            },
        ]);
        for (a, b) in [
            (NTy::Rec(0), NTy::Vec),
            (NTy::Rec(1), NTy::Opt),
            (NTy::Rec(2), NTy::Rec(3)),
            (NTy::Rec(6), NTy::Res),
            (NTy::Rec(7), NTy::VecStr),
        ] {
            assert!(same_owned(a, b));
            assert!(same_owned(b, a));
        }
        for (a, b) in [
            (NTy::Rec(4), NTy::Rec(5)),
            (NTy::Rec(0), NTy::Rec(7)),
            (NTy::Rec(0), NTy::Rec(4)),
            (NTy::Rec(2), NTy::Vec),
            (NTy::Opt, NTy::I64),
        ] {
            assert!(!same_owned(a, b));
            assert!(!same_owned(b, a));
        }
    }
}
