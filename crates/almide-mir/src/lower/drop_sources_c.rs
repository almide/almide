
/// The ALMIDE SOURCE of the UNIFORM closure-block release `$__drop_closure` (the closures
/// machinery — injected by the render pipeline whenever the program carries first-class
/// function values). A closure block is SELF-DESCRIBING: slot 0 = fnidx (a table index —
/// NEVER dereferenced here), slot 1 = n_heap | (n_closure << 16), slots 2.. = captured
/// closures (freed by RECURSING into this very routine — the `compose` shape), then
/// captured heap values (each freed by ONE `rc_dec` — the lowering's capture gate admits
/// only one-level-exact kinds), then scalars (untouched). Any drop site can free any
/// closure value without knowing its captures (a call-result closure's layout is
/// unknowable at the caller). Like every generated `$__drop_*`, a trusted prim-only
/// routine (outside the witness surface), pinned by the closure leak-loop test.
/// The ALMIDE SOURCE of `__drop_list_closure` — the per-element release of a
/// `List[<Fn>]` (each slot an OWNED closure block): recurse into `__drop_closure`
/// per element (the uniform, self-describing closure free — correct whether or not
/// each element captures anything), then free the list block. Requires
/// `CLOSURE_DROP_SRC` in scope (the program already carries it whenever any closure
/// value exists, which a `List[<Fn>]` literal necessarily does). A blind per-element
/// `rc_dec` (the plain masked `DropListStr`) would be unsound for a CAPTURING
/// closure element (it would decrement the block's own refcount without recursively
/// freeing its captured heap slots) — `__drop_closure` is required, not optional,
/// even though this session's only current caller (`call_closure_lambda_param`)
/// happens to use only non-capturing lambdas.
pub const LIST_CLOSURE_DROP_SRC: &str = "\
fn __drop_list_closure(xs: List[List[Int]]) -> Unit = {
  let h = prim.handle(xs)
  if prim.load32(h + 0) == 1 then __drop_list_closure_loop(h, prim.load32(h + 4), 0) else ()
  prim.rc_dec(h)
}
fn __drop_list_closure_loop(h: Int, n: Int, i: Int) -> Unit =
  if i >= n then ()
  else {
    let e: List[Int] = prim.load_handle(h + 12 + i * 8)
    __drop_closure(e)
    __drop_list_closure_loop(h, n, i + 1)
  }
";

/// The ALMIDE SOURCE of `__drop_list_str_clo` — the per-element release of a
/// `List[(String, <Fn>)]` (the closure-valued map's from_list pairs list): each
/// element tuple owns a String @12 (flat `rc_dec`) and a CLOSURE BLOCK @20
/// (freed via `__drop_closure` — a flat rc_dec would leak its captured env),
/// then the tuple block, then the list block. Requires `CLOSURE_DROP_SRC` in
/// scope (a closure-bearing pairs list implies the program creates closures).
pub const LIST_STR_CLO_DROP_SRC: &str = "\
fn __drop_list_str_clo(xs: List[Int]) -> Unit = {
  let h = prim.handle(xs)
  if prim.load32(h + 0) == 1 then __drop_list_str_clo_loop(h, prim.load32(h + 4), 0) else ()
  prim.rc_dec(h)
}
fn __drop_list_str_clo_loop(h: Int, n: Int, i: Int) -> Unit =
  if i >= n then ()
  else {
    let th = prim.load64(h + 12 + i * 8)
    if prim.load32(th + 0) == 1 then {
      prim.rc_dec(prim.load64(th + 12))
      let f: List[Int] = prim.load_handle(th + 20)
      __drop_closure(f)
    }
    else ()
    prim.rc_dec(th)
    __drop_list_str_clo_loop(h, n, i + 1)
  }
";

