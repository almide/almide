// ── Rendering a declared floor call ──

use crate::lower::LowerError;
use crate::render_native::{as_f64_arg, as_str_arg, wall, NTy};

/// A declared list param's argument: a slice borrow of the list the render
/// carries, when its element kind is the declared one.
/// The owned value of kind `want` that a call arg of kind `got` gives.
fn owned_arg(want: NTy, code: &str, got: NTy) -> Option<String> {
    if let Some(code) = crate::native_value_repr::scalar_recode(code, got, want) { return Some(code); }
    Some(match (want, got) {
        (NTy::F32, NTy::F32) => code.to_string(),
        (NTy::I64, NTy::I64) => code.to_string(),
        (NTy::I64, NTy::F64) => format!("{code}.to_bits() as i64"),
        (NTy::Str, NTy::Str) => format!("{code}.clone()"),
        (NTy::Str, NTy::StrRef) => format!("AlmideString::from({code})"),
        (NTy::Vec, NTy::Vec | NTy::VecRef) => format!("{code}.to_vec()"),
        (NTy::VecStr, NTy::VecStr | NTy::VecStrRef) => format!("{code}.to_vec()"),
        (a @ (NTy::Opt | NTy::Res | NTy::ResStr), b) if a == b => format!("{code}.clone()"),
        (NTy::Rec(i), NTy::Rec(j) | NTy::RecRef(j)) if i == j => format!("{code}.clone()"),
        _ => return crate::native_value_repr::tuple_copy(code, got, want),
    })
}

/// The (key, value) kinds of a generated list of pairs.
fn pair_kinds(list: u16) -> Option<(NTy, NTy)> {
    match crate::render_native_rec::list_elem(list)? {
        NTy::Vec => Some((NTy::I64, NTy::I64)),
        NTy::VecStr => Some((NTy::Str, NTy::Str)),
        NTy::Rec(t) => Some((crate::render_native_rec::rec_field(t, 0)?, crate::render_native_rec::rec_field(t, 1)?)),
        _ => None,
    }
}

/// A generated list of `(String, Value)` tuples: the program's spelling of a
/// `List[(String, Value)]` (a struct per tuple, fields `f0` / `f1`).
fn is_str_value_pairs(list: u16) -> bool {
    match crate::render_native_rec::list_elem(list) {
        Some(NTy::Rec(t)) => {
            crate::render_native_rec::field_count(t) == 2
                && crate::render_native_rec::rec_field(t, 0) == Some(NTy::Str)
                && crate::render_native_rec::rec_field(t, 1).is_some_and(crate::render_native_rec::is_value)
        }
        _ => false,
    }
}

/// The runtime's owned `Vec<(AlmideString, AlmideValue)>` of a `(String,
/// Value)` list the render carries: its own tuples moved out when the caller
/// does not read it again (v0's move into the owned twin), else each copied.
fn owned_pairs(code: &str, got: NTy, dying: bool) -> Option<String> {
    match got {
        NTy::Rec(i) if is_str_value_pairs(i) && dying => Some(format!(
            "{}.into_iter().map(|t| (t.f0, t.f1)).collect::<Vec<_>>()",
            crate::render_native_move::take_expr(code)
        )),
        NTy::Rec(i) | NTy::RecRef(i) if is_str_value_pairs(i) => {
            Some(format!("{code}.iter().map(|t| (t.f0.clone(), t.f1.clone())).collect::<Vec<_>>()"))
        }
        _ => None,
    }
}

