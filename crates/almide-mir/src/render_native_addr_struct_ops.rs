// Included from render_native_addr_op.rs; a text move preserving function scope.
/// The slot-list arms of [`render_addr_op`] over generated structs, in their
/// order: an enum's scalar read, the struct literals, a record's scalar read
/// and write.
fn struct_slot_op(op: &Op, s: AddrSink<'_>) -> Result<Option<bool>, LowerError> {
    if let Some(done) = const_or_empty_alloc(op, AddrSink { st: &mut *s.st, tys: &mut *s.tys, out: &mut *s.out, indent: s.indent }) {
        return Ok(Some(done));
    }
    let AddrSink { st, tys, out, indent } = s;
    match op {
        Op::ListGetScalar { dst, list, idx } if is_enum(ty(tys, list)).is_some() => {
            enum_scalar_get(AddrSink { st, tys, out, indent }, dst, list, idx).map(Some)
        }
        Op::ListLit { dst, elems } if st.alloc_rec.get(dst).is_some_and(|i| crate::render_native_rec::all_scalar_struct(*i)) => {
            scalar_struct_lit(AddrSink { st, tys, out, indent }, dst, elems).map(Some)
        }
        Op::ListLit { dst, elems } if st.alloc_rec.contains_key(dst) => {
            enum_slot_lit(AddrSink { st, tys, out, indent }, dst, elems).map(Some)
        }
        Op::ListGetScalar { dst, list, idx } if matches!(ty(tys, list), Some(NTy::Rec(_) | NTy::RecRef(_))) => {
            rec_scalar_get(AddrSink { st, tys, out, indent }, dst, list, idx).map(Some)
        }
        Op::ListSetScalar { list, idx, val } if matches!(ty(tys, list), Some(NTy::Rec(_) | NTy::RecRef(_))) => {
            rec_scalar_set(AddrSink { st, tys, out, indent }, list, idx, val).map(Some)
        }
        _ => Ok(None),
    }
}

/// `ListGetScalar` of an enum: its tag or a scalar payload at a constant slot.
fn enum_scalar_get(s: AddrSink<'_>, dst: &ValueId, list: &ValueId, idx: &ValueId) -> Result<bool, LowerError> {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let (Some(i), Some(&k)) = (is_enum(ty(tys, list)), st.consts.get(idx)) else {
        return Err(wall("native: an enum slot read at a computed slot"));
    };
    let (acc, _) = enum_read(i, k as usize, true).ok_or_else(|| wall("native: a scalar read of a heap enum slot"))?;
    tys.insert(*dst, NTy::I64);
    line!("let mut {}: i64 = {}.{acc};", var(*dst), var(*list));
    Ok(true)
}

/// The all-scalar struct literal arm of [`struct_slot_op`].
fn scalar_struct_lit(s: AddrSink<'_>, dst: &ValueId, elems: &[ValueId]) -> Result<bool, LowerError> {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
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
    Ok(true)
}

/// A scalar-only enum constructor built from its slot list.
fn enum_slot_lit(s: AddrSink<'_>, dst: &ValueId, elems: &[ValueId]) -> Result<bool, LowerError> {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let i = st.alloc_rec[dst];
    if elems.iter().any(|e| !matches!(ty(tys, e), Some(NTy::I64 | NTy::F64))) {
        return Err(wall("native: a heap element in an enum slot list"));
    }
    tys.insert(*dst, NTy::Rec(i));
    let n = crate::render_native_rec::struct_name(i);
    // A constant tag naming a payload-free case is that case: `from_slots` builds `with_tag(t)` and sets
    // no field of it, through a slot list and a tag match (4% on nested_variant_tag_box).
    let cases = crate::render_native_rec::enum_cases(i).unwrap_or_default();
    let tag = elems.first().filter(|t| !st.set_locals.contains(t)).and_then(|t| st.consts.get(t)).copied();
    let nullary = tag.filter(|&t| cases.iter().any(|(c, fs)| i64::from(*c) == t && fs.is_empty()));
    if let Some(t) = nullary {
        line!("let mut {}: {n} = {n}::C{t};", var(*dst));
        return Ok(true);
    }
    if cases.iter().filter(|(t, _)| tag.is_none_or(|v| v == i64::from(*t)))
        .any(|(_, fs)| fs.iter().any(|f| !crate::render_native_rec::has_default(*f))) {
        return Err(wall("native: scalar enum slots require defaultable payloads"));
    }
    // MIR's scalar slots carry Float bits, just as scalar_struct_lit does.
    // Preserve NaN payloads and signed zero; numeric `as i64` would lose both.
    let items = elems.iter().map(|e| match ty(tys, e) {
        Some(NTy::F64) => format!("{}.to_bits() as i64", var(*e)),
        _ => var(*e),
    }).collect::<Vec<_>>().join(", ");
    line!("let mut {}: {n} = {n}::from_slots(&[{items}]);", var(*dst));
    Ok(true)
}

/// `ListGetScalar` of a record: a scalar field at a constant slot.
fn rec_scalar_get(s: AddrSink<'_>, dst: &ValueId, list: &ValueId, idx: &ValueId) -> Result<bool, LowerError> {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let (Some(NTy::Rec(i) | NTy::RecRef(i)), Some(&k)) = (ty(tys, list), st.consts.get(idx)) else {
        return Err(wall("native: a record field read at a computed slot"));
    };
    if crate::render_native_rec::rec_field(i, k as usize) != Some(NTy::I64) {
        return Err(wall("native: a scalar read of a heap record field"));
    }
    tys.insert(*dst, NTy::I64);
    line!("let mut {}: i64 = {}.f{k};", var(*dst), var(*list));
    Ok(true)
}

/// `ListSetScalar` of an owned record: a scalar field at a constant slot.
fn rec_scalar_set(s: AddrSink<'_>, list: &ValueId, idx: &ValueId, val: &ValueId) -> Result<bool, LowerError> {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let (Some(NTy::Rec(i)), Some(&k)) = (ty(tys, list), st.consts.get(idx)) else {
        return Err(wall("native: a record field write on a borrowed record or at a computed slot"));
    };
    if crate::render_native_rec::rec_field(i, k as usize) != Some(NTy::I64) || ty(tys, val) != Some(NTy::I64) {
        return Err(wall("native: a scalar write of a heap record field"));
    }
    line!("{}.f{k} = {};", var(*list), var(*val));
    Ok(true)
}