/// The ALMIDE SOURCE of `__drop_map_mclo` — the recursive release of a
/// `Map[String, <Fn>]` (the closure-valued map, mclo class). The map is the
/// hval/skv SPLIT layout ([rc][n@4][cap], keys @ slots 0..n-1, values @ slots
/// n..2n-1): at the block's last ref, `rc_dec` each key String and free each
/// VALUE slot via `__drop_closure` (the uniform self-describing closure free —
/// `__drop_map_hval`'s blind per-slot `rc_dec` would decrement each closure
/// block without recursively freeing its captured env, the exact leak class
/// `__drop_list_closure` exists for), then the block. Requires
/// `CLOSURE_DROP_SRC` in scope (a populated closure-valued map implies the
/// program creates closures). The param type is spelled `Map[String, Int]` —
/// any heap map spelling; the routine is handle-level and never reads a value
/// slot as its declared type.
pub const MAP_MCLO_DROP_SRC: &str = "\
fn __drop_map_mclo(m: Map[String, Int]) -> Unit = {
  let h = prim.handle(m)
  if prim.load32(h + 0) == 1 then __drop_map_mclo_loop(h, prim.load32(h + 4), 0) else ()
  prim.rc_dec(h)
}
fn __drop_map_mclo_loop(h: Int, n: Int, i: Int) -> Unit =
  if i >= n then ()
  else {
    prim.rc_dec(prim.load64(h + 12 + i * 8))
    let f: List[Int] = prim.load_handle(h + 12 + (n + i) * 8)
    __drop_closure(f)
    __drop_map_mclo_loop(h, n, i + 1)
  }
";


/// Header layout: `n_heap | (n_nested_heap << 16) | (n_closure << 32) |
/// (n_cellmap << 48) | (n_rich << 56)` — three 16-bit counts plus two 8-bit
/// ones (ample for any realistic capture count). Widened twice from the
/// original 2-field `n_heap | (n_closure << 16)`: first for the
/// `n_nested_heap` class (a `List[String]` capture — freed via
/// `__drop_list_str`, NOT the flat `rc_dec` a one-level-exact heap capture
/// gets), then for the RICH class (#1547 shapes 2/3 — a `List[<rich variant>]`
/// / `List[<recursive record>]` capture, held behind a per-capture
/// `[tag@12][list-handle@20]` wrapper block and freed via the generated
/// `__drop_env_rich` tag dispatcher, since the header's counts alone cannot
/// name a type-specific `$__drop_list_<V>`). The rich count steals the top 8
/// bits of the former 16-bit cellmap field — an old-style header (n_rich = 0,
/// n_cellmap < 256 always in practice) decodes identically.
pub const CLOSURE_DROP_SRC: &str = "\
fn __drop_closure(c: List[Int]) -> Unit = {
  let h = prim.handle(c)
  if prim.load32(h + 0) == 1 then {
    let hdr = prim.load64(h + 20)
    let top = hdr / 281474976710656
    let nr = top / 256
    let ncm = top - nr * 256
    let rem0 = hdr - top * 281474976710656
    let nc = rem0 / 4294967296
    let rem1 = rem0 - nc * 4294967296
    let nnh = rem1 / 65536
    let nh = rem1 - nnh * 65536
    __drop_closure_loop(h, nc, nr, nnh, nh, ncm, 0)
  } else ()
  prim.rc_dec(h)
}
fn __drop_closure_loop(h: Int, nc: Int, nr: Int, nnh: Int, nh: Int, ncm: Int, i: Int) -> Unit =
  if i >= nc + nr + nnh + nh + ncm then ()
  else {
    if i < nc then {
      let q: List[Int] = prim.load_handle(h + 28 + i * 8)
      __drop_closure(q)
    } else if i < nc + nr then {
      __drop_env_rich(prim.load64(h + 28 + i * 8))
    } else if i < nc + nr + nnh then {
      let ls: List[String] = prim.load_handle(h + 28 + i * 8)
      __drop_list_str(ls)
    } else if i < nc + nr + nnh + nh then {
      prim.rc_dec(prim.load64(h + 28 + i * 8))
    } else {
      __drop_cellmap(prim.load64(h + 28 + i * 8))
    }
    __drop_closure_loop(h, nc, nr, nnh, nh, ncm, i + 1)
  }
fn __drop_cellmap(ch: Int) -> Unit = {
  if prim.load32(ch + 0) == 1 then {
    let mm: List[String] = prim.load_handle(ch + 12)
    __drop_list_str(mm)
  }
  else ()
  prim.rc_dec(ch)
}
";

