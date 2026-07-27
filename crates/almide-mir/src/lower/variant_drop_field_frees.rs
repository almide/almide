
// The per-FIELD free decision for one variant CTOR slot — the group fns lifted out of
// `generate_variant_drop_sources`' field loop (codopsy round-2 complexity sweep), so the
// generator reads as a router over field SHAPES instead of one 120-line if-cascade. Split
// into its own `include!`d file (drop_sources.rs is at the 800-line cap) — drop_sources.rs
// keeps the surrounding per-decl / per-ctor phases and owns `VariantDropNameSets`, the
// read-only name vocabulary every group below consults.

/// Extracted verbatim from `generate_variant_drop_sources` (codopsy round-2 complexity
/// sweep, phase 3 of 5): ONE ctor's per-field free statements — the field-shape router.
/// The group fns are consulted in the ORIGINAL cascade order and the first one to claim the
/// field wins, exactly as the original's `continue` chain did. The order is load-bearing:
/// the shapes are NOT all disjoint (a `List[<flat variant>]` whose element carries no heap
/// also answers the `List[scalar]` test, and the two free it DIFFERENTLY), so groups must
/// stay in this sequence. The `f{idx}` binding counter is per-ctor, threaded by `&mut` and
/// bumped only by the groups that actually emit a `let` — the same counter, bumped at the
/// same points, as the original loop.
fn variant_ctor_field_frees(kind: &almide_ir::IrVariantKind, sets: &VariantDropNameSets) -> String {
    use almide_ir::IrVariantKind;
    let tys: Vec<Ty> = match kind {
        IrVariantKind::Unit => vec![],
        IrVariantKind::Tuple { fields } => fields.clone(),
        IrVariantKind::Record { fields } => fields.iter().map(|f| f.ty.clone()).collect(),
    };
    // Per-field free statements (variant → recurse, String → rc_dec, scalar → skip).
    let mut frees = String::new();
    let mut idx = 0usize;
    for (i, ty) in tys.iter().enumerate() {
        let off = layout::slot_offset(1 + i);
        if let Some(s) = variant_field_free_nested_variant(ty, off, &mut idx, sets) {
            frees.push_str(&s);
            continue;
        }
        if let Some(s) = variant_field_free_builtin_shape(ty, off, &mut idx, sets) {
            frees.push_str(&s);
            continue;
        }
        if let Some(s) = variant_field_free_rich_variant_list(ty, off, &mut idx, sets) {
            frees.push_str(&s);
            continue;
        }
        if let Some(s) = variant_field_free_record(ty, off, &mut idx, sets) {
            frees.push_str(&s);
        }
    }
    frees
}

/// Extracted verbatim from `generate_variant_drop_sources`' field loop (codopsy round-2
/// complexity sweep, group 1 of 4): decides the free for a ctor field that is itself a user
/// VARIANT — a flat one (single owned block, one `rc_dec`) or a rich one (recurse through
/// its own `$__drop_<V>`). `None` = not a variant field, fall through to the next group.
fn variant_field_free_nested_variant(
    ty: &Ty,
    off: u32,
    idx: &mut usize,
    sets: &VariantDropNameSets,
) -> Option<String> {
    let fv = variant_field_name(ty, &sets.variant_names)?;
    if sets.flat_names.contains(&fv) {
        // A flat-variant field — a single owned block, freed by one `rc_dec` (no
        // recursive `__drop_<fv>` exists for a flat variant). No `let` binding needed.
        return Some(format!("        prim.rc_dec(prim.load64(h + {off}))\n"));
    }
    let fv_fn = drop_fn_ident(&fv);
    let free = format!(
        "        let f{idx}: {fv} = prim.load_handle(h + {off})\n        __drop_{fv_fn}(f{idx})\n"
    );
    *idx += 1;
    Some(free)
}

