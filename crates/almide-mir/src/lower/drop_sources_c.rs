
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

/// The ALMIDE SOURCE of `__drop_opt_str_int` — the recursive release of an
/// `Option[(String, Int)]` (`map.find`'s predicate-search result: `Some((key,
/// value))` on a hit). Wrapper `[rc][len@4=0-or-1 (Option's tag)][cap@8][@12
/// payload]`: at the wrapper's last ref (rc==1), IFF len==1 (Some) the @12
/// payload is the `(String, Int)` tuple's handle — at the TUPLE's own last ref,
/// `rc_dec` its String slot0 @12 (the Int slot1 @20 is scalar), then the tuple
/// block; len==0 (None) frees nothing at the payload. THEN the wrapper block,
/// always. A blind flat `rc_dec` of the @12 payload slot (the generic
/// `heap_elem_lists`/`DropListStr` route every OTHER self-host Option call
/// uses) would only decrement the TUPLE's own refcount, leaking its String —
/// the exact class of bug this session's `_str`-dispatch fix caught elsewhere.
/// Named for the (String, Int) shape specifically; a (String, Bool)/(String,
/// Float) `map.find` result reuses the SAME generated fn (the render never
/// reads the Int slot's bits, only rc_decs the String slot — the established
/// type-stand-in convention, e.g. `list_hshare.almd`'s `List[Int]` stand-in).
pub const OPT_STR_INT_DROP_SRC: &str = "\
fn __drop_opt_str_int(o: List[Int]) -> Unit = {
  let h = prim.handle(o)
  if prim.load32(h + 0) == 1 then {
    if prim.load32(h + 4) == 1 then {
      let th = prim.load64(h + 12)
      if prim.load32(th + 0) == 1 then prim.rc_dec(prim.load64(th + 12)) else ()
      prim.rc_dec(th)
    } else ()
  } else ()
  prim.rc_dec(h)
}
";

/// The ALMIDE SOURCE of `__drop_opt_str_str` — the recursive release of an
/// `Option[(String, String)]` (the if-merged `some((s1, s2))` ctor the fuzz
/// index-374 divergence exposed): at the wrapper's last ref, IFF Some the @12
/// payload tuple owns TWO Strings (@12 and @20 — both rc_dec'd at the tuple's
/// last ref), then the tuple block, then the wrapper. The `__drop_opt_str_int`
/// twin with the second slot's dec added.
pub const OPT_STR_STR_DROP_SRC: &str = "\
fn __drop_opt_str_str(o: List[Int]) -> Unit = {
  let h = prim.handle(o)
  if prim.load32(h + 0) == 1 then {
    if prim.load32(h + 4) == 1 then {
      let th = prim.load64(h + 12)
      if prim.load32(th + 0) == 1 then {
        prim.rc_dec(prim.load64(th + 12))
        prim.rc_dec(prim.load64(th + 20))
      }
      else ()
      prim.rc_dec(th)
    } else ()
  } else ()
  prim.rc_dec(h)
}
";