/// Generate the ALMIDE SOURCE for each RECORD type's recursive drop `$__drop_<R>` (the records
/// counterpart of [`generate_variant_drop_sources`]). Records have NO tag — fields sit at
/// `slot_offset(i)`, freed per CONCRETE field type: `String → rc_dec`, `Map[String,String] →
/// __drop_map_ss`, `List[String] → __drop_list_str`, `List[<recursive record>] → __drop_list_<R>`,
/// a recursive record → `__drop_<R>`, a `Value → __drop_value`, a scalar-only nested aggregate or
/// `List[scalar]` → flat `rc_dec` of the block, a scalar → skip. Emits the needed `__drop_list_<R>`
/// loops + the generic `__drop_map_ss` / `__drop_list_str` helpers. Also emits a synthesized
/// `__drop_anonrec_<hash>` for each ANONYMOUS record shape in `anon_records` that needs the
/// recursive drop (a heap-nested anon record return — aes cfb8). All `__drop_`-prefixed ⇒ on the
/// `prim.rc_dec` whitelist + an empty ownership cert (a trusted free, leak-loop verified).
pub fn generate_record_drop_sources(
    type_decls: &[almide_ir::IrTypeDecl],
    anon_records: &[Vec<(almide_lang::intern::Sym, Ty)>],
    uses_result_opt_str: bool,
) -> String {
    use almide_ir::IrTypeDeclKind;
    let rec_names = recursive_record_drop_names(type_decls);
    let generic_decls = generic_record_decls(type_decls);
    let flat_variant_names = flat_variant_type_names(type_decls);
    // The RICH variant names — a record `List[<rich variant>]` field (`Global.init: List[Instr]`) routes
    // to `$__drop_list_<V>` (generated by `generate_variant_drop_sources`, appended to the same program).
    let variant_names = variant_type_names(type_decls);
    let all_record_names: std::collections::HashSet<String> = type_decls
        .iter()
        .filter(|d| matches!(&d.kind, IrTypeDeclKind::Record { .. }))
        .map(|d| d.name.as_str().to_string())
        .collect();
    let rec_variant_names: std::collections::HashSet<String> = type_decls
        .iter()
        .filter(|d| variant_needs_recursive_drop(d, &variant_names, &all_record_names))
        .map(|d| d.name.as_str().to_string())
        .collect();
    let mut out = String::new();
    let mut list_drops: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut need_map_ss = false;
    let mut need_list_str = false;
    let mut need_matrix = false;
    let mut need_list_matrix = false;
    for decl in type_decls {
        let IrTypeDeclKind::Record { fields } = &decl.kind else { continue };
        if !rec_names.contains(decl.name.as_str()) {
            continue;
        }
        let tname = decl.name.as_str();
        let fname = drop_fn_ident(tname);
        let field_tys: Vec<Ty> = fields.iter().map(|f| f.ty.clone()).collect();
        out.push_str(&format!("fn __drop_{fname}(e: {tname}) -> Unit = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        out.push_str("  if prim.load32(h + 0) == 1 then {\n");
        let mut needs = DropNeeds {
            map_ss: need_map_ss, list_str: need_list_str,
            matrix: need_matrix, list_matrix: need_list_matrix,
        };
        out.push_str(&record_drop_field_frees(
            &field_tys,
            DropShapes {
                rec_names: &rec_names,
                flat_variant_names: &flat_variant_names,
                rec_variant_names: &rec_variant_names,
                generic_decls: &generic_decls,
            },
            &mut list_drops,
            &mut needs,
        ));
        let DropNeeds {
            map_ss: need_map_ss_new, list_str: need_list_str_new,
            matrix: need_matrix_new, list_matrix: need_list_matrix_new,
        } = needs;
        need_map_ss = need_map_ss_new;
        need_list_str = need_list_str_new;
        need_matrix = need_matrix_new;
        need_list_matrix = need_list_matrix_new;
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
    }
    emit_record_wrapper_drops(&mut out, type_decls, &rec_names, uses_result_opt_str);
    // SYNTHESIZED recursive drops for the ANONYMOUS record return/binding shapes the corpus uses
    // (`{ data: Bytes, state: Cfb8State }` — aes cfb8). An anon record is NOT a `type` decl, so the
    // loop above never names it; it would otherwise drop via the flat one-level mask `DropListStr`,
    // which `rc_dec`s the `state` BLOCK but LEAKS the Bytes INSIDE Cfb8State. Each shape gets a
    // content-hashed `__drop_anonrec_<hash>` (dedup'd) with the SAME per-field-type recursion the
    // named generator emits — so the `state` field is freed through `__drop_Cfb8State`. The param is
    // the structural anon record type in source (`e: { data: Bytes, state: Cfb8State }`). Sorted by
    // name for host-determinism. (The discovery of WHICH anon shapes appear is the caller's; see
    // `generate_anon_record_drop_sources`.)
    let mut anon_sorted: Vec<&Vec<(almide_lang::intern::Sym, Ty)>> = anon_records.iter().collect();
    anon_sorted.sort_by_key(|fields| anon_record_drop_name(fields));
    anon_sorted.dedup_by_key(|fields| anon_record_drop_name(fields));
    for fields in anon_sorted {
        if !anon_record_needs_recursive_drop(fields) {
            continue;
        }
        let name = anon_record_drop_name(fields);
        let field_tys: Vec<Ty> = fields.iter().map(|(_, t)| t.clone()).collect();
        let param_ty = anon_record_source_ty(fields);
        out.push_str(&format!("fn __drop_{name}(e: {param_ty}) -> Unit = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        out.push_str("  if prim.load32(h + 0) == 1 then {\n");
        let mut needs = DropNeeds {
            map_ss: need_map_ss, list_str: need_list_str,
            matrix: need_matrix, list_matrix: need_list_matrix,
        };
        out.push_str(&record_drop_field_frees(
            &field_tys,
            DropShapes {
                rec_names: &rec_names,
                flat_variant_names: &flat_variant_names,
                rec_variant_names: &rec_variant_names,
                generic_decls: &generic_decls,
            },
            &mut list_drops,
            &mut needs,
        ));
        need_map_ss = needs.map_ss;
        need_list_str = needs.list_str;
        need_matrix = needs.matrix;
        need_list_matrix = needs.list_matrix;
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
    }
    emit_anon_list_wrapper_drops(&mut out, anon_records);
    let _ = &list_drops; // (subsumed by rec_names below)
    emit_record_list_wrapper_drops(&mut out, &rec_names);
    emit_demanded_shared_drops(&mut out, need_map_ss, need_matrix, need_list_matrix);
    let _ = need_list_str;
    out
}


/// The per-record WRAPPER drops: `__drop_opt_<R>` (an `Option[R]` — the
/// `resrec:opt_<R>` bases), `__drop_opt_str` (only when the program constructs
/// `Result[Option[String], String]`), and `__drop_tup_int_<R>` (a `(R, Int)`
/// tuple block — the `resrec:tup_int_<R>` gguf shape). Verbatim.
fn emit_record_wrapper_drops(
    out: &mut String,
    type_decls: &[almide_ir::IrTypeDecl],
    rec_names: &std::collections::HashSet<String>,
    uses_result_opt_str: bool,
) {
    use almide_ir::IrTypeDeclKind;
    // `$__drop_opt_<R>` for each recursive-drop record R — frees an `Option[R]` (the 0-or-1-element
    // layout) used by `Result[Option[R], String]` wrappers (`resrec:opt_<R>`, porta read_message's
    // `ok(none)` / `ok(r)` bases). The `match` drops the bound record `r` at the Some-arm end (routing
    // to `$__drop_<R>`); a None is a no-op; consuming `e` frees the Option block. Same per-R set as the
    // `$__drop_<R>` loop above, so an `$__drop_opt_<R>` is emitted only when its `$__drop_<R>` exists.
    for decl in type_decls {
        let IrTypeDeclKind::Record { .. } = &decl.kind else { continue };
        if !rec_names.contains(decl.name.as_str()) {
            continue;
        }
        let tname = decl.name.as_str();
        let fname = drop_fn_ident(tname);
        out.push_str(&format!(
            "fn __drop_opt_{fname}(e: Option[{tname}]) -> Unit = {{\n  match e {{\n    some(r) => (),\n    none => (),\n  }}\n}}\n"
        ));
        // `$__drop_list_opt_<R>` — frees a `List[Option[R]]` (the derived-codec
        // opt-list accumulator, #1134's recursive-drop pair): each element is its
        // own 0-or-1 Option block freed through `$__drop_opt_<R>` above (Some →
        // the record recursion, None → the bare block), then the list block.
        // Same trusted prim-only class and the same loop shape as the variant
        // `$__drop_list_<V>`; emitted for the same per-R set as `$__drop_<R>`.
        out.push_str(&format!(
            "fn __drop_list_opt_{fname}(xs: List[Option[{tname}]]) -> Unit = {{\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_opt_{fname}_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}}\n\
             fn __drop_list_opt_{fname}_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else {{ let e: Option[{tname}] = prim.load_handle(h + 12 + i * 8)\n         __drop_opt_{fname}(e)\n         __drop_list_opt_{fname}_loop(h, n, i + 1) }}\n"
        ));
    }
    // The SCALAR-ONLY records (not in `rec_names`): their `Option[R]` / `List[Option[R]]`
    // wrappers still need the tag-aware pair — the Some payload's free is ONE `rc_dec`
    // (a flat block), which the same `match`/loop shapes deliver, so the emission is
    // uniform with the recursive set above (the codec `lc: List[Inner?]` cell, #1134).
    // Generic decls are excluded (an unparameterized generic name is not a source type).
    {
        let generic_names: std::collections::HashSet<&str> = type_decls
            .iter()
            .filter(|d| d.generics.as_ref().is_some_and(|g| !g.is_empty()))
            .map(|d| d.name.as_str())
            .collect();
        for decl in type_decls {
            let IrTypeDeclKind::Record { .. } = &decl.kind else { continue };
            let tname = decl.name.as_str();
            if rec_names.contains(tname) || generic_names.contains(tname) {
                continue;
            }
            let fname = drop_fn_ident(tname);
            out.push_str(&format!(
                "fn __drop_opt_{fname}(e: Option[{tname}]) -> Unit = {{\n  match e {{\n    some(r) => (),\n    none => (),\n  }}\n}}\n"
            ));
            out.push_str(&format!(
                "fn __drop_list_opt_{fname}(xs: List[Option[{tname}]]) -> Unit = {{\n  \
                   let h = prim.handle(xs)\n  \
                   if prim.load32(h + 0) == 1 then __drop_list_opt_{fname}_loop(h, prim.load32(h + 4), 0) else ()\n  \
                   prim.rc_dec(h)\n}}\n\
                 fn __drop_list_opt_{fname}_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
                   if i >= n then ()\n  \
                   else {{ let e: Option[{tname}] = prim.load_handle(h + 12 + i * 8)\n         __drop_opt_{fname}(e)\n         __drop_list_opt_{fname}_loop(h, n, i + 1) }}\n"
            ));
        }
    }
    // `$__drop_opt_str` — frees an `Option[String]` (the recursive-drop leaf of a `Result[Option[String],
    // String]`, the derived-Codec `__decode_option_string`). The `some(r)` arm binds the inner String
    // whose scope-end `rc_dec` frees it; consuming `e` frees the 0-or-1 Option block. Emitted ONLY when
    // the program constructs that shape (via `try_lower_result_option_scalar_str_ctor`'s `resrec:opt_str`),
    // so a program without it is not perturbed. (The scalar Option leaves — Int/Float/Bool — need no drop
    // fn: their `Result[Option[<scalar>], String]` frees flat via `DropListStr`.)
    if uses_result_opt_str {
        out.push_str(
            "fn __drop_opt_str(e: Option[String]) -> Unit = {\n  match e {\n    some(r) => (),\n    none => (),\n  }\n}\n",
        );
    }
    // `$__drop_tup_int_<R>` for each recursive-drop record R — frees a `(R, Int)` TUPLE
    // block (record handle @12 recursed via `$__drop_<R>`, the Int @20 is scalar), used
    // by `Result[(R, Int), String]` wrappers (`resrec:tup_int_<R>` — the gguf
    // parse_header `ok((GGUFHeader {…}, 24))` shape).
    for decl in type_decls {
        let IrTypeDeclKind::Record { .. } = &decl.kind else { continue };
        if !rec_names.contains(decl.name.as_str()) {
            continue;
        }
        let tname = decl.name.as_str();
        let fname = drop_fn_ident(tname);
        out.push_str(&format!(
            "fn __drop_tup_int_{fname}(e: ({tname}, Int)) -> Unit = {{
                 let h = prim.handle(e)
                 if prim.load32(h + 0) == 1 then {{
                     let r: {tname} = prim.load_handle(h + 12)
                     __drop_{fname}(r)
                 }} else ()
                 prim.rc_dec(h)
}}
"
        ));
    }
}

/// The per-element list wrapper for each synthesized ANON-record drop (and its
/// `(<anon>, <scalar>)` tuple-list twin) — a STRUCTURAL record-list literal
/// routes to `list_anonrec_<hash>`; without this wrapper the route referenced
/// a missing `$__drop_list_anonrec_<hash>`. Verbatim.
fn emit_anon_list_wrapper_drops(
    out: &mut String,
    anon_records: &[Vec<(almide_lang::intern::Sym, Ty)>],
) {
    // The SAME per-element list wrapper for each synthesized ANON-record drop — a
    // STRUCTURAL record-list literal (`take([{key: "x", val: "2"}])`, the checker
    // leaves the elements structural) routes to `list_anonrec_<hash>`; without this
    // wrapper the route referenced a missing `$__drop_list_anonrec_<hash>`.
    {
        let mut anon_sorted: Vec<&Vec<(almide_lang::intern::Sym, Ty)>> =
            anon_records.iter().collect();
        anon_sorted.sort_by_key(|fields| anon_record_drop_name(fields));
        anon_sorted.dedup_by_key(|fields| anon_record_drop_name(fields));
        for fields in anon_sorted {
            if !anon_record_needs_recursive_drop(fields) {
                continue;
            }
            let name = anon_record_drop_name(fields);
            let param_ty = anon_record_source_ty(fields);
            out.push_str(&format!(
                "fn __drop_list_{name}(xs: List[{param_ty}]) -> Unit = {{
                     let h = prim.handle(xs)
                     if prim.load32(h + 0) == 1 then __drop_list_{name}_loop(h, prim.load32(h + 4), 0) else ()
                     prim.rc_dec(h)
}}
                 fn __drop_list_{name}_loop(h: Int, n: Int, i: Int) -> Unit =
                     if i >= n then ()
                     else {{ let e: {param_ty} = prim.load_handle(h + 12 + i * 8)
         __drop_{name}(e)
         __drop_list_{name}_loop(h, n, i + 1) }}
"
            ));
            // The `(<anon record>, <scalar>)` tuple-list twin — same shape as the named
            // `$__drop_list_<R>_int` (see the rec_names loop below).
            out.push_str(&format!(
                "fn __drop_list_{name}_int(xs: List[({param_ty}, Int)]) -> Unit = {{
                     let h = prim.handle(xs)
                     if prim.load32(h + 0) == 1 then __drop_list_{name}_int_loop(h, prim.load32(h + 4), 0) else ()
                     prim.rc_dec(h)
}}
                 fn __drop_list_{name}_int_loop(h: Int, n: Int, i: Int) -> Unit =
                     if i >= n then ()
                     else {{ let th = prim.load64(h + 12 + i * 8)
         if prim.load32(th + 0) == 1 then {{ let r: {param_ty} = prim.load_handle(th + 12)
             __drop_{name}(r) }} else ()
         prim.rc_dec(th)
         __drop_list_{name}_int_loop(h, n, i + 1) }}
"
            ));
        }
    }
}

