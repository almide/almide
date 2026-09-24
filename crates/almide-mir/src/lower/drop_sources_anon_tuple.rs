// ── ANONYMOUS-TUPLE recursive drops (#2520), include!-spliced at module level ──
//
// A `List[(String, String, Int)]` / `List[(Int, List[String])]` element owns heap
// slots the flat pair drops (`DropListStrStr` / `DropListStrInt` / `DropListIntStr`)
// cannot free: those render a fixed two-slot `rc_dec`, and a third slot or a nested
// `List[String]` slot would leak. A tuple block has exactly a record block's layout
// (field i at `slot_offset(i)`), so its recursive drop is the SAME per-field free
// ladder the anonymous-record drops use (`record_drop_field_frees`), under a
// content-hashed `anontup_<hash>` identity. Discovery (`collect_anon_tuple_drops`)
// and admission (`is_anon_tuple_list_elem`, the list-literal builder's last rung)
// share one predicate, so a routed `$__drop_list_anontup_<hash>` is always generated.

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

/// Walk the IR (every fn signature and every expression's type) and collect the distinct
/// anonymous tuples that need a synthesized drop: each `List[T]` element with
/// [`is_anon_tuple_list_elem`], closed under nesting — a heap tuple reachable from a
/// registered tuple's slots (directly or as a `List` element) is registered too, so the
/// outer drop body's `__drop_anontup_<inner>` / `__drop_list_anontup_<inner>` exists.
/// Sorted by drop name (host-determinism).
pub fn collect_anon_tuple_drops(program: &almide_ir::IrProgram) -> Vec<Ty> {
    struct C {
        seen: std::collections::BTreeMap<String, Ty>,
    }
    impl C {
        fn register(&mut self, t: &Ty) {
            if self.seen.insert(anon_tuple_drop_name(t), t.clone()).is_some() {
                return;
            }
            if let Ty::Tuple(elems) = t {
                for e in elems {
                    self.register_nested(e);
                }
            }
        }
        /// A slot of a registered tuple: a heap tuple (or a `List` of one) gets its own drop.
        fn register_nested(&mut self, t: &Ty) {
            use almide_lang::types::constructor::TypeConstructorId;
            match t {
                Ty::Tuple(_) if tuple_needs_recursive_drop(t) => self.register(t),
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => self.register_nested(&a[0]),
                _ => {}
            }
        }
        fn consider(&mut self, t: &Ty) {
            use almide_lang::types::constructor::TypeConstructorId;
            match t {
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                    if is_anon_tuple_list_elem(&a[0]) {
                        self.register(&a[0]);
                    }
                    self.consider(&a[0]);
                }
                Ty::Applied(_, a) => a.iter().for_each(|x| self.consider(x)),
                Ty::Tuple(elems) => elems.iter().for_each(|x| self.consider(x)),
                Ty::Record { fields } | Ty::OpenRecord { fields } => {
                    fields.iter().for_each(|(_, x)| self.consider(x))
                }
                _ => {}
            }
        }
    }
    impl almide_ir::visit::IrVisitor for C {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            self.consider(&expr.ty);
            almide_ir::visit::walk_expr(self, expr);
        }
    }
    let mut c = C { seen: std::collections::BTreeMap::new() };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        c.consider(&f.ret_ty);
        for p in &f.params {
            c.consider(&p.ty);
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut c, &f.body);
    }
    c.seen.into_values().collect()
}

/// Does any synthesized tuple drop free a `List`/`Option` slot — so the shared
/// `__drop_list_str` must be in scope? Conservative (an unused helper is harmless; a
/// missing one would be a dangling call).
pub fn anon_tuples_may_use_list_str(tuples: &[Ty]) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    tuples.iter().any(|t| {
        matches!(t, Ty::Tuple(elems) if elems.iter().any(|e| matches!(e,
            Ty::Applied(TypeConstructorId::List | TypeConstructorId::Option, _))))
    })
}

/// Emit `$__drop_anontup_<h>` (per-slot free, the anon-record body) and its per-element
/// list wrapper `$__drop_list_anontup_<h>` for every collected tuple.
fn emit_anon_tuple_drops(out: &mut String, tuples: &[Ty], shapes: DropShapes<'_>, needs: &mut DropNeeds) {
    let mut list_drops: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for t in tuples {
        let Ty::Tuple(elems) = t else { continue };
        let name = anon_tuple_drop_name(t);
        let src = field_source_ty(t);
        out.push_str(&format!("fn __drop_{name}(e: {src}) -> Unit = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        out.push_str("  if prim.load32(h + 0) == 1 then {\n");
        out.push_str(&record_drop_field_frees(elems, shapes, &mut list_drops, needs));
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
        out.push_str(&format!(
            "fn __drop_list_{name}(xs: List[{src}]) -> Unit = {{
  let h = prim.handle(xs)
  if prim.load32(h + 0) == 1 then __drop_list_{name}_loop(h, prim.load32(h + 4), 0) else ()
  prim.rc_dec(h)
}}
fn __drop_list_{name}_loop(h: Int, n: Int, i: Int) -> Unit =
  if i >= n then ()
  else {{ let e: {src} = prim.load_handle(h + 12 + i * 8)
         __drop_{name}(e)
         __drop_list_{name}_loop(h, n, i + 1) }}
"
        ));
    }
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
