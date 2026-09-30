// ── Drop / repr NAME helpers, include!-spliced at module level ──
//
// The deterministic names and shape predicates the lowering stamps into MIR
// (drop routes, rich-env tags, repr idents). The `__drop_*` / `__repr_*` source
// generators that once consumed them died with the incumbent wasm pipeline
// (#2935, #2950); what remains is only what the lowering itself reads.

/// Does a record carrying a field of type `ty` need a generated recursive `$__drop_<R>` (rather than
/// a flat one-level `rc_dec` of its block)? ANY heap field does: a flat `rc_dec` of the record block
/// frees only the block, leaking every owned heap SLOT (a `String` handle, a `List`/`Map`/`Value`
/// handle, a nested record). This was historically `false` for `String` / `List[scalar]` because the
/// DIRECT-drop path masks those slots (`record_masks` → `DropListStr`); but a record so classified
/// gets NO `$__drop_<R>`, so when it is NESTED as a field of ANOTHER recursive record the outer's
/// per-field free (`record_drop_field_frees`) has no routine to call and falls back to a flat
/// `rc_dec` that LEAKS the inner slot (the porta `Parser = { bytes: List[Int], pos: Int }` nested in
/// `{ val, next: Parser }` — its `bytes` list leaked). Generating `$__drop_<R>` for every heap-field
/// record closes that: for an already-direct-dropped record the generated body frees the SAME slots
/// as the mask (`String`/`List[scalar]` → one `rc_dec` each), so the output is byte-identical and the
/// ownership cert stays a single `d`; the only delta is that the routine now EXISTS for nesting.
pub fn record_field_needs_recursive_drop(ty: &Ty) -> bool {
    is_heap_ty(ty)
}

/// A DETERMINISTIC, host-independent synthetic type name for an ANONYMOUS record shape, used as the
/// suffix of its synthesized recursive drop `$__drop_<name>` (and the `variant_drop_handles` route).
/// FNV-1a over the ordered `(field-name, field-type-tag)` shape — the SAME shape two structurally
/// equal anon records share, so they dedup to one `__drop`. The `anonrec_` prefix keeps it disjoint
/// from any user type name. Stable across native/wasm hosts (pure arithmetic, no pointer/order deps).
pub(crate) fn anon_record_drop_name(fields: &[(almide_lang::intern::Sym, Ty)]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut mix = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    for (name, ty) in fields {
        mix(name.as_str().as_bytes());
        mix(b"\x00");
        mix(ty_shape_tag(ty).as_bytes());
        mix(b"\x00");
    }
    format!("anonrec_{h:016x}")
}

/// A structural string tag for a field type, fine enough that two anon records with DIFFERENT field
/// types (hence different drop bodies) get different names, recursing into nested aggregates so a
/// `{ st: A }` and `{ st: B }` never collide. Only the drop-relevant structure matters.
fn ty_shape_tag(ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Named(n, _) => format!("N{}", n.as_str()),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => format!("N{n}"),
        Ty::Applied(c, a) => {
            let inner: Vec<String> = a.iter().map(ty_shape_tag).collect();
            format!("A{c:?}[{}]", inner.join(","))
        }
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            let inner: Vec<String> =
                fields.iter().map(|(k, t)| format!("{}:{}", k.as_str(), ty_shape_tag(t))).collect();
            format!("R{{{}}}", inner.join(","))
        }
        Ty::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(ty_shape_tag).collect();
            format!("T({})", inner.join(","))
        }
        other => format!("{other:?}"),
    }
}

