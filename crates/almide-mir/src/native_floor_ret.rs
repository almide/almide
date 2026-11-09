// the floor call's result renders of native_floor_tail.rs, include!-spliced back at module level (a code move under the
// 800-line ceiling the codopsy gate holds this crate to).


/// The call's result expression and the NTy it binds as.
fn render_ret(r: FloorRet, call: String, cx: &CallCx, dst_map: Option<u16>) -> Result<(String, Option<NTy>), LowerError> {
    match r {
        FloorRet::Runtime(t) => Ok((call, Some(t.kind()))),
        FloorRet::RuntimeResult(t, optional) => {
            let result = crate::render_native_rec::result_type_of(t.kind(), NTy::Str);
            if optional {
                let kind = crate::render_native_rec::option_type_of(NTy::Rec(result));
                Ok((format!("({call}).map(|r| r.map_err(AlmideString::from))"), Some(NTy::Rec(kind))))
            } else {
                Ok((format!("({call}).map_err(AlmideString::from)"), Some(NTy::Rec(result))))
            }
        }
        FloorRet::ResProcessStatus | FloorRet::ResBytesStrInt => aggregate_host_ret(r, call, dst_map),
        FloorRet::Map | FloorRet::MapOptVal | FloorRet::MapKeys | FloorRet::MapVals | FloorRet::MapEntries => map_ret_of(r, call, cx, dst_map),
        FloorRet::Elem(shape) => elem_ret_of(shape, cx.name, call, cx),
        FloorRet::Matrix | FloorRet::Set | FloorRet::SetElems | FloorRet::OptMatrix | FloorRet::ListMatrix | FloorRet::FloatRows => {
            set_matrix_ret_of(r, call, cx, dst_map)
        }
        FloorRet::Res(_)
        | FloorRet::ResStr
        | FloorRet::ResValue
        | FloorRet::OptStr
        | FloorRet::ResListValue
        | FloorRet::ResStrValuePairs
        | FloorRet::ResListStr
        | FloorRet::ResBytes
        | FloorRet::ResOptStr | FloorRet::ResOptBytes | FloorRet::ResOptListStr | FloorRet::ResFileStat => Ok(carrier_ret_of(r, call)),
        _ => Ok(plain_ret_of(r, call)),
    }
}

/// [`render_ret`] for a scalar, String, list, Bytes, Value or scalar-Option result.
fn plain_ret_of(r: FloorRet, call: String) -> (String, Option<NTy>) {
    match r {
        FloorRet::Unit => (call, None),
        FloorRet::Int => (call, Some(NTy::I64)),
        FloorRet::Bool => (format!("({call}) as i64"), Some(NTy::I64)),
        FloorRet::Float => (call, Some(NTy::F64)),
        FloorRet::Str => (format!("AlmideString::from({call})"), Some(NTy::Str)),
        FloorRet::ListInt => (call, Some(NTy::Vec)),
        FloorRet::Bytes => (call, Some(NTy::Rec(crate::render_native_rec::bytes_index()))),
        FloorRet::Value => (call, Some(NTy::Rec(crate::render_native_rec::value_index()))),
        FloorRet::ListStr => (
            format!("crate::almide_str_block::almide_str_list({call})"),
            Some(NTy::VecStr),
        ),
        FloorRet::Opt(Scalar::Int) => (call, Some(NTy::Opt)),
        FloorRet::Opt(Scalar::Bool) => (format!("({call}).map(|b| b as i64)"), Some(NTy::Opt)),
        FloorRet::Opt(Scalar::Float) => (format!("({call}).map(|x| x.to_bits() as i64)"), Some(NTy::Opt)),
        _ => unreachable!("render_ret routes every other result kind elsewhere"),
    }
}

