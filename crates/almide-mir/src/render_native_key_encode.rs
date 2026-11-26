// Hashable keys for generated types (#4787). Included in render_native_rec.
//
// A Set element or a Map key of a generated type (a record, a tuple, a
// variant, or a list / option / result of one) used to stay on the runtime's
// linear scan: the runtime hashes only the key types it can name. Here the
// render gives each such type its canonical key bytes (`AlmideKeyEncode`,
// runtime map.rs) and registers the key types at the start of `main`, so the
// runtime indexes them like an Int or String key.
//
// A type qualifies when its `==` is the derived field-by-field one over
// exact source equality (`eq_safe`) and every field's own type qualifies — so
// two equal values always write equal bytes — or when it compares its Float
// fields per field (`eq_fields`, #4687): those fields write their canonical
// bits (every NaN one value, -0.0 as 0.0), so the bytes are one-to-one under
// the collection key equality (ALS-C11), and the type registers as a Float
// key, compared by its bytes where `==` says no. Anything else (a closure, a
// Map or Set field, a region twin, a Value) keeps the linear scan: the
// decision is the type's, never a name's, and an unknown type is never hashed.

/// The marker line in `main` that becomes the key-type registration call
/// (or nothing) once every generated type is known.
pub(crate) const KEY_TYPES_MARKER: &str = "    //__ALMIDE_KEY_TYPES__\n";

/// The scalar and runtime-container field kinds with canonical key bytes
/// (runtime map.rs `AlmideKeyEncode`). Every other kind (a Float, a closure,
/// a Value) has none; the checker refuses a Float Map key (E058) and a Set
/// of one walls before a key is chosen, so the table answers it as a value.
const KEY_FIELD_KINDS: [NTy; 8] = [NTy::I64, NTy::Bool, NTy::Str, NTy::Vec, NTy::VecStr, NTy::Opt, NTy::Res, NTy::ResStr];

/// Whether a field of native kind `t` has canonical key bytes.
fn key_field_ok(t: NTy, seen: &mut std::collections::BTreeSet<u16>) -> bool {
    match t {
        NTy::Rec(j) => key_type_ok(j, seen),
        other => KEY_FIELD_KINDS.contains(&other),
    }
}

/// Whether generated type `i` has canonical key bytes consistent with its `==`.
fn key_type_ok(i: u16, seen: &mut std::collections::BTreeSet<u16>) -> bool {
    if !seen.insert(i) {
        // A recursive type: its own fields decide (greatest fixpoint).
        return true;
    }
    // `NTy::Rec(i)` always names a generated type, as in `key_impl_types`.
    let d = RECS.with(|r| r.borrow()[i as usize].clone());
    if d.alias.is_some() {
        return d.alias == Some("AlmideList<u8>");
    }
    if d.map_of.is_some() || d.set_of.is_some() || d.closure_of.is_some() || d.cell_of.is_some() {
        return false;
    }
    if let Some(p) = d.option_of {
        return key_field_ok(p, seen);
    }
    if let Some((o, e)) = d.result_of {
        return key_field_ok(o, seen) && key_field_ok(e, seen);
    }
    if let Some(e) = d.list_of {
        return key_field_ok(e, seen);
    }
    if is_region(i) {
        return false;
    }
    let kinds: Vec<&[NTy]> = match &d.cases {
        Some(cases) => cases.iter().map(|(_, fs)| fs.as_slice()).collect(),
        None => vec![d.fields.as_slice()],
    };
    match &d.eq_fields {
        // A Float lane encodes canonically; every other field qualifies itself.
        Some(eqs) => kinds.iter().zip(eqs).all(|(ks, es)| {
            ks.iter().zip(es).all(|(k, e)| *e != FieldEq::Exact || matches!(k, NTy::F64 | NTy::F32) || key_field_ok(*k, seen))
        }),
        None => d.eq_safe && kinds.iter().all(|ks| ks.iter().all(|f| key_field_ok(*f, seen))),
    }
}

/// Whether generated type `i`'s key bytes encode a Float (ALS-C11): its own
/// per-field Float comparison, or a field type's.
fn key_type_floaty(i: u16, seen: &mut std::collections::BTreeSet<u16>) -> bool {
    if !seen.insert(i) {
        return false;
    }
    let d = RECS.with(|r| r.borrow()[i as usize].clone());
    if d.eq_fields.as_ref().is_some_and(|eqs| eqs.iter().flatten().any(|e| *e != FieldEq::Exact)) {
        return true;
    }
    let mut fields: Vec<NTy> = d.fields.clone();
    if let Some(cases) = &d.cases {
        fields.extend(cases.iter().flat_map(|(_, fs)| fs.iter().copied()));
    }
    fields.extend(d.option_of);
    fields.extend(d.list_of);
    if let Some((o, e)) = d.result_of {
        fields.extend([o, e]);
    }
    // A real `f64` / `f32` field (the runtime's canonical impl) is a Float too.
    fields.into_iter().any(|f| matches!(f, NTy::F64 | NTy::F32) || matches!(f, NTy::Rec(j) if key_type_floaty(j, seen)))
}

