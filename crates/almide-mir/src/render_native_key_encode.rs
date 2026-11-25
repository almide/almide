// Hashable keys for generated types (#4787). Included in render_native_rec.
//
// A Set element or a Map key of a generated type (a record, a tuple, a
// variant, or a list / option / result of one) used to stay on the runtime's
// linear scan: the runtime hashes only the key types it can name. Here the
// render gives each such type its canonical key bytes (`AlmideKeyEncode`,
// runtime map.rs) and registers the key types at the start of `main`, so the
// runtime indexes them like an Int or String key.
//
// A type qualifies only when its `==` is the derived field-by-field one over
// exact source equality (`eq_safe`, no hand-written `eq_fields` impl) and
// every field's own type qualifies — so two equal values always write equal
// bytes. Anything else (a Float field, a closure, a Map or Set field, a region
// twin, a Value) keeps the linear scan: the decision is the type's, never a
// name's, and an unknown type is never hashed.

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
    d.eq_safe
        && d.eq_fields.is_none()
        && !is_region(i)
        && match &d.cases {
            Some(cases) => cases.iter().all(|(_, fs)| fs.iter().all(|f| key_field_ok(*f, seen))),
            None => d.fields.iter().all(|f| key_field_ok(*f, seen)),
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
                        for b in &binds {
                            out.push_str(&format!("                {b}.almide_key_encode(out);\n"));
                        }
                        out.push_str("            }\n");
                    }
                }
                out.push_str("        }\n");
            }
            None => {
                for k in 0..d.fields.len() {
                    out.push_str(&format!("        self.f{k}.almide_key_encode(out);\n"));
                }
            }
        }
        out.push_str("    }\n}\n\n");
    }
    out.push_str("fn almide_register_key_types() {\n");
    for i in &roots {
        out.push_str(&format!("    almide_map_register_key::<{}>();\n", struct_name(*i)));
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
