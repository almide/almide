// the block allocations of render_native_addr.rs, include!-spliced back at module level (a code move under the
// 800-line ceiling the codopsy gate holds this crate to).


/// The block-allocating arms of [`render_addr_op`], in their order: a string
/// list, a generated list, a record or enum block, a typed string list, and
/// a scalar list.
fn block_alloc_op(op: &Op, s: AddrSink<'_>) -> Result<Option<bool>, LowerError> {
    let AddrSink { st, tys, out, indent } = s;
    if let Some(done) = const_or_empty_alloc(op, AddrSink { st: &mut *st, tys: &mut *tys, out: &mut *out, indent }) {
        return Ok(Some(done));
    }
    match op {
        Op::Alloc { dst, init: Init::DynListStr { len }, .. } if ty(tys, len) == Some(NTy::I64) && !st.alloc_rec.contains_key(dst) => {
            Ok(Some(alloc_str_list(AddrSink { st, tys, out, indent }, dst, len)))
        }
        Op::Alloc { dst, init: Init::DynList { len } | Init::DynListStr { len }, .. }
            if st.alloc_rec.get(dst).is_some_and(|i| crate::render_native_rec::list_elem(*i).is_some()) =>
        {
            alloc_rec_list(AddrSink { st, tys, out, indent }, dst, len).map(Some)
        }
        Op::Alloc { dst, init: Init::DynList { .. }, .. } if st.alloc_rec.contains_key(dst) => {
            alloc_rec_block(AddrSink { st, tys, out, indent }, dst).map(Some)
        }
        Op::Alloc { dst, init: Init::DynList { len }, .. } if ty(tys, len) == Some(NTy::I64) && st.str_lists.contains(dst) => {
            Ok(Some(alloc_typed_str_list(AddrSink { st, tys, out, indent }, dst, len)))
        }
        Op::Alloc { dst, init: Init::DynList { len }, .. } if ty(tys, len) == Some(NTy::I64) => {
            Ok(Some(alloc_scalar_list(AddrSink { st, tys, out, indent }, dst, len)))
        }
        _ => Ok(None),
    }
}

/// A typed empty container, a constant string table and the literals only
/// such a table is filled from. `None` = none of them.
fn const_or_empty_alloc(op: &Op, s: AddrSink<'_>) -> Option<bool> {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    if let Some(dst) = match op { Op::Alloc { dst, .. } | Op::ListLit { dst, .. } => Some(dst), _ => None }
        .filter(|dst| st.empty_lists.contains(dst)) {
        let t = st.alloc_rec.get(dst).copied().filter(|i| crate::render_native_rec::is_empty_collection(*i)).map(NTy::Rec).or_else(|| st.str_lists.contains(dst).then_some(NTy::VecStr));
        if let Some(t) = t {
            tys.insert(*dst, t);
            let empty = if matches!(t, NTy::Rec(i) if crate::render_native_rec::map_kinds(i).is_some()
                || crate::render_native_rec::set_elem(i).is_some()) { "Default::default()" } else { "Vec::new()" };
            line!("let mut {}: {} = {empty};", var(*dst), crate::render_native_rec::owned_ty(t));
            return Some(true);
        }
    }
    match op {
        // A typed empty container: its type's empty value.
        Op::Alloc { dst, init: Init::Empty, .. }
            if st.alloc_rec.get(dst).is_some_and(|i| crate::render_native_rec::has_default(NTy::Rec(*i))) =>
        {
            let i = st.alloc_rec[dst];
            tys.insert(*dst, NTy::Rec(i));
            line!("let mut {}: {} = Default::default();", var(*dst), crate::render_native_rec::struct_name(i));
            Some(true)
        }
        // A literal only a constant table is filled from: in the static.
        Op::Alloc { dst, init: Init::Str(_), .. } if st.const_elems.contains(dst) => {
            tys.insert(*dst, NTy::Str);
            Some(true)
        }
        // A constant string table: built once, on its first use, from the
        // immortal static string blocks (never written, so thread-safe), and
        // borrowed — the lazy static v0 keeps for a top-level `let` table.
        Op::Alloc { dst, init: Init::DynListStr { .. }, .. } if st.const_lists.contains_key(dst) => {
            tys.insert(*dst, NTy::VecStrRef);
            let elems: Vec<String> = st.const_lists[dst]
                .iter()
                .map(|s| {
                    let lit = almide_base::rust_lit::escape_rust_str(s);
                    format!(
                        "{{ static S: crate::almide_str_block::AlmideStrStatic<{}> = crate::almide_str_block::AlmideStrStatic::new(\"{lit}\"); AlmideString::from_static(&S) }}",
                        s.len()
                    )
                })
                .collect();
            line!(
                "static {k}: std::sync::LazyLock<Vec<AlmideString>> = std::sync::LazyLock::new(|| vec![{}]);",
                elems.join(", "),
                k = format!("ALMD_TABLE_{}", var(*dst))
            );
            line!("let {}: &Vec<AlmideString> = &*ALMD_TABLE_{};", var(*dst), var(*dst));
            Some(true)
        }
        _ => None,
    }
}

/// `Alloc{DynListStr}` of an untyped block: a string list.
fn alloc_str_list(s: AddrSink<'_>, dst: &ValueId, len: &ValueId) -> bool {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    tys.insert(*dst, NTy::VecStr);
    if crate::native_map_entries::defer_pair(*dst) {
        line!("// {}: a map entry, its key and value bound one by one", var(*dst));
        return true;
    }
    if st.single.contains_key(dst) {
        line!("// {}: a one-element list, its element pushed at the append", var(*dst));
        return true;
    }
    if st.pushed.contains(dst) {
        line!("let mut {}: Vec<AlmideString> = {};", var(*dst), reserved_vec(&var(*len)));
        return true;
    }
    line!(
        "let mut {}: Vec<AlmideString> = (0..{}.max(0)).map(|_| AlmideString::new()).collect();",
        var(*dst),
        var(*len)
    );
    true
}