/// A per-element-recursive `$__drop_list_<R>` (+ the `(R, Int)` tuple-list
/// twin) for EVERY recursive-drop record R — so a standalone `List[R]` LITERAL
/// value routes its drop here too. Sorted for host-determinism. Verbatim.
fn emit_record_list_wrapper_drops(out: &mut String, rec_names: &std::collections::HashSet<String>) {
    let mut list_drop_names: Vec<&String> = rec_names.iter().collect();
    list_drop_names.sort();
    for rn in list_drop_names {
        // fn NAMES sanitize the module prefix; the `List[{rn}]` / `e: {rn}` type annotations keep
        // the dotted module-qualified name (a valid Almide type reference).
        let rn_fn = drop_fn_ident(rn);
        out.push_str(&format!(
            "fn __drop_list_{rn_fn}(xs: List[{rn}]) -> Unit = {{\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_{rn_fn}_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}}\n\
             fn __drop_list_{rn_fn}_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else {{ let e: {rn} = prim.load_handle(h + 12 + i * 8)\n         __drop_{rn_fn}(e)\n         __drop_list_{rn_fn}_loop(h, n, i + 1) }}\n"
        ));
        // The `(<R>, <scalar>)` TUPLE-list twin (`$__drop_list_<R>_int` — compound_eq's
        // `Map[P, Int]` from_list pairs, ListElemDrop::RecordInt): per element, the tuple's
        // slot0 record recurses via `$__drop_<R>`, slot1 is scalar (nothing to free), then the
        // tuple block frees. Mirrors `__drop_list_str_<V>`'s walk with the recursive slot
        // swapped. Scalar-slot-type-agnostic (the drop never reads slot1), so the `Int`
        // annotation covers Bool/Float instances too.
        out.push_str(&format!(
            "fn __drop_list_{rn_fn}_int(xs: List[({rn}, Int)]) -> Unit = {{\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_{rn_fn}_int_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}}\n\
             fn __drop_list_{rn_fn}_int_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else {{\n    \
                 let th = prim.load64(h + 12 + i * 8)\n    \
                 if prim.load32(th + 0) == 1 then {{\n      \
                   let r: {rn} = prim.load_handle(th + 12)\n      \
                   __drop_{rn_fn}(r)\n    \
                 }} else ()\n    \
                 prim.rc_dec(th)\n    \
                 __drop_list_{rn_fn}_int_loop(h, n, i + 1)\n  \
               }}\n"
        ));
    }
}

