# ADR-0036: Every route reaches code through one admission stage — `admit(route, target, artifact)` in almide-driver runs every pass/refuse gate, and only what it returns can be run, written or published

- **Status**: Accepted (design), 2026-11-19. Implementation waits for a VM; the bug fixes listed under [Migration](#migration) phase 0 land first, as they are.
- **Date**: 2026-11-19
- **Scope**: where the compiler's pass/refuse decisions run, relative to the routes (`check`, `check --json`, `check --effects`, `run`, `build`, `test`, `bench`, `emit`, LSP, MCP) and the targets (native, rust, wasm, wasi, component). The files involved are `src/compile_driver.rs`, `src/cli/{check,run,build,build_wasm,wasm_compile,test_native,test_wasm,bench,emit,verify,crate_inputs,mod}.rs`, `src/dispatch.rs`, `src/project.rs`, `crates/almide-driver`, and `crates/almide-wasm-run` (fs scope).
- **Related**:
  - The 2026-11-19 audits:
    - #4815: `check --json` / `check --effects` run no or partial `[permissions]`.
    - #4816: wasm test lane and wasm/wasi builds lack `[permissions] fs`.
    - #4829: an unparsable manifest is read as no manifest.
    - #4830: the native-grant gate and the build read different manifests.
    - #4831: native `bench` lacks the fs scope.
    - #4833: a process-global fs scope is shared by parallel test files.
    - #4835: temp-dir entries find a manifest above TMPDIR.
    - #4836: the certificate gate runs only in `build --target wasm`.
    - #4842: two Tool-scope switches relax a check in release.
    - #4843: `emit --target rust` lacks the fs scope.
  - The name-keyed findings #4823–#4828 and #4840 concern what a gate decides from. This ADR concerns where a gate runs. The two meet in the decision-site ledger.
  - #3275 (the wasm route skipped `[permissions]`) and #4151–#4159 (the earlier route audit) are the same class, found before.
  - `docs/roadmap/active/security-by-construction.md` rows 3, 10, 14, 15, 17/18 and 19.
  - `docs/contracts/proven-vs-trusted.md` ("What each gate actually claims": `scripts/check-build-certifies.sh`).
  - ADR-0035 (a dependency is a typed identity): the manifest and dependency half of the same "one reader" principle.

## Context

The compiler's pass/refuse decisions include `[permissions]` allow / proc / fs / native / media, E109, the compile budgets, the certificate gate, and the native-grant backstop. Each one is called from inside individual commands. Each command was written with its own list of gates, and the list differs:

| Gate | Where it is called today | Routes that miss it |
|---|---|---|
| E109 + `[permissions]` allow/proc + budgets | `compile_driver::optimize_gate_and_link` (build routes), `check.rs:238`, `emit.rs:210`, `test_wasm.rs:456` | `check --json`, `check --effects` through a module (#4815) |
| fs scope, native | `build::inject_fs_scope` at `build.rs:359`, `run.rs:66`, `test_native.rs:367` | `bench` (#4831), `emit --target rust` (#4843) |
| fs scope, wasm | `almide_wasm_run::set_fs_scope`, a process-global (`cli/mod.rs:304`) | the wasm test lane run from outside the project, wasm/wasi build artifacts (#4816); parallel test files overwrite one another's scope (#4833) |
| certificate (`verify::gate_build`) | `build_wasm.rs:103`, core module only | `run` / `test` / `bench --target wasm`, components (#4836) |
| manifest lookup | `resolve_cwd_deps_or_exit` (`./almide.toml`), `manifest_for_entry` (walk up from the entry), `load_native_build_config` (the entry's directory, then cwd), `cwd_project` (`.ok()`) | the readers disagree (#4830); a parse failure is read as "no manifest" (#4829); a temp-dir entry walks up into TMPDIR (#4835) |
| env switch policy | the release lock holds back `Gate` / `Ablation` switches | two `Tool` switches relax a check (#4842) |

The decision-site inspection of 2026-11-19 (`proofs/decision-sites.toml`, see [The ledger](#the-ledger)) gives a row to each of 143 decision sites, in 14 route columns. Of its 107 UNSTAMPED rows, **46 fail because a route is skipped**, and the draft holds **193 `skipped` route cells**:

| bench | check --json | test | wasi | emit | check --effects | run | MCP | component | build | native | LSP | check | wasm |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 23 | 22 | 20 | 19 | 17 | 16 | 15 | 12 | 11 | 9 | 9 | 7 | 7 | 6 |

Each of these gaps has the same cause. A gate is called at a place a route chooses to pass through; nothing makes the route pass there. A new route, a new target or a refactor of one command creates a gap without anything noticing. `optimize_gate_and_link` was the first step against this (#3275). It holds the IR-level gates for the build routes. But `check`, `emit` and the wasm test lane call the same gate a second time from their own code, and every artifact-level gate (fs scope, certificate, native grants) still lives in the commands. Fixing each route locally closes the instance. It does not close the class: #4151–#4159 closed seven route gaps in October, and the November audit found eleven more.

## Decision

1. **One stage.** `almide-driver` gets `admit(route, target, artifact) -> Result<Admitted<A>, Refusal>`. It is the only place a pass/refuse gate for a program runs. Each per-command call listed in Context is removed.
2. **Only admitted things reach a sink.** The functions that execute, write, or publish a compiler product take `Admitted<A>`, never `A`. Those sinks are:
   - spawn a native binary;
   - instantiate a module on the embedded host;
   - hand a module to an external runner;
   - write an artifact to disk;
   - print emitted source;
   - publish diagnostics over LSP;
   - answer an MCP tool.

   `Admitted<A>` has a private constructor in `almide-driver`. A route that skips `admit` does not compile, so the type system enforces the rule instead of a convention or review.
3. **One project resolution feeds it.** `admit` takes a `ProjectRef`, resolved once from the entry file. A manifest that does not parse is a `Refusal`, not `None`. The cwd fallback and the TMPDIR walk-up are removed. The manifest unification in progress for #4829 / #4830 / #4835 becomes this resolution.
4. **Scope travels with the artifact, never through a global.** The fs scope, proc list and preopens that `admit` derives are fields of `Admitted<A>`. The wasm host and the native `main` read them from there. `set_fs_scope` and the process-global `SCOPE` are deleted (#4833).

## The stage's contract

### What an artifact is

```rust
pub enum Artifact {
    Verdict,                 // check, check --json, check --effects, LSP, MCP: a program judged, nothing produced
    RustSource(String),      // emit --target rust; the input of every native build
    NativeBinary(PathBuf),   // run / build / test / bench, native
    WasmCore(Vec<u8>),       // run / build / test / bench --target wasm
    WasiModule(Vec<u8>),     // build --target wasi
    Component(Vec<u8>),      // build --component (its core module is admitted first)
}
pub enum Route { Check, CheckJson, CheckEffects, Run, Build, Test, Bench, Emit, Lsp, Mcp }
pub enum Target { Native, Rust, Wasm, Wasi, Component }
```

`Verdict` is an artifact on purpose. The check routes are where a model or an editor learns whether a program is admissible. If they run fewer gates than `build`, they report "ok" for a program `build` refuses. That is #4815.

### What `admit` checks, in this order

| # | Gate | Today | Applies to |
|---|---|---|---|
| A1 | project resolution: the entry's own manifest, parsed or refused; switch policy (release lock, including the reader-scoped rule of #4842) | the four lookups above, `cwd_project` | every route |
| A2 | program gates on the whole lowered program: E109, `[permissions]` allow (+ `allow = []` deny-all), proc, the fs platform check, compile budgets E111–E113 | `optimize_gate_and_link`, `check.rs:238`, `emit.rs:210`, `test_wasm.rs:456` | every route, including `Verdict` and LSP |
| A3 | dependency gates: `[permissions] native` and `[trust] media` grants over the fetched graph, on the same file set the resolver loads (#4809, fixed on bugs-final) | `resolve_cwd_deps_or_exit` (dispatch), `crate_inputs` backstop | every route that resolves dependencies; LSP is cache-only (security row 25) |
| A4 | scope binding: the fs scope, proc list and preopens become fields of `Admitted`; for `RustSource` the scope is injected into `main` | `inject_fs_scope` ×3, `set_fs_scope` | every artifact except `Verdict` |
| A5 | certificate: `verify::gate_build` over every wasm core module, a component's included | `build_wasm.rs:103` only | `WasmCore`, `WasiModule`, `Component`, on run / build / test / bench |
| A6 | native backstop: the `CrateInputs` grant re-check on the crate actually built | `crate_inputs.rs:68` | `NativeBinary` |

Two rules apply to every gate:
- **Fail closed.** An error inside a gate is a `Refusal`. The `.ok()` / `if let Some` admit paths that #4829 and #4839 found are not allowed inside `admit`.
- **Same verdict on every route.** A route may skip a gate only when its artifact type has no meaning for that gate: A5 on `RustSource`, or A4 on `Verdict`. The table above is the complete list of such omissions. Each one appears as `n.a.` with a reason in the ledger's route columns.

**No verifier installed.** A missing `almide-verify` is a refusal, like every other gate that cannot run. The release archives already ship `almide-verify` next to `almide`, so only hand-built or partially copied installs are affected. The refusal says where `almide-verify` was looked for and how to install it. This replaces the documented exception in `proven-vs-trusted.md` (a build without the verifier was not checked), which is updated in the same change.

**Cost.** A5 on `run` and `test` adds a verifier pass per module. `Admitted<WasmCore>` is cached by the module's bytes, next to the existing wasm run cache. The VM phase measures the overhead on the spec corpus before A5 is enabled on `test`.

## How every route and target reaches it

| Route | native | rust | wasm | wasi | component |
|---|---|---|---|---|---|
| check / check --json / check --effects | `Verdict`: A1–A3 | same | same | same | same |
| LSP | `Verdict`: A1–A3, cache-only A3, refusal published as diagnostics (never `exit`) | same | same | same | same |
| MCP `almide_check` | child `almide check --json` → `Verdict` | same | same | same | same |
| emit | — | `RustSource`: A1–A4 | n.a. (emit has no wasm form) | n.a. | n.a. |
| run | `NativeBinary`: A1–A4, A6 | via native | `WasmCore`: A1–A5 | `WasiModule`: A1–A5 | `Component`: A1–A5 |
| build | `NativeBinary`: A1–A4, A6 | via native | `WasmCore`: A1–A5 | `WasiModule`: A1–A5 | `Component`: A1–A5 |
| test | each file → its own `Admitted`, scope per file (#4833) | via native | `WasmCore` per file, the file's own project (#4816) | n.a. (no wasi test lane) | n.a. |
| bench | `NativeBinary`: A1–A4, A6 (#4831) | via native | `WasmCore`: A1–A5 | n.a. | n.a. |

The row and column sets are the route and target enums, so a new route or target is a new arm in an exhaustive match inside `admit`. It cannot be added without deciding its gates.

## Migration

- **Phase 0 (in progress, unchanged by this ADR).** The bug engineer's local fixes land as they are:
  - manifest lookup unified (#4829, #4830, #4835);
  - certificate gate on run/test/bench (#4836);
  - `inject_fs_scope` on bench (#4831) and emit (#4843).

  Each fix is a correct gate at a correct place. Phase 1 moves it, without rewriting it. Their tests become route-matrix rows.
- **Phase 1: the stage.** Add `Artifact` / `Route` / `Target` / `Admitted` / `Refusal` to `almide-driver`. `admit` wraps `optimize_gate_and_link` (A2) and absorbs, unchanged in behaviour:
  - the unified manifest resolution (A1);
  - `enforce_native_grants` / media grants (A3);
  - `inject_fs_scope` and `set_fs_scope` (A4);
  - `gate_build` (A5);
  - the `CrateInputs` backstop (A6).

  Every route calls `admit`. The old call sites stay temporarily, and the route-matrix test must stay green.
- **Phase 2: the sinks.** Change the sink signatures to take `Admitted<A>`. The compiler then lists every route that does not yet call `admit`; fix each one. Delete the per-command gate calls, the global `SCOPE`, and `set_fs_scope`.
- **Phase 3: the gates.** Land the CI gate below. Flip the ledger rows whose c3 gap the stage closed, and lower `skipped_route_ceiling` in the same PR.

Each phase is one PR with the route-matrix test run on every route × target before and after. A phase that turns a `runs` cell into anything else is refused.

## The CI gate

The gate is the decision-site gate. Its rules live in the `proofs/decision-sites.toml` header when it lands:

- **Detection, in Rust (`syn`), sharing its scanner with `tests/name_keyed_sites_test.rs`.** It finds every decision site by what it emits, never by its name:
  - S1: a diagnostic code marked `decision` in `docs/diagnostics/codes.toml`;
  - S2: construction of a refusal carrier (after this ADR, `Refusal`);
  - S3: a read of a switch that relaxes a check;
  - S4: the route and target enums, which supply the column set.

  Each site is keyed `file::item::signal::payload`. A site without a row fails, and so does a row without a site.
- **After this ADR, S4 is one fact, not 14 per row.** The test asserts two things:
  - every sink function takes `Admitted<_>`;
  - every `Refusal` is constructed inside `admit` or a function it calls.

  A gate called outside the stage is a detected site with no stage path, and it fails.
- **Route matrix.** One test generalises `tests/permissions_routes_test.rs`. It runs each ledger row's probe on every `runs` cell (CLI argv, LSP over stdio, MCP over stdio), from inside and outside the project, and requires the refusal.
- **Controls.** Each STAMPED row has a `proofs/decision-controls/<id>.patch`. PR CI checks that each patch still applies. The nightly or a VM applies it and requires the row's test to go red.
- **The judge is Almide** (`tools/almide-gates decision-sites`, #2128). It reads the ledger, `codes.toml`, `name-keyed-sites.toml` and `gate-verification.toml`, and enforces the four conditions and the shrink-only ceilings.

## The ledger

`proofs/decision-sites.toml` lands with the gate's first PR. It has one row per decision site, with these fields:
- `id`, `at`, `decides`;
- `from` (declaration / type / reach / name / prefix / param-name / text);
- `unknown` (refuse / allow);
- one cell per route column;
- `tests`, `control`, `issues`, `status`, `reasons`.

A row is STAMPED when four conditions hold:
1. it decides from declarations, types or reach;
2. it refuses an unknown item, with a novel-item test;
3. it runs on every route, or the route is `n.a.` with a reason;
4. it has a negative control.

Ceilings for unstamped, unclassified, skipped route cells and fail-open rows only fall.

How it relates to the other ledgers:
- **`proofs/gate-verification.toml`** covers CI scripts. This ledger covers in-compiler decisions. A row may cite a script as its control, but never copies it.
- **`proofs/name-keyed-sites.toml`** (being widened to prefixes, parameter names, single names and fail-open `_ =>` defaults) is joined for condition 1 and the default-arm half of condition 2. The decision gate does not detect names itself.
- **This ADR closes condition 3 by construction for every site moved into `admit`.** Such a row's route cells derive from the table in [How every route and target reaches it](#how-every-route-and-target-reaches-it), instead of being proved one row at a time. Of the 46 UNSTAMPED rows that fail condition 3, all but the run-time host rows (unix sockets #4832, shm temp files) move into the stage. Those keep per-row route evidence.

All 143 rows are stamped before the v0.70.0 final.

## Alternatives

- **Keep the gates per command, add the route-matrix test only.** This was rejected. The test finds a gap after a route exists. The next route repeats the omission, and the matrix grows a column that every row must re-prove. That is the state that produced 193 skipped cells.
- **Put the gates in codegen.** Rejected for three reasons:
  - codegen sees neither the route nor the manifest;
  - LSP and the check routes have no codegen;
  - the certificate is judged on the bytes after codegen.
- **Enforce only at run time, in the host.** Rejected for three reasons:
  - native has no host between the program and the OS;
  - a check route must refuse before anything runs;
  - the embedded host's checks remain, as defence in depth behind A4.
- **A wrapper function every command is asked to call** (today's `optimize_gate_and_link`). This was rejected. It is the current design, and `check`, `emit` and the wasm test lane went around it. Without `Admitted<A>` at the sinks, nothing forces a route through the wrapper.

## Consequences

- A new route or target cannot reach a sink without deciding its gates. The decision is one exhaustive match, reviewed in one place.
- `check` and `build` give the same verdict for the same program. Today a model is told "ok" by `check --json` and refused by `build`.
- One manifest reader, so the parse-failure and reader-disagreement classes (#4829, #4830, #4835) have no second reader to disagree with.
- The cost is a refactor of every command's tail and the sink signatures. A5 adds verifier time to wasm `run` and `test`; the cache by module bytes bounds it, and the VM phase measures it.
- LSP must take a `Refusal` as diagnostics, not as process exit. `admit` returns and never exits.

## Falsifier

- A route reaches a sink without `Admitted<A>`, through a sink signature the S4 assertion does not see. The type-level rule is then not complete, and this ADR's claim that condition 3 holds "by construction" is withdrawn for that sink.
- A5 on `run` / `test` costs more than 10 % of the median route time on the spec corpus even with the byte-keyed cache. In that case A5 moves to `build` + nightly for `run` / `test`, and the ledger records those cells as `skipped` with this ADR as the reason.