/// [`render_ret`] for a Result or Option-of-String carrier result.
fn carrier_ret_of(r: FloorRet, call: String) -> (String, Option<NTy>) {
    match r {
        FloorRet::Res(k) => {
            let ok = match k {
                None => "|_| 0i64",
                Some(Scalar::Int) => "|v| v",
                Some(Scalar::Bool) => "|v| v as i64",
                Some(Scalar::Float) => "|v| v.to_bits() as i64",
            };
            (format!("({call}).map({ok}).map_err(AlmideString::from)"), Some(NTy::Res))
        }
        FloorRet::ResStr => (
            format!("({call}).map(AlmideString::from).map_err(AlmideString::from)"),
            Some(NTy::ResStr),
        ),
        FloorRet::ResValue => {
            let v = NTy::Rec(crate::render_native_rec::value_index());
            let r = crate::render_native_rec::result_type_of(v, NTy::Str);
            (format!("({call}).map_err(AlmideString::from)"), Some(NTy::Rec(r)))
        }
        FloorRet::OptStr => (
            format!("({call}).map(AlmideString::from)"),
            Some(NTy::Rec(crate::render_native_rec::option_type_of(NTy::Str))),
        ),
        FloorRet::ResListStr => (
            format!("({call}).map(crate::almide_str_block::almide_str_list).map_err(AlmideString::from)"),
            Some(NTy::Rec(crate::render_native_rec::result_type_of(NTy::VecStr, NTy::Str))),
        ),
        FloorRet::ResStrValuePairs => {
            let fields = [NTy::Str, NTy::Rec(crate::render_native_rec::value_index())];
            let ty = almide_lang::types::Ty::Tuple(vec![almide_lang::types::Ty::String,
                almide_lang::types::Ty::Named(almide_base::intern::sym("Value"), Default::default())].into());
            let pair = crate::render_native_rec::rec_index(&crate::render_native_rec::tuple_key(&ty))
                .unwrap_or_else(|| crate::render_native_rec::slots_type_of(&fields));
            let pair_name = crate::render_native_rec::struct_name(pair);
            let list = NTy::Rec(crate::render_native_rec::list_type_of(NTy::Rec(pair)));
            let result = crate::render_native_rec::result_type_of(list, NTy::Str);
            (format!("({call}).map(|xs| xs.into_iter().map(|(k, v)| {pair_name} {{ f0: k, f1: v }}).collect::<Vec<{pair_name}>>()).map_err(AlmideString::from)"), Some(NTy::Rec(result)))
        }
        FloorRet::ResListValue => {
            let l = NTy::Rec(crate::render_native_rec::list_type_of(NTy::Rec(crate::render_native_rec::value_index())));
            (format!("({call}).map_err(AlmideString::from)"), Some(NTy::Rec(crate::render_native_rec::result_type_of(l, NTy::Str))))
        }
        FloorRet::ResBytes => {
            let b = NTy::Rec(crate::render_native_rec::bytes_index());
            (format!("({call}).map_err(AlmideString::from)"), Some(NTy::Rec(crate::render_native_rec::result_type_of(b, NTy::Str))))
        }
        FloorRet::ResOptListStr => {
            let o = NTy::Rec(crate::render_native_rec::option_type_of(NTy::VecStr));
            (format!("({call}).map(|o| o.map(|xs| xs.into_iter().map(AlmideString::from).collect::<Vec<AlmideString>>())).map_err(AlmideString::from)"),
                Some(NTy::Rec(crate::render_native_rec::result_type_of(o, NTy::Str))))
        }
        FloorRet::ResFileStat => (
            format!("({call}).map(|s| vec![s.size, s.is_dir as i64, s.is_file as i64, s.modified]).map_err(AlmideString::from)"),
            Some(NTy::Rec(crate::render_native_rec::result_type_of(NTy::Vec, NTy::Str))),
        ),
        FloorRet::ResOptBytes => {
            let b = NTy::Rec(crate::render_native_rec::bytes_index());
            let o = NTy::Rec(crate::render_native_rec::option_type_of(b));
            (format!("({call}).map_err(AlmideString::from)"),
                Some(NTy::Rec(crate::render_native_rec::result_type_of(o, NTy::Str))))
        }
        FloorRet::ResOptStr => {
            let o = NTy::Rec(crate::render_native_rec::option_type_of(NTy::Str));
            (
                format!("({call}).map(|o| o.map(AlmideString::from)).map_err(AlmideString::from)"),
                Some(NTy::Rec(crate::render_native_rec::result_type_of(o, NTy::Str))),
            )
        }
        _ => unreachable!("render_ret routes only carrier results here"),
    }
}

