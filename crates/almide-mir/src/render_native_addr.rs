//! #4006 List[String] slab: the native render of the MIR's slot-address algebra.
//!
//! The lowering builds and reads a heap list through byte addresses: a
//! `Prim{Handle}` turns the block into an i64 address, `IntBinOp::Add` steps
//! over the `[rc][len][cap]` header and the uniform 8-byte slots, and
//! `Prim{Load,LoadHandle,Store}` touch one word. Native has no addresses, so
//! this module interprets the arithmetic SYMBOLICALLY: an address value is
//! (the block's value, a constant byte offset, at most one index term), and
//! a load or store at one renders as the Vec operation at that slot. The
//! offsets come from the layout declaration (`crate::lower::layout`), the
//! one place the lowering's block shape is defined.
//!
//! Only shapes that name a whole slot or the `len` word render; anything else
//! a symbolic address flows into — a comparison, a call, a store as a plain
//! integer, a second index term — walls, so an address never reaches the
//! rendered program as a number.

use crate::lower::layout::{BLOCK_HEADER, BLOCK_LEN_OFFSET, SLOT_SIZE};
use crate::lower::LowerError;
use crate::render_native::{var, wall, NTy};
use crate::{Init, IntOp, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

#[path = "render_native_carrier.rs"]
mod carrier;
#[path = "render_native_map.rs"]
mod map;
#[path = "render_native_slots.rs"]
mod slots;
#[path = "render_native_heap_slot.rs"]
mod heap_slot;
#[path = "render_native_value.rs"]
mod value;
#[path = "render_native_byte_slots.rs"]
mod byte_slots;
#[path = "render_native_scalar_slots.rs"]
mod scalar_slots;
#[path = "render_native_env_slots.rs"]
mod env_slots;
#[path = "render_native_cell_slots.rs"]
mod cell_slots;
#[path = "render_native_option_tags.rs"]
mod option_tags;
pub(crate) use carrier::{carrier_values, is_carrier, CarrierSig};
pub(crate) use slots::flag_exits;

/// The index term of a slot address.
#[derive(Clone, Copy, Debug)]
enum Idx {
    /// A byte offset `i * SLOT_SIZE` (the loop form): the slot is `value / SLOT_SIZE`.
    Bytes(ValueId),
    /// An element index already bounds-checked by `Prim{ElemAddr}`.
    Elem(ValueId),
}

/// A symbolic address: `base`'s block + `off` bytes (+ the index term).
#[derive(Clone, Copy, Debug)]
struct Addr {
    base: ValueId,
    off: i64,
    idx: Option<Idx>,
}

include!("render_native_addr_state.rs");

/// The owned spelling of a generated list's kind.
fn owned_ty_of(t: Option<NTy>) -> String {
    t.map(crate::render_native_rec::owned_ty).unwrap_or_default()
}

/// The record kinds the declared signatures give a function's blocks: its own
/// return, a typed drop, a return, an argument to a typed param.
fn note_signature_recs(func: &MirFunction, sigs: &crate::render_native::NativeSigs, alloc_rec: &mut BTreeMap<ValueId, u16>) {
    use crate::render_native::NativeSigKind;
    let rec_kind = |k: Option<&NativeSigKind>| match k {
        Some(NativeSigKind::Rec(i)) => Some(*i),
        _ => None,
    };
    let own_ret = rec_kind(sigs.get(func.name.as_str()).and_then(|(_, r)| r.as_ref()));
    let mut note = |v: ValueId, i: Option<u16>| {
        if let Some(i) = i {
            alloc_rec.entry(v).or_insert(i);
        }
    };
    if let Some(r) = func.ret {
        note(r, own_ret);
    }
    let empty = empty_list_blocks(func);
    for op in &func.ops {
        match op {
            Op::DropVariant { v, ty } => note(*v, crate::render_native_rec::rec_index(ty)),
            Op::Return { val: Some(v) } => note(*v, own_ret),
            Op::CallFn { name, args, .. } => {
                if let Some((ps, _)) = sigs.get(name.as_str()) {
                    for (a, k) in args.iter().zip(ps) {
                        if let crate::CallArg::Handle(v) = a {
                            note(*v, rec_kind(Some(k)));
                        }
                    }
                } else if let Some(f) = crate::native_floor::declared(name) {
                    // A floor body's declared list of generated elements, for an
                    // empty list (`value.array([])`), which no store types; a
                    // filled one is typed from its stores as before.
                    for (a, p) in args.iter().zip(&f.params) {
                        if let crate::CallArg::Handle(v) = a {
                            if empty.contains(v) {
                                note(*v, crate::native_floor::generated_list_of(*p));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// `b` is appended by a self-append window `d = s + b; drop s; s = d` whose
/// `s` is declared `b`'s own generated list kind and whose `d` nothing else reads.
fn pushes_onto_own_kind(func: &MirFunction, b: ValueId, alloc_rec: &BTreeMap<ValueId, u16>) -> bool {
    (0..func.ops.len()).any(|j| {
        crate::render_native_append::self_append_window(func, j).is_some_and(|(s, x, d)| {
            x == b && alloc_rec.get(&s) == alloc_rec.get(&b) && func.ret != Some(d) && crate::render_native_append::touches(func, d) == 2
        })
    })
}

/// The list blocks a function allocates empty: a zero-length dynamic list
/// or an empty literal.
fn empty_list_blocks(func: &MirFunction) -> BTreeSet<ValueId> {
    let zeros: BTreeSet<ValueId> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::ConstInt { dst, value: 0 } => Some(*dst),
            _ => None,
        })
        .collect();
    func.ops
        .iter()
        .filter_map(|op| match op {
            Op::Alloc { dst, init: Init::DynList { len } | Init::DynListStr { len }, .. } if zeros.contains(len) => Some(*dst),
            Op::Alloc { dst, init: Init::Empty, .. } => Some(*dst),
            Op::Alloc { dst, init: Init::IntList(vs), .. } if vs.is_empty() => Some(*dst),
            Op::ListLit { dst, elems } if elems.is_empty() => Some(*dst),
            _ => None,
        })
        .collect()
}

/// The handle, constant and base-offset facts of a function's blocks.
#[allow(clippy::type_complexity)]
pub(crate) fn block_bases(func: &MirFunction) -> (BTreeMap<ValueId, ValueId>, BTreeMap<ValueId, i64>, BTreeMap<ValueId, (ValueId, Option<i64>)>) {
    // A block stored into a typed parent's slot takes the slot's declared
    // kind: a list's element, a record's field, an enum slot's one record kind.
    let mut handle_of: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    let mut consts: BTreeMap<ValueId, i64> = BTreeMap::new();
    let mut base_of: BTreeMap<ValueId, (ValueId, Option<i64>)> = BTreeMap::new();
    for op in &func.ops {
        match op {
            Op::ConstInt { dst, value } => {
                consts.insert(*dst, *value);
            }
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => {
                handle_of.insert(*d, args[0]);
                base_of.insert(*d, (args[0], Some(0)));
            }
            Op::IntBinOp { dst, op: IntOp::Add, a, b } => {
                let (x, y) = if base_of.contains_key(a) { (a, b) } else { (b, a) };
                if let Some(&(r, off)) = base_of.get(x) {
                    base_of.insert(*dst, (r, off.and_then(|o| consts.get(y).map(|c| o + c))));
                }
            }
            _ => {}
        }
    }
    (handle_of, consts, base_of)
}

/// Values read as an address operand (address arithmetic, a slot or header
/// access).
fn addr_operands_of(func: &MirFunction) -> BTreeSet<ValueId> {
    let mut addr_operands = BTreeSet::new();
    for op in &func.ops {
        match op {
            Op::IntBinOp { op: IntOp::Add, a, b, .. } => {
                addr_operands.insert(*a);
                addr_operands.insert(*b);
            }
            Op::Prim {
                kind: PrimKind::ElemAddr | PrimKind::Load { .. } | PrimKind::LoadHandle | PrimKind::Store { .. },
                args,
                ..
            } if !args.is_empty() => {
                addr_operands.insert(args[0]);
            }
            _ => {}
        }
    }
    addr_operands
}

fn is_block(t: NTy) -> bool {
    matches!(t, NTy::Vec | NTy::VecRef | NTy::VecStr | NTy::VecStrRef | NTy::Rec(_) | NTy::RecRef(_))
}

/// A list block (has a length), as opposed to a record struct.
fn is_list_block(t: NTy) -> bool {
    match t {
        NTy::Vec | NTy::VecRef | NTy::VecStr | NTy::VecStrRef => true,
        NTy::Rec(i) | NTy::RecRef(i) => crate::render_native_rec::list_elem(i).is_some(),
        _ => false,
    }
}

/// The element kind of a generated list type.
fn list_of(t: Option<NTy>) -> Option<NTy> {
    match t {
        Some(NTy::Rec(i) | NTy::RecRef(i)) => crate::render_native_rec::list_elem(i),
        _ => None,
    }
}

/// The record field a whole-slot address names (no index term).
fn rec_slot(a: &Addr) -> Option<usize> {
    let (h, w) = (i64::from(BLOCK_HEADER), i64::from(SLOT_SIZE));
    (a.idx.is_none() && a.off >= h && (a.off - h) % w == 0).then(|| ((a.off - h) / w) as usize)
}

/// An enum's read of payload slot `k` as `want` (`None`: a heap kind, when it
/// is the only heap kind the slot holds), or the tag at slot 0.
fn enum_read(i: u16, k: usize, scalar: bool) -> Option<(String, NTy)> {
    use crate::render_native_rec::{enum_slot_kinds, kind_suffix};
    if k == 0 {
        return scalar.then(|| ("tag()".to_string(), NTy::I64));
    }
    let kinds = enum_slot_kinds(i, k);
    if scalar {
        return kinds.contains(&NTy::I64).then(|| (format!("s{k}_i()"), NTy::I64));
    }
    let heap: Vec<NTy> = kinds.into_iter().filter(|t| *t != NTy::I64).collect();
    match heap.as_slice() {
        [one] => Some((format!("s{k}_{}()", kind_suffix(*one)), *one)),
        _ => None,
    }
}

fn is_enum(t: Option<NTy>) -> Option<u16> {
    let i = t?.rec()?;
    crate::render_native_rec::enum_cases(i).map(|_| i)
}

/// The owned copy of a heap value `code` of kind `t` (for a field write).
fn owned_from(code: &str, t: NTy) -> Option<String> {
    Some(match t {
        NTy::Opt => code.to_string(),
        NTy::Res | NTy::ResStr => format!("{code}.clone()"),
        NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_) => crate::render_native_move::take_expr(code),
        NTy::StrRef => format!("AlmideString::from({code})"),
        NTy::VecRef | NTy::VecStrRef => format!("{code}.to_vec()"),
        NTy::RecRef(_) => format!("{code}.clone()"),
        _ => return None,
    })
}

/// A field of kind `field` accepts a value of kind `got`.
fn field_accepts(field: NTy, got: NTy) -> bool {
    crate::native_value_repr::same_owned(field, got)
}

thread_local! {
    static STR_FIELDS: std::cell::RefCell<BTreeMap<ValueId, (ValueId, String)>> = const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// A borrowed read of a record field (#4006, no-perf-loss): the record and
/// the field's place. A copy of the read is then the field itself, moved out
/// when the record dies at the copy, else for a string the field's own handle
/// shared (`AlmideStr` clones by its count) — never the bytes copied into a
/// new block. The read borrows the field, so the field holds the same value
/// at the copy.
fn note_field(v: ValueId, base: ValueId, place: String) {
    STR_FIELDS.with(|m| m.borrow_mut().insert(v, (base, place)));
}

/// The block `b`, read at `ops[at]`, is rebound next in the same straight run.
pub(crate) fn rebound_next(func: &MirFunction, at: usize, b: ValueId) -> bool {
    slots::rebound_in_block(func, at, b).is_some()
}

pub(crate) fn str_field_of(v: ValueId) -> Option<String> {
    STR_FIELDS.with(|m| m.borrow().get(&v).map(|p| p.1.clone()))
}

/// The record and field place a borrowed read was taken from.
pub(crate) fn field_of(v: ValueId) -> Option<(ValueId, String)> {
    STR_FIELDS.with(|m| m.borrow().get(&v).cloned())
}

/// Every borrowed read taken from `place` so far.
pub(crate) fn reads_of_place(place: &str) -> Vec<ValueId> {
    STR_FIELDS.with(|m| m.borrow().iter().filter(|(_, p)| p.1 == place).map(|(v, _)| *v).collect())
}

pub(crate) fn clear_str_fields() {
    STR_FIELDS.with(|m| m.borrow_mut().clear());
    PAYLOAD_TAKES.with(|m| m.borrow_mut().clear());
    FIELD_COPIES.with(|m| m.borrow_mut().clear());
}

thread_local! {
    static FIELD_COPIES: std::cell::RefCell<std::collections::BTreeSet<ValueId>> = const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

/// An owned copy of a record's heap field (`r.f.clone()`). The MIR read lends
/// the field itself, so a write through the read edits the record; through
/// the copy it would edit only the copy, and the write would be lost.
pub(crate) fn note_field_copy(v: ValueId) {
    FIELD_COPIES.with(|m| m.borrow_mut().insert(v));
}

/// Whether the rendered operand `code` is a binding [`note_field_copy`] made.
pub(crate) fn is_field_copy(code: &str) -> bool {
    code.strip_prefix('v')
        .and_then(|n| n.parse::<u32>().ok())
        .is_some_and(|n| FIELD_COPIES.with(|m| m.borrow().contains(&ValueId(n))))
}

thread_local! {
    static PAYLOAD_TAKES: std::cell::RefCell<BTreeMap<ValueId, (ValueId, String)>> = const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// A borrowed read of a payload that has no place expression of its own (an
/// enum case's slot, a Result side): the carrier, and the expression that
/// moves the payload out of it, leaving the payload's `Default` behind. A
/// copy of the read is then that move when the carrier dies at the copy
/// (#4006, no-perf-loss), as for a record field ([`note_field`]).
pub(crate) fn note_payload_take(v: ValueId, base: ValueId, take: String) {
    PAYLOAD_TAKES.with(|m| m.borrow_mut().insert(v, (base, take)));
}

/// The carrier and move-out expression a borrowed payload read was taken from.
pub(crate) fn payload_take_of(v: ValueId) -> Option<(ValueId, String)> {
    PAYLOAD_TAKES.with(|m| m.borrow().get(&v).cloned())
}

/// Every borrowed read noted with the move-out expression `take`.
pub(crate) fn reads_of_take(take: &str) -> Vec<ValueId> {
    PAYLOAD_TAKES.with(|m| m.borrow().iter().filter(|(_, p)| p.1 == take).map(|(v, _)| *v).collect())
}

/// The borrowed kind of a heap value of kind `f` read in place, its Rust
/// type, and the suffix that turns `&place` into that type.
fn borrowed_kind(f: NTy) -> Option<(NTy, String, &'static str)> {
    match f {
        NTy::Str => Some((NTy::StrRef, "&str".to_string(), "[..]")),
        NTy::Vec => Some((NTy::VecRef, "&[i64]".to_string(), "[..]")),
        NTy::VecStr => Some((NTy::VecStrRef, "&[AlmideString]".to_string(), "[..]")),
        NTy::Rec(j) => Some((NTy::RecRef(j), format!("&{}", crate::render_native_rec::struct_name(j)), "")),
        _ => None,
    }
}

/// The bounds-witness slot of the element `a` addresses (render_native_bounds).
fn bounds_slot(st: &AddrState, a: &Addr) -> crate::render_native_bounds::Slot {
    let k = (a.off - i64::from(BLOCK_HEADER)) / i64::from(SLOT_SIZE);
    let idx = match a.idx {
        None => None,
        Some(Idx::Elem(i)) => Some(i),
        Some(Idx::Bytes(b)) => Some(st.scaled_src.get(&b).copied().unwrap_or(b)),
    };
    crate::render_native_bounds::Slot { base: a.base, idx, k }
}

/// The check an element access at `a` (Rust index `ix`) needs before it, and
/// its slot: none after `ElemAddr`'s own check or under the loop's exit
/// test; otherwise the explicit check, recorded as the slot's `c`.
fn bounds_guard(st: &AddrState, a: &Addr, ix: &str) -> (Option<String>, crate::render_native_bounds::Slot) {
    let s = bounds_slot(st, a);
    if matches!(a.idx, Some(Idx::Elem(_))) || crate::render_native_bounds::exit_covered(s) {
        return (None, s);
    }
    crate::render_native_bounds::check(s);
    (
        Some(format!("if {ix} >= {}.len() {{ almide_oob() }}", var(a.base))),
        s,
    )
}

/// log2(SLOT_SIZE): the shift that turns a slot-scaled byte offset into an index.
const SLOT_SHIFT: u32 = {
    assert!(SLOT_SIZE.is_power_of_two(), "slot_index shifts a byte offset by log2(SLOT_SIZE)");
    SLOT_SIZE.trailing_zeros()
};

/// The Rust index expression of the slot `a` names, or `None` when `a` is
/// not on a slot boundary past the header.
///
/// An `Idx::Bytes` offset is always `x * SLOT_SIZE`: `addr_add` builds one
/// only from a value in `scaled`, which `addr_arith_op` fills with exactly
/// those products (and records each one in `scaled_src`). It is a multiple
/// of the slot size, a power of two, so it is shifted right as an unsigned
/// value. For an offset
/// `>= 0` that is the same index as the signed division. For a negative one
/// (a wrapped product) both spellings give an index past any list, so the
/// same check fails. The signed spelling kept LLVM from dropping the bounds
/// check of a counted loop over a list, which cost a per-element loop over a
/// record list 6% against the standard codegen at opt-level 1
/// (`record_list_dynamic`).
fn slot_index(a: &Addr) -> Option<String> {
    let (h, w) = (i64::from(BLOCK_HEADER), i64::from(SLOT_SIZE));
    if a.off < h || (a.off - h) % w != 0 {
        return None;
    }
    let k = (a.off - h) / w;
    Some(match a.idx {
        None => format!("{k}usize"),
        Some(Idx::Bytes(b)) => format!("({k}usize + (({} as u64) >> {SLOT_SHIFT}u32) as usize)", var(b)),
        Some(Idx::Elem(i)) => format!("({k}i64 + {}) as usize", var(i)),
    })
}

/// One store into an enum block built whole (`AddrState::whole_enum`): the
/// tag store renders nothing (a nullary case is built there), a payload
/// waits in its own local, and the last payload builds the case in one
/// expression — no default payloads allocated and replaced.
fn build_enum_slot(
    s: AddrSink<'_>,
    base: ValueId,
    i: u16,
    k: usize,
    v: ValueId,
) -> Result<bool, LowerError> {
    let AddrSink { st, tys, out, indent } = s;
    use crate::render_native_rec::{enum_cases, owned_ty, struct_name};
    let line = |out: &mut String, s: String| {
        for _ in 0..indent {
            out.push_str("    ");
        }
        out.push_str(&s);
        out.push('\n');
    };
    let b = var(base);
    let (t, _) = st.building_enum[&base];
    let fields = enum_cases(i)
        .and_then(|cs| cs.into_iter().find(|(c, _)| *c == t).map(|(_, fs)| fs))
        .ok_or_else(|| wall("native: an enum built whole at an undeclared case"))?;
    if k == 0 {
        if st.addrs.contains_key(&v) || tys.get(&v).copied() != Some(NTy::I64) {
            return Err(wall("native: a non-scalar enum tag"));
        }
        if fields.is_empty() {
            line(out, format!("{b} = {}::C{t};", struct_name(i)));
            st.building_enum.remove(&base);
        }
        return Ok(true);
    }
    let f = *fields.get(k - 1).ok_or_else(|| wall("native: a store past an enum case's payload"))?;
    let rhs = match (f, st.addrs.get(&v).copied(), tys.get(&v).copied()) {
        (f, None, Some(NTy::Opt)) if field_accepts(f, NTy::Opt) => var(v),
        (NTy::I64, None, Some(NTy::I64)) => var(v),
        (NTy::I64, None, Some(NTy::F64)) => format!("{}.to_bits() as i64", var(v)),
        (f, Some(sv), _) if sv.off == 0 && sv.idx.is_none() => match tys.get(&sv.base).copied() {
            Some(g) if field_accepts(f, g) => owned_from(&var(sv.base), g).ok_or_else(|| wall("native: an unowned enum payload"))?,
            g => return Err(wall(format!("native: a {g:?} payload stored into a {f:?} enum slot"))),
        },
        (f, _, g) => return Err(wall(format!("native: a {g:?} value stored into a {f:?} enum slot"))),
    };
    line(out, format!("let mut {b}_f{k}: {} = {rhs};", owned_ty(f)));
    let done = &mut st.building_enum.get_mut(&base).expect("building").1;
    done[k - 1] = Some(f);
    if done.iter().all(Option::is_some) {
        let args = done
            .iter()
            .enumerate()
            .map(|(j, f)| match f {
                Some(f) if crate::render_native_rec::enum_field_boxed(i, *f) => {
                    format!("{}({b}_f{})", crate::render_native_rec::box_new(i), j + 1)
                }
                _ => format!("{b}_f{}", j + 1),
            })
            .collect::<Vec<_>>()
            .join(", ");
        line(out, format!("{b} = {}::C{t}({args});", struct_name(i)));
        st.building_enum.remove(&base);
    }
    Ok(true)
}

/// The append of a one-element list held as its element
/// ([`AddrState::single`]), opening at `ops[i]`: `acc.push(e)` when the
/// self-append window holds (an owned string list `acc`, its new value read
/// by nothing else); otherwise the list is built here (`vec![e]`) and the
/// caller renders the append as before. `None` when `ops[i]` is no such
/// append.
/// A list `+` whose right side is a one-element string list held as its
/// element (`render_native_slots::single_elem_lists`): the left list's
/// elements then that element, no one-element list built.
pub(crate) fn single_concat(func: &MirFunction, i: usize, tys: &mut BTreeMap<ValueId, NTy>, st: &mut AddrState) -> Option<(String, bool)> {
    let Op::CallFn { dst: Some(d), name, args, .. } = func.ops.get(i)? else { return None };
    if !crate::native_list_ops::is_list_concat(name) {
        return None;
    }
    let [crate::CallArg::Handle(a), crate::CallArg::Handle(x)] = args.as_slice() else { return None };
    if st.single.get(x) != Some(&true) {
        return None;
    }
    let left = match tys.get(a)? {
        NTy::VecStr if crate::render_native_move::moving_arg(*a) => {
            format!("let mut r: Vec<AlmideString> = {}; r.push({}_e0);", crate::render_native_move::take_expr(&var(*a)), var(*x))
        }
        NTy::VecStr => format!("let mut r: Vec<AlmideString> = Vec::with_capacity({a}.len() + 1); r.extend_from_slice(&{a}[..]); r.push({}_e0);", var(*x), a = var(*a)),
        NTy::VecStrRef => format!("let mut r: Vec<AlmideString> = Vec::with_capacity({a}.len() + 1); r.extend_from_slice({a}); r.push({}_e0);", var(*x), a = var(*a)),
        // Any other left side: the list is built here for the `+` below.
        _ => {
            st.single.remove(x);
            return Some((format!("let mut {x}: Vec<AlmideString> = vec![{x}_e0];", x = var(*x)), false));
        }
    };
    st.single.remove(x);
    tys.insert(*d, NTy::VecStr);
    Some((format!("let mut {}: Vec<AlmideString> = {{ {left} r }};", var(*d)), true))
}


include!("render_native_addr_op.rs");
include!("render_native_addr_alloc.rs");
include!("render_native_addr_recs.rs");