fn list_arg(elem: ListElem, code: &str, got: NTy) -> Option<String> {
    if elem == ListElem::StrValuePairs {
        return owned_pairs(code, got, false).map(|v| format!("&{v}"));
    }
    let elem_of = |i: u16| crate::render_native_rec::list_elem(i);
    match (elem, got) {
        // A type parameter's list: a generated list too, borrowed whole.
        (ListElem::Any, NTy::Rec(i)) if elem_of(i).is_some() => return Some(format!("&{code}")),
        (ListElem::Any, NTy::RecRef(i)) if elem_of(i).is_some() => return Some(code.to_string()),
        // The render's float list holds each cell's bits.
        (ListElem::Float, NTy::Vec | NTy::VecRef) => {
            return Some(format!("&{code}.iter().map(|x| f64::from_bits(*x as u64)).collect::<Vec<f64>>()"))
        }
        // And a bool list each cell's 0 / 1.
        (ListElem::Bool, NTy::Vec | NTy::VecRef) => return Some(format!("&{code}.iter().map(|x| *x != 0).collect::<Vec<bool>>()")),
        (ListElem::FloatRows, NTy::Rec(i) | NTy::RecRef(i)) if elem_of(i) == Some(NTy::Vec) => {
            return Some(format!(
                "&{code}.iter().map(|r| r.iter().map(|x| f64::from_bits(*x as u64)).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>()"
            ))
        }
        (ListElem::Matrix, NTy::Rec(i)) if elem_of(i).is_some_and(crate::render_native_rec::is_matrix) => return Some(format!("&{code}")),
        (ListElem::Matrix, NTy::RecRef(i)) if elem_of(i).is_some_and(crate::render_native_rec::is_matrix) => return Some(code.to_string()),
        (ListElem::Float | ListElem::Bool | ListElem::FloatRows | ListElem::Matrix, _) => return None,
        _ => {}
    }
    let int_list = matches!(elem, ListElem::Int | ListElem::Any);
    let str_list = matches!(elem, ListElem::Str | ListElem::Any);
    match got {
        NTy::Vec if int_list => Some(format!("&{code}")),
        NTy::VecRef if int_list => Some(code.to_string()),
        NTy::VecStr if str_list => Some(format!("&{code}")),
        NTy::VecStrRef if str_list => Some(code.to_string()),
        NTy::Rec(i) if elem == ListElem::Value && crate::render_native_rec::list_elem(i).is_some_and(crate::render_native_rec::is_value) => {
            Some(format!("&{code}"))
        }
        NTy::RecRef(i) if elem == ListElem::Value && crate::render_native_rec::list_elem(i).is_some_and(crate::render_native_rec::is_value) => {
            Some(code.to_string())
        }
        _ => None,
    }
}

/// Whether the owned twin can take the list arg `(code, got)` of `param` as
/// is: a `Vec<i64>` / `Vec<AlmideString>`, or a generated list the param
/// borrows whole (a `List[Value]`'s `Vec<AlmideValue>`), never one the call
/// converts first.
fn twin_takes(param: Option<&FloorParam>, (code, got): &(String, NTy)) -> bool {
    match (param, got) {
        (_, NTy::Vec | NTy::VecStr) => true,
        (Some(FloorParam::List(elem)), NTy::Rec(_)) => list_arg(*elem, code, *got).is_some_and(|r| r == format!("&{code}")),
        // A dying owned Value (`as_array(v)` at v's last use): its items move.
        (Some(FloorParam::Value(_)), NTy::Rec(_)) => crate::render_native_rec::is_value(*got),
        _ => false,
    }
}

