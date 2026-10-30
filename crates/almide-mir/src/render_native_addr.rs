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
        let alloc_rec = func
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Alloc { dst, init: Init::DynList { .. }, .. } => alloc_rec.get(dst).map(|i| (*dst, *i)),
                _ => None,
            })
            .collect();
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
        AddrState { alloc_rec, addr_operands, consts: BTreeMap::new(), scaled: BTreeSet::new(), addrs: BTreeMap::new(), reassigned, stored }
    }
}

fn is_block(t: NTy) -> bool {
    matches!(t, NTy::Vec | NTy::VecRef | NTy::VecStr | NTy::VecStrRef | NTy::Rec(_) | NTy::RecRef(_))
}

/// A list block (has a length), as opposed to a record struct.
fn is_list_block(t: NTy) -> bool {
    matches!(t, NTy::Vec | NTy::VecRef | NTy::VecStr | NTy::VecStrRef)
}

/// The record field a whole-slot address names (no index term).
fn rec_slot(a: &Addr) -> Option<usize> {
    let (h, w) = (i64::from(BLOCK_HEADER), i64::from(SLOT_SIZE));
    (a.idx.is_none() && a.off >= h && (a.off - h) % w == 0).then(|| ((a.off - h) / w) as usize)
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
            if let (Some(NTy::Rec(i) | NTy::RecRef(i)), Some(k)) = (ty(tys, &ad.base), rec_slot(&ad)) {
                if let Some(f) = crate::render_native_rec::rec_field(i, k).filter(|f| *f != NTy::I64) {
                    tys.insert(*d, f);
                    line!(
                        "let mut {}: {} = {}.f{k}.clone();",
                        var(*d),
                        crate::render_native_rec::owned_ty(f),
                        var(ad.base)
                    );
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
            if let (Some(NTy::Rec(i)), Some(k)) = (ty(tys, &ad.base), rec_slot(&ad)) {
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
                line!("{}.f{k} = {rhs};", var(ad.base));
                return Ok(true);
            }
            let Some(ix) = slot_index(&ad) else {
                return Err(wall(format!("native: Store at {ad:?} — outside the slot algebra")));
            };
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
        Op::Alloc { dst, init: Init::DynListStr { len }, .. } if ty(tys, len) == Some(NTy::I64) => {
            tys.insert(*dst, NTy::VecStr);
            line!(
                "let mut {}: Vec<AlmideString> = (0..{}.max(0)).map(|_| AlmideString::new()).collect();",
                var(*dst),
                var(*len)
            );
            return Ok(true);
        }
        Op::Alloc { dst, init: Init::DynList { .. }, .. } if st.alloc_rec.contains_key(dst) => {
            let i = st.alloc_rec[dst];
            tys.insert(*dst, NTy::Rec(i));
            line!("let mut {}: {} = Default::default();", var(*dst), crate::render_native_rec::struct_name(i));
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