/// Does an ANONYMOUS record (`Ty::Record`) need a SYNTHESIZED recursive `$__drop_<hash>`? It does iff
/// ANY field needs a recursive drop ([`record_field_needs_recursive_drop`]) — EXACTLY the predicate
/// `recursive_record_drop_names` uses for NAMED records, since the slot layout is identical. A flat
/// one-level mask `rc_dec`s only each field's HANDLE: that fully frees a flat-heap field (Bytes /
/// String — a single buffer) but only frees the BLOCK of a field that itself holds heap handles (a
/// nested record / Value / Map / `List[heap]`), leaking what's inside. So an anon record that owns
/// any heap field at all needs the synthesized recursive drop (the body flat-frees the
/// single-buffer fields and recurses into the handle-holding ones via `record_drop_field_frees`).
/// `record_field_needs_recursive_drop` is structural and host-independent.
pub(crate) fn anon_record_needs_recursive_drop(fields: &[(almide_lang::intern::Sym, Ty)]) -> bool {
    fields.iter().any(|(_, t)| record_field_needs_recursive_drop(t))
}

/// Is `ty` `Result[Map[String, <scalar>], String]` (the fs.fold_lines msi class)?
pub fn is_res_map_si_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    matches!(ty, Ty::Applied(TC::Result, a) if a.len() == 2
        && matches!(&a[0], Ty::Applied(TC::Map, kv) if kv.len() == 2
            && matches!(kv[0], Ty::String) && !is_heap_ty(&kv[1]))
        && matches!(a[1], Ty::String))
}

/// Is `ty` `Result[List[Map[String, <scalar>]], String]` (the chunked partials class)?
pub fn is_res_list_map_si_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    matches!(ty, Ty::Applied(TC::Result, a) if a.len() == 2
        && matches!(&a[0], Ty::Applied(TC::List, e) if e.len() == 1
            && matches!(&e[0], Ty::Applied(TC::Map, kv) if kv.len() == 2
                && matches!(kv[0], Ty::String) && !is_heap_ty(&kv[1])))
        && matches!(a[1], Ty::String))
}

/// Is `ty` exactly `Result[(Float, String), String]` (the result.zip_fs return —
/// the tag-aware `$__drop_res_fs` class)?
pub fn is_res_fs_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    matches!(ty, Ty::Applied(TC::Result, a) if a.len() == 2
        && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
            && matches!(ts[0], Ty::Float) && matches!(ts[1], Ty::String))
        && matches!(a[1], Ty::String))
}

/// Is `ty` a heap-Ok `Result[List[E], String]` whose element E is ONE-LEVEL-EXACT
/// (one `rc_dec` is a slot's full free — the registry-free subset: String, Bytes,
/// `List[<scalar>]`, a Flat-class ctor)? The `$__drop_res_lsl` class; String
/// elements (fs.read_lines) are an instance, not the rule. A DEEPER element
/// (`List[List[String]]`, a record) is excluded — the one-level slot sweep would
/// leak its interior, so those keep their existing route or honest wall.
pub fn is_res_lenlist_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    matches!(ty, Ty::Applied(TC::Result, a) if a.len() == 2
        && matches!(a[1], Ty::String)
        && matches!(&a[0], Ty::Applied(TC::List, e) if e.len() == 1
            && (matches!(e[0], Ty::String | Ty::Bytes)
                || matches!(&e[0], Ty::Applied(TC::List, b)
                    if b.len() == 1 && !crate::lower::is_heap_ty(&b[0]))
                || crate::lower::lenlist_elem_class(&e[0])
                    == Some(crate::lower::CtorElemClass::Flat))))
}

/// Is `ty` exactly `Result[List[Int], List[String]]` (the result.collect return —
/// the tag-aware `$__drop_res_ilsl` class)?
pub fn is_res_intlist_strlist_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    matches!(ty, Ty::Applied(TC::Result, a) if a.len() == 2
        && matches!(&a[0], Ty::Applied(TC::List, i) if i.len() == 1 && matches!(i[0], Ty::Int))
        && matches!(&a[1], Ty::Applied(TC::List, s) if s.len() == 1 && matches!(s[0], Ty::String)))
}

/// Does a TUPLE own heap that a synthesized per-field drop must free? Any heap slot does
/// (a flat `rc_dec` of the tuple block frees only the block).
pub(crate) fn tuple_needs_recursive_drop(ty: &Ty) -> bool {
    matches!(ty, Ty::Tuple(elems) if elems.iter().any(is_heap_ty))
}

