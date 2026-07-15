# Native Trust Spine — Perceus as the single memory model (#764)

> Goal: `almide build --target rust` flows through the SAME v1 Perceus MIR as the
> wasm leg, rendered to ownership-idiomatic Rust — `Dup` → `.clone()`, `Drop`
> erased to Rust's scope-end drop, the runtime boundary mapped to a closed native
> shim floor. `verify_ownership` (the Lean-mirrored checker) certifies the Perceus
> balance on the same ops the render erases — the Drop-erasure rendering theorem,
> checked per build. Ship pattern mirrors the wasm leg: opt-in → ladder → default.

## Architecture findings (2026-07-14, rung 1)

- User functions lower to TARGET-NEUTRAL MIR ops (`IntBinOp`/`CallFn`/control
  flow/`Alloc`/`Dup`/`Drop`) — the wasm-ness is concentrated in (a) the
  self-hosted runtime fns (prim/linear-memory based) and (b) heap handle reprs.
- The native leg therefore renders user fns directly and maps RUNTIME-BOUNDARY
  names (`int.to_string`, `print_str`, `__chk_div`, …) to a closed native shim
  floor — the same discipline as v0's `runtime/rs`, never re-implemented inline.
- Rendering the prim-based self-hosted bodies natively is explicitly rejected:
  it would emit a linear-memory emulator in Rust (correct, unidiomatic, slow).

## Ladder

- [x] **Rung 1** (shipped, opt-in `--verified` on `--target rust`): i64 scalars,
      String literals + `int.to_string`, `println`, full scalar control flow
      (if-as-value, loops), scalar user fns, C-001/C-002 abort shims.
      Gate: `tests/native_v1_differential_test.rs` — corpus byte-compared v1 vs
      v0 (stdout/stderr/exit), wall cases assert `Err`. HONEST WALL everywhere
      else (`__str_concat` was the first observed wall).
- [x] **Rung 2** (shipped): dynamic String ops as shims (`__str_concat`,
      `string.eq`, `string.len` — codepoint count); String params/returns on
      user fns (borrowed `&str` / owned `String` per the MIR call-mode
      signature); String-valued if-joins (decl patched at first arm yield);
      PRECISION WALL in the pipeline — a heap `Repr::Ptr` param/result renders
      as a string only when the DECLARED `Ty` says so, any signature outside
      {Int, Bool, String, Unit-ret} walls before lowering.
- [x] **Rung 3** (shipped): the String floor broadened to the full boundary
      surface reachable today — `string.contains` / `starts_with` / `ends_with`
      / `to_upper` / `to_lower` / `trim` / `repeat` / `cmp` (each shim is the
      EXACT v0 oracle expression, so C-016/C-019/C-020 discipline carries over)
      — plus `almide run --verified` native wiring (`compile_to_binary_with`).
- [x] **Rung 4 — SCALAR LISTS, SHIPPED via the shared-MIR ops (2026-07-15, the
      ceangal/module-var workstream built it as planned)**: three target-neutral
      ops — `Op::ListLit { dst, elems }` / `ListGetScalar { dst, list, idx }` /
      `ListSetScalar { list, idx, val }` — replace the inline `Alloc{DynList}` +
      `Handle`/`ElemAddr`/`Load|Store` prim sequences ONE-FOR-ONE at their three
      producers (the scalar literal builder, `lower_scalar_index_access`, the
      IndexAssign stores incl. the mutable-global path). render_wasm expands each
      to the exact prior WAT (behavior-identical; RATCHET 0 + KERNEL re-verified
      the whole corpus — the cert stream is UNCHANGED: ListLit is alloc-class
      `i`, get/set are neutral borrows, so no Coq vocabulary moved). The native
      leg types them `Vec<i64>`/`&[i64]` via a SIG-KIND table the pipeline builds
      from declared types (a heap `Repr::Ptr` alone cannot tell String from
      List), with bounds shims aborting byte-identically to `$elem_addr_chk`
      ("Error: index out of bounds", exit 1). Differential corpus: list_param /
      list_index_math / list_set. Scope: `List[Int]`/`List[Bool]` signatures
      (Float lists ride rung 5's f64 convention); `list.len` stays the
      self-host CallFn (already target-neutral — 4b decides whether to op it).
- [ ] Rung 4 residue (original design note kept for the record):
      the v1 lower materializes list literals as `Alloc{DynList}` + inline
      `Prim` stores and admits direct `xs[i]` prim loads over materialized
      lists — the list world is BELOW the prim floor by design, so no
      boundary-name mapping can reach it natively. Pattern-matching prim idioms
      in the native renderer is rejected (guessing, not op fidelity). The path:
      target-neutral list ops (e.g. `Op::ListLit`/`ListGet`/`ListLen`/`ListSet`)
      that render_wasm lowers to EXACTLY today's prim sequences (byte-identity
      guarded by the existing gates) and the native leg maps to `Vec` ops. This
      touches the same `lower/binds*` bricks the ceangal/module-var workstream
      is actively editing — do it WITH that workstream, not alongside it.
