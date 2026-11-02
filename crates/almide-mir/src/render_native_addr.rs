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
pub(crate) use carrier::{carrier_values, is_carrier, CarrierSig};

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

/// Per-function state of the address algebra.
pub(crate) struct AddrState {
    consts: BTreeMap<ValueId, i64>,
    /// Values computed as `i * SLOT_SIZE`.
    scaled: BTreeSet<ValueId>,
    addrs: BTreeMap<ValueId, Addr>,
    /// `SetLocal` targets: a handle of one of these could go stale, so it walls.
    reassigned: BTreeSet<ValueId>,
    /// Values some `Store` writes into a slot. A string's handle joins the
    /// algebra only as one of these; any other string handle (the abort
    /// message of `Die`, …) stays with the op family that owns it.
    stored: BTreeSet<ValueId>,
    /// Values read as an address operand (address arithmetic, a slot or
    /// header access). A block's handle joins the algebra only as one of these.
    addr_operands: BTreeSet<ValueId>,
    /// DynList allocs whose declared type is a generated record struct, read
    /// from the type-bearing ops that consume them (a typed drop, a return, an
    /// argument to a user fn) — the declarations, never the slot contents.
    alloc_rec: BTreeMap<ValueId, u16>,
    /// DynList allocs whose every stored element is a string.
    str_lists: BTreeSet<ValueId>,
    /// A Result carrier's tag read → the carrier (its `if` selects the arm).
    res_tags: BTreeMap<ValueId, ValueId>,
    /// A Result carrier whose slot 0 was stored, waiting for its tag word.
    pending: BTreeMap<ValueId, ValueId>,
    /// The open `if`s: the Result whose tag each one tests, and whether the
    /// render is in its then-arm (tag 1 = Err).
    arms: Vec<(Option<ValueId>, bool)>,
    /// Result payload reads hoisted above their tag test → the side their uses select.
    hoisted: BTreeMap<ValueId, bool>,
    /// A map's length read → the map.
    map_lens: BTreeMap<ValueId, ValueId>,
    /// `len + i` over a map's length read → (the map, `i`).
    len_plus: BTreeMap<ValueId, (ValueId, ValueId)>,
    /// `x * SLOT_SIZE` → `x`.
    scaled_src: BTreeMap<ValueId, ValueId>,
    /// Values computed by integer arithmetic (not a plain counter).
    arith: BTreeSet<ValueId>,
    /// Payload reads that render as a borrow into their block.
    borrow_reads: BTreeSet<ValueId>,
    /// Heap slot reads of an owned record that move the value out
    /// (`render_native_slots::movable_loads`): nothing reads the slot again.
    moves: BTreeSet<ValueId>,
    /// Struct blocks built whole (`render_native_slots::built_whole`), and
    /// the fields stored so far of the one being built.
    whole: BTreeSet<ValueId>,
    building: BTreeMap<ValueId, Vec<bool>>,
    /// Enum blocks built whole (`render_native_slots::enum_built_whole`):
    /// the case's tag and payload width, and the payloads stored so far of
    /// the one being built (each waits in its own local).
    whole_enum: BTreeMap<ValueId, (u32, usize)>,
    building_enum: BTreeMap<ValueId, (u32, Vec<Option<NTy>>)>,
}