/// Is `ty` a `List` ELEMENT tuple the fixed pair drops cannot free — the shapes that
/// route to a synthesized `$__drop_list_anontup_<hash>`: a heap-bearing tuple of THREE or
/// more slots, or a pair one of whose slots is a `List[String]`, a `List` of heap tuples, or
/// a heap tuple. Every other pair keeps the dedicated drop it has (`(String, String)`,
/// `(String, Int)`, `(String, List[Option[Int]])`, …) — the set is kept disjoint from those
/// on purpose, so no shape that lowered before changes route.
pub(crate) fn is_anon_tuple_list_elem(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let Ty::Tuple(elems) = ty else { return false };
    if !tuple_needs_recursive_drop(ty) {
        return false;
    }
    elems.len() >= 3
        || elems.iter().any(|t| {
            tuple_needs_recursive_drop(t)
                || matches!(t, Ty::Applied(TypeConstructorId::List, a)
                    if a.len() == 1
                        && (matches!(a[0], Ty::String) || tuple_needs_recursive_drop(&a[0])))
        })
}

/// The drop identity of an anonymous heap tuple: `anontup_<FNV-1a of its shape tag>`.
/// Host-independent (pure arithmetic over the structural tag), disjoint from
/// `anonrec_` and from any user type name.
pub(crate) fn anon_tuple_drop_name(ty: &Ty) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in ty_shape_tag(ty).as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("anontup_{h:016x}")
}

/// The type-driven drop route of a `List[<anon heap tuple>]` VALUE (a call result, an
/// argument temp, a bound call) — the same `list_anontup_<hash>` the list-literal builder
/// registers, so a list built in one fn and dropped by its caller frees every slot. `None`
/// for every other type (the caller's existing ladder decides).
pub(crate) fn anon_tuple_list_route(ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_anon_tuple_list_elem(&a[0]) => {
            Some(format!("list_{}", anon_tuple_drop_name(&a[0])))
        }
        _ => None,
    }
}

/// The RICH-capture env tag for a type name — FNV-1a 64 folded positive. Both
/// sides of the closure-env RICH class (#1547 shapes 2/3) derive tags through
/// THIS one function: the lowering stamps `rich_env_tag(elem_name)` into the
/// capture's wrapper block, and `generate_closure_env_rich_sources` emits the
/// matching `if tag == <same>` dispatcher arm — no shared registry, no
/// ordering to keep in sync. A collision between two of one program's type
/// names would mis-dispatch a free, so the generator PANICS on one (64-bit
/// FNV over a program's handful of short names — unreachable in practice,
/// loud if ever).
pub fn rich_env_tag(name: &str) -> i64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in name.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    (h & 0x7fff_ffff_ffff_ffff) as i64
}

/// The generated-repr key for a scalar-component tuple shape — one tag per
/// component (`(Int, String)` → `i_s`). Derived IDENTICALLY at the interp call
/// site (mod_p4) and in the generator, so the call links by construction.
/// `None` for any component outside Int/Bool/String (the honest-wall gate).
pub(crate) fn tuple_repr_ident(tys: &[Ty]) -> Option<String> {
    let tag = |t: &Ty| -> Option<&'static str> {
        match t {
            Ty::Int => Some("i"),
            Ty::Bool => Some("b"),
            Ty::String => Some("s"),
            _ => None,
        }
    };
    Some(tys.iter().map(tag).collect::<Option<Vec<_>>>()?.join("_"))
}

/// The instantiation-keyed repr ident (`ReprEither[Int, String]` →
/// `ReprEither_Int_String`) — derived IDENTICALLY at the interp call site
/// (mod_p4's variant part) and in the generator, so the call links by
/// construction. Args spell via their `Debug` form, sanitized to identifier
/// chars (the `instantiate_variant_layout` key discipline).
pub(crate) fn repr_inst_ident(name: &str, args: &[Ty]) -> String {
    let sane = |s: String| -> String {
        s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
    };
    format!(
        "{}_{}",
        drop_fn_ident(name),
        args.iter().map(|a| sane(format!("{a:?}"))).collect::<Vec<_>>().join("_")
    )
}