/// The DEMAND-gated shared sweep drops (`__drop_map_ss`, `__drop_matrix`,
/// `__drop_list_matrix`) — emitted only when a freed field shape demanded
/// them. Verbatim.
fn emit_demanded_shared_drops(
    out: &mut String,
    need_map_ss: bool,
    need_matrix: bool,
    need_list_matrix: bool,
) {
    if need_map_ss {
        // v1's `Map[String,String]` borrows the `map_skv` (String,Int) layout: the n KEYS are the
        // first n slots (`@ 12 + i*8`), DEEP-COPIED + owned by the map (`__skv_store_key` store_str);
        // the n VALUES are the next n slots, stored RAW (`store64`) — NOT owned by the map (the proper
        // owned-value `Map[String,String]` self-host is a separate brick, docs/roadmap v1-records-svg).
        // So the drop frees ONLY the owned key copies (rc_dec the first n slots) — freeing the borrowed
        // values would DOUBLE-FREE. (`n = load32(h+4)` is the entry count.)
        out.push_str(
            "fn __drop_map_ss(m: Map[String, String]) -> Unit = {\n  \
               let h = prim.handle(m)\n  \
               if prim.load32(h + 0) == 1 then __drop_map_ss_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}\n\
             fn __drop_map_ss_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else { prim.rc_dec(prim.load64(h + 12 + i * 8))\n         __drop_map_ss_loop(h, n, i + 1) }\n",
        );
    }
    if need_matrix {
        // The v1 Matrix free: at the block's last ref, `rc_dec` each owned flat row
        // (slot i64-widened handles @12 + i*8, count @4), then the block — the
        // `__drop_list_str` sweep typed over Matrix.
        out.push_str(
            "fn __drop_matrix(m: Matrix) -> Unit = {\n  \
               let h = prim.handle(m)\n  \
               if prim.load32(h + 0) == 1 then __drop_matrix_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}\n\
             fn __drop_matrix_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else { prim.rc_dec(prim.load64(h + 12 + i * 8))\n         __drop_matrix_loop(h, n, i + 1) }\n",
        );
    }
    if need_list_matrix {
        // A `List[Matrix]` field: each element recurses through `__drop_matrix`, then
        // the list block — the two-level sweep `DropListListStr` performs for values.
        out.push_str(
            "fn __drop_list_matrix(xs: List[Matrix]) -> Unit = {\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_matrix_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}\n\
             fn __drop_list_matrix_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else { let e: Matrix = prim.load_handle(h + 12 + i * 8)\n         __drop_matrix(e)\n         __drop_list_matrix_loop(h, n, i + 1) }\n",
        );
    }
    // `__drop_list_str` itself is no longer emitted HERE — it is now a SHARED source
    // block (`LIST_STR_DROP_SRC`) the pipeline injects once, gated by
    // `program_uses_list_str_drop_field` — the generated variant-drop generator ALSO
    // references this same fn name for its own `List[String]` ctor fields, and two
    // independent inline copies would be a duplicate-fn compile error. `need_list_str`
    // is still computed above (by `record_drop_field_frees`) purely to preserve that
    // function's shared signature with the anon-record caller; its value is unused here.
}