impl AddrState {
    pub(crate) fn new(func: &MirFunction, sigs: &crate::render_native::NativeSigs) -> Self {
        use crate::render_native::NativeSigKind;
        let rec_kind = |k: Option<&NativeSigKind>| match k {
            Some(NativeSigKind::Rec(i)) => Some(*i),
            _ => None,
        };
        let own_ret = rec_kind(sigs.get(func.name.as_str()).and_then(|(_, r)| r.as_ref()));
        let mut alloc_rec: BTreeMap<ValueId, u16> = BTreeMap::new();
        let mut note = |v: ValueId, i: Option<u16>| {
            if let Some(i) = i {
                alloc_rec.entry(v).or_insert(i);
            }
        };
        if let Some(r) = func.ret {
            note(r, own_ret);
        }
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
                    }
                }
                _ => {}
            }
        }
        // #4006: heap Option/Result carriers, typed by the declared signatures
        // (this function's and its callees') through joins and copies.
        let carrier_k = |k: &NativeSigKind| match k {
            // (A map alias rides the same declared typing: `map.new()`'s
            // result takes the map type the program declares for it.)
            NativeSigKind::Rec(i) if carrier::is_carrier(*i) || crate::render_native_rec::map_kinds(*i).is_some() => Some(*i),
            _ => None,
        };
        let sig_of = |n: &str| -> Option<CarrierSig> {
            sigs.get(n).map(|(ps, r)| (ps.iter().map(carrier_k).collect(), r.as_ref().and_then(carrier_k)))
        };
        let own = sig_of(func.name.as_str()).unwrap_or_default();
        let declared = carrier::carrier_values(func, &own, &sig_of);
        crate::render_native_rec::set_declared(
            declared.iter().filter(|(_, i)| crate::render_native_rec::map_kinds(**i).is_some()).map(|(v, i)| (*v, *i)).collect(),
        );
        for (v, i) in declared {
            alloc_rec.entry(v).or_insert(i);
        }
        // An if-value join holds one declared type on both arms: a typed join
        // types its arms' values, and a typed arm types the join. Only for an
        // all-scalar struct (built by value from its slot list), the one kind
        // whose arms are otherwise untyped literals.
        {
            let mut edges: Vec<(ValueId, ValueId)> = Vec::new();
            let mut ifs: Vec<Option<ValueId>> = Vec::new();
            for op in &func.ops {
                match op {
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
                let before = alloc_rec.len();
                for (a, b) in &edges {
                    let scalar_struct = |v: Option<&u16>| v.copied().filter(|i| crate::render_native_rec::all_scalar_struct(*i));
                    if let Some(i) = scalar_struct(alloc_rec.get(a)) {
                        alloc_rec.entry(*b).or_insert(i);
                    }
                    if let Some(i) = scalar_struct(alloc_rec.get(b)) {
                        alloc_rec.entry(*a).or_insert(i);
                    }
                }
                if alloc_rec.len() == before {
                    break;
                }
            }
        }
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
        // The untyped blocks, typed from their stores and the declarations
        // that consume them, in one fixpoint (render_native_slots.rs).
        let moves = slots::movable_loads(func, &base_of);
        let str_lists = slots::type_blocks(func, sigs, &mut alloc_rec, &slots::Facts { handle_of: &handle_of, base_of: &base_of, consts: &consts });
        let alloc_rec = func
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Alloc { dst, init: Init::DynList { .. } | Init::DynListStr { .. }, .. } => alloc_rec.get(dst).map(|i| (*dst, *i)),
                Op::Alloc { dst, init: Init::OptNone | Init::ResOkScalar { .. }, .. } => {
                    alloc_rec.get(dst).filter(|i| carrier::is_carrier(**i)).map(|i| (*dst, *i))
                }
                // A scalar-only constructor of a heap variant, or an all-scalar
                // generated struct, is built as a slot list.
                Op::ListLit { dst, .. } => alloc_rec
                    .get(dst)
                    .filter(|i| crate::render_native_rec::enum_cases(**i).is_some() || crate::render_native_rec::all_scalar_struct(**i))
                    .map(|i| (*dst, *i)),
                _ => None,
            })
            .collect();
        let whole = slots::built_whole(func, &alloc_rec, &base_of);
        let whole_enum = slots::enum_built_whole(func, &alloc_rec, &base_of, &consts);
        let reassigned = func
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::SetLocal { local, .. } => Some(*local),
                _ => None,
            })
            .collect();
        let stored = func
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Prim { kind: PrimKind::Store { .. }, args, .. } if args.len() == 2 => Some(args[1]),
                _ => None,
            })
            .collect();
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
        AddrState {
            str_lists,
            alloc_rec,
            addr_operands,
            consts: BTreeMap::new(),
            scaled: BTreeSet::new(),
            addrs: BTreeMap::new(),
            reassigned,
            stored,
            res_tags: BTreeMap::new(),
            pending: BTreeMap::new(),
            arms: Vec::new(),
            hoisted: carrier::hoisted_sides(func),
            map_lens: BTreeMap::new(),
            len_plus: BTreeMap::new(),
            scaled_src: BTreeMap::new(),
            arith: BTreeSet::new(),
            borrow_reads: crate::render_native_borrow_read::borrow_reads(func),
            moves,
            whole,
            building: BTreeMap::new(),
            whole_enum,
            building_enum: BTreeMap::new(),
        }
    }
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
    match t {
        Some(NTy::Rec(i) | NTy::RecRef(i)) => crate::render_native_rec::enum_cases(i).map(|_| i),
        _ => None,
    }
}