/// The Rust expression calling `f`'s runtime body with `args` (each rendered
/// with the NTy it carries), and the NTy its result binds as (`None`: Unit).
pub(crate) fn render_call(
    f: &FloorFn,
    name: &str,
    args: &[(String, NTy)],
    dying: &[bool],
    dst_map: Option<u16>,
) -> Result<(String, Option<NTy>), LowerError> {
    if args.len() != f.params.len() {
        return Err(wall(format!("native: floor call `{name}` arity mismatch")));
    }
    let mismatch = || wall(format!("native: floor call `{name}` arg type mismatch"));
    // The (key, value) kinds of the call's map, from its `Map[K, V]` arg or
    // its `List[(K, V)]` pairs.
    let map_kv: Option<(NTy, NTy)> = dst_map.and_then(crate::render_native_rec::map_kinds).or_else(|| f.params.iter().zip(args).find_map(|(p, (_, got))| match (p, got) {
        (FloorParam::Map(_), NTy::Rec(i) | NTy::RecRef(i)) => crate::render_native_rec::map_kinds(*i),
        (FloorParam::MapPairs, NTy::Rec(i) | NTy::RecRef(i)) => pair_kinds(*i),
        _ => None,
    }));
    // The element kind of the call's set, from its `Set[A]` arg.
    let set_e: Option<NTy> = f.params.iter().zip(args).find_map(|(p, (_, got))| match (p, got) {
        (FloorParam::Set(_), NTy::Rec(i) | NTy::RecRef(i)) => crate::render_native_rec::set_elem(*i),
        _ => None,
    }).or_else(|| dst_map.and_then(crate::render_native_rec::set_elem));
    // The element kind of the call's `List[A]` arg, for an element-typed
    // param or return: an Int list's `i64`, a String list's `AlmideString`.
    // A generated list's element is its declared kind (a record, a nested
    // list, a runtime `Value`, …).
    let elem_kind: Option<NTy> = f
        .params
        .iter()
        .zip(args)
        .find(|(p, _)| matches!(p, FloorParam::List(ListElem::Any)))
        .and_then(|(_, (_, got))| match got {
            NTy::Vec | NTy::VecRef => Some(NTy::I64),
            NTy::VecStr | NTy::VecStrRef => Some(NTy::Str),
            NTy::Rec(i) | NTy::RecRef(i) => crate::render_native_rec::list_elem(*i),
            _ => None,
        });
    let elem_str = elem_kind.map(|k| k == NTy::Str);
    if f.str_only && elem_str != Some(true) {
        return Err(wall(format!("native: floor call `{name}` bounds its element by equality or ordering, served for a String list only")));
    }
    // A generated struct or enum (a declared record or variant, a tuple) has
    // no `Ord`: an ordering-bound element of that kind is v0's to render.
    if f.ordered && elem_kind.and_then(NTy::rec).is_some_and(|i| crate::render_native_rec::field_count(i) > 0 || crate::render_native_rec::enum_cases(i).is_some()) {
        return Err(wall(format!("native: floor call `{name}` orders a declared record or variant, whose generated type derives no ordering")));
    }
    let cx = CallCx { mismatch: &mismatch, map_kv, elem_str, elem_kind, set_e, name, params: &f.params, args };
    let mut rendered = Vec::new();
    for ((code, got), p) in args.iter().zip(&f.params) {
        let dying_here = dying.get(rendered.len()).copied().unwrap_or(false);
        // The generic mismatch names which param refused which kind, so the
        // wall histogram can tell one shape of mismatch from another.
        let k = rendered.len();
        rendered.push(render_arg(&cx, p, code, got, dying_here).map_err(|e| {
            if e.to_string() == mismatch().to_string() {
                wall(format!("native: floor call `{name}` arg type mismatch (param {k}: {p:?} given {got:?})"))
            } else {
                e
            }
        })?);
    }
    // The list source not read again moves into the owned twin.
    let mut symbol = f.symbol.as_str();
    if let Some((k, twin, _)) = &f.twin {
        // A `(String, Value)` list is converted into the runtime's pairs in
        // any case: the owned twin takes that copy (or the moved-out source)
        // instead of copying it once more.
        if matches!(f.params.get(*k), Some(FloorParam::Set(_) | FloorParam::Map(_) | FloorParam::Bytes)) {
            // The caller's set, map or Bytes, not read again: moved into the owned twin.
            if dying.get(*k) == Some(&true) && matches!(args.get(*k).map(|a| a.1), Some(NTy::Rec(_))) {
                rendered[*k] = crate::render_native_move::take_expr(&args[*k].0);
                symbol = twin;
            }
        } else if f.params.get(*k) == Some(&FloorParam::List(ListElem::StrValuePairs)) {
            let (code, got) = &args[*k];
            rendered[*k] = owned_pairs(code, *got, dying.get(*k) == Some(&true)).ok_or_else(mismatch)?;
            symbol = twin;
        } else if dying.get(*k) == Some(&true) && twin_takes(f.params.get(*k), &args[*k]) {
            rendered[*k] = crate::render_native_move::take_expr(&args[*k].0);
            symbol = twin;
        }
    }
    let call = format!("{symbol}({})", rendered.join(", "));
    render_ret(f.ret, call, &cx, dst_map)
}

