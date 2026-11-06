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

/// Per-function state of the address algebra.
pub(crate) struct AddrState {
    consts: BTreeMap<ValueId, i64>,
    /// Values computed as `i * SLOT_SIZE`.
    scaled: BTreeSet<ValueId>,
    addrs: BTreeMap<ValueId, Addr>,
    /// Handles of a `SetLocal` target whose addresses are still read after a
    /// reassignment of it (or across a loop edge): such a handle could go
    /// stale, so it walls.
    reassigned: BTreeSet<ValueId>,
    /// Every `SetLocal` target: its `consts` entry is only its first value.
    set_locals: BTreeSet<ValueId>,
    /// Constant `List[String]` blocks (`render_native_slots::const_str_lists`):
    /// rendered once, as a lazy static the function borrows.
    const_lists: BTreeMap<ValueId, Vec<String>>,
    /// The literals those tables are filled from (`const_list_elems`).
    const_elems: BTreeSet<ValueId>,
    /// Empty list literals (`[]`): stored into a slot of a list type, each
    /// is that type's empty list.
    empty_lists: BTreeSet<ValueId>,
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
    /// The movable reads after which nothing reads the block at all.
    moves_last: BTreeSet<ValueId>,
    /// Heap slot reads that move the value out, or that a rebuild walls
    /// (`native_slot_takes`).
    slot_reads: crate::native_slot_takes::SlotReads,
    /// Struct blocks built whole (`render_native_slots::built_whole`), and
    /// the fields stored so far of the one being built.
    whole: BTreeSet<ValueId>,
    building: BTreeMap<ValueId, Vec<bool>>,
    /// Enum blocks built whole (`render_native_slots::enum_built_whole`):
    /// the case's tag and payload width, and the payloads stored so far of
    /// the one being built (each waits in its own local).
    whole_enum: BTreeMap<ValueId, (u32, usize)>,
    /// One-element string lists that exist only to be appended
    /// (`render_native_slots::single_elem_lists`): the element waits in
    /// `<list>_e0` and the append pushes it. `true` once it is stored.
    single: BTreeMap<ValueId, bool>,
    /// The generated-list twin of `single`: a one-element list of records,
    /// lists or variants that exists only to be appended to its own kind. Its
    /// element waits in `<list>_e0` (the built-whole literal's binding) and the
    /// append pushes it, instead of a `vec![…]` allocated and appended per
    /// iteration. `true` once the element is stored.
    single_rec: BTreeMap<ValueId, bool>,
    building_enum: BTreeMap<ValueId, (u32, Vec<Option<NTy>>)>,
    /// List blocks filled in slot order right after their allocation
    /// (`render_native_slots::filled_in_order`): reserved, then pushed.
    pushed: BTreeSet<ValueId>,
    /// Those of them that are generated lists, built as one `vec![…]` at the
    /// last store (each element bound as `<list>_e<k>` at its store), as the
    /// standard codegen builds a list literal: (length, elements stored).
    literal: BTreeMap<ValueId, (usize, usize)>,
}

