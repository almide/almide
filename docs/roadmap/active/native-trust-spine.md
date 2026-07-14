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
- [ ] Rung 2: dynamic String ops (`__str_concat`, comparisons, `string.len`) as
      shims; heap params/returns on user fns (borrowed `&str` / owned `String`
      per the MIR call-mode signature).
- [ ] Rung 3: `List[Int]` / `List[String]` (`Vec<i64>` / `Vec<String>`), the
      typed Drop family mapped to scope-end (`DropListStr` etc. erase once the
      element type is a real Rust type — no recursive free needed natively).
- [ ] Rung 4: records/variants (native structs/enums), Float (real `f64` — no
      i64-bits convention on native), closures.
- [ ] Rung 5: `almide run --verified` wiring; org byte-verify sweep column for
      the native leg; multi-module + top-lets.
- [ ] Default flip: v1-first native (`--no-verified` opt-out), README memory
      claim updated to the unified statement — closes #764.

## Invariants (every rung)

- A v1-rendered program is NEVER wrong: anything outside the subset returns
  `Err` and the CLI falls back to v0 (`src/cli/build.rs`).
- `verify_ownership` runs on every lowered fn before render.
- The differential gate corpus only grows; a rung ships WITH its corpus rows.
- The shim floor is a CLOSED map (`render_native.rs::shim`) — additions are
  trusted-floor changes and need a differential row in the same PR.