/// What a floor call's argument and result renders read off the whole call:
/// its arg-mismatch wall, its map's (key, value) kinds and its list's element
/// kind (`Some(true)`: String).
struct CallCx<'a> {
    mismatch: &'a dyn Fn() -> LowerError,
    map_kv: Option<(NTy, NTy)>,
    elem_str: Option<bool>,
    /// The element kind of the call's `List[A]` arg.
    elem_kind: Option<NTy>,
    /// The element kind of the call's `Set[A]` arg.
    set_e: Option<NTy>,
    name: &'a str,
    params: &'a [FloorParam],
    args: &'a [(String, NTy)],
}

/// One declared param's argument; `dying_here`: the caller's value is not
/// read again.
fn render_arg(cx: &CallCx, p: &FloorParam, code: &str, got: &NTy, dying_here: bool) -> Result<String, LowerError> {
    let mismatch = cx.mismatch;
    Ok(match p {
        FloorParam::Int => (*got == NTy::I64).then(|| code.to_string()).ok_or_else(mismatch)?,
        FloorParam::Bool => (*got == NTy::I64).then(|| format!("({code} != 0)")).ok_or_else(mismatch)?,
        FloorParam::Float => as_f64_arg(code, *got)?,
        FloorParam::Float32 => crate::render_native_float32::lane_arg(code, *got)?,
        FloorParam::Str => matches!(got, NTy::Str | NTy::StrRef)
            .then(|| as_str_arg(code, *got))
            .ok_or_else(mismatch)?,
        FloorParam::List(ListElem::Any) if cx.set_e.is_some() =>
            set_source_list_arg(cx.set_e.unwrap(), code, *got).ok_or_else(mismatch)?,
        FloorParam::List(elem) => list_arg(*elem, code, *got).ok_or_else(mismatch)?,
        FloorParam::Elem(by_ref) => elem_arg(cx, *by_ref, code, got)?,
        FloorParam::Map(by_ref) => map_arg(cx, *by_ref, code, got, dying_here)?,
        FloorParam::MapKey(by_ref) | FloorParam::MapVal(by_ref) => map_entry_arg(cx, p, *by_ref, code, got, dying_here)?,
        FloorParam::MapPairs => map_pairs_arg(cx, code, got)?,
        FloorParam::Value(by_ref) => value_arg(cx, *by_ref, code, got, dying_here)?,
        FloorParam::Set(_) | FloorParam::SetElem(_) | FloorParam::Matrix(_) | FloorParam::BytesMut => runtime_arg(cx, p, code, got, dying_here)?,
        FloorParam::Bytes => match got {
            NTy::Rec(_) if crate::render_native_rec::is_bytes(*got) => format!("&{code}"),
            NTy::RecRef(_) if crate::render_native_rec::is_bytes(*got) => code.to_string(),
            _ => return Err(mismatch()),
        },
    })
}

/// A runtime Set, Set element, Matrix or writable Bytes param's argument.
fn runtime_arg(cx: &CallCx, p: &FloorParam, code: &str, got: &NTy, dying_here: bool) -> Result<String, LowerError> {
    let mismatch = cx.mismatch;
    Ok(match p {
        FloorParam::Set(by_ref) => {
            let ok = matches!(got, NTy::Rec(i) | NTy::RecRef(i) if crate::render_native_rec::set_elem(*i).is_some());
            if !ok {
                return Err(mismatch());
            }
            match (got, by_ref) {
                (NTy::Rec(_), true) => format!("&{code}"),
                (NTy::RecRef(_), true) => code.to_string(),
                _ => format!("{code}.clone()"),
            }
        }
        FloorParam::SetElem(by_ref) => {
            let want = cx.set_e.ok_or_else(mismatch)?;
            let owned = owned_arg(want, code, *got).ok_or_else(mismatch)?;
            if !crate::native_value_repr::same_owned(want, crate::render_native::owned_nty(*got)) {
                return Ok(if *by_ref { format!("&({owned})") } else { owned });
            }
            let own_value = matches!(got, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_));
            match (by_ref, own_value, dying_here) {
                (true, true, _) => format!("&{code}"),
                (true, false, _) if matches!(got, NTy::StrRef | NTy::VecRef | NTy::VecStrRef | NTy::RecRef(_)) => code.to_string(),
                (true, false, _) if *got == NTy::I64 => format!("&{code}"),
                (true, false, _) => format!("&{owned}"),
                (false, true, true) if crate::render_native_rec::has_default(*got) => format!("std::mem::take(&mut {code})"),
                (false, _, _) => owned,
            }
        }
        FloorParam::Matrix(_) | FloorParam::BytesMut => matrix_or_bytes_mut_arg(cx, p, code, got)?,
        _ => return Err(mismatch()),
    })
}