/// [`render_ret`] for a Matrix or Set result, or one derived from them.
fn set_matrix_ret_of(r: FloorRet, call: String, cx: &CallCx, dst_map: Option<u16>) -> Result<(String, Option<NTy>), LowerError> {
    let mismatch = cx.mismatch;
    let (name, set_e) = (cx.name, cx.set_e);
    Ok(match r {
        FloorRet::Matrix => (call, Some(NTy::Rec(crate::render_native_rec::matrix_index()))),
        FloorRet::Set => match (set_e, dst_map) {
            (Some(e), _) => (call, Some(NTy::Rec(crate::render_native_rec::set_type_of(e)))),
            // No set arg (`set.new()`, `set.from_list(xs)`): the result's
            // declared set type, whose elements the list arg must be.
            (None, Some(i)) if crate::render_native_rec::set_elem(i).is_some() => {
                let e = crate::render_native_rec::set_elem(i).ok_or_else(mismatch)?;
                let list_ok = set_list_agrees(cx.params, cx.args, e);
                if !list_ok {
                    return Err(wall(format!("native: floor call `{name}` set element {e:?} disagrees with list arguments {:?}", cx.args)));
                }
                (call, Some(NTy::Rec(i)))
            }
            // Or the type the self-hosted body this call specializes declares.
            _ => match specialized_ret(name).as_ref().and_then(|t| declared_kinds(t, "Set")) {
                Some(e) if e.len() == 1 && crate::render_native_rec::native_set_element_kind(e[0]) && set_list_agrees(cx.params, cx.args, e[0]) => {
                    (call, Some(NTy::Rec(crate::render_native_rec::set_type_of(e[0]))))
                }
                _ => return Err(mismatch()),
            },
        },
        FloorRet::SetElems => match set_e.ok_or_else(mismatch)? {
            NTy::F32 => (format!("({call}).into_iter().map(|x| x.to_bits() as i64).collect::<Vec<_>>()"), Some(NTy::Vec)),
            NTy::I64 => (call, Some(NTy::Vec)),
            NTy::Str => (call, Some(NTy::VecStr)),
            e if crate::render_native_rec::native_set_element_kind(e) => (call, Some(NTy::Rec(crate::render_native_rec::list_type_of(e)))),
            _ => return Err(mismatch()),
        },
        FloorRet::OptMatrix => {
            let m = NTy::Rec(crate::render_native_rec::matrix_index());
            (call, Some(NTy::Rec(crate::render_native_rec::option_type_of(m))))
        }
        FloorRet::ListMatrix => {
            let m = NTy::Rec(crate::render_native_rec::matrix_index());
            (call, Some(NTy::Rec(crate::render_native_rec::list_type_of(m))))
        }
        FloorRet::FloatRows => (
            format!("({call}).into_iter().map(|r| r.into_iter().map(|x| x.to_bits() as i64).collect::<Vec<i64>>()).collect::<Vec<Vec<i64>>>()"),
            Some(NTy::Rec(crate::render_native_rec::list_type_of(NTy::Vec))),
        ),
        _ => unreachable!("render_ret routes only Set and Matrix results here"),
    })
}