impl AddrState {
    pub(crate) fn new(func: &MirFunction, sigs: &crate::render_native::NativeSigs) -> Self {
        let mut alloc_rec: BTreeMap<ValueId, u16> = BTreeMap::new();
        note_signature_recs(func, sigs, &mut alloc_rec);
        note_carrier_recs(func, sigs, &mut alloc_rec);
        note_join_recs(func, &mut alloc_rec);
        heap_slot::note_heap_slot_recs(func, &mut alloc_rec);
        let (handle_of, consts, base_of) = block_bases(func);
        // The untyped blocks, typed from their stores and the declarations
        // that consume them, in one fixpoint (render_native_slots.rs).
        let (moves, moves_last) = slots::movable_loads(func, &base_of);
        let str_lists = slots::type_blocks(func, sigs, &mut alloc_rec, &slots::Facts { handle_of: &handle_of, base_of: &base_of, consts: &consts });
        let alloc_rec = func
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Alloc { dst, init: Init::DynList { .. } | Init::DynListStr { .. } | Init::Empty, .. } => alloc_rec.get(dst).map(|i| (*dst, *i)),
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
        // A generated list (records, lists, variants) takes the generated
        // list's push below, whatever its block's alloc spelling.
        let generated = |b: &ValueId| alloc_rec.get(b).is_some_and(|i| crate::render_native_rec::list_elem(*i).is_some());
        let single: BTreeMap<ValueId, bool> =
            slots::single_elem_lists(func, &base_of, &consts).into_iter().filter(|b| !generated(b)).map(|b| (b, false)).collect();
        let pushed: BTreeSet<ValueId> = slots::filled_in_order(func, &base_of, &consts)
            .into_iter()
            .chain(slots::dense_loop_fills(func, &consts))
            .filter(|b| !single.contains_key(b))
            .collect();
        // Chosen only where the push is sure: the list the window appends to
        // is of the block's own kind, and the window's result is the rebind's
        // alone.
        let single_rec = slots::single_elem_blocks(func, &base_of, &consts)
            .into_iter()
            .filter(|b| pushed.contains(b) && generated(b) && pushes_onto_own_kind(func, *b, &alloc_rec))
            .map(|b| (b, false))
            .collect();
        let reassigned = stale_handles(func);
        let const_lists = slots::const_str_lists(func, &base_of, &consts);
        set_const_lists(const_lists.keys().copied().collect());
        let const_elems = const_list_elems(func, &const_lists.keys().copied().collect(), &base_of);
        let stored = func
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Prim { kind: PrimKind::Store { .. }, args, .. } if args.len() == 2 => Some(args[1]),
                _ => None,
            })
            .collect();
        let addr_operands = addr_operands_of(func);
        AddrState {
            str_lists,
            alloc_rec,
            addr_operands,
            consts: BTreeMap::new(),
            scaled: BTreeSet::new(),
            addrs: BTreeMap::new(),
            reassigned,
            set_locals: func.ops.iter().filter_map(|op| match op {
                Op::SetLocal { local, .. } => Some(*local),
                _ => None,
            }).collect(),
            const_lists,
            const_elems,
            empty_lists: func
                .ops
                .iter()
                .filter_map(|op| match op {
                    Op::Alloc { dst, init: Init::IntList(vs), .. } if vs.is_empty() => Some(*dst),
                    Op::ListLit { dst, elems } if elems.is_empty() => Some(*dst),
                    _ => None,
                })
                .collect(),
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
            moves_last,
            slot_reads: crate::native_slot_takes::slot_reads(func),
            whole,
            building: BTreeMap::new(),
            whole_enum,
            building_enum: BTreeMap::new(),
            single,
            single_rec,
            pushed,
            literal: BTreeMap::new(),
        }
    }
}

impl AddrState {
    /// `v` copies a heap slot's snapshot and only borrows (#4562,
    /// `native_slot_takes::SlotReads::aliases`): read through its reference.
    pub(crate) fn slot_alias(&self, v: &ValueId) -> bool {
        self.slot_reads.aliases.contains(v)
    }
}

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