/// A Matrix or writable Bytes param's argument.
fn matrix_or_bytes_mut_arg(cx: &CallCx, p: &FloorParam, code: &str, got: &NTy) -> Result<String, LowerError> {
    let mismatch = cx.mismatch;
    Ok(match p {
        FloorParam::Matrix(by_ref) => match (got, by_ref) {
            (NTy::Rec(_), true) if crate::render_native_rec::is_matrix(*got) => format!("&{code}"),
            (NTy::RecRef(_), true) if crate::render_native_rec::is_matrix(*got) => code.to_string(),
            (NTy::Rec(_) | NTy::RecRef(_), false) if crate::render_native_rec::is_matrix(*got) => format!("{code}.clone()"),
            _ => return Err(mismatch()),
        },
        // Only the caller's own binding: a borrowed one cannot be written, and
        // an owned copy of a record's field would take the write in place of
        // the field.
        FloorParam::BytesMut => match got {
            NTy::Rec(_) if crate::render_native_rec::is_bytes(*got) && !crate::render_native_addr::is_field_copy(code) => {
                format!("&mut {code}")
            }
            _ => return Err(mismatch()),
        },
        _ => return Err(mismatch()),
    })
}

/// An element-typed param's argument.
fn elem_arg(cx: &CallCx, by_ref: bool, code: &str, got: &NTy) -> Result<String, LowerError> {
    let v = match (cx.elem_kind, got) {
        (Some(NTy::I64), NTy::I64) => code.to_string(),
        (Some(NTy::Str), NTy::Str) => format!("{code}.clone()"),
        (Some(NTy::Str), NTy::StrRef) => format!("AlmideString::from({code})"),
        (Some(want @ (NTy::Rec(_) | NTy::Vec | NTy::VecStr)), _) => owned_arg(want, code, *got).ok_or_else(cx.mismatch)?,
        _ => return Err((cx.mismatch)()),
    };
    Ok(if by_ref { format!("&{v}") } else { v })
}

/// A constructor preserves its declared Set member type when source literals
/// have structural record identities. Rebuild fields before runtime insertion.
fn set_source_list_arg(want: NTy, code: &str, got: NTy) -> Option<String> {
    let elem = match got {
        NTy::Vec | NTy::VecRef => NTy::I64,
        NTy::VecStr | NTy::VecStrRef => NTy::Str,
        NTy::Rec(i) | NTy::RecRef(i) => crate::render_native_rec::list_elem(i)?,
        _ => return None,
    };
    if crate::native_value_repr::same_owned(want, elem) { return list_arg(ListElem::Any, code, got); }
    let value = owned_arg(want, "(*__item)", elem)?;
    Some(format!("&({code}).iter().map(|__item| {value}).collect::<Vec<_>>()"))
}

/// A `Map[K, V]` param's argument.
fn map_arg(cx: &CallCx, by_ref: bool, code: &str, got: &NTy, dying_here: bool) -> Result<String, LowerError> {
    let ok = matches!(got, NTy::Rec(i) | NTy::RecRef(i) if crate::render_native_rec::map_kinds(*i).is_some());
    if !ok {
        return Err((cx.mismatch)());
    }
    Ok(match (got, by_ref, dying_here) {
        (NTy::Rec(_), true, _) => format!("&{code}"),
        (NTy::RecRef(_), true, _) => code.to_string(),
        // The caller's map, not read again: v0's move.
        (NTy::Rec(_), false, true) => format!("std::mem::take(&mut {code})"),
        _ => format!("{code}.clone()"),
    })
}

