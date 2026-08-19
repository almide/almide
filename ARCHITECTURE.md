# Greenfield Architecture — Almide on a Query Spine

Status: DRAFT — load-bearing decisions listed in §6 are ratified one-by-one (○×).
Origin: full audit of almide v0.57.2 + ../almide-references (2026-08-19).

## 0. Premise

- **The language is still Almide.** Surface, dialect-epoch lineage, llms.txt,
  CHEATSHEET, and the `spec/` corpus are the crown-jewel assets and port over.
  This is a new engine under the same language, not a new language.
- **Mission unchanged:** modification survival rate (MSR). Every decision below
  is scored against "does an LLM's edit survive".
- **The analogy:** current almide is a steam locomotive fitted with the world's
  best instruments and safety systems (diagnostics, Lean belts, contract
  ledger, 3-way oracle). Greenfield moves those finished instruments — one at a
  time, unmodified where possible — onto an electric drivetrain (query core,
  single semantics). We replace the propulsion, never the instruments.

## 1. The two laws this architecture exists to enforce

1. **Everything derived is a query.** No batch pipeline; `check`, LSP, MCP are
   thin clients over the same memoized query graph. (Fixes: keystroke-cost
   LSP, dead `.almdi`, phantom cache, `slope_check 1.144`.)
2. **There is exactly one semantics.** One lowering, one canonical target.
   Cross-target equivalence stops being a 280-contract liability and becomes a
   2-point bind: executable spec ↔ the single backend.

## 2. Layers

```
L5  clients      almide-cli / almide-lsp / almide-mcp     (thin; no logic)
L4  execution    wasmtime JIT (dev) | cranelift AOT .cwasm (dist)
L3  lowering     IR → WASM Component (wasm-encoder, structural; Perceus,
                 emit-time certificates)                   [single semantics]
L2  spec         almide-spec: the interpreter over linked IR = the semantics.
                 Contracts bind L2 ↔ L3 output.
L1  query core   salsa DB: parse / interface / typecheck / lower as queries
L0  evidence     spec/ corpus, contracts.toml, Lean belts, dialect epochs,
                 MSR harness (Dojo) — gates every layer above
```

Diagnostics (`almide-diag`) is a leaf library used by L1–L3; it is ported
verbatim first because everything else reports through it.

## 3. Crate map

| crate | role | provenance |
|---|---|---|
| `almide-diag` | Diagnostic struct, 60+ codes, `try_replace`, JSON | **ported verbatim** (almide-base/diagnostic) |
| `almide-spine` | salsa database, query definitions, invalidation | new |
| `almide-syntax` | lexer/parser, AST | ported, re-cut as queries |
| `almide-sema` | type checker (per-function queries) | logic ported, structure new |
| `almide-spec` | interpreter over linked IR = executable semantics | **ported** (almide-interp), re-anchored as the spec |
| `almide-ir` | linked IR schema | ported |
| `almide-wasm` | IR → wasm component, Perceus, certificates | Perceus+certificates ported; emission new (no WAT text, no TOML templates) |
| `almide-run` | wasmtime embed: JIT dev loop, cranelift AOT dist | new |
| `almide-cli` / `almide-lsp` / `almide-mcp` | clients | MCP ported; CLI/LSP thin rewrites |
| `proofs/` | Lean belts (perceus/race/edit), contract ledger, sealed releases | ported |
| `spec/` | .almd corpus (1098 tests) | ported, arrives FIRST |

Explicitly **not ported**: v0 codegen, dual native renderers + wall fallback,
TOML string templates, WAT-text emission, WASI p1 shims, whole-program
`ir_link`-last ordering, package-level 7-category permissions.

## 4. Porting doctrine

- **Only world-class finished units.** A unit qualifies if the audit rated it
  frontier (c) and it has its own test evidence.
- **One unit at a time.** Each port lands as one PR-sized unit with its tests.
- **The incumbent is the oracle.** Acceptance gate for every executable unit:
  A/B against released almide v0.57.2 — identical stdout/stderr/exit over the
  relevant `spec/` slice. A port is "done" when the released binary can no
  longer be distinguished from it on its slice.
- Adaptation at the boundary is allowed (query wrapping, error-type lift);
  rewriting the unit's internals during port is not. Rewrite = separate, later.

## 5. Porting order

