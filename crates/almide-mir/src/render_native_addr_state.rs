// Per-function address facts and initialization, included by render_native_addr.
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
    declared_values: BTreeMap<ValueId, NTy>,
    guarded_enum_reads: BTreeMap<ValueId, u32>,
    /// The one initializing tag store of each fresh, declared Null Value.
    null_init: BTreeSet<ValueId>,
    list_finish: BTreeMap<ValueId, ValueId>,
    option_tags: BTreeMap<ValueId, bool>,
    known_none: BTreeSet<ValueId>,
    /// DynList allocs whose every stored element is a string.
    str_lists: BTreeSet<ValueId>,
    /// A Result carrier's tag read → the carrier (its `if` selects the arm).
    res_tags: BTreeMap<ValueId, (ValueId, bool)>,
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
    /// The borrow reads whose place moves under them
    /// (`render_native_borrow_read::moved_index_reads`): never noted as a
    /// place a copy may read again.
    moved_index_reads: BTreeSet<ValueId>,
    /// Heap slot reads of an owned record that move the value out
    /// (`render_native_slots::movable_loads`): nothing reads the slot again.
    moves: BTreeSet<ValueId>,
    /// The movable reads after which nothing reads the block at all.
    moves_last: BTreeSet<ValueId>,
    /// The movable reads that are their slot's first read since the block
    /// was bound: the ones a borrow may give way to a move.
    moves_first: BTreeSet<ValueId>,
    /// The movable reads whose block is rebound next in the same straight
    /// run: they move the field out by a plain (partial) move.
    moves_partial: BTreeSet<ValueId>,
    /// Heap slot reads that move out or retain a snapshot (`native_slot_takes`).
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
    borrowed_list_slots: BTreeSet<(ValueId, ValueId)>,
}

impl AddrState {
    pub(crate) fn new(func: &MirFunction, sigs: &crate::render_native::NativeSigs) -> Self {
        let mut alloc_rec = slots::declared_blocks(func);
        note_signature_recs(func, sigs, &mut alloc_rec);
        note_carrier_recs(func, sigs, &mut alloc_rec);
        note_join_recs(func, &mut alloc_rec);
        heap_slot::note_heap_slot_recs(func, &mut alloc_rec);
        let (handle_of, consts, base_of) = block_bases(func);
        // The untyped blocks, typed from their stores and the declarations
        // that consume them, in one fixpoint (render_native_slots.rs).
        let (moves, moves_last, moves_first, moves_partial) = slots::movable_loads(func, &base_of);
        let str_lists = slots::type_blocks(func, sigs, &mut alloc_rec, &slots::Facts { handle_of: &handle_of, base_of: &base_of, consts: &consts });
        // Tag exclusions need the declared enum of borrowed parameters too.
        // The allocation table below deliberately drops non-allocation values.
        let mut enum_sources = alloc_rec.clone();
        if let Some((params, _)) = sigs.get(&func.name) {
            for (param, kind) in func.params.iter().zip(params) {
                if let crate::render_native::NativeSigKind::Rec(i) = kind {
                    enum_sources.insert(param.value, *i);
                }
            }
        }
        let guarded_enum_reads = crate::native_borrowed_tco::guarded_reads(func, &enum_sources);
        let alloc_rec = func
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Alloc { dst, init: Init::DynList { .. } | Init::DynListStr { .. } | Init::DynStr { .. } | Init::Empty, .. } => alloc_rec.get(dst).map(|i| (*dst, *i)),
                Op::Alloc { dst, init: Init::OptNone | Init::ResOkScalar { .. } | Init::ResErrStr { .. }, .. } => {
                    alloc_rec.get(dst).filter(|i| carrier::is_carrier(**i)).map(|i| (*dst, *i))
                }
                Op::Alloc { dst, init: Init::IntList(vs), .. } if vs.is_empty() => alloc_rec.get(dst).copied()
                    .filter(|i| crate::render_native_rec::is_empty_collection(*i)).map(|i| (*dst, i)),
                // A scalar-only constructor of a heap variant, or an all-scalar
                // generated struct, is built as a slot list.
                Op::ListLit { dst, elems } => alloc_rec
                    .get(dst)
                    .filter(|i| crate::render_native_rec::enum_cases(**i).is_some() || crate::render_native_rec::all_scalar_struct(**i) || crate::render_native_rec::closure_sig(**i).is_some()
                        || (elems.is_empty() && crate::render_native_rec::is_empty_collection(**i)))
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
        // A carrier can share DynListStr's physical layout, but is not a
        // constant string table. Its payload still needs a real binding.
        let const_lists: BTreeMap<_, _> = slots::const_str_lists(func, &base_of, &consts)
            .into_iter().filter(|(v, _)| !alloc_rec.contains_key(v) && func.ret != Some(*v)).collect();
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
        let null_init = value::null_initializers(func, &alloc_rec);
        let option_tags = option_tags::sites(func, &alloc_rec, &base_of, &consts);
        AddrState {
            guarded_enum_reads,
            option_tags,
            known_none: option_tags::known_none(func),
            declared_values: func.aggregate_blocks.iter().filter_map(|(v, key)|
                crate::render_native_rec::declared_kind(key).map(|k| (*v, k))).collect(),
            str_lists,
            alloc_rec,
            null_init,
            list_finish: crate::native_list_finish::finalizers(func),
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
            empty_lists: empty_list_blocks(func),
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
            moved_index_reads: crate::render_native_borrow_read::moved_index_reads(func),
            moves,
            moves_last,
            moves_first,
            moves_partial,
            slot_reads: crate::native_slot_takes::slot_reads(func),
            whole,
            building: BTreeMap::new(),
            whole_enum,
            building_enum: BTreeMap::new(),
            single,
            single_rec,
            pushed,
            literal: BTreeMap::new(),
            borrowed_list_slots: slots::borrowed_stores(func, &handle_of, &base_of),
        }
    }
}

impl AddrState {
    /// `v` copies a heap slot's snapshot and only borrows (#4562,
    /// `native_slot_takes::SlotReads::aliases`): read through its reference.
    /// Its slot is one held for snapshots, so the read it copies renders as
    /// the snapshot's reference (render_native_heap_slot).
    pub(crate) fn slot_alias(&self, v: &ValueId) -> bool {
        self.slot_reads.aliases.get(v).is_some_and(|s| crate::native_slot_takes::snapshot_slot(*s))
    }
}
