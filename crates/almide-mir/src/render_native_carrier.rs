//! #4006: a heap-payload `Option` or a `Result` outside the scalar carriers
//! renders as a typed Rust `Option<T>` / `Result<T, E>` (the generated
//! carrier types of `render_native_rec`, `T`/`E` spelled by `owned_ty`).
//!
//! The MIR is NOT rewritten: the wasm leg's ops reach the verifier and the
//! certificate unchanged (the carrier block's `Alloc`, its `Handle`s, the
//! payload's `Consume`, the carrier's release), so the ownership accounting
//! is the one the wasm leg proves (#4349). The render interprets the block's
//! window through the slot-address algebra, the way it reads a record:
//!
//!   Option: `Alloc{OptNone}` = `None`; a one-slot block whose slot 0 is
//!           stored = `Some(payload)`; the length word = `is_some`; slot 0
//!           read = the payload.
//!   Result: a one-slot block whose slot 0 is stored, then its tag word
//!           (`RESULT_TAG_OFFSET`, 0 = ok) = `Ok(payload)` / `Err(payload)`;
//!           the tag read = `is_err`; slot 0 read = the payload of the arm the
//!           enclosing tag test selects.
//!
//! Which values are carriers comes from DECLARATIONS, never from the block's
//! shape (a one-element `List[String]` has the same window as `Some(s)`): the
//! function's own declared params and return, each callee's declared params
//! and return, joined through if-values and copies ([`carrier_values`]).
//! Anything else touching a carrier walls.