/// A list alloc whose declared type is a generated list.
fn alloc_rec_list(s: AddrSink<'_>, dst: &ValueId, len: &ValueId) -> Result<bool, LowerError> {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let i = st.alloc_rec[dst];
    tys.insert(*dst, NTy::Rec(i));
    if st.pushed.contains(dst) {
        // A constant length (filled_in_order requires one): the list
        // is one `vec![…]` once its last element is stored.
        if let Some(n) = st.consts.get(len).and_then(|n| usize::try_from(*n).ok()) {
            st.literal.insert(*dst, (n, 0));
            if crate::native_map_entries::leave_unbuilt(*dst, crate::render_native_rec::list_elem(i)) {
                line!("// {}: a map's {n} entries, handed over as an array", var(*dst));
                return Ok(true);
            }
            line!("// {}: a list of {n}, built whole at its last element", var(*dst));
            return Ok(true);
        }
        line!("let mut {}: {} = {};", var(*dst), crate::render_native_rec::struct_name(i), reserved_vec(&var(*len)));
        return Ok(true);
    }
    if st.consts.get(len) == Some(&0) {
        line!("let mut {}: {} = Vec::new();", var(*dst), crate::render_native_rec::struct_name(i));
        return Ok(true);
    }
    if !crate::render_native_rec::list_elem(i).is_some_and(crate::render_native_rec::has_default) {
        return Err(wall("native: list elements must be built before reading their value"));
    }
    line!(
        "let mut {}: {} = (0..{}.max(0)).map(|_| Default::default()).collect();",
        var(*dst),
        crate::render_native_rec::struct_name(i),
        var(*len)
    );
    Ok(true)
}

/// A `DynList` alloc whose declared type is a record or an enum.
fn alloc_rec_block(s: AddrSink<'_>, dst: &ValueId) -> Result<bool, LowerError> {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
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
    if !crate::render_native_rec::has_default(NTy::Rec(i)) {
        return Err(wall("native: all record fields must be initialized before use"));
    }
    line!("let mut {}: {} = Default::default();", var(*dst), crate::render_native_rec::struct_name(i));
    Ok(true)
}

/// A `DynList` alloc every stored element of which is a string.
fn alloc_typed_str_list(s: AddrSink<'_>, dst: &ValueId, len: &ValueId) -> bool {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    tys.insert(*dst, NTy::VecStr);
    if st.pushed.contains(dst) {
        line!("let mut {}: Vec<AlmideString> = {};", var(*dst), reserved_vec(&var(*len)));
        return true;
    }
    line!(
        "let mut {}: Vec<AlmideString> = (0..{}.max(0)).map(|_| AlmideString::new()).collect();",
        var(*dst),
        var(*len)
    );
    true
}

/// A `DynList` alloc of scalars.
fn alloc_scalar_list(s: AddrSink<'_>, dst: &ValueId, len: &ValueId) -> bool {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    tys.insert(*dst, NTy::Vec);
    if st.pushed.contains(dst) {
        line!("let mut {}: Vec<i64> = {};", var(*dst), reserved_vec(&var(*len)));
        return true;
    }
    line!("let mut {}: Vec<i64> = vec![0i64; {}.max(0) as usize];", var(*dst), var(*len));
    true
}

/// The string literals a constant table (`const_str_lists`) is filled from
/// and nothing else reads: each one's whole life is the handle the table's
/// store takes and its hand-over into the block. The table is built once, in
/// its static, so the render writes nothing for them — materializing each one
/// per call only to release it was the table's whole per-call cost.
fn const_list_elems(
    func: &MirFunction,
    tables: &BTreeSet<ValueId>,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
) -> BTreeSet<ValueId> {
    let handle_of: BTreeMap<ValueId, ValueId> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => Some((*d, args[0])),
            _ => None,
        })
        .collect();
    let stored: BTreeSet<ValueId> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. }
                if args.len() == 2 && args.first().and_then(|a| base_of.get(a)).is_some_and(|(r, _)| tables.contains(r)) =>
            {
                handle_of.get(&args[1]).copied()
            }
            _ => None,
        })
        .collect();
    let is_lit = |e: &ValueId| func.ops.iter().any(|op| matches!(op, Op::Alloc { dst, init: Init::Str(_), .. } if dst == e));
    stored
        .into_iter()
        .filter(|e| {
            is_lit(e)
                && func.ops.iter().all(|op| {
                    let mut reads = Vec::new();
                    crate::mir_ops::op_reads(op, &mut reads);
                    !reads.contains(e)
                        || matches!(op, Op::Prim { kind: PrimKind::Handle, args, .. } if args.as_slice() == [*e])
                        || matches!(op, Op::Consume { v } if v == e)
                })
        })
        .collect()
}

/// A list reserved for `len` elements, failing as the runtime's own
/// reservation fails (C-197: `Error: out of memory`, exit 1) — never with
/// `Vec::with_capacity`'s capacity-overflow panic. Since #4006's direct range
/// source the length can be a range's span, which no machine may hold.
pub(crate) fn reserved_vec(len: &str) -> String {
    format!("{{ let mut r = Vec::new(); if r.try_reserve_exact({len}.max(0) as usize).is_err() {{ eprintln!(\"Error: out of memory\"); std::process::exit(1); }} r }}")
}

// Materialize a singleton only when it cannot be appended directly.
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