- [ ] Rung 5: records/variants (native structs/enums), closures.
      **Records slab SHIPPED (2026-07-15 late)**: scalar-record literals lower
      through `Op::ListLit` (declaration-ordered slots, zero-filled defaults —
      `try_lower_scalar_record_construct` keeps `materialized_aggregates`);
      scalar field reads through `Op::ListGetScalar` with the静的 slot index
      (`lower_scalar_field_access`, tuple path included; the pre-Handle block
      resolver split out as `resolve_aggregate_container_block`). Pipeline
      threads record/variant layouts through `try_render_rust_source` and
      admits all-scalar `Named` records as `NativeSigKind::ListI64`. Native
      renders the record block as `Vec<i64>`/`&[i64]`; the mask-driven
      `DropListStr` record drop erases to scope-end (all-scalar ⇒ empty mask).
      3-way byte-identical; differential rows record_field / record_out_of_order
      / record_return / record_float_field; classify wall-list byte-identical.
      **Variants next (same recipe, probed)**: the ctor block is the SAME
      DynList (slot0 = tag, slots1+ = payload — `try_lower_variant_ctor`'s
      Alloc+stores → ListLit with a leading tag const); match destructure = tag
      Load(slot 0) + Eq dispatch + payload loads (binds_p4:451/538 destructure
      sites → ListGetScalar); sig admission = scalar-payload variants → ListI64;
      drop = `DropVariant` erase arm on native (scalar payload ⇒ block free).
      **Records/variants scouting (2026-07-15, probe_native --mir over the REAL
      lower path — layouts threaded)**: a scalar record lowers to the SAME
      DynList block as a list (12-byte header + 8-byte slots; `Init::DynList`),
      and a field read is the raw `Handle + Add(12 + 8*slot) + Load` prim
      sequence — the below-prim-floor class rung 4 already solved for lists.
      Consequence: the records slab is smaller than feared — scalar-record
      literals can REUSE `Op::ListLit` verbatim (same block, same cert `i`),
      and field reads need ONE new op (`Op::FieldGetScalar { rec, slot }`,
      static offset, no bounds check, wasm render byte-equal to today's prim
      sequence; native `rec[slot]`). The prim sequences are emitted from
      MULTIPLE lower sites (binds.rs:672, calls_p2.rs:1270, binds_p4.rs:451/538,
      defunc_tuple_fold.rs ×2) — op-ify them one site at a time against the
      classify wall-list byte gate. Variant ctors are `CallFn "<Ctor>"` to
      SYNTHESIZED fns (assembler-side) whose bodies are prim stores — same
      op-ification unlocks them; match destructure needs the tag-dispatch
      lowering audited after that. CAUTION: my first probe used
      `lower_function_all` WITHOUT layouts and misread records as a lowering
      gap — always dump through `lower_function_all_with_globals` (the fixed
      `debug_dump_mir` does).
      **Float slab SHIPPED (2026-07-15)**: `NTy::F64`/`NativeSigKind::F64`;
      float literals stay bits-typed `i64` locals and convert at each float-op
      boundary (`f64::from_bits`, bit-exact); FloatBin Add/Sub/Mul/Div,
      FloatUn Neg/Abs/Sqrt/Floor/Ceil, FloatCmp (all IEEE-hardware-identical
      cross-target); Min/Max/CopySign wall (Rust vs wasm NaN semantics — only
      reachable from self-host bodies, which never render natively).
      `float.to_string` shims to the exact v0 oracle expression. Differential
      rows: float_print / float_arith / float_branch / float_fn_param, plus a
      3-way probe (v0-native == v1-native == wasm). `debug_dump_mir` +
      examples/probe_native.rs added as rung-development tooling.
      **Original design note (kept for the record)**: MIR carries Float as i64
      locals holding f64 BITS (`PrimKind::FloatUn/FloatBin` reinterpret around
      each op — render_wasm_p2 831-851). Native "real f64" needs: (1)
      `NTy::F64` + `NativeSigKind::F64` (the sig table already disambiguates
      declared Ty where `Repr` can't); (2) internal-local typing: a value is
      F64 iff produced by FloatBin/FloatUn or a Float-kind param/call-result —
      same op-driven inference `Str`/`Vec` locals use; (3) `FBits`/`FFromBits`
      prims become `f64::to_bits`/`from_bits` (or no-ops where the consumer is
      the matching reinterpret); (4) shims: `float.to_string` → the same
      `almide_rt_float_to_string` v0 calls (Dragon4, byte-identical);
      `prim.ffrombits(<const>)` on a literal folds to the exact f64 constant.
      Differential rows: float literal print, arithmetic chain, compare/branch,
      fn param/return, `float.to_string` round-trip. Gate: the corpus grows,
      wall shrinks by the `float` row currently asserted in
      `out_of_subset_walls_honestly`.
- [ ] Rung 6: org byte-verify sweep column for the native leg; multi-module +
      top-lets.
- [ ] Default flip: v1-first native (`--no-verified` opt-out), README memory
      claim updated to the unified statement — closes #764.

## Invariants (every rung)

- A v1-rendered program is NEVER wrong: anything outside the subset returns
  `Err` and the CLI falls back to v0 (`src/cli/build.rs`).
- `verify_ownership` runs on every lowered fn before render.
- The differential gate corpus only grows; a rung ships WITH its corpus rows.
- The shim floor is a CLOSED map (`render_native.rs::shim`) — additions are
  trusted-floor changes and need a differential row in the same PR.