/// Header layout: `n_heap | (n_nested_heap << 16) | (n_closure << 32)` — three
/// 16-bit counts (ample for any realistic capture count). Widened from the
/// original 2-field `n_heap | (n_closure << 16)` to add the `n_nested_heap`
/// class (a `List[String]` capture — each element itself owned heap, freed via
/// `__drop_list_str`, NOT the flat `rc_dec` a one-level-exact heap capture
/// gets — the same class of bug this session's `_str`-dispatch fix and the
/// `map.find` near-miss both found, caught here BEFORE it shipped).
pub const CLOSURE_DROP_SRC: &str = "\
fn __drop_closure(c: List[Int]) -> Unit = {
  let h = prim.handle(c)
  if prim.load32(h + 0) == 1 then {
    let hdr = prim.load64(h + 20)
    let ncm = hdr / 281474976710656
    let rem0 = hdr - ncm * 281474976710656
    let nc = rem0 / 4294967296
    let rem1 = rem0 - nc * 4294967296
    let nnh = rem1 / 65536
    let nh = rem1 - nnh * 65536
    __drop_closure_loop(h, nc, nnh, nh, ncm, 0)
  } else ()
  prim.rc_dec(h)
}
fn __drop_closure_loop(h: Int, nc: Int, nnh: Int, nh: Int, ncm: Int, i: Int) -> Unit =
  if i >= nc + nnh + nh + ncm then ()
  else {
    if i < nc then {
      let q: List[Int] = prim.load_handle(h + 28 + i * 8)
      __drop_closure(q)
    } else if i < nc + nnh then {
      let ls: List[String] = prim.load_handle(h + 28 + i * 8)
      __drop_list_str(ls)
    } else if i < nc + nnh + nh then {
      prim.rc_dec(prim.load64(h + 28 + i * 8))
    } else {
      __drop_cellmap(prim.load64(h + 28 + i * 8))
    }
    __drop_closure_loop(h, nc, nnh, nh, ncm, i + 1)
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
        out.push_str(&record_drop_field_frees(
            &field_tys,
            &rec_names,
            &flat_variant_names,
            &rec_variant_names,
            &generic_decls,
            &mut list_drops,
            &mut need_map_ss,
            &mut need_list_str,
            &mut need_matrix,
            &mut need_list_matrix,
        ));
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
    }
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
        out.push_str(&record_drop_field_frees(
            &field_tys,
            &rec_names,
            &flat_variant_names,
            &rec_variant_names,
            &generic_decls,
            &mut list_drops,
            &mut need_map_ss,
            &mut need_list_str,
            &mut need_matrix,
            &mut need_list_matrix,
        ));
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
    }
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
    // A per-element-recursive `$__drop_list_<R>` for EVERY recursive-drop record R (not just the
    // field-referenced ones in `list_drops`) — so a standalone `List[R]` LITERAL value (`group([…])`)
    // routes its drop here too. Sorted for host-determinism.
    let _ = &list_drops; // (subsumed by rec_names below)
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
    let _ = need_list_str;
    out
}

