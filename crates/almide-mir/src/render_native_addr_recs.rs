// the carrier and join record kinds of render_native_addr.rs, include!-spliced back at module level (a code move under
// the 800-line ceiling the codopsy gate holds this crate to).

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