/// One field's key encoding: an exact field writes its own bytes, a Float
/// lane its canonical bits (ALS-C11). `x` is the field place, `deref` turns a
/// bound reference into the lane value.
fn key_field_encode(e: FieldEq, x: &str, deref: &str) -> String {
    match e {
        FieldEq::Exact => format!("{x}.almide_key_encode(out);"),
        FieldEq::F64Bits => format!("almide_key_f64_bits(f64::from_bits({deref}{x} as u64)).almide_key_encode(out);"),
        FieldEq::F32Bits => format!("almide_key_f32_bits(f32::from_bits({deref}{x} as u32)).almide_key_encode(out);"),
        FieldEq::VecF64Bits => format!(
            "out.extend_from_slice(&({x}.len() as u64).to_le_bytes()); for y in {x}.iter() {{ almide_key_f64_bits(f64::from_bits(*y as u64)).almide_key_encode(out); }}"
        ),
        FieldEq::VecF32Bits => format!(
            "out.extend_from_slice(&({x}.len() as u64).to_le_bytes()); for y in {x}.iter() {{ almide_key_f32_bits(f32::from_bits(*y as u32)).almide_key_encode(out); }}"
        ),
    }
}

/// The struct and enum types that need an `AlmideKeyEncode` impl: every key
/// type and the generated types its fields reach.
fn key_impl_types(roots: &[u16]) -> std::collections::BTreeSet<u16> {
    let mut out = std::collections::BTreeSet::new();
    let mut work: Vec<u16> = roots.to_vec();
    while let Some(i) = work.pop() {
        if !out.insert(i) {
            continue;
        }
        let d = RECS.with(|r| r.borrow()[i as usize].clone());
        let mut fields: Vec<NTy> = d.fields.clone();
        if let Some(cases) = &d.cases {
            fields.extend(cases.iter().flat_map(|(_, fs)| fs.iter().copied()));
        }
        fields.extend(d.option_of);
        fields.extend(d.list_of);
        if let Some((o, e)) = d.result_of {
            fields.extend([o, e]);
        }
        work.extend(fields.into_iter().filter_map(|f| if let NTy::Rec(j) = f { Some(j) } else { None }));
    }
    out
}

/// The key types a Set or Map of the program indexes: each `Set[A]` element
/// and `Map[K, V]` key of a generated type that qualifies.
fn key_roots() -> Vec<u16> {
    let decls = RECS.with(|r| r.borrow().clone());
    let mut roots: Vec<u16> = decls
        .iter()
        .filter_map(|d| d.set_of.or(d.map_of.map(|(k, _)| k)))
        .filter_map(|t| if let NTy::Rec(j) = t { Some(j) } else { None })
        .filter(|j| key_type_ok(*j, &mut Default::default()))
        .collect();
    roots.sort_unstable();
    roots.dedup();
    roots
}

/// The `AlmideKeyEncode` impls and the registration fn, or "" when no Set
/// or Map of the program has a generated key type.
pub(crate) fn key_encode_defs() -> String {
    let roots = key_roots();
    if roots.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for i in key_impl_types(&roots) {
        let d = RECS.with(|r| r.borrow()[i as usize].clone());
        // Aliases (Bytes, a list / option / result of a generated type) are
        // covered by the runtime's impls for Vec, Option and Result.
        if d.alias.is_some() || d.option_of.is_some() || d.result_of.is_some() || d.list_of.is_some() {
            continue;
        }
        let n = struct_name(i);
        out.push_str(&format!("impl AlmideKeyEncode for {n} {{\n    fn almide_key_encode(&self, out: &mut std::vec::Vec<u8>) {{\n"));
        match &d.cases {
            Some(cases) => {
                out.push_str("        match self {\n");
                for (t, fs) in cases {
                    if fs.is_empty() {
                        out.push_str(&format!("            {n}::C{t} => out.extend_from_slice(&{t}u32.to_le_bytes()),\n"));
                    } else {
                        let binds: Vec<String> = (0..fs.len()).map(|k| format!("x{k}")).collect();
                        out.push_str(&format!("            {n}::C{t}({}) => {{\n                out.extend_from_slice(&{t}u32.to_le_bytes());\n", binds.join(", ")));
                        let eqs = case_field_eqs(&d, cases, *t, binds.len());
                        for (b, e) in binds.iter().zip(eqs) {
                            out.push_str(&format!("                {}\n", key_field_encode(e, b, "*")));
                        }
                        out.push_str("            }\n");
                    }
                }
                out.push_str("        }\n");
            }
            None => {
                let eqs: Vec<FieldEq> = d.eq_fields.as_ref().map(|e| e[0].clone()).unwrap_or_else(|| vec![FieldEq::Exact; d.fields.len()]);
                for (k, e) in eqs.into_iter().enumerate() {
                    out.push_str(&format!("        {}\n", key_field_encode(e, &format!("self.f{k}"), "")));
                }
            }
        }
        out.push_str("    }\n}\n\n");
    }
    out.push_str("fn almide_register_key_types() {\n");
    for i in &roots {
        // A Float-holding key compares by its canonical bytes (ALS-C11).
        let register = if key_type_floaty(*i, &mut Default::default()) { "almide_map_register_float_key" } else { "almide_map_register_key" };
        out.push_str(&format!("    {register}::<{}>();\n", struct_name(*i)));
    }
    out.push_str("}\n\n");
    out
}

/// `main`'s marker line, replaced by the registration call when the program
/// has key types, else removed.
pub(crate) fn place_key_registration(bodies: &str, defs: &str) -> String {
    let call = if defs.is_empty() { "" } else { "    almide_register_key_types();\n" };
    bodies.replacen(KEY_TYPES_MARKER, call, 1)
}

/// The per-field comparison kinds of enum case `tag` (all exact when the type
/// derives its equality).
fn case_field_eqs(d: &RecDecl, cases: &[(u32, Vec<NTy>)], tag: u32, n: usize) -> Vec<FieldEq> {
    let pos = cases.iter().position(|(t, _)| *t == tag);
    match (&d.eq_fields, pos) {
        (Some(eqs), Some(p)) => eqs[p].clone(),
        _ => vec![FieldEq::Exact; n],
    }
}