/// #4006: heap Option/Result carriers (and map aliases), typed by the
/// declared signatures through joins and copies.
fn note_carrier_recs(func: &MirFunction, sigs: &crate::render_native::NativeSigs, alloc_rec: &mut BTreeMap<ValueId, u16>) {
    use crate::render_native::NativeSigKind;
    // #4006: heap Option/Result carriers, typed by the declared signatures
    // (this function's and its callees') through joins and copies.
    let carrier_k = |k: &NativeSigKind| match k {
        // (A map alias rides the same declared typing: `map.new()`'s
        // result takes the map type the program declares for it.)
        NativeSigKind::Rec(i)
            if carrier::is_carrier(*i)
                || crate::render_native_rec::map_kinds(*i).is_some()
                || crate::render_native_rec::set_elem(*i).is_some() =>
        {
            Some(*i)
        }
        _ => None,
    };
    let sig_of = |n: &str| -> Option<CarrierSig> {
        sigs.get(n).map(|(ps, r)| (ps.iter().map(carrier_k).collect(), r.as_ref().and_then(carrier_k)))
    };
    let own = sig_of(func.name.as_str()).unwrap_or_default();
    let declared = carrier::carrier_values(func, &own, &sig_of);
    crate::render_native_rec::set_declared(
        declared
            .iter()
            .filter(|(_, i)| crate::render_native_rec::map_kinds(**i).is_some() || crate::render_native_rec::set_elem(**i).is_some())
            .map(|(v, i)| (*v, *i))
            .collect(),
    );
    for (v, i) in declared {
        alloc_rec.entry(v).or_insert(i);
    }
    // An empty container (`map.clear(m)`, `xs = []`) takes the type the
    // lowering recorded for it, or the one its floor call declares.
    let empties: BTreeSet<ValueId> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Alloc { dst, init: Init::Empty, .. } => Some(*dst),
            _ => None,
        })
        .collect();
    // Or the type the lowering recorded for it.
    for v in &empties {
        if let Some(i) = func.aggregate_blocks.get(v).and_then(|k| crate::render_native_rec::rec_index(k)) {
            alloc_rec.entry(*v).or_insert(i);
        }
    }
    // Or the type the floor call it is handed to declares for it.
    for op in &func.ops {
        match op {
            Op::CallFn { name, args, .. } => {
                for (k, a) in args.iter().enumerate() {
                    if let crate::CallArg::Handle(v) = a {
                        if let Some(i) = Some(*v).filter(|v| empties.contains(v)).and_then(|_| crate::native_floor::specialized_param_rec(name, k)) {
                            alloc_rec.entry(*v).or_insert(i);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// The all-scalar struct kinds carried across if-value joins, to a fixpoint.
fn note_join_recs(func: &MirFunction, alloc_rec: &mut BTreeMap<ValueId, u16>) {
    // An if-value join holds one declared type on both arms: a typed join
    // types its arms' values, and a typed arm types the join. Only for an
    // all-scalar struct (built by value from its slot list), the one kind
    // whose arms are otherwise untyped literals.
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

/// The handle, constant and base-offset facts of a function's blocks.
#[allow(clippy::type_complexity)]
fn block_bases(func: &MirFunction) -> (BTreeMap<ValueId, ValueId>, BTreeMap<ValueId, i64>, BTreeMap<ValueId, (ValueId, Option<i64>)>) {
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
        NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_) => crate::render_native_move::take_expr(code),
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
        Some(format!("if {ix} >= {}.len() {{ eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }}", var(a.base))),
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

pub(crate) fn single_push(func: &MirFunction, i: usize, tys: &BTreeMap<ValueId, NTy>, st: &mut AddrState) -> Option<(String, bool)> {
    let (s, x, d) = crate::render_native_append::self_append_window(func, i)?;
    // A generated list's element: its window was checked when it was chosen.
    if st.single_rec.remove(&x) == Some(true) {
        return Some((format!("{}.push({}_e0);", var(s), var(x)), true));
    }
    if st.single.get(&x) != Some(&true) {
        return None;
    }
    st.single.remove(&x);
    if tys.get(&s) == Some(&NTy::VecStr) && func.ret != Some(d) && crate::render_native_append::touches(func, d) == 2 {
        return Some((format!("{}.push({}_e0);", var(s), var(x)), true));
    }
    Some((format!("let mut {x}: Vec<AlmideString> = vec![{x}_e0];", x = var(x)), false))
}

include!("render_native_addr_op.rs");
include!("render_native_addr_alloc.rs");
