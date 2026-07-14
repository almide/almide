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
- [ ] **Rung 4 — LISTS (needs a shared-MIR design, coordinate before building)**:
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
- [ ] Rung 5: records/variants (native structs/enums), Float (real `f64` — no
      i64-bits convention on native), closures.
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