/// [`render_ret`] for a result derived from the call's map.
fn map_ret_of(r: FloorRet, call: String, cx: &CallCx, dst_map: Option<u16>) -> Result<(String, Option<NTy>), LowerError> {
    let mismatch = cx.mismatch;
    Ok(match r {
        FloorRet::Map => match (cx.map_kv, dst_map) {
            (Some((k, v)), _) => (call, Some(NTy::Rec(crate::render_native_rec::map_type_of(k, v)))),
            // No map arg (`map.new()`): the result's declared map type.
            (None, Some(i)) => (call, Some(NTy::Rec(i))),
            // Or the type the self-hosted body this call specializes declares
            // (`map_new_skv() -> Map[String, Int]`).
            (None, None) => match specialized_ret(cx.name).as_ref().and_then(|t| declared_kinds(t, "Map")) {
                Some(kv) if kv.len() == 2 => (call, Some(NTy::Rec(crate::render_native_rec::map_type_of(kv[0], kv[1])))),
                _ => return Err(mismatch()),
            },
        },
        FloorRet::MapOptVal => match cx.map_kv.ok_or_else(mismatch)?.1 {
            NTy::F32 => (format!("({call}).map(|x| x.to_bits() as i64)"), Some(NTy::Opt)),
            NTy::I64 => (call, Some(NTy::Opt)),
            v => (call, Some(NTy::Rec(crate::render_native_rec::option_type_of(v)))),
        },
        FloorRet::MapEntries => {
            let (k, v) = cx.map_kv.ok_or_else(mismatch)?;
            let lane = |t| if t == NTy::F32 { NTy::I64 } else { t };
            let recode = |code: &str, ty| if ty == NTy::F32 { format!("({code}).to_bits() as i64") } else { code.to_string() };
            let (key, value) = (recode("k", k), recode("v", v));
            let (k, v) = (lane(k), lane(v));
            // The generated list of the program's `(K, V)` tuple, as the
            // self-hosted body this call specializes declares it.
            // A generic runtime route may have no specialized return type.
            // Reuse a checked structural tuple with these field kinds before
            // falling back to an untyped slot struct.
            let list = specialized_ret(cx.name)
                .as_ref()
                .and_then(ty_of)
                .and_then(|t| crate::render_native_rec::rec_index(&crate::render_native_rec::tuple_key(&t)))
                .filter(|list| matches!(crate::render_native_rec::list_elem(*list), Some(NTy::Rec(pair))
                    if crate::render_native_rec::field_count(pair) == 2
                        && crate::render_native_rec::rec_field(pair, 0) == Some(k)
                        && crate::render_native_rec::rec_field(pair, 1) == Some(v)))
                .unwrap_or_else(|| {
                    crate::render_native_rec::list_type_of(NTy::Rec(crate::render_native_rec::tuple_type_of(&[k, v])))
                });
            let Some(NTy::Rec(pair)) = crate::render_native_rec::list_elem(list) else { return Err(mismatch()) };
            let fields_agree = crate::render_native_rec::field_count(pair) == 2
                && crate::render_native_rec::rec_field(pair, 0) == Some(k)
                && crate::render_native_rec::rec_field(pair, 1) == Some(v);
            if !fields_agree {
                return Err(mismatch());
            }
            let s = crate::render_native_rec::struct_name(pair);
            (format!("({call}).into_iter().map(|(k, v)| {s} {{ f0: {key}, f1: {value} }}).collect::<Vec<{s}>>()"), Some(NTy::Rec(list)))
        }
        _ => {
            let (k, v) = cx.map_kv.ok_or_else(mismatch)?;
            let e = if r == FloorRet::MapKeys { k } else { v };
            if e == NTy::F32 { return Ok((format!("({call}).into_iter().map(|x| x.to_bits() as i64).collect::<Vec<_>>()"), Some(NTy::Vec))); }
            let t = match e {
                NTy::I64 => NTy::Vec,
                NTy::Str => NTy::VecStr,
                other => NTy::Rec(crate::render_native_rec::list_type_of(other)),
            };
            (call, Some(t))
        }
    })
}

