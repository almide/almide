// A borrowed native/host value copied to the other side of the declared ABI.
// Native scalar slots can hold float bits; the value kind fixes the distinction.
fn convert(e: &str, ty: &Ty, kind: NTy, to_host: bool, depth: usize) -> Result<String, LowerError> {
    convert_value(e, ty, kind, to_host, depth, false)
}

// Callback signatures retain Almide's value types (including shared strings
// and Bytes/Matrix handles); only a direct foreign value boundary translates
// structural strings into std::String.
pub(crate) fn convert_value(e: &str, ty: &Ty, kind: NTy, to_host: bool, depth: usize, host_uses_almide_types: bool) -> Result<String, LowerError> {
    let kind = owned_nty(kind);
    if let Some(host) = scalar_host_type(ty) {
        return Ok(match (ty, kind, to_host) {
            (Ty::Bool, NTy::I64, true) => format!("(*({e}) != 0)"),
            (Ty::Bool, NTy::I64, false) => format!("(*({e}) as i64)"),
            (Ty::Float | Ty::Float64, NTy::I64, true) => format!("f64::from_bits(*({e}) as u64)"),
            (Ty::Float | Ty::Float64, NTy::I64, false) => format!("({e}).to_bits() as i64"),
            (Ty::Float32, NTy::I64, true) => format!("f32::from_bits(*({e}) as u32)"),
            (Ty::Float32, NTy::I64, false) => format!("({e}).to_bits() as i64"),
            (_, NTy::I64, true) => format!("(*({e}) as {host})"),
            (_, NTy::I64, false) => format!("(*({e}) as i64)"),
            (Ty::Float | Ty::Float64, NTy::F64, _) => format!("*({e})"),
            (Ty::Float32, NTy::F32, _) => format!("*({e})"),
            _ => return Err(wall(format!("native: foreign scalar {ty:?} disagrees with {kind:?}"))),
        });
    }
    let x = format!("__foreign_x{depth}");
    let nested = |e: &str, t: &Ty, k| convert_value(e, t, k, to_host, depth + 1, host_uses_almide_types);
    match ty {
        Ty::Unit if kind == NTy::I64 => Ok(if to_host { "()" } else { "0i64" }.into()),
        Ty::String if kind == NTy::Str => Ok(if to_host && !host_uses_almide_types {
            format!("({e}).to_string()")
        } else { format!("AlmideString::from(({e}).as_str())") }),
        Ty::Bytes if crate::render_native_rec::is_bytes(kind) => Ok(if to_host && host_uses_almide_types {
            format!("AlmideRcCow::from(({e}).to_vec())")
        } else { format!("({e}).to_vec()") }),
        Ty::Applied(C::List, a) if a.len() == 1 => {
            let k = match kind {
                NTy::Vec => Some(NTy::I64), NTy::VecStr => Some(NTy::Str),
                NTy::Rec(i) => crate::render_native_rec::list_elem(i), _ => None,
            }.ok_or_else(|| wall("native: foreign list has no declared element kind"))?;
            Ok(format!("({e}).iter().map(|{x}| {}).collect::<Vec<_>>()", nested(&x, &a[0], k)?))
        }
        Ty::Applied(C::Set, a) if a.len() == 1 => {
            let k = match kind {
                NTy::Vec => Some(NTy::I64),
                NTy::Rec(i) => crate::render_native_rec::set_elem(i), _ => None,
            }.ok_or_else(|| wall("native: foreign Set has no declared element kind"))?;
            let collection = if to_host || kind != NTy::Vec { "AlmideSet<_>" } else { "Vec<_>" };
            Ok(format!("({e}).iter().map(|{x}| {}).collect::<{collection}>()", nested(&x, &a[0], k)?))
        }
        Ty::Applied(C::Map, a) if a.len() == 2 => {
            let (k, v) = match kind {
                NTy::Rec(i) => crate::render_native_rec::map_kinds(i), _ => None,
            }.ok_or_else(|| wall("native: foreign Map has no declared key/value kinds"))?;
            let y = format!("__foreign_y{depth}");
            Ok(format!("({e}).iter().map(|({x}, {y})| ({}, {})).collect::<AlmideMap<_, _>>()",
                nested(&x, &a[0], k)?, nested(&y, &a[1], v)?))
        }
        t if crate::render_native_rec::is_matrix_ty(t) && crate::render_native_rec::is_matrix(kind) => {
            Ok(if !to_host {
                // The ABI accepts either a raw Matrix or its value handle.
                format!("{{ let __foreign_matrix: &AlmideMatrix = {e}; __foreign_matrix.clone() }}")
            } else if host_uses_almide_types {
                format!("AlmideRcCow::from((*({e})).clone())")
            } else { format!("(*({e})).clone()") })
        }
        Ty::Applied(C::Option, a) if a.len() == 1 => {
            let k = match kind { NTy::Opt => Some(NTy::I64), NTy::Rec(i) => crate::render_native_rec::option_payload(i), _ => None }
                .ok_or_else(|| wall("native: foreign Option has no declared payload"))?;
            Ok(format!("({e}).as_ref().map(|{x}| {})", nested(&x, &a[0], k)?))
        }
        Ty::Applied(C::Result, a) if a.len() == 2 => {
            let (ok, err) = match kind {
                NTy::Res => Some((NTy::I64, NTy::Str)), NTy::ResStr => Some((NTy::Str, NTy::Str)),
                NTy::Rec(i) => crate::render_native_rec::result_payloads(i), _ => None,
            }.ok_or_else(|| wall("native: foreign Result has no declared payloads"))?;
            Ok(format!("match {e} {{ Ok({x}) => Ok({}), Err({x}) => Err({}) }}", nested(&x, &a[0], ok)?, nested(&x, &a[1], err)?))
        }
        Ty::Tuple(ts) => {
            let fields = match kind {
                NTy::Vec => Some(vec![NTy::I64; ts.len()]),
                NTy::Rec(i) => crate::render_native_rec::tuple_fields(i), _ => None,
            }.filter(|ks| ks.len() == ts.len()).ok_or_else(|| wall("native: foreign tuple has no declared fields"))?;
            let mut parts = Vec::new();
            for (j, (t, k)) in ts.iter().zip(fields).enumerate() {
                let access = if !to_host { format!("&({e}).{j}") }
                    else if kind == NTy::Vec { format!("&({e})[{j}]") }
                    else { format!("&({e}).f{j}") };
                let value = nested(&access, t, k)?;
                parts.push(if !to_host && kind != NTy::Vec { format!("f{j}: {value}") } else { value });
            }
            Ok(if to_host { format!("({},)", parts.join(", ")) }
                else if let NTy::Rec(i) = kind { format!("{} {{ {} }}", crate::render_native_rec::struct_name(i), parts.join(", ")) }
                else { format!("vec![{}]", parts.join(", ")) })
        }
        Ty::Named(..) if host_uses_almide_types => crate::native_callback_types::convert(e, ty, kind, to_host),
        _ => Err(wall(format!("native: no foreign value adapter for {ty:?} / {kind:?}"))),
    }
}