use super::{field_accepts, owned_from, rec_slot, AddrState};
use crate::lower::layout::{BLOCK_HEADER, BLOCK_LEN_OFFSET};
use crate::lower::LowerError;
use crate::render_native::{var, wall, NTy};
use crate::render_native_rec::{has_default, option_payload, owned_ty, result_payloads, struct_name};
use crate::{CallArg, Init, MirFunction, Op, PrimKind, ValueId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The Result block's tag word: right after the payload handle in slot 0.
const RESULT_TAG_OFFSET: i64 = BLOCK_HEADER as i64 + 4;

/// A function's carrier signature: each param's and the return's generated
/// carrier index, if the declared type is one.
pub(crate) type CarrierSig = (Vec<Option<u16>>, Option<u16>);

/// Generated type `i` is an Option or Result carrier.
pub(crate) fn is_carrier(i: u16) -> bool {
    option_payload(i).is_some() || result_payloads(i).is_some()
}

/// The declared kind of a carrier's slot 0, when it is one kind: an Option's
/// payload; a Result's when both sides agree or only one is a generated type.
pub(crate) fn slot_kind(i: u16, k: usize) -> Option<NTy> {
    if k != 0 {
        return None;
    }
    if let Some(p) = option_payload(i) {
        return Some(p);
    }
    let (o, e) = result_payloads(i)?;
    match (o, e) {
        _ if o == e => Some(o),
        (NTy::Rec(_), NTy::Rec(_)) => None,
        (NTy::Rec(_), _) => Some(o),
        (_, NTy::Rec(_)) => Some(e),
        _ => None,
    }
}

/// The carrier values of `f`, from its own declared signature and its
/// callees', closed over if-value joins and copies.
pub(crate) fn carrier_values(
    f: &MirFunction,
    own: &CarrierSig,
    callee: &dyn Fn(&str) -> Option<CarrierSig>,
) -> BTreeMap<ValueId, u16> {
    fn set(ty: &mut BTreeMap<ValueId, u16>, v: ValueId, i: Option<u16>) {
        if let Some(i) = i {
            ty.entry(v).or_insert(i);
        }
    }
    let mut ty = BTreeMap::new();
    for (p, k) in f.params.iter().zip(&own.0) {
        set(&mut ty, p.value, *k);
    }
    if let Some(r) = f.ret {
        set(&mut ty, r, own.1);
    }
    let mut edges: Vec<(ValueId, ValueId)> = Vec::new();
    let mut ifs: Vec<Option<ValueId>> = Vec::new();
    for op in &f.ops {
        match op {
            Op::Return { val: Some(v) } => set(&mut ty, *v, own.1),
            Op::CallFn { dst, name, args, .. } => {
                if let Some((ps, r)) = callee(name) {
                    if let Some(d) = dst {
                        set(&mut ty, *d, r);
                    }
                    for (a, k) in args.iter().zip(&ps) {
                        if let CallArg::Handle(v) = a {
                            set(&mut ty, *v, *k);
                        }
                    }
                }
            }
            Op::Dup { dst, src } => edges.push((*dst, *src)),
            Op::IfThen { dst, .. } => ifs.push(*dst),
            Op::Else { val } | Op::EndIf { val } => {
                if let (Some(v), Some(Some(d))) = (val, ifs.last()) {
                    edges.push((*d, *v));
                }
                if matches!(op, Op::EndIf { .. }) {
                    ifs.pop();
                }
            }
            _ => {}
        }
    }
    loop {
        let before = ty.len();
        for (a, b) in &edges {
            if let Some(i) = ty.get(a).copied() {
                ty.entry(*b).or_insert(i);
            }
            if let Some(i) = ty.get(b).copied() {
                ty.entry(*a).or_insert(i);
            }
        }
        if ty.len() == before {
            break;
        }
    }
    ty
}

/// The carrier index of a value's kind.
fn carrier_of(t: Option<NTy>) -> Option<u16> {
    match t {
        Some(NTy::Rec(i) | NTy::RecRef(i)) if is_carrier(i) => Some(i),
        _ => None,
    }
}

fn emit(out: &mut String, indent: usize, s: &str) {
    for _ in 0..indent {
        out.push_str("    ");
    }
    writeln!(out, "{s}").unwrap();
}

/// The owned copy of `v` as a slot of kind `k` (a carrier payload write).
fn payload_rhs(st: &AddrState, tys: &BTreeMap<ValueId, NTy>, v: ValueId, k: NTy) -> Result<String, LowerError> {
    let Some(sv) = st.addrs.get(&v).copied().filter(|a| a.off == 0 && a.idx.is_none()) else {
        return Err(wall(format!("native: a {:?} carrier payload that is not a heap handle", tys.get(&v))));
    };
    let g = tys.get(&sv.base).copied();
    match g {
        Some(g) if field_accepts(k, g) => owned_from(&var(sv.base), g).ok_or_else(|| wall("native: an unowned carrier payload")),
        g => Err(wall(format!("native: a {g:?} value as a {k:?} carrier payload"))),
    }
}

/// Render `op` if it touches a carrier. `None` = not a carrier op.
pub(super) fn render_carrier_op(
    op: &Op,
    st: &mut AddrState,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Option<Result<bool, LowerError>> {
    let base_of = |st: &AddrState, tys: &BTreeMap<ValueId, NTy>, a: &ValueId| {
        st.addrs.get(a).copied().and_then(|ad| carrier_of(tys.get(&ad.base).copied()).map(|i| (ad, i)))
    };
    match op {
        Op::IfThen { cond, .. } => {
            let res = st.res_tags.get(cond).copied();
            st.arms.push((res, true));
            None
        }
        Op::Else { .. } => {
            if let Some(a) = st.arms.last_mut() {
                a.1 = false;
            }
            None
        }
        Op::EndIf { .. } => {
            st.arms.pop();
            None
        }
        Op::Alloc { dst, init: Init::DynList { .. } | Init::DynListStr { .. } | Init::OptNone, .. }
            if st.alloc_rec.get(dst).is_some_and(|i| is_carrier(*i)) =>
        {
            let i = st.alloc_rec[dst];
            tys.insert(*dst, NTy::Rec(i));
            let n = struct_name(i);
            Some(match (option_payload(i).is_some(), op) {
                (true, _) => {
                    emit(out, indent, &format!("let mut {}: {n} = None;", var(*dst)));
                    Ok(true)
                }
                (false, Op::Alloc { init: Init::OptNone, .. }) => Err(wall("native: an OptNone block typed as a Result")),
                (false, _) => {
                    // Its slot 0 and tag word are stored next; the tag picks the side.
                    emit(out, indent, &format!("let mut {}: {n};", var(*dst)));
                    Ok(true)
                }
            })
        }
        Op::Prim { kind: PrimKind::Load { width }, dst: Some(d), args } if args.len() == 1 => {
            let (ad, i) = base_of(st, tys, &args[0])?;
            let b = var(ad.base);
            Some(if *width == 4 && ad.idx.is_none() && option_payload(i).is_some() && ad.off == i64::from(BLOCK_LEN_OFFSET) {
                tys.insert(*d, NTy::I64);
                emit(out, indent, &format!("let mut {}: i64 = {b}.is_some() as i64;", var(*d)));
                Ok(true)
            } else if *width == 4 && ad.idx.is_none() && result_payloads(i).is_some() && ad.off == RESULT_TAG_OFFSET {
                tys.insert(*d, NTy::I64);
                st.res_tags.insert(*d, ad.base);
                emit(out, indent, &format!("let mut {}: i64 = {b}.is_err() as i64;", var(*d)));
                Ok(true)
            } else {
                Err(wall(format!("native: Load{{{width}}} at {ad:?} of a {} carrier", owned_ty(NTy::Rec(i)))))
            })
        }
        Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } if args.len() == 1 => {
            let (ad, i) = base_of(st, tys, &args[0])?;
            if rec_slot(&ad) != Some(0) {
                return Some(Err(wall(format!("native: LoadHandle at {ad:?} of a carrier"))));
            }
            let b = var(ad.base);
            let read = if let Some(p) = option_payload(i) {
                if !has_default(p) {
                    return Some(Err(wall("native: an Option payload read with no Default for its absent case")));
                }
                (p, format!("match &{b} {{ Some(v) => v.clone(), None => Default::default() }}"))
            } else {
                let (o, e) = result_payloads(i)?;
                // The arm of the innermost tag test of this Result, if any.
                let arm = st.arms.iter().rev().find(|(r, _)| *r == Some(ad.base)).map(|(_, then)| *then);
                match arm {
                    Some(true) => (e, format!("match &{b} {{ Err(v) => v.clone(), Ok(_) => unreachable!() }}")),
                    Some(false) => (o, format!("match &{b} {{ Ok(v) => v.clone(), Err(_) => unreachable!() }}")),
                    None if o == e => (o, format!("match &{b} {{ Ok(v) | Err(v) => v.clone() }}")),
                    None => return Some(Err(wall("native: a Result payload read outside its tag test"))),
                }
            };
            if read.0 == NTy::I64 {
                return Some(Err(wall("native: a scalar carrier payload read as a handle")));
            }
            tys.insert(*d, read.0);
            emit(out, indent, &format!("let mut {}: {} = {};", var(*d), owned_ty(read.0), read.1));
            Some(Ok(true))
        }
        Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args } if args.len() == 2 => {
            let (ad, i) = base_of(st, tys, &args[0])?;
            if rec_slot(&ad) != Some(0) {
                return Some(Err(wall(format!("native: Store at {ad:?} of a carrier"))));
            }
            if let Some(p) = option_payload(i) {
                return Some(payload_rhs(st, tys, args[1], p).map(|rhs| {
                    emit(out, indent, &format!("{} = Some({rhs});", var(ad.base)));
                    true
                }));
            }
            // The tag word that follows picks the side.
            st.pending.insert(ad.base, args[1]);
            emit(out, indent, &format!("// {}: payload {}, side by the tag next", var(ad.base), var(args[1])));
            Some(Ok(true))
        }
        Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args } if args.len() == 2 => {
            let (ad, i) = base_of(st, tys, &args[0])?;
            let (o, e) = result_payloads(i)?;
            Some((|| {
                if ad.off != RESULT_TAG_OFFSET || ad.idx.is_some() {
                    return Err(wall(format!("native: Store4 at {ad:?} of a Result carrier")));
                }
                let tag = st.consts.get(&args[1]).copied().ok_or_else(|| wall("native: a computed Result tag"))?;
                let v = st.pending.remove(&ad.base).ok_or_else(|| wall("native: a Result tag with no payload"))?;
                let (side, k) = if tag == 0 { ("Ok", o) } else { ("Err", e) };
                let rhs = payload_rhs(st, tys, v, k)?;
                emit(out, indent, &format!("{} = {side}({rhs});", var(ad.base)));
                Ok(true)
            })())
        }
        _ => None,
    }
}
