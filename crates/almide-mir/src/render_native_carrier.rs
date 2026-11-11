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

use super::{field_accepts, owned_from, rec_slot, Addr, AddrState};
use crate::lower::layout::{BLOCK_HEADER, BLOCK_LEN_OFFSET};
use crate::lower::LowerError;
use crate::render_native::{var, wall, NTy};
use crate::render_native_rec::{has_default, option_payload, owned_ty, result_payloads, struct_name};
use crate::{CallArg, Init, MirFunction, Op, PrimKind, ValueId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The Result block's tag word: right after the payload handle in slot 0.
const RESULT_TAG_OFFSET: i64 = BLOCK_HEADER as i64 + 4;

/// A Result of two scalar payloads: the materializers build its Ok as
/// `Init::ResOkScalar` (length word 0) and its Err as a one-slot block
/// (length word 1), so the length word is its tag and slot 0 its payload.
fn len_tagged(i: u16) -> bool {
    result_payloads(i).is_some_and(|(o, e)| o == NTy::I64 && e == NTy::I64)
}

fn scalar_ok(i: u16) -> bool {
    result_payloads(i).is_some_and(|(o, _)| o == NTy::I64)
}

/// The i64 a scalar payload slot holds for value `v`.
fn scalar_rhs(tys: &BTreeMap<ValueId, NTy>, v: ValueId) -> Result<String, LowerError> {
    match tys.get(&v) {
        Some(NTy::I64) => Ok(var(v)),
        Some(NTy::F64) => Ok(format!("{}.to_bits() as i64", var(v))),
        other => Err(wall(format!("native: a {other:?} value as a scalar carrier payload"))),
    }
}

/// A function's carrier signature: each param's and the return's generated
/// carrier index, if the declared type is one.
pub(crate) type CarrierSig = (Vec<Option<u16>>, Option<u16>);

/// Generated type `i` is an Option or Result carrier.
pub(crate) fn is_carrier(i: u16) -> bool {
    option_payload(i).is_some() || result_payloads(i).is_some()
}

/// The declared kind of a carrier's slot 0, when it is one kind: an Option's
/// payload, or a Result whose two sides agree. Being the only generated
/// record is insufficient: the other side may own a String or scalar list.
pub(crate) fn slot_kind(i: u16, k: usize) -> Option<NTy> {
    if k != 0 {
        return None;
    }
    if let Some(p) = option_payload(i) {
        return Some(p);
    }
    let (o, e) = result_payloads(i)?;
    (o == e).then_some(o)
}

fn set_carrier(ty: &mut BTreeMap<ValueId, u16>, v: ValueId, i: Option<u16>) {
    if let Some(i) = i {
        ty.entry(v).or_insert(i);
    }
}

/// The carrier values of `f`, from its own declared signature and its
/// callees', closed over if-value joins and copies.
pub(crate) fn carrier_values(
    f: &MirFunction,
    own: &CarrierSig,
    callee: &dyn Fn(&str) -> Option<CarrierSig>,
) -> BTreeMap<ValueId, u16> {
    let mut ty = BTreeMap::new();
    for (v, key) in &f.aggregate_blocks {
        set_carrier(&mut ty, *v, crate::render_native_rec::rec_index(key).filter(|i| {
            is_carrier(*i) || crate::render_native_rec::map_kinds(*i).is_some()
                || crate::render_native_rec::set_elem(*i).is_some()
        }));
    }
    for (p, k) in f.params.iter().zip(&own.0) {
        set_carrier(&mut ty, p.value, *k);
    }
    if let Some(r) = f.ret {
        set_carrier(&mut ty, r, own.1);
    }
    let edges = carrier_edges(f, own, callee, &mut ty);
    close_over_edges(&mut ty, &edges);
    ty
}

/// The carrier values the ops of `f` fix (returns, callee signatures), and
/// the copy / if-value join edges a carrier flows along.
fn carrier_edges(
    f: &MirFunction,
    own: &CarrierSig,
    callee: &dyn Fn(&str) -> Option<CarrierSig>,
    ty: &mut BTreeMap<ValueId, u16>,
) -> Vec<(ValueId, ValueId)> {
    let mut edges: Vec<(ValueId, ValueId)> = Vec::new();
    let mut ifs: Vec<Option<ValueId>> = Vec::new();
    for op in &f.ops {
        match op {
            Op::Return { val: Some(v) } => set_carrier(ty, *v, own.1),
            Op::CallFn { dst, name, args, .. } => {
                if let Some((ps, r)) = callee(name) {
                    set_callee_carriers(ty, dst, args, (ps.as_slice(), r));
                }
            }
            Op::Dup { dst, src } | Op::SetLocal { local: dst, src } => edges.push((*dst, *src)),
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
    edges
}

/// A call's result and handle args, typed by the callee's carrier signature.
fn set_callee_carriers(
    ty: &mut BTreeMap<ValueId, u16>,
    dst: &Option<ValueId>,
    args: &[CallArg],
    (ps, r): (&[Option<u16>], Option<u16>),
) {
    if let Some(d) = dst {
        set_carrier(ty, *d, r);
    }
    for (a, k) in args.iter().zip(ps) {
        if let CallArg::Handle(v) = a {
            set_carrier(ty, *v, *k);
        }
    }
}

/// Spread carrier indices along `edges` (both ways) to a fixpoint.
fn close_over_edges(ty: &mut BTreeMap<ValueId, u16>, edges: &[(ValueId, ValueId)]) {
    loop {
        let before = ty.len();
        for (a, b) in edges {
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
}

/// Compare a proven binary tag (possibly inverted) against 0 or 1. The
/// boolean records which original tag side a true predicate selects.
fn tag_comparison(
    op: crate::IntOp, a: ValueId, b: ValueId,
    consts: &BTreeMap<ValueId, i64>, tags: &BTreeMap<ValueId, (ValueId, bool)>,
) -> Option<(ValueId, bool)> {
    let compare = |x, y| Some(match op {
        crate::IntOp::Eq => x == y, crate::IntOp::Ne => x != y,
        crate::IntOp::Lt => x < y, crate::IntOp::Le => x <= y,
        crate::IntOp::Gt => x > y, crate::IntOp::Ge => x >= y,
        _ => return None,
    });
    [(a, b, false), (b, a, true)].into_iter().find_map(|(tag, constant, reversed)| {
        let &(root, side) = tags.get(&tag)?;
        let &n = consts.get(&constant)?;
        if !matches!(n, 0 | 1) { return None; }
        let at = |v| if reversed { compare(n, v) } else { compare(v, n) };
        let positive = at(1)?;
        if at(0)? == positive { return None; } // Does not select one tag side.
        Some((root, if positive { side } else { !side }))
    })
}

include!("render_native_carrier_guards.rs");

#[cfg(test)]
#[path = "render_native_carrier_guard_test.rs"]
mod guard_tests;

/// The carrier index of a value's kind.
fn carrier_of(t: Option<NTy>) -> Option<u16> {
    t?.rec().filter(|i| is_carrier(*i))
}

fn emit(out: &mut String, indent: usize, s: &str) {
    for _ in 0..indent {
        out.push_str("    ");
    }
    writeln!(out, "{s}").expect("writing to a String cannot fail");
}

/// The owned copy of `v` as a slot of kind `k` (a carrier payload write).
fn payload_rhs(st: &AddrState, tys: &BTreeMap<ValueId, NTy>, v: ValueId, k: NTy) -> Result<String, LowerError> {
    if st.known_none.contains(&v) && k.rec().is_some_and(|i| option_payload(i).is_some()) {
        return Ok("None".into());
    }
    // A scalar Option is Copy and already materialized by the Option
    // rewrite. It is a typed payload, not an address into an allocation.
    if crate::native_value_repr::same_owned(k, NTy::Opt) && tys.get(&v) == Some(&NTy::Opt) && !st.addrs.contains_key(&v) {
        return Ok(var(v));
    }
    let Some(sv) = st.addrs.get(&v).copied().filter(|a| a.off == 0 && a.idx.is_none()) else {
        return Err(wall(format!("native: a {:?} carrier payload that is not a heap handle", tys.get(&v))));
    };
    let g = tys.get(&sv.base).copied();
    if st.empty_lists.contains(&sv.base) && (matches!(k, NTy::Vec | NTy::VecStr)
        || k.rec().is_some_and(|i| crate::render_native_rec::list_elem(i).is_some())) {
        return Ok("Vec::new()".into());
    }
    match g {
        Some(g) if field_accepts(k, g) => owned_from(&var(sv.base), g).ok_or_else(|| wall("native: an unowned carrier payload")),
        Some(g) if crate::native_value_repr::tuple_copy(&var(sv.base), g, k).is_some() =>
            Ok(crate::native_value_repr::tuple_copy(&var(sv.base), g, k).expect("the arm's guard saw this tuple copy succeed")),
        g => Err(wall(format!("native: a {g:?} value as a {k:?} carrier payload (source {:?})", sv.base))),
    }
}

/// Render `op` if it touches a carrier. `None` = not a carrier op.
pub(super) fn render_carrier_op(
    op: &Op,
    used: &std::collections::BTreeSet<ValueId>,
    st: &mut AddrState,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Option<Result<bool, LowerError>> {
    let mut cx = CarrierCx { st, tys, out, indent };
    match op {
        Op::IntBinOp { dst, op: cmp, a, b } => {
            if ![dst, a, b].iter().any(|v| cx.st.set_locals.contains(v)) {
                if let Some(tag) = tag_comparison(*cmp, *a, *b, &cx.st.consts, &cx.st.res_tags) {
                    cx.st.res_tags.insert(*dst, tag);
                }
            }
            None
        }
        Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } => {
            cx.track_arms(op);
            None
        }
        Op::Alloc { dst, init, .. } => cx.alloc(op, *dst, init),
        Op::Prim { kind: PrimKind::Load { width }, dst: Some(d), args } if args.len() == 1 => {
            let (ad, i) = cx.base_of(&args[0])?;
            Some(cx.load(*width, *d, ad, i))
        }
        Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } if args.len() == 1 => {
            let (ad, i) = cx.base_of(&args[0])?;
            // Lowering reads both Result payload views before testing the
            // tag. An unused view has no ownership event or observable
            // read; it need not pick a Rust payload type at all. Keep the
            // declared carrier and exact payload-slot checks here.
            if rec_slot(&ad) == Some(0) && !used.contains(d) {
                return Some(Ok(true));
            }
            cx.load_handle(*d, ad, i)
        }
        Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args } if args.len() == 2 => {
            let (ad, i) = cx.base_of(&args[0])?;
            Some(cx.store_payload(ad, i, args[1]))
        }
        Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args } if args.len() == 2 => {
            let (ad, i) = cx.base_of(&args[0])?;
            if option_payload(i).is_some() {
                return Some(cx.store_option_tag(ad, args));
            }
            let (o, e) = result_payloads(i)?;
            Some(cx.store_tag(ad, args, o, e))
        }
        _ => None,
    }
}

/// What [`render_carrier_op`] renders into.
struct CarrierCx<'a> {
    st: &'a mut AddrState,
    tys: &'a mut BTreeMap<ValueId, NTy>,
    out: &'a mut String,
    indent: usize,
}

impl CarrierCx<'_> {
    fn store_option_tag(&mut self, ad: Addr, args: &[ValueId]) -> Result<bool, LowerError> {
        let tag = self.st.option_tags.remove(&args[0])
            .ok_or_else(|| wall("native: an Option tag write outside its checked constructor"))?;
        if ad.off != i64::from(BLOCK_LEN_OFFSET) || ad.idx.is_some()
            || self.st.consts.get(&args[1]) != Some(&i64::from(tag)) {
            return Err(wall("native: an Option constructor's tag changed"));
        }
        // The fresh allocation is None; a committed payload store already
        // assigned Some. No arbitrary length mutation is erased here.
        if !tag { self.emit(&format!("{} = None;", var(ad.base))); }
        Ok(true)
    }

    fn emit(&mut self, s: &str) {
        emit(self.out, self.indent, s);
    }

    /// The address `a` names and the carrier its base is, if it is one.
    fn base_of(&self, a: &ValueId) -> Option<(Addr, u16)> {
        self.st.addrs.get(a).copied().and_then(|ad| carrier_of(self.tys.get(&ad.base).copied()).map(|i| (ad, i)))
    }

    /// The side of `base`'s tag the read `d` is on: the innermost open tag test of
    /// `base`, else a test above whose other arm left the function.
    fn arm_of(&self, d: ValueId, base: ValueId) -> Option<bool> {
        self.st.arms.iter().rev().find(|(r, _)| *r == Some(base)).map(|(_, then)| *then)
            .or_else(|| self.st.exit_proven.get(&d).copied())
    }

    /// The `if` arms open around the current op, by the Result tag each tests.
    fn track_arms(&mut self, op: &Op) {
        match op {
            Op::IfThen { cond, .. } => {
                let (res, side) = self.st.res_tags.get(cond).copied()
                    .map(|(r, s)| (Some(r), s)).unwrap_or((None, true));
                self.st.arms.push((res, side));
            }
            Op::Else { .. } => {
                if let Some(a) = self.st.arms.last_mut() {
                    a.1 = !a.1;
                }
            }
            _ => {
                self.st.arms.pop();
            }
        }
    }

    fn alloc(&mut self, op: &Op, dst: ValueId, init: &Init) -> Option<Result<bool, LowerError>> {
        match init {
            Init::ResErrStr { piece } if self.st.alloc_rec.get(&dst).is_some_and(|i| result_payloads(*i).is_some()) => {
                let i = self.st.alloc_rec[&dst];
                let (_, expected) = result_payloads(i).expect("the arm's guard saw this Result's payloads");
                let rhs = self.tys.get(piece).copied().and_then(|got| {
                    if field_accepts(expected, got) { owned_from(&var(*piece), got) }
                    else { crate::native_value_repr::tuple_copy(&var(*piece), got, expected) }
                }).ok_or_else(|| wall("native: semantic Err payload disagrees with its declared carrier"));
                Some(rhs.map(|rhs| {
                    self.tys.insert(dst, NTy::Rec(i));
                    self.emit(&format!("let mut {}: {} = Err({rhs});", var(dst), struct_name(i)));
                    true
                }))
            }
            Init::ResOkScalar { payload } if self.st.alloc_rec.get(&dst).is_some_and(|i| scalar_ok(*i)) => {
                let i = self.st.alloc_rec[&dst];
                self.tys.insert(dst, NTy::Rec(i));
                Some(scalar_rhs(self.tys, *payload).map(|rhs| {
                    self.emit(&format!("let mut {}: {} = Ok({rhs});", var(dst), struct_name(i)));
                    true
                }))
            }
            Init::DynList { len } if self.st.alloc_rec.get(&dst).is_some_and(|i| len_tagged(*i)) => {
                let i = self.st.alloc_rec[&dst];
                if self.st.consts.get(len) != Some(&1) {
                    return Some(Err(wall("native: a scalar Result block that is not one Err slot")));
                }
                self.tys.insert(dst, NTy::Rec(i));
                // Its one slot is stored next: the length word 1 made it an Err.
                self.emit(&format!("let mut {}: {};", var(dst), struct_name(i)));
                Some(Ok(true))
            }
            Init::DynList { .. } | Init::DynListStr { .. } | Init::OptNone
                if self.st.alloc_rec.get(&dst).is_some_and(|i| is_carrier(*i)) =>
            {
                Some(self.alloc_carrier(op, dst))
            }
            _ => None,
        }
    }

    fn alloc_carrier(&mut self, op: &Op, dst: ValueId) -> Result<bool, LowerError> {
        let i = self.st.alloc_rec[&dst];
        self.tys.insert(dst, NTy::Rec(i));
        let n = struct_name(i);
        match (option_payload(i).is_some(), op) {
            (true, _) => {
                self.emit(&format!("let mut {}: {n} = None;", var(dst)));
                Ok(true)
            }
            (false, Op::Alloc { init: Init::OptNone, .. }) => Err(wall("native: an OptNone block typed as a Result")),
            (false, _) => {
                // Its slot 0 and tag word are stored next; the tag picks the side.
                self.emit(&format!("let mut {}: {n};", var(dst)));
                Ok(true)
            }
        }
    }

    /// A tag word read: an Option's length word as `is_some`, or (`err`) a
    /// Result's tag as `is_err`, recorded as that Result's tag.
    fn tag_read(&mut self, d: ValueId, b: &str, base: ValueId, err: bool) -> Result<bool, LowerError> {
        self.tys.insert(d, NTy::I64);
        // Both carriers expose a true tag: Err for Result, Some for Option.
        if !self.st.set_locals.contains(&d) { self.st.res_tags.insert(d, (base, true)); }
        if err {
            self.emit(&format!("let mut {}: i64 = {b}.is_err() as i64;", var(d)));
        } else {
            self.emit(&format!("let mut {}: i64 = {b}.is_some() as i64;", var(d)));
        }
        Ok(true)
    }

    fn load(&mut self, width: u8, d: ValueId, ad: Addr, i: u16) -> Result<bool, LowerError> {
        let b = var(ad.base);
        let len_word = width == 4 && ad.idx.is_none() && ad.off == i64::from(BLOCK_LEN_OFFSET);
        if len_word && option_payload(i).is_some() {
            return self.tag_read(d, &b, ad.base, false);
        }
        if len_word && scalar_ok(i) {
            return self.tag_read(d, &b, ad.base, true);
        }
        if width == 8 && scalar_ok(i) && rec_slot(&ad) == Some(0) {
            let arm = self.arm_of(d, ad.base);
            let read = match arm.or_else(|| self.st.hoisted.get(&d).copied()) {
                Some(true) if len_tagged(i) => format!("match &{b} {{ Err(v) => *v, Ok(_) => 0 }}"),
                Some(false) => format!("match &{b} {{ Ok(v) => *v, Err(_) => 0 }}"),
                None if len_tagged(i) => format!("match &{b} {{ Ok(v) | Err(v) => *v }}"),
                _ => return Err(wall("native: a scalar Result payload read outside its Ok guard")),
            };
            self.tys.insert(d, NTy::I64);
            self.emit(&format!("let mut {}: i64 = {read};", var(d)));
            return Ok(true);
        }
        if width == 4 && ad.idx.is_none() && result_payloads(i).is_some() && !len_tagged(i) && ad.off == RESULT_TAG_OFFSET {
            return self.tag_read(d, &b, ad.base, true);
        }
        Err(wall(format!("native: Load{{{width}}} at {ad:?} of a {} carrier", owned_ty(NTy::Rec(i)))))
    }

    fn load_handle(&mut self, d: ValueId, ad: Addr, i: u16) -> Option<Result<bool, LowerError>> {
        if rec_slot(&ad) != Some(0) {
            return Some(Err(wall(format!("native: LoadHandle at {ad:?} of a carrier"))));
        }
        let b = var(ad.base);
        let read = if let Some(p) = option_payload(i) {
            let guarded_some = self.arm_of(d, ad.base) == Some(true);
            let lent = crate::render_native_borrow_read::borrowed_option(ad.base);
            if lent && self.lent_option_read(d, &b, p, guarded_some) {
                return Some(Ok(true));
            }
            let absent = if guarded_some { "unreachable!()".to_string() }
                else if has_default(p) { "Default::default()".to_string() }
                else if self.st.hoisted.get(&d) == Some(&true) {
                    // Every use is under this unchanged Option's Some tag.
                    // A nested Result has no Default; its absent-side value
                    // is an unobserved placeholder, never a fabricated hit.
                    let Some(value) = crate::native_value_repr::unused_payload(p) else {
                        return Some(Err(wall("native: no value for an unobserved Option payload")));
                    };
                    value
                } else {
                    return Some(Err(wall("native: an Option payload without Default read outside its Some guard")));
                };
            // The last read of an owned carrier's payload moves it out
            // (render_native_slots::movable_loads): nothing reads it again.
            // A by-value match, not `.take()`: `take` writes `None` back
            // through `mem::replace`, whose pointer checks the program's
            // debug-assertion profile keeps (string_index_loop, 1.037x v0).
            if lent {
                // The lent element, needed owned: the one copy the program asks for.
                (p, format!("match {b} {{ Some(v) => v.clone(), None => {absent} }}"))
            } else if self.st.moves_last.contains(&d) && matches!(self.tys.get(&ad.base), Some(NTy::Rec(_))) {
                (p, format!("match {b} {{ Some(v) => v, None => {absent} }}"))
            } else {
                (p, format!("match &{b} {{ Some(v) => v.clone(), None => {absent} }}"))
            }
        } else {
            match self.result_read(d, ad, i, &b)? {
                Ok(read) => read,
                Err(done) => return Some(done),
            }
        };
        if read.0 == NTy::I64 {
            return Some(Err(wall("native: a scalar carrier payload read as a handle")));
        }
        self.tys.insert(d, read.0);
        self.emit(&format!("let mut {}: {} = {};", var(d), owned_ty(read.0), read.1));
        Some(Ok(true))
    }

    /// The payload read of an element Option held through the borrowing twin
    /// (`Option<&A>`, render_native_borrow_read::borrowed_options) whose
    /// every use only borrows: the lent element itself, no copy. `false`
    /// when the read needs an owned value, or an absent side it cannot name.
    fn lent_option_read(&mut self, d: ValueId, b: &str, p: NTy, guarded_some: bool) -> bool {
        if !self.st.borrow_reads.contains(&d) {
            return false;
        }
        let dv = var(d);
        let (t, ty, val, absent) = match p {
            NTy::Rec(j) if guarded_some => (NTy::RecRef(j), format!("&{}", struct_name(j)), "v", "unreachable!()".to_string()),
            NTy::Rec(j) if has_default(p) => {
                self.emit(&format!("let {dv}_absent: {} = Default::default();", struct_name(j)));
                (NTy::RecRef(j), format!("&{}", struct_name(j)), "v", format!("&{dv}_absent"))
            }
            NTy::Str => (NTy::StrRef, "&str".to_string(), "&v[..]", "\"\"".to_string()),
            NTy::Vec => (NTy::VecRef, "&[i64]".to_string(), "&v[..]", "&[]".to_string()),
            NTy::VecStr => (NTy::VecStrRef, "&[AlmideString]".to_string(), "&v[..]", "&[]".to_string()),
            _ => return false,
        };
        if t == NTy::StrRef {
            crate::render_native_borrow_read::note_lent_str_read(d, b);
        }
        self.tys.insert(d, t);
        self.emit(&format!("let mut {dv}: {ty} = match {b} {{ Some(v) => {val}, None => {absent} }};"));
        true
    }

    /// A Result payload read (see [`PayloadRead`]).
    fn result_read(&mut self, d: ValueId, ad: Addr, i: u16, b: &str) -> Option<PayloadRead> {
        let (o, e) = result_payloads(i)?;
        // The arm of the innermost tag test of this Result, if any.
        let arm = self.arm_of(d, ad.base);
        // A floor result held through the runtime's borrowing twin: its Ok
        // side is already a reference (`&AlmideValue`).
        let ok_ref = crate::render_native_borrow_read::borrowed_result(ad.base);
        let ok_v = if ok_ref { "*v" } else { "v" };
        // A borrow-only read inside its tag test (render_native_borrow_read):
        // a reference into the carrier, no copy of the payload.
        if let Some(then) = arm {
            if self.borrowed_result_read((d, ad.base), b, then, (o, e), ok_v) {
                return Some(Err(Ok(true)));
            }
        }
        // A borrow-only read above the tag test, used under one side of
        // it: a reference into the carrier, the other side's (never
        // used) a reference to a Default the render keeps beside it.
        if let (None, Some(err_side), true) = (arm, self.st.hoisted.get(&d).copied(), self.st.borrow_reads.contains(&d)) {
            // `val`: the read side's payload reference (`*v` for an Ok side
            // held through the borrowing twin).
            let (k, side, other, val) = if err_side { (e, "Err", "Ok", "v") } else { (o, "Ok", "Err", ok_v) };
            let dv = var(d);
            match k {
                // A string payload: a slice of it, the other side's an empty
                // one. A copy where the carrier dies moves it out.
                NTy::Str => {
                    self.tys.insert(d, NTy::StrRef);
                    self.emit(&format!("let mut {dv}: &str = match &{b} {{ {side}(v) => &v[..], {other}(_) => \"\" }};"));
                    crate::render_native_addr::note_payload_take(
                        d,
                        ad.base,
                        format!("match &mut {b} {{ {side}(v) => std::mem::take(v), {other}(_) => Default::default() }}"),
                    );
                    return Some(Err(Ok(true)));
                }
                NTy::Rec(j) if has_default(k) => {
                    self.tys.insert(d, NTy::RecRef(j));
                    let t = struct_name(j);
                    self.emit(&format!("let {dv}_absent: {t} = Default::default();"));
                    self.emit(&format!("let mut {dv}: &{t} = match &{b} {{ {side}(v) => {val}, {other}(_) => &{dv}_absent }};"));
                    // A borrowed Ok payload (`*v`) is not the carrier's to give away.
                    if val == "v" {
                        crate::render_native_addr::note_payload_take(
                            d,
                            ad.base,
                            format!("match &mut {b} {{ {side}(v) => std::mem::take(v), {other}(_) => Default::default() }}"),
                        );
                    }
                    return Some(Err(Ok(true)));
                }
                _ => {}
            }
        }
        Some(Ok(match (arm, self.st.hoisted.get(&d).copied()) {
            (Some(true), _) => (e, format!("match &{b} {{ Err(v) => v.clone(), Ok(_) => unreachable!() }}")),
            (Some(false), _) => (o, format!("match &{b} {{ Ok(v) => ({ok_v}).clone(), Err(_) => unreachable!() }}")),
            (None, _) if o == e && !ok_ref => (o, format!("match &{b} {{ Ok(v) | Err(v) => v.clone() }}")),
            // Read above the tag test, used under its Ok side: the Err
            // side's read is never used, so it is a Default.
            (None, Some(side)) => {
                let (kind, selected, other) = if side { (e, "Err", "Ok") } else { (o, "Ok", "Err") };
                let Some(unused) = crate::native_value_repr::unused_payload(kind) else {
                    return Some(Err(Err(wall("native: no value for an unobserved carrier payload"))));
                };
                let val = if side { "v" } else { ok_v };
                (kind, format!("match &{b} {{ {selected}(v) => ({val}).clone(), {other}(_) => {unused} }}"))
            }
            _ => return Some(Err(Err(wall("native: a Result payload read outside its tag test")))),
        }))
    }

    /// The borrow-only read inside the `then` (Err) or else (Ok) arm of its
    /// tag test, rendered when the payload kind has a reference form.
    fn borrowed_result_read(&mut self, (d, base): (ValueId, ValueId), b: &str, then: bool, (o, e): (NTy, NTy), ok_v: &str) -> bool {
        let (k, side, other) = if then { (e, "Err", "Ok") } else { (o, "Ok", "Err") };
        let by_ref = match k {
            NTy::Rec(j) => Some((NTy::RecRef(j), format!("&{}", struct_name(j)), if then { "v" } else { ok_v })),
            NTy::Str => Some((NTy::StrRef, "&str".to_string(), "&v[..]")),
            // A list payload read only through its slots: a view of it, where
            // the owned read copied every element (a callback's list result
            // that a flat_map walks, a fallible HOF's Ok list measured).
            NTy::Vec => Some((NTy::VecRef, "&[i64]".to_string(), "&v[..]")),
            NTy::VecStr => Some((NTy::VecStrRef, "&[AlmideString]".to_string(), "&v[..]")),
            _ => None,
        };
        let (true, Some((t, ty, val))) = (self.st.borrow_reads.contains(&d), by_ref) else { return false };
        self.tys.insert(d, t);
        self.emit(&format!("let mut {}: {ty} = match &{b} {{ {side}(v) => {val}, {other}(_) => unreachable!() }};", var(d)));
        // A borrowed Ok payload (`*v`) is not the carrier's to give away.
        if has_default(k) && (then || ok_v == "v") {
            crate::render_native_addr::note_payload_take(
                d,
                base,
                format!("match &mut {b} {{ {side}(v) => std::mem::take(v), {other}(_) => unreachable!() }}"),
            );
        }
        true
    }

    fn store_payload(&mut self, ad: Addr, i: u16, v: ValueId) -> Result<bool, LowerError> {
        if rec_slot(&ad) != Some(0) {
            return Err(wall(format!("native: Store at {ad:?} of a carrier")));
        }
        if let Some(p) = option_payload(i) {
            return payload_rhs(self.st, self.tys, v, p).map(|rhs| {
                self.emit(&format!("{} = Some({rhs});", var(ad.base)));
                true
            });
        }
        if scalar_ok(i) {
            // A one-slot block of this Result is its Err (length word 1).
            let (_, err) = result_payloads(i).expect("scalar_ok holds only for a Result with payloads");
            let rhs = if err == NTy::I64 { scalar_rhs(self.tys, v) }
                else { payload_rhs(self.st, self.tys, v, err) };
            return rhs.map(|rhs| {
                self.emit(&format!("{} = Err({rhs});", var(ad.base)));
                true
            });
        }
        // The tag word that follows picks the side.
        self.st.pending.insert(ad.base, v);
        self.emit(&format!("// {}: payload {}, side by the tag next", var(ad.base), var(v)));
        Ok(true)
    }

    /// The Result tag word stored after its payload: picks the side.
    fn store_tag(&mut self, ad: Addr, args: &[ValueId], o: NTy, e: NTy) -> Result<bool, LowerError> {
        if ad.off != RESULT_TAG_OFFSET || ad.idx.is_some() {
            return Err(wall(format!("native: Store4 at {ad:?} of a Result carrier")));
        }
        let tag = self.st.consts.get(&args[1]).copied().ok_or_else(|| wall("native: a computed Result tag"))?;
        let v = self.st.pending.remove(&ad.base).ok_or_else(|| wall("native: a Result tag with no payload"))?;
        let (side, k) = if tag == 0 { ("Ok", o) } else { ("Err", e) };
        let rhs = payload_rhs(self.st, self.tys, v, k)?;
        self.emit(&format!("{} = {side}({rhs});", var(ad.base)));
        Ok(true)
    }
}

/// A Result payload read: `Ok` = its (kind, rhs) to bind as an owned copy;
/// `Err` = the op's whole outcome (a borrowed read rendered, or a wall).
type PayloadRead = Result<(NTy, String), Result<bool, LowerError>>;