/// A map key or value param's argument.
fn map_entry_arg(cx: &CallCx, p: &FloorParam, by_ref: bool, code: &str, got: &NTy, dying_here: bool) -> Result<String, LowerError> {
    let mismatch = cx.mismatch;
    let (k, v) = cx.map_kv.ok_or_else(mismatch)?;
    let want = if matches!(p, FloorParam::MapKey(_)) { k } else { v };
    let owned = owned_arg(want, code, *got).ok_or_else(mismatch)?;
    if !crate::native_value_repr::same_owned(want, crate::render_native::owned_nty(*got)) {
        return Ok(if by_ref { format!("&({owned})") } else { owned });
    }
    let own_value = matches!(got, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_));
    Ok(match (by_ref, own_value, dying_here) {
        // Borrow the caller's exact owned value or existing borrowed view.
        (true, true, _) => format!("&{code}"),
        (true, false, _) if matches!(got, NTy::StrRef | NTy::VecRef | NTy::VecStrRef | NTy::RecRef(_)) => code.to_string(),
        (true, false, _) if *got == NTy::I64 => format!("&{code}"),
        (true, false, _) => format!("&{owned}"),
        // The caller's value, not read again: moved in.
        (false, true, true) if crate::render_native_rec::has_default(*got) => format!("std::mem::take(&mut {code})"),
        (false, _, _) => owned,
    })
}

/// A `List[(K, V)]` param's argument.
fn map_pairs_arg(cx: &CallCx, code: &str, got: &NTy) -> Result<String, LowerError> {
    let mismatch = cx.mismatch;
    let (NTy::Rec(i) | NTy::RecRef(i)) = got else { return Err(mismatch()) };
    let pair = crate::render_native_rec::list_elem(*i).ok_or_else(mismatch)?;
    let (a, b) = match pair {
        NTy::Vec | NTy::VecRef => ("t[0]".to_string(), "t[1]".to_string()),
        // A pair of Strings the slot typing read as a String list.
        NTy::VecStr | NTy::VecStrRef => ("t[0].clone()".to_string(), "t[1].clone()".to_string()),
        NTy::Rec(j) if crate::render_native_rec::field_count(j) == 2 => ("t.f0.clone()".to_string(), "t.f1.clone()".to_string()),
        _ => return Err(mismatch()),
    };
    let (got_k, got_v) = pair_kinds(*i).ok_or_else(mismatch)?;
    let (want_k, want_v) = cx.map_kv.ok_or_else(mismatch)?;
    let a = if crate::native_value_repr::same_owned(want_k, got_k) { a } else { owned_arg(want_k, &a, got_k).ok_or_else(mismatch)? };
    let b = if crate::native_value_repr::same_owned(want_v, got_v) { b } else { owned_arg(want_v, &b, got_v).ok_or_else(mismatch)? };
    Ok(format!("{code}.iter().map(|t| ({a}, {b})).collect::<Vec<_>>()"))
}