/// The owned copy of a heap value `code` of kind `t` (for a field write).
fn owned_from(code: &str, t: NTy) -> Option<String> {
    Some(match t {
        NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_) => format!("std::mem::take(&mut {code})"),
        NTy::StrRef => format!("AlmideString::from({code})"),
        NTy::VecRef | NTy::VecStrRef => format!("{code}.to_vec()"),
        NTy::RecRef(_) => format!("{code}.clone()"),
        _ => return None,
    })
}

/// A field of kind `field` accepts a value of kind `got`.
fn field_accepts(field: NTy, got: NTy) -> bool {
    match field {
        NTy::Str => matches!(got, NTy::Str | NTy::StrRef),
        NTy::Vec => matches!(got, NTy::Vec | NTy::VecRef),
        NTy::VecStr => matches!(got, NTy::VecStr | NTy::VecStrRef),
        NTy::Rec(i) => matches!(got, NTy::Rec(j) | NTy::RecRef(j) if j == i),
        _ => false,
    }
}

/// The Rust index expression of the slot `a` names, or `None` when `a` is
/// not on a slot boundary past the header.
fn slot_index(a: &Addr) -> Option<String> {
    let (h, w) = (i64::from(BLOCK_HEADER), i64::from(SLOT_SIZE));
    if a.off < h || (a.off - h) % w != 0 {
        return None;
    }
    let k = (a.off - h) / w;
    Some(match a.idx {
        None => format!("{k}usize"),
        Some(Idx::Bytes(b)) => format!("({k}i64 + {} / {w}i64) as usize", var(b)),
        Some(Idx::Elem(i)) => format!("({k}i64 + {}) as usize", var(i)),
    })
}

/// One store into an enum block built whole (`AddrState::whole_enum`): the
/// tag store renders nothing (a nullary case is built there), a payload
/// waits in its own local, and the last payload builds the case in one
/// expression — no default payloads allocated and replaced.
#[allow(clippy::too_many_arguments)]
fn build_enum_slot(
    st: &mut AddrState,
    tys: &BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
    base: ValueId,
    i: u16,
    k: usize,
    v: ValueId,
) -> Result<bool, LowerError> {
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
                Some(NTy::Rec(_)) => format!("Box::new({b}_f{})", j + 1),
                _ => format!("{b}_f{}", j + 1),
            })
            .collect::<Vec<_>>()
            .join(", ");
        line(out, format!("{b} = {}::C{t}({args});", struct_name(i)));
        st.building_enum.remove(&base);
    }
    Ok(true)
}

