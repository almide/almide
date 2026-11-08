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
        (NTy::I64, NTy::F64) => Some(format!("f64::from_bits({code} as u64)")),
        (NTy::F64, NTy::I64) => Some(format!("{code}.to_bits() as i64")),
        _ => None,
    }
}

/// Runtime tuples choose a structural identity from their native field kinds.
/// Several checked tuples can share those kinds (Float and Int use i64 lanes)
/// while still having distinct Rust structs. Rebuild the requested struct;
/// never declare those structs interchangeable or reinterpret a nominal type.
pub(crate) fn tuple_copy(code: &str, from: NTy, to: NTy) -> Option<String> {
    let (NTy::Rec(i), NTy::Rec(j)) = (owned_nty(from), owned_nty(to)) else { return None };
    let (a, b) = (rec::tuple_fields(i)?, rec::tuple_fields(j)?);
    if a.len() != b.len() || !a.iter().zip(&b).all(|(a, b)| same_owned(*a, *b)) {
        return None;
    }
    let fields = a.iter().enumerate().map(|(k, _)|
        format!("f{k}: __tuple.f{k}.clone()")).collect::<Vec<_>>().join(", ");
    Some(format!("{{ let __tuple = &({code}); {} {{ {fields} }} }}", rec::struct_name(j)))
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