| # | unit | gate |
|---|---|---|
| 0 | `spec/` corpus + contracts.toml schema + checker scripts | ledger checker green on ported set — **LANDED 2026-08-19**, see PORTLOG.md |
| 1 | `almide-diag` | unit tests + JSON snapshot parity — **LANDED 2026-08-19**, see PORTLOG.md |
| 2 | `almide-syntax` | corpus parses; AST JSON parity vs `--emit-ast` oracle at the port SHA — **LANDED 2026-08-19**, 1,095/1,095 byte-identical, see PORTLOG.md |
| 3 | `almide-spec` (interpreter) + `almide-ir` | corpus output parity vs the port-SHA oracle — **LANDED 2026-08-19**: 451/451 comparable contract fixtures identical (stdout+exit), 138+1 doctrine-consistent skips ceilinged shrink-only, see PORTLOG.md |
| 4 | `almide-spine` + `almide-sema` | corpus diagnostics parity; keystroke re-check touches only edited function's queries (measured); **diagnostic stability across edits** (Zig-style incremental scenarios — survey steal, RESEARCH-diagnostics.md) |
| 5 | Lean belts + dialect epochs + llms.txt gate | lean-proofs green; llm-surface gate green |
| 6 | `almide-wasm` (Perceus + certificates onto new emission) | corpus parity interpreter ↔ wasm; wasmparser-validate wall |
| 7 | `almide-run` (JIT + AOT) + `almide-cli` | `almide run` end-to-end on corpus |
| 8 | `almide-mcp` + `almide-lsp` | MCP tool tests; LSP over queries (no full re-analyze) |
| 9 | stdlib self-host, `fan`, ratchet scripts, release sealing | tiered suite green |

## 6. Ratification ledger (○× one at a time)

| id | decision | recommendation | status |
|---|---|---|---|
| R1 | Single semantics: canonical target = WASM Component; native = cranelift AOT of the same artifact; Rust-transpile demoted to a future non-canonical tier | adopt | **RATIFIED 2026-08-19** |
| R1a | The future Rust-transpile tier is the **certification seat**, not a perf seat: aviation-grade builds go IR → Rust → qualified rustc (Ferrocene-class), with interp ↔ wasm ↔ cert-build output identity proven by the ported oracle machinery. Canonical semantics stays wasm-only. No implementation obligation today; later design must not foreclose this seat (certificate chain must remain extendable to the cert tier). | adopt | **RATIFIED 2026-08-19** |
| R1b | Second certification seat on the wasm side: cert profile is **AOT-only (no JIT)**, last mile is wasm → C via a small-TCB translator (wasm2c-class, verifiable against the mechanized wasm semantics) → qualified/verified C compiler (CompCert-class). Coexists with R1a under the same oracle gates; which seat becomes primary is decided later on ecosystem maturity. Canonical semantics unchanged. | adopt | **RATIFIED 2026-08-19** |
| R2 | Query core = salsa (crates.io, 2024+ rewrite), not hand-rolled | adopt | **RATIFIED 2026-08-19** |
| R3 | Module-boundary ABI = dictionary passing; monomorphization is an intra-CU optimization only (separate compilation preserved). The dictionary is the **semantic contract, not a performance ceiling**: shape-based stenciling (Go 1.18 precedent) and opt-in cross-module specialization (Swift `@inlinable` precedent) remain legal optimizations under the contract — the ABI caps invalidation granularity, never peak performance. Runtime (JIT-time) specialization is the one stronger scheme and is foreclosed by the R1b cert profile (AOT-only), knowingly. | adopt | **RATIFIED 2026-08-19** |
| R4 | `T ! E` from day one; tail ok-lift owned by fallibility, not by notation (ADR-0002/0012 as founding law). Refining an error type must be a signature-only diff — this is an edit-locality theorem obligation, not just a style goal. Two-layer doctrine (erased default, variant `E` in closed domains, visible `map_err` demotion, E035/E036) ports as-is. | adopt | **RATIFIED 2026-08-19** |
| R5 | WASI 0.3 component from day one; no Preview-1 compatibility layer. Component-model async (`stream`/`future`) is a host-boundary capability only — the language surface stays deterministic `fan`, inviolable. | adopt | **RATIFIED 2026-08-19** |

Each ratified R gets its status flipped here in the same commit as any code it
gates. A rejected R gets its alternative written in, not deleted.

## 6.5 Re-scoping decision (ratified 2026-08-19): spike before port

The port order in §5 is SUSPENDED after unit 2 in favor of validating the
highest-risk, highest-value bet first. Rationale: units 0–2 produced faithful
copies plus transferable wins; the value that only greenfield can deliver
(R1/R2/R3) had zero experimental evidence, and the reference corpus's own
stability survey warns that rewrites die exactly here.

**Spike S1 — salsa spine over the ported parser, measured on the real corpus**
(spec/, 1,098 files / 55,089 lines) against the incumbent's own edit-loop
ladder (scripts/edit-loop-scale-baseline.txt: check 65.99ms + front-end at
30k lines, slope_check 1.144, LSP full re-analysis per keystroke).

What S1 can prove (front-end slice only — sema is not ported):
- (a) per-edit re-derivation touches ONLY the edited file's queries (measured,
  not asserted);
- (b) salsa's cold overhead over raw batch parsing is small (<20%);
- (c) warm re-derive after a one-file edit beats batch front-end re-parse by
  ≥10x on the corpus.