/// Render `op` if it belongs to the address algebra. `Ok(false)` = not this
/// module's op (the caller renders it); an address reaching any other op walls.
pub(crate) fn render_addr_op(
    op: &Op,
    used: &BTreeSet<ValueId>,
    st: &mut AddrState,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Result<bool, LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let ty = |tys: &BTreeMap<ValueId, NTy>, v: &ValueId| tys.get(v).copied();
    if let Some(r) = carrier::render_carrier_op(op, st, tys, out, indent) {
        return r;
    }
    if let Some(r) = map::render_map_op(op, st, tys, out, indent) {
        return r;
    }
    match op {
        Op::ConstInt { dst, value } => {
            st.consts.insert(*dst, *value);
            return Ok(false);
        }
        Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if used.contains(d) && args.len() == 1 => {
            let x = args[0];
            match ty(tys, &x) {
                Some(t)
                    if (is_block(t) && (st.addr_operands.contains(d) || st.stored.contains(d)))
                        || (matches!(t, NTy::Str | NTy::StrRef) && st.stored.contains(d)) =>
                {
                    if st.reassigned.contains(&x) {
                        return Err(wall("native: a slot address of a reassigned local"));
                    }
                    st.addrs.insert(*d, Addr { base: x, off: 0, idx: None });
                    line!("// {}: the address of {}", var(*d), var(x));
                    return Ok(true);
                }
                _ => return Ok(false),
            }
        }
        Op::IntBinOp { dst, op: IntOp::Add, a, b } if st.addrs.contains_key(a) || st.addrs.contains_key(b) => {
            let (ad, other) = match (st.addrs.get(a), st.addrs.get(b)) {
                (Some(x), None) => (*x, *b),
                (None, Some(x)) => (*x, *a),
                _ => return Err(wall("native: the sum of two slot addresses")),
            };
            let next = if let Some(c) = st.consts.get(&other) {
                Addr { off: ad.off + c, ..ad }
            } else if st.scaled.contains(&other) && ad.idx.is_none() {
                Addr { idx: Some(Idx::Bytes(other)), ..ad }
            } else {
                return Err(wall("native: a slot address offset by an unscaled value"));
            };
            st.addrs.insert(*dst, next);
            line!("// {}: {} + {} bytes", var(*dst), var(next.base), next.off);
            return Ok(true);
        }
        Op::IntBinOp { dst, op: IntOp::Mul, a, b }
            if !st.addrs.contains_key(a) && !st.addrs.contains_key(b) =>
        {
            let w = i64::from(SLOT_SIZE);
            if st.consts.get(a) == Some(&w) || st.consts.get(b) == Some(&w) {
                st.scaled.insert(*dst);
            }
            return Ok(false);
        }
        Op::Prim { kind: PrimKind::ElemAddr, dst: Some(d), args } if args.len() == 2 => {
            let (h, i) = (args[0], args[1]);
            let Some(ad) = st.addrs.get(&h).copied() else { return Ok(false) };
            if ad.off != 0 || ad.idx.is_some() || !ty(tys, &ad.base).is_some_and(is_list_block) {
                return Err(wall("native: ElemAddr off a non-block address"));
            }
            if ty(tys, &i) != Some(NTy::I64) {
                return Err(wall("native: ElemAddr with a non-scalar index"));
            }
            line!(
                "if {i} < 0 || {i} as usize >= {b}.len() {{ eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }}",
                i = var(i),
                b = var(ad.base)
            );
            st.addrs.insert(*d, Addr { base: ad.base, off: i64::from(BLOCK_HEADER), idx: Some(Idx::Elem(i)) });
            return Ok(true);
        }
        Op::Prim { kind: PrimKind::Load { width }, dst: Some(d), args } if args.len() == 1 && st.addrs.contains_key(&args[0]) => {
            let ad = st.addrs[&args[0]];
            let bt = ty(tys, &ad.base);
            if *width == 4 && ad.off == i64::from(BLOCK_LEN_OFFSET) && ad.idx.is_none() && bt.is_some_and(is_list_block) {
                tys.insert(*d, NTy::I64);
                line!("let mut {}: i64 = {}.len() as i64;", var(*d), var(ad.base));
                return Ok(true);
            }
            if let (8, Some(i), Some(k)) = (*width, is_enum(bt), rec_slot(&ad)) {
                if let Some((acc, t)) = enum_read(i, k, true) {
                    tys.insert(*d, t);
                    line!("let mut {}: i64 = {}.{acc};", var(*d), var(ad.base));
                    return Ok(true);
                }
            }
            if let (8, Some(NTy::Rec(i) | NTy::RecRef(i)), Some(k)) = (*width, bt, rec_slot(&ad)) {
                if crate::render_native_rec::rec_field(i, k) == Some(NTy::I64) {
                    tys.insert(*d, NTy::I64);
                    line!("let mut {}: i64 = {}.f{k};", var(*d), var(ad.base));
                    return Ok(true);
                }
            }
            if *width == 8 && matches!(bt, Some(NTy::Vec | NTy::VecRef)) {
                if let Some(ix) = slot_index(&ad) {
                    tys.insert(*d, NTy::I64);
                    line!("let mut {}: i64 = {}[{ix}];", var(*d), var(ad.base));
                    return Ok(true);
                }
            }
            return Err(wall(format!("native: Load{{{width}}} at {ad:?} — outside the slot algebra")));
        }
        Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } if args.len() == 1 && st.addrs.contains_key(&args[0]) => {
            let ad = st.addrs[&args[0]];
            if let (Some(i), Some(k)) = (is_enum(ty(tys, &ad.base)), rec_slot(&ad)) {
                let Some((acc, t)) = enum_read(i, k, false) else {
                    return Err(wall("native: a heap read of an enum slot of several heap kinds"));
                };
                // A borrow-only read of a boxed record: a reference into the block.
                if let NTy::Rec(j) = t {
                    if st.borrow_reads.contains(d) && crate::render_native_rec::has_default(t) {
                        let s = crate::render_native_rec::struct_name(j);
                        let r = acc.replacen("()", "_ref()", 1);
                        tys.insert(*d, NTy::RecRef(j));
                        line!(
                            "let {v}_d: {s}; let mut {v}: &{s} = match {b}.{r} {{ Some(r) => r, None => {{ {v}_d = {s}::default(); &{v}_d }} }};",
                            v = var(*d),
                            b = var(ad.base)
                        );
                        return Ok(true);
                    }
                }
                tys.insert(*d, t);
                line!("let mut {}: {} = {}.{acc};", var(*d), crate::render_native_rec::owned_ty(t), var(ad.base));
                return Ok(true);
            }
            if let (Some(NTy::Rec(i) | NTy::RecRef(i)), Some(k)) = (ty(tys, &ad.base), rec_slot(&ad)) {
                if let Some(f) = crate::render_native_rec::rec_field(i, k).filter(|f| *f != NTy::I64) {
                    // A borrow-only read (crate::render_native_borrow_read): a
                    // reference into the record's field, where the owned read
                    // would copy it (a string's bytes, a list's elements).
                    let by_ref = match f {
                        NTy::Str => Some((NTy::StrRef, "&str".to_string(), "[..]")),
                        NTy::Vec => Some((NTy::VecRef, "&[i64]".to_string(), "[..]")),
                        NTy::VecStr => Some((NTy::VecStrRef, "&[AlmideString]".to_string(), "[..]")),
                        NTy::Rec(j) => Some((NTy::RecRef(j), format!("&{}", crate::render_native_rec::struct_name(j)), "")),
                        _ => None,
                    };
                    if let (true, Some((t, spelled, whole))) = (st.borrow_reads.contains(d), by_ref) {
                        tys.insert(*d, t);
                        line!("let mut {}: {spelled} = &{}.f{k}{whole};", var(*d), var(ad.base));
                        return Ok(true);
                    }
                    tys.insert(*d, f);
                    // The last read of the slot of an owned record moves the value out.
                    if st.moves.contains(d) && matches!(ty(tys, &ad.base), Some(NTy::Rec(_))) {
                        line!(
                            "let mut {}: {} = std::mem::take(&mut {}.f{k});",
                            var(*d),
                            crate::render_native_rec::owned_ty(f),
                            var(ad.base)
                        );
                        return Ok(true);
                    }
                    line!(
                        "let mut {}: {} = {}.f{k}.clone();",
                        var(*d),
                        crate::render_native_rec::owned_ty(f),
                        var(ad.base)
                    );
                    return Ok(true);
                }
            }
            if let (Some(e), Some(ix)) = (list_of(ty(tys, &ad.base)), slot_index(&ad)) {
                if e != NTy::I64 {
                    tys.insert(*d, e);
                    line!("let mut {}: {} = {}[{ix}].clone();", var(*d), crate::render_native_rec::owned_ty(e), var(ad.base));
                    return Ok(true);
                }
            }
            match (ty(tys, &ad.base), slot_index(&ad)) {
                (Some(NTy::VecStr | NTy::VecStrRef), Some(ix)) => {
                    // A borrowed alias in MIR; an owned copy here has the same
                    // value and leaves the list free to change afterwards.
                    tys.insert(*d, NTy::Str);
                    line!("let mut {}: AlmideString = {}[{ix}].clone();", var(*d), var(ad.base));
                    return Ok(true);
                }
                _ => return Err(wall(format!("native: LoadHandle at {ad:?} — outside the slot algebra"))),
            }
        }
        Op::Prim { kind: PrimKind::Store { width }, dst: None, args } if *width != 8 && args.len() == 2 && st.addrs.contains_key(&args[0]) => {
            return Err(wall(format!("native: Store{{{width}}} at {:?} — outside the slot algebra", st.addrs[&args[0]])));
        }
        Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args } if args.len() == 2 && st.addrs.contains_key(&args[0]) => {
            let ad = st.addrs[&args[0]];
            let v = args[1];
            if let (Some(i), Some(NTy::Rec(_)), Some(k)) = (is_enum(ty(tys, &ad.base)), ty(tys, &ad.base), rec_slot(&ad)) {
                use crate::render_native_rec::{enum_slot_kinds, kind_suffix, struct_name};
                let b = var(ad.base);
                if st.building_enum.contains_key(&ad.base) {
                    return build_enum_slot(st, tys, out, indent, ad.base, i, k, v);
                }
                if k == 0 {
                    if st.addrs.contains_key(&v) || ty(tys, &v) != Some(NTy::I64) {
                        return Err(wall("native: a non-scalar enum tag"));
                    }
                    line!("{b} = {}::with_tag({});", struct_name(i), var(v));
                    return Ok(true);
                }
                let kinds = enum_slot_kinds(i, k);
                match (st.addrs.get(&v).copied(), ty(tys, &v)) {
                    (None, Some(NTy::I64)) if kinds.contains(&NTy::I64) => line!("{b}.set_s{k}_i({});", var(v)),
                    (None, Some(NTy::F64)) if kinds.contains(&NTy::I64) => line!("{b}.set_s{k}_i({}.to_bits() as i64);", var(v)),
                    (Some(sv), _) if sv.off == 0 && sv.idx.is_none() => {
                        let g = ty(tys, &sv.base).ok_or_else(|| wall("native: an untyped enum payload"))?;
                        let fits: Vec<NTy> = kinds.into_iter().filter(|f| field_accepts(*f, g)).collect();
                        let [f] = fits.as_slice() else {
                            return Err(wall(format!("native: a {g:?} payload fits no single kind of an enum slot")));
                        };
                        let rhs = owned_from(&var(sv.base), g).ok_or_else(|| wall("native: an unowned enum payload"))?;
                        line!("{b}.set_s{k}_{}({rhs});", kind_suffix(*f));
                    }
                    (_, g) => return Err(wall(format!("native: a {g:?} value stored into an enum slot"))),
                }
                return Ok(true);
            }
            if let (Some(NTy::Rec(i)), Some(k), None) = (ty(tys, &ad.base), rec_slot(&ad), list_of(ty(tys, &ad.base))) {
                let field = crate::render_native_rec::rec_field(i, k)
                    .ok_or_else(|| wall("native: a store past a record's last field"))?;
                let rhs = match (field, st.addrs.get(&v).copied(), ty(tys, &v)) {
                    (NTy::I64, None, Some(NTy::I64)) => var(v),
                    (NTy::I64, None, Some(NTy::F64)) => format!("{}.to_bits() as i64", var(v)),
                    (f, Some(sv), _) if sv.off == 0 && sv.idx.is_none() => match ty(tys, &sv.base) {
                        Some(g) if field_accepts(f, g) => owned_from(&var(sv.base), g)
                            .ok_or_else(|| wall("native: a record field write of an unowned kind"))?,
                        g => return Err(wall(format!("native: a {g:?} value stored into a {f:?} record field"))),
                    },
                    (f, _, g) => return Err(wall(format!("native: a {g:?} value stored into a {f:?} record field"))),
                };
                if let Some(done) = st.building.get_mut(&ad.base) {
                    // A struct built whole: the field waits in its own local
                    // until the last one is stored.
                    let b = var(ad.base);
                    line!("let mut {b}_f{k}: {} = {rhs};", crate::render_native_rec::owned_ty(field));
                    done[k] = true;
                    if done.iter().all(|d| *d) {
                        let fields = (0..done.len()).map(|f| format!("f{f}: {b}_f{f}")).collect::<Vec<_>>().join(", ");
                        line!("{b} = {} {{ {fields} }};", crate::render_native_rec::struct_name(i));
                        st.building.remove(&ad.base);
                    }
                    return Ok(true);
                }
                line!("{}.f{k} = {rhs};", var(ad.base));
                return Ok(true);
            }
            let Some(ix) = slot_index(&ad) else {
                return Err(wall(format!("native: Store at {ad:?} — outside the slot algebra")));
            };
            if let (Some(NTy::Rec(_)), Some(e), Some(sv)) = (ty(tys, &ad.base), list_of(ty(tys, &ad.base)), st.addrs.get(&v).copied()) {
                let g = ty(tys, &sv.base).filter(|g| sv.off == 0 && sv.idx.is_none() && field_accepts(e, *g));
                let rhs = g.and_then(|g| owned_from(&var(sv.base), g)).ok_or_else(|| {
                    wall(format!("native: a {:?} element stored into a list of {e:?}", ty(tys, &sv.base)))
                })?;
                line!("{}[{ix}] = {rhs};", var(ad.base));
                return Ok(true);
            }
            match (ty(tys, &ad.base), st.addrs.get(&v).copied()) {
                (Some(NTy::VecStr), Some(sv)) if sv.off == 0 && sv.idx.is_none() => {
                    // The stored handle is Consumed by the list: move it in.
                    let rhs = match ty(tys, &sv.base) {
                        Some(NTy::Str) => format!("std::mem::take(&mut {})", var(sv.base)),
                        Some(NTy::StrRef) => format!("AlmideString::from({})", var(sv.base)),
                        other => return Err(wall(format!("native: a {other:?} handle stored into a List[String] slot"))),
                    };
                    line!("{}[{ix}] = {rhs};", var(ad.base));
                    return Ok(true);
                }
                (Some(NTy::Vec), None) if ty(tys, &v) == Some(NTy::I64) => {
                    line!("{}[{ix}] = {};", var(ad.base), var(v));
                    return Ok(true);
                }
                (bt, sv) => {
                    return Err(wall(format!(
                        "native: Store of {:?}/{:?} into a {bt:?} block at {ad:?} — outside the slot algebra",
                        ty(tys, &v),
                        sv.map(|a| ty(tys, &a.base))
                    )))
                }
            }
        }
        Op::Alloc { dst, init: Init::DynListStr { len }, .. } if ty(tys, len) == Some(NTy::I64) && !st.alloc_rec.contains_key(dst) => {
            tys.insert(*dst, NTy::VecStr);
            line!(
                "let mut {}: Vec<AlmideString> = (0..{}.max(0)).map(|_| AlmideString::new()).collect();",
                var(*dst),
                var(*len)
            );
            return Ok(true);
        }
        Op::Alloc { dst, init: Init::DynList { len } | Init::DynListStr { len }, .. }
            if st.alloc_rec.get(dst).is_some_and(|i| crate::render_native_rec::list_elem(*i).is_some()) =>
        {
            let i = st.alloc_rec[dst];
            tys.insert(*dst, NTy::Rec(i));
            line!(
                "let mut {}: {} = (0..{}.max(0)).map(|_| Default::default()).collect();",
                var(*dst),
                crate::render_native_rec::struct_name(i),
                var(*len)
            );
            return Ok(true);
        }
        Op::Alloc { dst, init: Init::DynList { .. }, .. } if st.alloc_rec.contains_key(dst) => {
            let i = st.alloc_rec[dst];
            tys.insert(*dst, NTy::Rec(i));
            if let (true, Some(n)) = (st.whole.contains(dst), slots::struct_width(i)) {
                line!("let mut {}: {};", var(*dst), crate::render_native_rec::struct_name(i));
                st.building.insert(*dst, vec![false; n]);
                return Ok(true);
            }
            if let Some(&(t, n)) = st.whole_enum.get(dst) {
                line!("let mut {}: {};", var(*dst), crate::render_native_rec::struct_name(i));
                st.building_enum.insert(*dst, (t, vec![None; n]));
                return Ok(true);
            }
            line!("let mut {}: {} = Default::default();", var(*dst), crate::render_native_rec::struct_name(i));
            return Ok(true);
        }
        Op::ListGetScalar { dst, list, idx } if is_enum(ty(tys, list)).is_some() => {
            let (Some(i), Some(&k)) = (is_enum(ty(tys, list)), st.consts.get(idx)) else {
                return Err(wall("native: an enum slot read at a computed slot"));
            };
            let (acc, _) = enum_read(i, k as usize, true).ok_or_else(|| wall("native: a scalar read of a heap enum slot"))?;
            tys.insert(*dst, NTy::I64);
            line!("let mut {}: i64 = {}.{acc};", var(*dst), var(*list));
            return Ok(true);
        }
        Op::ListLit { dst, elems } if st.alloc_rec.get(dst).is_some_and(|i| crate::render_native_rec::all_scalar_struct(*i)) => {
            // An all-scalar struct literal: slot K is field `fK` (a real f64
            // goes in as its bits, the value the MIR stores there).
            let i = st.alloc_rec[dst];
            let mut fields = Vec::with_capacity(elems.len());
            for (k, e) in elems.iter().enumerate() {
                fields.push(match ty(tys, e) {
                    Some(NTy::I64) => format!("f{k}: {}", var(*e)),
                    Some(NTy::F64) => format!("f{k}: {}.to_bits() as i64", var(*e)),
                    _ => return Err(wall("native: a heap element in an all-scalar struct literal")),
                });
            }
            if fields.len() != crate::render_native_rec::field_count(i) {
                return Err(wall("native: a struct literal of the wrong width"));
            }
            tys.insert(*dst, NTy::Rec(i));
            line!("let mut {}: {n} = {n} {{ {} }};", var(*dst), fields.join(", "), n = crate::render_native_rec::struct_name(i));
            return Ok(true);
        }
        Op::ListLit { dst, elems } if st.alloc_rec.contains_key(dst) => {
            let i = st.alloc_rec[dst];
            if elems.iter().any(|e| ty(tys, e) != Some(NTy::I64)) {
                return Err(wall("native: a heap element in an enum slot list"));
            }
            tys.insert(*dst, NTy::Rec(i));
            let items = elems.iter().map(|e| var(*e)).collect::<Vec<_>>().join(", ");
            line!("let mut {}: {n} = {n}::from_slots(&[{items}]);", var(*dst), n = crate::render_native_rec::struct_name(i));
            return Ok(true);
        }
        Op::ListGetScalar { dst, list, idx } if matches!(ty(tys, list), Some(NTy::Rec(_) | NTy::RecRef(_))) => {
            let (Some(NTy::Rec(i) | NTy::RecRef(i)), Some(&k)) = (ty(tys, list), st.consts.get(idx)) else {
                return Err(wall("native: a record field read at a computed slot"));
            };
            if crate::render_native_rec::rec_field(i, k as usize) != Some(NTy::I64) {
                return Err(wall("native: a scalar read of a heap record field"));
            }
            tys.insert(*dst, NTy::I64);
            line!("let mut {}: i64 = {}.f{k};", var(*dst), var(*list));
            return Ok(true);
        }
        Op::ListSetScalar { list, idx, val } if matches!(ty(tys, list), Some(NTy::Rec(_) | NTy::RecRef(_))) => {
            let (Some(NTy::Rec(i)), Some(&k)) = (ty(tys, list), st.consts.get(idx)) else {
                return Err(wall("native: a record field write on a borrowed record or at a computed slot"));
            };
            if crate::render_native_rec::rec_field(i, k as usize) != Some(NTy::I64) || ty(tys, val) != Some(NTy::I64) {
                return Err(wall("native: a scalar write of a heap record field"));
            }
            line!("{}.f{k} = {};", var(*list), var(*val));
            return Ok(true);
        }
        Op::Alloc { dst, init: Init::DynList { len }, .. } if ty(tys, len) == Some(NTy::I64) && st.str_lists.contains(dst) => {
            tys.insert(*dst, NTy::VecStr);
            line!(
                "let mut {}: Vec<AlmideString> = (0..{}.max(0)).map(|_| AlmideString::new()).collect();",
                var(*dst),
                var(*len)
            );
            return Ok(true);
        }
        Op::Alloc { dst, init: Init::DynList { len }, .. } if ty(tys, len) == Some(NTy::I64) => {
            tys.insert(*dst, NTy::Vec);
            line!("let mut {}: Vec<i64> = vec![0i64; {}.max(0) as usize];", var(*dst), var(*len));
            return Ok(true);
        }
        _ => {}
    }
    // Any other op that reads a symbolic address would see a number native
    // does not have.
    let mut reads = Vec::new();
    crate::mir_ops::op_reads(op, &mut reads);
    if let Some(v) = reads.iter().find(|v| st.addrs.contains_key(v)) {
        return Err(wall(format!(
            "native: slot address {} escapes into {:?} — outside the slot algebra",
            var(*v),
            std::mem::discriminant(op)
        )));
    }
    Ok(false)
}
