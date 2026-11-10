//! Typed reads that reach rendering after the earlier scalar Result rewrite.
use super::*;

pub(super) fn render(
    op: &Op,
    st: &AddrState,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Option<Result<bool, LowerError>> {
    let Op::Prim {
        kind,
        dst: Some(dst),
        args,
    } = op
    else {
        return None;
    };
    let [address] = args.as_slice() else {
        return None;
    };
    let ad = *st.addrs.get(address)?;
    if ad.idx.is_some() {
        return None;
    }
    let native_kind = match (tys.get(&ad.base)?, kind, ad.off) {
        (NTy::Res, PrimKind::Load { width: 4 }, 4)
        | (NTy::ResStr, PrimKind::Load { width: 4 }, 16) => PrimKind::ResTag,
        (NTy::Res, PrimKind::Load { width: 8 }, 12) => PrimKind::ResOkScalar,
        (NTy::Res | NTy::ResStr, PrimKind::LoadHandle, 12) => PrimKind::ResErrStr,
        _ => return None,
    };
    let scalar = Op::Prim {
        kind: native_kind,
        dst: Some(*dst),
        args: vec![ad.base],
    };
    Some(crate::render_native_op_families::render_native_result_op(
        &scalar,
        crate::render_native::OpSink {
            tys,
            out,
            indent,
            used_shims: &mut Default::default(),
        },
    ))
}