/// The C-015 STRING-FIELD-record key/element twins (`__krec_*`) — generated per
/// record shape used as a Map key / Set element / `list.unique` element anywhere
/// in the program. The key normalizes INJECTIVELY into a String (each String
/// field length-prefixed `<len>:<bytes>,`, each scalar field `<digits>,` — the
/// netstring discipline, so distinct field values can never collide), and the
/// backing container is the proven `_str`/`_skv` family; `krec_call_name`
/// (control_p2.rs) routes the call sites to these names. Over-generation is
/// harmless (a shape whose call never fires leaves inert fns); a record with a
/// non-String/scalar field is never collected (its calls keep their wall).
pub fn generate_krec_sources(
    program: &almide_ir::IrProgram,
    type_decls: &[almide_ir::IrTypeDecl],
) -> String {
    use almide_ir::visit::{walk_expr, IrVisitor};
    use almide_lang::types::constructor::TypeConstructorId;

    // Admissible record decls: name -> declaration-ordered field types.
    let recs: std::collections::HashMap<String, Vec<Ty>> = type_decls
        .iter()
        .filter_map(|d| match &d.kind {
            almide_ir::IrTypeDeclKind::Record { fields } => {
                let tys: Vec<Ty> = fields.iter().map(|f| f.ty.clone()).collect();
                (!tys.is_empty()
                    && tys.iter().all(|t| matches!(t, Ty::Int | Ty::Bool | Ty::String))
                    && tys.iter().any(|t| matches!(t, Ty::String)))
                .then(|| (d.name.as_str().to_string(), tys))
            }
            _ => None,
        })
        .collect();
    if recs.is_empty() {
        return String::new();
    }

    #[derive(Default)]
    struct Uses {
        map_iv: std::collections::BTreeSet<String>,
        map_sv: std::collections::BTreeSet<String>,
        sets: std::collections::BTreeSet<String>,
        uniques: std::collections::BTreeSet<String>,
        /// STRUCTURAL record element shapes (anon hash -> field types, SOURCE order).
        uniq_structs: std::collections::BTreeMap<String, Vec<Ty>>,
    }
    struct Scan<'a> {
        recs: &'a std::collections::HashMap<String, Vec<Ty>>,
        uses: Uses,
    }
    impl Scan<'_> {
        fn note(&mut self, ty: &Ty) {
            match ty {
                Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2 => {
                    if let Ty::Named(n, _) = &a[0] {
                        if self.recs.contains_key(n.as_str()) {
                            match &a[1] {
                                Ty::Int | Ty::Bool => {
                                    self.uses.map_iv.insert(n.as_str().to_string());
                                }
                                Ty::String => {
                                    self.uses.map_sv.insert(n.as_str().to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Ty::Applied(TypeConstructorId::Set, a) if a.len() == 1 => {
                    if let Ty::Named(n, _) = &a[0] {
                        if self.recs.contains_key(n.as_str()) {
                            self.uses.sets.insert(n.as_str().to_string());
                        }
                    }
                }
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                    if let Ty::Named(n, _) = &a[0] {
                        if self.recs.contains_key(n.as_str()) {
                            self.uses.uniques.insert(n.as_str().to_string());
                        }
                    }
                    // An UNANNOTATED literal's STRUCTURAL record element — keyed by
                    // the anon hash, fields in the block's SOURCE order (r5 lesson).
                    if let Ty::Record { fields } = &a[0] {
                        if !fields.is_empty()
                            && fields
                                .iter()
                                .all(|(_, t)| matches!(t, Ty::Int | Ty::Bool | Ty::String))
                            && fields.iter().any(|(_, t)| matches!(t, Ty::String))
                        {
                            self.uses.uniq_structs.insert(
                                crate::lower::anon_record_drop_name(fields),
                                fields.iter().map(|(_, t)| t.clone()).collect(),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
    impl IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            self.note(&e.ty);
            walk_expr(self, e);
        }
    }
    let mut scan = Scan { recs: &recs, uses: Uses::default() };
    for f in program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()))
    {
        for p in &f.params {
            scan.note(&p.ty);
        }
        scan.note(&f.ret_ty);
        almide_ir::visit::IrVisitor::visit_expr(&mut scan, &f.body);
    }
    let uses = scan.uses;
    if uses.map_iv.is_empty()
        && uses.map_sv.is_empty()
        && uses.sets.is_empty()
        && uses.uniques.is_empty()
        && uses.uniq_structs.is_empty()
    {
        return String::new();
    }

    let mut out = String::new();
    let mut norm_emitted: std::collections::BTreeSet<String> = Default::default();
    let mut emit_norm_tys = |out: &mut String, r: &str, tys: &[Ty]| {
        if !norm_emitted.insert(r.to_string()) {
            return;
        }
        out.push_str(&format!("fn __krec_norm_{r}(k: Value) -> String = {{\n"));
        out.push_str("  let h = prim.handle(k)\n");
        out.push_str("  let a0 = \"\"\n");
        for (i, t) in tys.iter().enumerate() {
            let off = 12 + 8 * i;
            let prev = format!("a{i}");
            let cur = format!("a{}", i + 1);
            if matches!(t, Ty::String) {
                out.push_str(&format!(
                    "  let s{i}: String = prim.load_str(h + {off})\n  \
                     let {cur} = {prev} + int.to_string(string.len(s{i})) + \":\" + s{i} + \",\"\n"
                ));
            } else {
                out.push_str(&format!(
                    "  let {cur} = {prev} + int.to_string(prim.load64(h + {off})) + \",\"\n"
                ));
            }
        }
        out.push_str(&format!("  a{}\n}}\n", tys.len()));
    };

    // `r` is a record NAME (`recs`' HashMap key) — a cross-module record carries
    // its dotted module prefix (`m.Cfg`), which is only valid Almide syntax as a
    // TYPE reference. Every `__krec_*` string below uses it as a FUNCTION NAME, so
    // each loop derives the sanitized `rf` (via `drop_fn_ident`, the same dots→
    // underscores mangling `generate_record_drop_sources` applies) and formats
    // with `{rf}`, keeping `r`/`recs[r]` only for the HashMap lookup.
    for r in uses.map_iv.iter() {
        let rf = drop_fn_ident(r);
        emit_norm_tys(&mut out, &rf, &recs[r]);
        out.push_str(&format!(
            "fn __krec_mfl_{rf}_iv_at(pairs: List[(Value, Int)], i: Int, m: Map[String, Int]) -> Map[String, Int] =\n  \
               if i >= list.len(pairs) then m\n  \
               else match list.get(pairs, i) {{\n    \
                 some(p) => {{\n      \
                   let (k, v) = p\n      \
                   __krec_mfl_{rf}_iv_at(pairs, i + 1, map.set(m, __krec_norm_{rf}(k), v))\n    }},\n    \
                 none => m,\n  }}\n\
             fn __krec_map_from_list_{rf}_iv(pairs: List[(Value, Int)]) -> Map[String, Int] = {{\n  \
               let m: Map[String, Int] = map.new()\n  \
               __krec_mfl_{rf}_iv_at(pairs, 0, m)\n}}\n\
             fn __krec_map_set_{rf}_iv(m: Map[String, Int], k: Value, v: Int) -> Map[String, Int] =\n  \
               map.set(m, __krec_norm_{rf}(k), v)\n\
             fn __krec_map_get_{rf}_iv(m: Map[String, Int], k: Value) -> Option[Int] =\n  \
               map.get(m, __krec_norm_{rf}(k))\n\
             fn __krec_map_contains_{rf}_iv(m: Map[String, Int], k: Value) -> Bool =\n  \
               map.contains(m, __krec_norm_{rf}(k))\n"
        ));
    }
    for r in uses.map_sv.iter() {
        let rf = drop_fn_ident(r);
        emit_norm_tys(&mut out, &rf, &recs[r]);
        out.push_str(&format!(
            "fn __krec_mfl_{rf}_sv_at(pairs: List[(Value, String)], i: Int, m: Map[String, String]) -> Map[String, String] =\n  \
               if i >= list.len(pairs) then m\n  \
               else match list.get(pairs, i) {{\n    \
                 some(p) => {{\n      \
                   let (k, v) = p\n      \
                   __krec_mfl_{rf}_sv_at(pairs, i + 1, map.set(m, __krec_norm_{rf}(k), v))\n    }},\n    \
                 none => m,\n  }}\n\
             fn __krec_map_from_list_{rf}_sv(pairs: List[(Value, String)]) -> Map[String, String] = {{\n  \
               let m: Map[String, String] = map.new()\n  \
               __krec_mfl_{rf}_sv_at(pairs, 0, m)\n}}\n\
             fn __krec_map_set_{rf}_sv(m: Map[String, String], k: Value, v: String) -> Map[String, String] =\n  \
               map.set(m, __krec_norm_{rf}(k), v)\n\
             fn __krec_map_get_{rf}_sv(m: Map[String, String], k: Value) -> Option[String] =\n  \
               map.get(m, __krec_norm_{rf}(k))\n\
             fn __krec_map_contains_{rf}_sv(m: Map[String, String], k: Value) -> Bool =\n  \
               map.contains(m, __krec_norm_{rf}(k))\n"
        ));
    }
    for r in uses.sets.iter() {
        let rf = drop_fn_ident(r);
        emit_norm_tys(&mut out, &rf, &recs[r]);
        out.push_str(&format!(
            "fn __krec_sfl_{rf}_at(xs: List[Value], i: Int, acc: Set[String]) -> Set[String] =\n  \
               if i >= list.len(xs) then acc\n  \
               else match list.get(xs, i) {{\n    \
                 some(x) => __krec_sfl_{rf}_at(xs, i + 1, set.insert(acc, __krec_norm_{rf}(x))),\n    \
                 none => acc,\n  }}\n\
             fn __krec_set_from_list_{rf}(xs: List[Value]) -> Set[String] = {{\n  \
               let acc: Set[String] = set.new()\n  \
               __krec_sfl_{rf}_at(xs, 0, acc)\n}}\n\
             fn __krec_set_insert_{rf}(s: Set[String], x: Value) -> Set[String] = set.insert(s, __krec_norm_{rf}(x))\n\
             fn __krec_set_contains_{rf}(s: Set[String], x: Value) -> Bool = set.contains(s, __krec_norm_{rf}(x))\n"
        ));
    }
    for (hash, tys) in uses.uniq_structs.iter() {
        emit_norm_tys(&mut out, hash, tys);
        let r = hash;
        out.push_str(&format!(
            "fn __krec_uniqfill_{r}(h: Int, oh: Int, n: Int, i: Int, cnt: Int, seen: Set[String]) -> Int =\n  \
               if i >= n then cnt\n  \
               else {{\n    \
                 let x: Value = prim.load_handle(h + 12 + i * 8)\n    \
                 let key = __krec_norm_{r}(x)\n    \
                 if set.contains(seen, key) then __krec_uniqfill_{r}(h, oh, n, i + 1, cnt, seen)\n    \
                 else {{\n      \
                   let e = prim.load64(h + 12 + i * 8)\n      \
                   prim.rc_inc(e)\n      \
                   prim.store64(oh + 12 + cnt * 8, e)\n      \
                   __krec_uniqfill_{r}(h, oh, n, i + 1, cnt + 1, set.insert(seen, key))\n    }}\n  }}\n\
             fn __krec_list_unique_{r}(xs: List[Value]) -> List[Value] = {{\n  \
               let h = prim.handle(xs)\n  \
               let n = prim.load32(h + 4)\n  \
               let out: List[Value] = prim.alloc_list_str(n)\n  \
               let seen: Set[String] = set.new()\n  \
               let cnt = __krec_uniqfill_{r}(h, prim.handle(out), n, 0, 0, seen)\n  \
               prim.store32(prim.handle(out) + 4, cnt)\n  \
               out\n}}\n"
        ));
    }
    for r in uses.uniques.iter() {
        let rf = drop_fn_ident(r);
        emit_norm_tys(&mut out, &rf, &recs[r]);
        out.push_str(&format!(
            "fn __krec_uniqfill_{rf}(h: Int, oh: Int, n: Int, i: Int, cnt: Int, seen: Set[String]) -> Int =\n  \
               if i >= n then cnt\n  \
               else {{\n    \
                 let x: Value = prim.load_handle(h + 12 + i * 8)\n    \
                 let key = __krec_norm_{rf}(x)\n    \
                 if set.contains(seen, key) then __krec_uniqfill_{rf}(h, oh, n, i + 1, cnt, seen)\n    \
                 else {{\n      \
                   let e = prim.load64(h + 12 + i * 8)\n      \
                   prim.rc_inc(e)\n      \
                   prim.store64(oh + 12 + cnt * 8, e)\n      \
                   __krec_uniqfill_{rf}(h, oh, n, i + 1, cnt + 1, set.insert(seen, key))\n    }}\n  }}\n\
             fn __krec_list_unique_{rf}(xs: List[Value]) -> List[Value] = {{\n  \
               let h = prim.handle(xs)\n  \
               let n = prim.load32(h + 4)\n  \
               let out: List[Value] = prim.alloc_list_str(n)\n  \
               let seen: Set[String] = set.new()\n  \
               let cnt = __krec_uniqfill_{rf}(h, prim.handle(out), n, 0, 0, seen)\n  \
               prim.store32(prim.handle(out) + 4, cnt)\n  \
               out\n}}\n"
        ));
    }
    out
}
