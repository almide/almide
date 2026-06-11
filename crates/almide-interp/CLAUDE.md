# almide-interp

A tree-walking interpreter over the **pre-codegen** `IrProgram` — the third leg of the cross-target oracle and an executable spec.

## Why it exists

The cross-target gate (`spec/wasm_cross/*.almd`, enforced by `tests/wasm_runtime_test.rs::wasm_cross_target_spec`) compares the **native** and **WASM** backends and requires byte-identical `(stdout, stderr, exit)`. That 2-way vote is structurally blind to a *both-wrong-the-same-way* bug: if codegen and the WASM emitter share a lowering pass that is wrong, both agree and the gate stays green while the behaviour is wrong.

This crate adds a third, independent judge. It evaluates the IR at the cut point **after** `lower → optimize → mono → ir_link` but **before** any of `almide-codegen`'s target-lowering passes (`ClosureConversion` / `Perceus` / `StdlibLowering` / `IterChain` / …). So it shares *none* of codegen's target passes with either backend. The ~22 codegen-inserted `IrExprKind` variants are unreachable here and `eval.rs` asserts them `unreachable!` to guard the boundary.

`tests/interp_cross_target_test.rs::interp_cross_target_spec` is the 3-way harness:
- `interp == native == wasm` → corroborated by a spec that cannot share a codegen bug.
- `native != wasm` → a backend split **owned by the 2-way `@xt-allow` gate**; this harness only logs which backend the interp sides with (tie-break diagnostic), it does NOT fail.
- `native == wasm` but interp dissents → **LOUD failure** (`BOTH-BACKENDS-WRONG` banner). Diagnose: fix the interpreter, or you just found a both-backends bug the 2-way gate is blind to.

## Module map

- `value.rs` — the dynamic `Value` model + the two display modes (`display_bare` = `println`/Display, `almide_repr` = compound/container form). Both replicate `almide_repr_prelude` (`crates/almide-codegen/src/lib.rs`) so they are byte-identical to native.
- `env.rs` — `VarId`-keyed, `Rc`-shared frames. Reproduces native `RcCow` capture semantics.
- `eval.rs` — the tree-walker for every eval-able IR node, fuel accounting, the pattern engine (incl. list patterns, which survive past the cut point), and record-repr nominal-name recovery.
- `dispatch.rs` — `Call` routing (`Named` / `Module` / `Method` / `Computed`), the variant-ctor registry, and the **HOF allowlist** (`is_hof`).
- `hofs.rs` — the in-interp HOFs (map/filter/fold/…) and the interp-native container ops, plus the in-place-mutation guard.
- `bridge.rs` — the scalar/string/math `(module, func)` bridge.

## Coverage model — does a NEW stdlib fn get covered automatically?

**No. A new stdlib fn needs manual glue here, or it becomes an honest skip.** Dispatch is three hand-maintained surfaces keyed by `(module, func)`:

1. `dispatch.rs::is_hof` — the closure-taking combinators (the allowlist).
2. `hofs.rs::eval_container_op` — non-HOF structural container ops.
3. `bridge.rs::dispatch` — scalar `int`/`float`/`string`/`math`/`bool` fns.

Anything not matched by one of these falls through to `Flow::Unsupported(...)`, which the 3-way gate records as a **data-driven, reasoned skip** (printed with its concrete reason; there is no hardcoded skip-list). Coverage shrinkage is no longer silent: the abstain set is audited against the committed `interp-abstain-ledger.txt` by `tests/interp_cross_target_test.rs::interp_abstain_ledger` (backend-free, never self-skips). A fixture abstaining without a ledger entry fails CI — so a new stdlib fn whose fixture the interp cannot run forces either interp glue or a reviewed ledger entry in the same PR — and a stale entry whose fixture now evaluates also fails (the ledger only shrinks; CG-1 gap-audit ratchet, issue #564). Widening this bridge surface (the partial json/regex/fan/map-constructor/bytes/string surface) is the highest-leverage follow-up: each added fn converts a skip into a real third vote and deletes ledger lines.

When you add a stdlib fn and want the oracle to cover it: add the arm to the right surface above, mirroring the native runtime fn's behaviour exactly (the Rust-std behaviour IS the oracle for the scalar fns). Do NOT path-depend on `almide_rt` for this — that crate pulls in rustls/webpki (network/TLS) and effectful surface the interp does not need.

## Rules

- **The interp must MATCH the backends, not "be correct" in the abstract.** Where the backends share a quirk (e.g. anonymous-record fields render in sorted order; `${float}` uses plain `{}` Display with no `.0`; the `0.30000000000000004` shortest-roundtrip), the interp replicates the quirk. A divergence here is a third vote, and a wrong third vote is worse than a skip.
- **A wrong vote is worse than an honest skip.** When the interp cannot faithfully model something (in-place `mut`-receiver container ops whose binding is unreachable by-value; platform-libm transcendentals that diverge from the backends' vendored musl-libm in the last ULP; non-deterministic `fan.*`), return `Flow::Unsupported` with a reason — never guess.
- **Stay at the pre-codegen cut point.** Codegen-inserted IR nodes (`Clone`, `Borrow`, `IterChain`, `ClosureCreate`, `RuntimeCall`, …) are `unreachable!` by construction. If one becomes reachable, the cut point moved — fix the boundary, don't silently handle it.
- **Skips are data-driven and loud.** Every skip is the interpreter's own `RunStatus::Unsupported`/`FuelExhausted`, logged with its reason. Never add a hardcoded skip-list to the harness.
- **Fuel is mandatory.** Every eval step burns one unit (`DEFAULT_FUEL`); deep recursion is bounded by `MAX_DEPTH`. An adversarial loop must terminate as `FuelExhausted`, never hang.
- **Display contracts are derived from codegen, with a cite.** When you touch `value.rs` display, point at the exact `almide-codegen` / `runtime/rs` line you are mirroring (as the existing comments do).