/// Extracted verbatim from `generate_variant_drop_sources`' field loop (codopsy round-2
/// complexity sweep, group 2 of 4): decides the free for a ctor field of a BUILT-IN heap
/// shape — `String`, `List[scalar]`, `List[String]`, `List[<flat variant>]`,
/// `Option[scalar]`, or a closure `Fn`. The six tests keep their original relative order
/// (see [`variant_ctor_field_frees`] on why that matters). `None` = none of these shapes.
fn variant_field_free_builtin_shape(
    ty: &Ty,
    off: u32,
    idx: &mut usize,
    sets: &VariantDropNameSets,
) -> Option<String> {
    if matches!(ty, Ty::String) {
        return Some(format!("        prim.rc_dec(prim.load64(h + {off}))\n"));
    }
    if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
        if a.len() == 1 && !is_heap_ty(&a[0]))
    {
        // A List[scalar] ctor field — a FLAT block, one rc_dec is its full free.
        return Some(format!(
            "        prim.rc_dec(prim.load64(h + {off}))
"
        ));
    }
    if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
        if a.len() == 1 && matches!(a[0], Ty::String))
    {
        // A `List[String]` ctor field (`Node(String, List[String])`): each element is
        // an OWNED String handle — the generic `__drop_list_str` (shared with the
        // record-drop generator via `LIST_STR_DROP_SRC`, gated once at the pipeline
        // top level so both generators' identical references never double-define it)
        // frees every element then the list block. A flat `rc_dec` of just the list
        // block would leak each String.
        let free = format!(
            "        let f{idx}: List[String] = prim.load_handle(h + {off})\n        __drop_list_str(f{idx})\n"
        );
        *idx += 1;
        return Some(free);
    }
    if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
        if a.len() == 1 && is_flat_variant_elem(&a[0], &sets.flat_names))
    {
        // A `List[<flat variant>]` ctor field (`Wrapped(List[Policy])` — #484): each
        // element is a single owned FLAT block (no inner handles), so `__drop_list_str`'s
        // per-element `rc_dec` sweep is its exact free — the record-drop generator's
        // List[flat-variant] precedent mirrored (incl. its `List[String]` binding type,
        // the handle-level reinterpretation that precedent already uses).
        let free = format!(
            "        let f{idx}: List[String] = prim.load_handle(h + {off})\n        __drop_list_str(f{idx})\n"
        );
        *idx += 1;
        return Some(free);
    }
    if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a)
        if a.len() == 1 && !is_heap_ty(&a[0]))
    {
        // An Option[scalar] ctor field (`Box(Option[Int])`) — the 0-or-1-element
        // len-tag block owns NO children (a Some payload is a scalar slot), so one
        // rc_dec is its full free. Mirrored in BOTH `needs_recursive_drop` gates and
        // `try_lower_variant_ctor`'s field admission — construction and drop agree.
        return Some(format!(
            "        prim.rc_dec(prim.load64(h + {off}))
"
        ));
    }
    if matches!(ty, Ty::Fn { .. }) {
        // A CLOSURE ctor field (`Run(() -> Unit)` — the variant-stored closure
        // class): the slot holds a self-describing closure block whose captured
        // heap env a flat rc_dec would LEAK — free it via `__drop_closure`, the
        // SAME routine the record-drop generator's Fn arm uses (CLOSURE_DROP_SRC
        // is linked whenever the program creates closures, which a populated Fn
        // payload requires). The binding type is the block's List[Int] rep.
        let free = format!(
            "        let f{idx}: List[Int] = prim.load_handle(h + {off})\n        __drop_closure(f{idx})\n"
        );
        *idx += 1;
        return Some(free);
    }
    None
}

/// Extracted verbatim from `generate_variant_drop_sources`' field loop (codopsy round-2
/// complexity sweep, group 3 of 4): decides the free for a `List[<rich variant>]` ctor
/// field — the per-element recursive `$__drop_list_<V>`. `None` = not such a list.
fn variant_field_free_rich_variant_list(
    ty: &Ty,
    off: u32,
    idx: &mut usize,
    sets: &VariantDropNameSets,
) -> Option<String> {
    let ev = list_rich_variant_elem(ty, &sets.rec_variant_names)?;
    // A `List[<rich variant>]` ctor field (`Block(_, List[Instr])`): each element is a
    // recursive-drop variant block, freed per-element by the generated `$__drop_list_<ev>`
    // (→ `$__drop_<ev>`). A flat `rc_dec` of the list block would leak every element.
    let ev_fn = drop_fn_ident(&ev);
    let free = format!(
        "        let f{idx}: List[{ev}] = prim.load_handle(h + {off})\n        __drop_list_{ev_fn}(f{idx})\n"
    );
    *idx += 1;
    Some(free)
}

/// Extracted verbatim from `generate_variant_drop_sources`' field loop (codopsy round-2
/// complexity sweep, group 4 of 4): decides the free for a RECORD-type ctor field —
/// recursive `$__drop_<R>` or a flat `rc_dec`. `None` reproduces the original's two trailing
/// `continue`s: a non-`Named` type, and a `Named` type that is not a known record.
fn variant_field_free_record(
    ty: &Ty,
    off: u32,
    idx: &mut usize,
    sets: &VariantDropNameSets,
) -> Option<String> {
    let Ty::Named(rn, _) = ty else {
        return None;
    };
    if !sets.all_record_names.contains(rn.as_str()) {
        return None;
    }
    // A RECORD-type ctor field (`Wrap(Color)` / `Box(Inner)`). A recursive-drop
    // record (a String / nested-heap field) recurses via `$__drop_<R>`; a
    // scalar-only record block is a single owned allocation, one `rc_dec` its full
    // free. Either way the ctor stored its HANDLE at this slot.
    if sets.rec_record_names.contains(rn.as_str()) {
        let rn_fn = drop_fn_ident(rn.as_str());
        let rn_s = rn.as_str();
        let free = format!(
            "        let f{idx}: {rn_s} = prim.load_handle(h + {off})\n        __drop_{rn_fn}(f{idx})\n"
        );
        *idx += 1;
        Some(free)
    } else {
        Some(format!("        prim.rc_dec(prim.load64(h + {off}))\n"))
    }
}