/// [`render_ret`] for an element-typed result.
fn elem_ret_of(shape: ElemRet, name: &str, call: String, cx: &CallCx) -> Result<(String, Option<NTy>), LowerError> {
    let (elem_str, elem_kind) = (cx.elem_str, cx.elem_kind);
    Ok(match (elem_str, shape) {
        // A generated list's element: itself, its generated `Option`, its list.
        (Some(false), _) if matches!(elem_kind, Some(NTy::Rec(_) | NTy::Vec | NTy::VecStr | NTy::Opt | NTy::Res | NTy::ResStr)) => {
            let e = elem_kind.expect("matched");
            let t = match shape {
                ElemRet::Value => e,
                ElemRet::Opt => NTy::Rec(crate::render_native_rec::option_type_of(e)),
                ElemRet::List => NTy::Rec(crate::render_native_rec::list_type_of(e)),
            };
            (call, Some(t))
        }
        (Some(false), ElemRet::Value) => (call, Some(NTy::I64)),
        (Some(false), ElemRet::Opt) => (call, Some(NTy::Opt)),
        (Some(false), ElemRet::List) => (call, Some(NTy::Vec)),
        (Some(true), ElemRet::Value) => (call, Some(NTy::Str)),
        (Some(true), ElemRet::List) => (call, Some(NTy::VecStr)),
        // An Option of a String element: the generated `Option<AlmideString>` carrier.
        (Some(true), ElemRet::Opt) => (call, Some(NTy::Rec(crate::render_native_rec::option_type_of(NTy::Str)))),
        _ => return Err(wall(format!("native: floor call `{name}` element-typed result outside the native carriers"))),
    })
}

/// Convert a host aggregate using its checked field order and the program's
/// declared record/tuple identity. The runtime owns each returned field.
fn aggregate_host_ret(r: FloorRet, call: String, declared: Option<u16>) -> Result<(String, Option<NTy>), LowerError> {
    use almide_lang::types::Ty;
    use almide_base::intern::sym;
    use crate::render_native_rec as rec;
    let (ty, fields, values) = match r {
        FloorRet::ResProcessStatus => (
            Ty::Record { fields: vec![(sym("code"), Ty::Int), (sym("stdout"), Ty::String), (sym("stderr"), Ty::String)] },
            [NTy::I64, NTy::Str, NTy::Str], ["s.code", "s.stdout", "s.stderr"],
        ),
        FloorRet::ResBytesStrInt => (
            Ty::Tuple(vec![Ty::Bytes, Ty::String, Ty::Int].into()),
            [NTy::Rec(rec::bytes_index()), NTy::Str, NTy::I64], ["s.0", "s.1", "s.2"],
        ),
        _ => unreachable!("only aggregate host results"),
    };
    let record = declared.and_then(rec::result_payloads)
        .and_then(|(ok, err)| if err == NTy::Str { ok.rec() } else { None })
        .or_else(|| rec::rec_index(&rec::block_key(&ty)))
        .ok_or_else(|| wall(format!("native: host result has no declared aggregate {ty:?}")))?;
    if rec::field_count(record) != fields.len()
        || fields.iter().enumerate().any(|(i, k)| rec::rec_field(record, i) != Some(*k)) {
        return Err(wall("native: host result aggregate fields disagree with runtime ABI"));
    }
    if r == FloorRet::ResProcessStatus && !rec::record_fields(record).is_some_and(|(_, fields)|
        fields.iter().map(|(name, _)| name.as_str()).eq(["code", "stdout", "stderr"])) {
        return Err(wall("native: host status field labels disagree with its bundled declaration"));
    }
    let result = rec::result_type_of(NTy::Rec(record), NTy::Str);
    let name = rec::struct_name(record);
    Ok((format!("({call}).map(|s| {name} {{ f0: {}, f1: {}, f2: {} }}).map_err(AlmideString::from)", values[0], values[1], values[2]), Some(NTy::Rec(result))))
}