What S1 cannot prove: the check phase (73% of the incumbent's loop) stays
unvalidated until sema-as-queries (unit 4) — S1 de-risks the mechanics, not
the end number. Explicitly: full-loop speedup TODAY would be ~1.4x; the big
number requires unit 4.

Decision gate: all three of (a)(b)(c) green → continue greenfield with unit 4
(sema spike next, before the interp port). Any red → fold greenfield: keep
the branch frozen as evidence, backport the transferable wins to develop.
Independent of the gate: E1 (misspellings/codes/multifix) and ADR-0002/0012
are develop-eligible and are NOT counted as greenfield returns.

**VERDICT (2026-08-19): (a) PASS max-1-parse/edit, 0 on no-edit; (b) PASS
+9.0% cold overhead; (c) PASS 351x warm vs batch (0.182ms vs 63.81ms,
1,098 files / 55,089 lines). Greenfield CONTINUES; next = unit-4 sema spike.
Full report: docs/spikes/S1-salsa-spine.md.**

Cross-compiler reconciliation: the reference corpus's own anti-recommendation
("no query-level invalidation" — championship, citing rustc's 15,695-line
hand-rolled retrofit and Swift's unconsumed graph) warns against hand-rolling
and retrofitting; S1 does neither (library salsa, ~90-line spine, consumer
from day one), and the architecture that got it right (rust-analyzer) was
absent from that corpus — the audit's identified sampling error. The
comparison also surfaced the next trap: **absolute spans defeat per-function
invalidation** (any early-line edit shifts every span downstream). Known
cures are rust-analyzer's firewall pattern (interface-fingerprint query
separated from body-check query) and MoonBit's relative positions (Rloc).

**Spike S2 — sema-as-queries, gates:**
- (d) body-only edit → exactly 1 function's check query re-runs (fan-out 0
  via unchanged interface fingerprint);
- (e) interface-changing edit → only dependent functions re-check (measured
  against the true dependency set, not the whole module);
- (f) span-only edit (insert a blank line above) → **0 check re-runs** — the
  fingerprint must be span-independent;
- (g) warm full-loop (front-end + check) ≥ 10x vs the batch equivalent on
  the same corpus slice.
Same fold clause as S1: any red → freeze and backport.

**S2a VERDICT (2026-08-19): (d) PASS max-1 re-check on body edits; (e) PASS
exactly decl+true-dependents on interface edits; (f) PASS 0 re-checks on
span-only edits (20 rounds) — on the real corpus graph (6,973 decls, 5,408
symbols, 15,673 edges). (g) remains open and becomes unit 4's acceptance
bar. Unit 4 is GO. Full report: docs/spikes/S2a-sema-mechanics.md.**

**Unit 4 STAGE 1 VERDICT (2026-08-19): (g) PASS — 1539x (4.42 ms warm vs
6,807 ms batch, 1,062 files / 53,943 lines) with the REAL checker (the full
~35k-line types/lang/ir/frontend stack ported verbatim, zero source edits)
behind a per-file query; invalidation exact (max 1 check/edit, 0 on
no-edit). All §6.5 criteria are now green: 8/8. Still owed before unit 4
LANDS: diagnostics parity vs oracle, per-decl granularity, incremental
diagnostic scenarios, E1 wiring. Report: docs/spikes/S3-real-checker-query.md.**

**Diagnostics parity (2026-08-19): GREEN — 1,062/1,062 files byte-identical
to oracle `almide check --json` stdout (checker diagnostics, ordering, and
the post-check unused-variable warning stage included); 33 purity-skips
printed, 3 oracle exclusions. The speed numbers are now backed by proven
behavioral identity on the check path. Details: PORTLOG.md unit 4.**

**Stage 2 tax removal (2026-08-19): a phase probe showed 89% of every check
was stdlib tax (63% bundled-stdlib re-inference + 25.5% canonicalize/env);
two parity-adjudicated variants removed both — warm keystroke check is now
0.62 ms (was 4.42), batch 7.4x cheaper, (g) at 1499x, all three variants
1,062/1,062 byte-identical to the oracle. Structural (non-port) changes:
split canonicalize (modules/entry halves; verbatim fn untouched) + three
Clone derives. Per-decl granularity remains open for the largest files.
Report: docs/spikes/S4-stage2-tax-removal.md.**

## 7. Certification trajectory (R1a + R1b)

Aviation-grade (DO-178C-class) is a declared destination, reached from either
of two seats — never by qualifying wasmtime/cranelift, which stay dev/dist
only:

```
canonical (always):  IR ──certificates──▶ wasm component ──▶ wasmtime JIT/AOT
cert seat A (R1a):   IR ──▶ Rust ──▶ qualified rustc (Ferrocene-class)
cert seat B (R1b):   IR ──▶ wasm ──▶ C (small-TCB translator) ──▶ CompCert-class
```

- Both seats sit under the same oracle gates: interp ↔ wasm ↔ cert-build
  output identity over `spec/`.
- Seat B's structural advantage: wasm is the only mainstream target with a
  complete mechanized formal semantics (WasmCert), so the wasm → C link is
  *verifiable*, not merely qualifiable (DO-333 credit).
- Route-independent prerequisites, owned by the language/runtime side and
  deferred to their own R items: a certification language profile (static
  allocation bounds, bounded recursion), WCET-analyzable output, MC/DC
  coverage measured on `.almd`, stack bounds. No later design decision may
  foreclose these.

