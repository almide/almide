//! Byte buffers use byte offsets, independently of the eight-byte value slots.
//! Every byte touched by a multi-byte primitive carries a bounds event.
use super::*;

pub(super) fn render(
    op: &Op,
    st: &mut AddrState,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Option<Result<bool, LowerError>> {
    let pad = "    ".repeat(indent);
    if let Op::Alloc {
        dst,
        init: Init::DynStr { len },
        ..
    } = op
    {
        let i = *st.alloc_rec.get(dst)?;
        if !crate::render_native_rec::is_bytes(NTy::Rec(i)) {
            return None;
        }
        if tys.get(len) != Some(&NTy::I64) {
            return Some(Err(wall(
                "native: byte allocation with a non-scalar length",
            )));
        }
        let (v, n) = (var(*dst), var(*len));
        writeln!(out, "{pad}let mut {v}: Vec<u8> = Vec::new();").expect("writing to a String cannot fail");
        writeln!(out, "{pad}if {n} < 0 || {v}.try_reserve_exact({n} as usize).is_err() {{ eprintln!(\"Error: out of memory\"); std::process::exit(1); }}").expect("writing to a String cannot fail");
        writeln!(out, "{pad}{v}.resize({n} as usize, 0);").expect("writing to a String cannot fail");
        tys.insert(*dst, NTy::Rec(i));
        return Some(Ok(true));
    }
    let (width, dst, args) = match op {
        Op::Prim {
            kind: PrimKind::Load { width },
            dst: Some(d),
            args,
        } if args.len() == 1 => (*width, Some(*d), args),
        Op::Prim {
            kind: PrimKind::Store { width },
            dst: None,
            args,
        } if args.len() == 2 => (*width, None, args),
        _ => return None,
    };
    let ad = *st.addrs.get(&args[0])?;
    let kind = *tys.get(&ad.base)?;
    if !(crate::render_native_rec::is_bytes(kind) || matches!(kind, NTy::Str | NTy::StrRef))
        || ad.off < i64::from(BLOCK_HEADER)
    {
        return None;
    }
    if !matches!(width, 1 | 2 | 4 | 8) {
        return Some(Err(wall("native: byte access of unsupported width")));
    }
    if dst.is_none() && !matches!(kind, NTy::Rec(_)) {
        return Some(Err(wall("native: byte store through a borrowed buffer")));
    }
    let offset = ad.off - i64::from(BLOCK_HEADER);
    let idx = ad.idx.map(|i| match i {
        Idx::Bytes(v) | Idx::Elem(v) => v,
    });
    let index = idx
        .map(|v| format!("({} as i128)", var(v)))
        .unwrap_or_else(|| "0i128".into());
    let b = if matches!(kind, NTy::Str | NTy::StrRef) {
        format!("{}.as_bytes()", var(ad.base))
    } else {
        var(ad.base)
    };
    let at = format!("__byte_at_{}", args[0].0);
    writeln!(out, "{pad}let {at} = {index} + {offset};").expect("writing to a String cannot fail");
    writeln!(out, "{pad}if {at} < 0 || {at} > {b}.len() as i128 - {width} {{ almide_oob() }}").expect("writing to a String cannot fail");
    writeln!(out, "{pad}let {at} = {at} as usize;").expect("writing to a String cannot fail");
    let slots: Vec<_> = (0..width)
        .map(|n| crate::render_native_bounds::Slot {
            base: ad.base,
            idx,
            k: offset + i64::from(n),
        })
        .collect();
    for slot in &slots {
        crate::render_native_bounds::check(*slot);
    }
    if let Some(d) = dst {
        for slot in &slots {
            crate::render_native_bounds::access(*slot);
        }
        let bytes = (0..width)
            .map(|n| format!("{b}[{at} + {n}]"))
            .collect::<Vec<_>>()
            .join(", ");
        let expr = match width {
            1 => format!("{b}[{at}] as i64"),
            2 => format!("u16::from_le_bytes([{bytes}]) as i64"),
            4 => format!("u32::from_le_bytes([{bytes}]) as i64"),
            _ => format!("i64::from_le_bytes([{bytes}])"),
        };
        writeln!(out, "{pad}let mut {}: i64 = {expr};", var(d)).expect("writing to a String cannot fail");
        tys.insert(d, NTy::I64);
    } else {
        let value = args[1];
        let bits = match tys.get(&value) {
            Some(NTy::I64) => var(value),
            Some(NTy::F64) => format!("{}.to_bits() as i64", var(value)),
            _ => return Some(Err(wall("native: a non-scalar byte store"))),
        };
        let bytes = format!("__byte_bits_{}", args[0].0);
        writeln!(out, "{pad}let {bytes} = ({bits}).to_le_bytes();").expect("writing to a String cannot fail");
        for (n, slot) in slots.iter().enumerate() {
            crate::render_native_bounds::store(*slot);
            writeln!(out, "{pad}{b}[{at} + {n}] = {bytes}[{n}];").expect("writing to a String cannot fail");
        }
    }
    Some(Ok(true))
}

#[cfg(test)]
#[path = "render_native_byte_slots_test.rs"]
mod tests;
