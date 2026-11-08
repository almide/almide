// Typed reads from symbolic native slot addresses. Included by render_native_addr_op.rs.

/// `Prim{Load}` at an address: a list's length, an enum's tag or scalar
/// payload, a record's scalar field, or a scalar list's element.
fn slot_load(s: AddrSink<'_>, width: &u8, d: &ValueId, args: &[ValueId]) -> Result<bool, LowerError> {
    let AddrSink { st, tys, out, indent } = s;
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let ad = st.addrs[&args[0]];
    let bt = ty(tys, &ad.base);
    if bt == Some(NTy::Opt) && ad.idx.is_none() {
        // Reads of a scalar Option extracted from a typed container reach
        // rendering after the earlier Option rewrite. Use the same exact
        // layout windows and the same native primitives, without rewriting
        // the MIR ownership events of that borrowed container payload.
        let op = crate::native_option_rewrite::load_window(
            &PrimKind::Load { width: *width }, ad.off, *d, ad.base,
        ).ok_or_else(|| wall("native: an invalid scalar Option slot read"))?;
        return crate::render_native_op_families::render_option_carrier_op(
            &op, crate::render_native::OpSink { tys, out, indent, used_shims: &mut Default::default() },
        );
    }
    if *width == 4 && ad.off == i64::from(BLOCK_LEN_OFFSET) && ad.idx.is_none()
        && bt.is_some_and(|t| is_list_block(t) || crate::render_native_rec::is_bytes(t) || matches!(t, NTy::Str | NTy::StrRef)) {
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
            let (guard, slot) = bounds_guard(st, &ad, &ix);
            if let Some(g) = guard {
                line!("{g}");
            }
            crate::render_native_bounds::access(slot);
            tys.insert(*d, NTy::I64);
            line!("let mut {}: i64 = {}[{ix}];", var(*d), var(ad.base));
            return Ok(true);
        }
    }
    Err(wall(format!("native: Load{{{width}}} at {ad:?} — outside the slot algebra")))
}