/// A `Value` param's argument.
fn value_arg(cx: &CallCx, by_ref: bool, code: &str, got: &NTy, dying_here: bool) -> Result<String, LowerError> {
    Ok(match (got, by_ref) {
        (NTy::Rec(_), true) if crate::render_native_rec::is_value(*got) => format!("&{code}"),
        // The caller's Value, not read again: v0's move (#4006, no-perf-loss).
        (NTy::Rec(_), false) if dying_here && crate::render_native_rec::is_value(*got) => {
            crate::render_native_move::take_expr(code)
        }
        (NTy::RecRef(_), true) if crate::render_native_rec::is_value(*got) => code.to_string(),
        (NTy::Rec(_) | NTy::RecRef(_), false) if crate::render_native_rec::is_value(*got) => format!("{code}.clone()"),
        _ => return Err((cx.mismatch)()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_entry_carriers_copy_only_matching_payload_representations() {
        for kind in [NTy::Opt, NTy::Res, NTy::ResStr] {
            assert!(owned_arg(kind, "entry", kind).is_some());
            assert!(owned_arg(kind, "entry", NTy::I64).is_none());
        }
        assert!(owned_arg(NTy::Res, "entry", NTy::ResStr).is_none());
        assert!(owned_arg(NTy::ResStr, "entry", NTy::Res).is_none());
    }

    #[test]
    fn lexer_skips_literals_and_tracks_depth() {
        let t = lex("fn a() { let s = \"}\"; let c = '{'; x(s) }\nfn b<'a>(x: &'a str) {}");
        let fns: Vec<_> = t.iter().filter(|t| t.text == "fn").map(|t| t.depth).collect();
        assert_eq!(fns, vec![0, 0]);
    }

    /// The memo hands back exactly what the lexer produces, on the first call
    /// and on a hit, and a different text of the same length is not a hit.
    #[test]
    fn lex_memo_matches_the_lexer() {
        let view = |t: &[Tok]| t.iter().map(|t| (t.text.to_string(), t.off, t.depth)).collect::<Vec<_>>();
        let a = "fn a() { let s = \"}\"; x(s) }\n".repeat(40);
        let b = a.replace("x(s)", "y(s)");
        assert_eq!(a.len(), b.len());
        assert_eq!(view(&lex(&a)), view(&lex_uncached(&a)));
        assert_eq!(view(&lex(&a)), view(&lex_uncached(&a)), "a memo hit");
        assert_eq!(view(&lex(&b)), view(&lex_uncached(&b)), "same length, other text");
    }

    #[test]
    fn rust_signature_reads_params_and_ret() {
        let (p, r) = rust_signature("pub fn almide_rt_x<T: Clone>(xs: &[T], n: i64) -> Result<i64, AlmideString> { x }").unwrap();
        assert_eq!(p, vec!["&[T]", "i64"]);
        assert_eq!(r, "Result<i64, AlmideString>");
        let (p, r) = rust_signature("pub fn apply<A, F: Fn(A) -> Result<A, String> + Send>(xs: Vec<A>, f: F) -> Vec<A> { x }").unwrap();
        assert_eq!(p, ["Vec<A>", "F"]);
        assert_eq!(r, "Vec<A>");
        let (p, r) = rust_signature("fn apply(f: std::rc::Rc<dyn Fn(i64) -> i64>, n: i64) -> i64 { x }").unwrap();
        assert_eq!(p, ["std::rc::Rc<dyn Fn(i64) -> i64>", "i64"]);
        assert_eq!(r, "i64");
    }

    #[test]
    fn runtime_scanner_preserves_unsafe_impl_qualifier() {
        let source = "unsafe impl Sync for Gate {}";
        let items = top_level_items(source);
        assert_eq!(items.len(), 1);
        assert!(items[0].0 == ItemKind::Impl);
        assert_eq!(items[0].1, "Gate");
        assert_eq!(items[0].2, source);
        assert_eq!(impl_parts(&lex(items[0].2)), Some((Some("Sync"), "Gate")));
    }

    /// The interpolation fold reads `@scalar_text` off the declaration: the
    /// two declared fns, and nothing that merely shares a name prefix.
    #[test]
    fn scalar_text_is_read_from_the_declaration() {
        assert_eq!(declared_scalar_text("int.to_string"), Some(ScalarText::Int));
        assert_eq!(declared_scalar_text("bool.to_string"), Some(ScalarText::Bool));
        assert_eq!(declared_scalar_text("int.to_string_radix"), None);
        assert_eq!(declared_scalar_text("float.to_string"), None);
        assert_eq!(declared_scalar_text("float.__to_string_compound"), Some(ScalarText::Float));
        assert_eq!(declared_scalar_text("float.to_string_compound"), Some(ScalarText::Float));
    }

    /// A range's `..` is no access: the helper a runtime body slices by is
    /// taken along (`string.drop_end`'s `s[..almide_string_tail_start(s, n)]`).
    #[test]
    fn a_name_after_a_range_is_read_by_its_name() {
        let f = declared("string.drop_end").expect("drop_end is on the floor");
        assert!(f.items.iter().any(|t| t.contains("fn almide_string_tail_start")), "{:?}", f.items);
    }
}
