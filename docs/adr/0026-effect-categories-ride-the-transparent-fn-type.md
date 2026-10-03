# ADR-0026: Effect categories ride the transparent fn type — sets flow through callbacks unannotated, categories form a two-level hierarchy, and inferred ⊆ manifest ⊆ host is one chain

- **Status**: Accepted (D1–D3, 2026-10-03). Nothing beyond the inference
  fixpoint (#3238) is implemented. D4 (diagnostics) and the open questions in
  §Open are not decided.
- **Date**: 2026-10-03
- **Scope**: what an effect *category* is (as opposed to the effect bit), how
  a category set moves through function values, callbacks and closures, how
  categories are named and grouped, and how the compiler's view relates to
  `[permissions]` in `almide.toml` and to the capabilities a host grants.
- **Builds on**: [ADR-0002](./0002-fallibility-effect-orthogonal.md) (§D6:
  effect implies fallible — unchanged here),
  [ADR-0009](./0009-fn-type-quadrant-transparency.md) (D2: a bare fn-type
  parameter is transparent — this ADR widens the transparent bit to a set),
  [ADR-0022](./0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)
  (output and abort builtins in a pure `fn`).
- **Related**: #3235 (ADR-0025, in flight: wasm `process` as a private host
  capability — the host side of the `Proc` leaf in D2/D3).
- **Charter**: [effect-system-capability.md](../roadmap/active/effect-system-capability.md)
  (its Phase 4 "EffectSet in Ty::Fn" is the type-level work this ADR designs;
  its `IO` shorthand is superseded by D2).
- **History**: 2026-10-03. An external design proposal ("Almide effect仕様案
  v0.1", baseline 902e58d23) asked for a closed category set as an upper bound
  on function types. A cross-language comparison (below) was run before
  ruling, and three questions were put to the maintainer one at a time; all
  three were answered ○.

## Context

What exists on develop (7f2f03cca):

- **The effect bit** is checked by the type checker: a `fn` cannot call an
  `effect fn`, and fn-typed parameters carry the bit per ADR-0009.
- **Categories** are a separate, coarser analysis:
  `crates/almide-ir/src/effect.rs` has six (`IO`, `Net`, `Env`, `Time`,
  `Rand`, `Fan`), and `pass_effect_inference.rs` assigns them **by module
  name** (`path` and `url` count as `IO`/`Net` although they are pure string
  work). They do not appear in any type.
- `src/cli/mod.rs::check_permissions` compares the transitive sets to
  `[permissions].allow`. An empty list allows everything
  (`src/project.rs:81`), and an unrecognised name is silently dropped.
- The transitive closure stopped after 20 rounds, so a deep call chain
  reported an effectful root as clean. Fixed in #3238 (worklist to the least
  fixpoint).
- `docs/wasm/capability-system.md` and the roadmap describe 13 categories;
  the code has 6.

So the compiler can say *that* a function touches the world, but not *which
part*, and what it says about which part does not survive a callback.

### Comparison (2026)

| Language | Representation | Polymorphism, burden | Granularity | Static ↔ runtime |
|---|---|---|---|---|
| Koka | row `<console,exn\|e>` | implicit open rows; row variables leak into errors | user-defined + handlers | none |
| Effekt | capabilities passed implicitly | contextual, no effect variables written; second-class functions | user-defined + handlers | none |
| Flix | set `\ {FsRead, Net}`, Boolean/set unification | `\ ef`, `ef1 + ef2`, `ef - Block` | fixed base set + user-defined | per-package effect lock |
| Scala 3 capture checking | capabilities are values, `T^{io}` | capture sets; heavy; experimental | capabilities | in-language only |
| Roc | `->` pure / `=>` effectful | none | two-valued | platform supplies effects |
| Wado | `with (Stdout, FileSystem)` | one effect variable per fn, inferred from HOF args; local inferred, `pub` explicit | 1:1 with WASI interfaces | component imports |
| Aver | `! [Disk.readText]`, `! [Disk]` | — | per method, namespaced | enforced at runtime too |
| Jacquard | `->{net}` | rows | handlers | `--allow net`; `why-effect` command |
| Deno | not in types | — | `--allow-read=path` | runtime only, no reason shown |

The closest design is Wado's (local inference, explicit `pub`, no handlers,
direct mapping to component imports); the base vocabulary is Flix's minus
handlers. A closed set without handlers is not behind the frontier. The gaps
were package-boundary effect diffs (Flix), one chain from static effects to
host grants (WASI, Jacquard), and path-carrying diagnostics (Jacquard).

## Decision

### D1. A bare fn-type parameter is transparent for the category set too (○ 2026-10-03)

ADR-0009 D2 already makes a bare `f: (A) -> B` pass the argument's effect
*bit* to the HOF call. The same parameter now passes the argument's category
*set*:

```almide
fn apply(xs: List[String], f: (String) -> Int) -> List[Int] = xs |> list.map(f)
// apply(paths, (p) => count_lines(p)!)  — the call carries FS.read
// apply(names, (s) => string.len(s))     — the call carries nothing
```

- The writer never names an effect variable. There is no `effects E` in
  the surface language, and **diagnostics never print a variable** — only
  concrete sets such as `{FS.read}`.
- Internally the checker solves sets (union only on the surface; the
  solver may use Flix-style set unification). Bare parameter and
  lambda-wrapped argument yield the same set.
- An **explicit** slot (ADR-0009 D3, e.g. `effect (Req) -> Resp!`) fixes an
  upper bound; a stored or returned function value keeps the set of the
  value stored, and no adapter may drop it.

### D2. Categories are a closed two-level hierarchy (○ 2026-10-03)

| Parent | Leaves |
|---|---|
| `FS` | `FS.read`, `FS.write` |
| `Net` | `Net.fetch`, `Net.listen` |
| `IO` | `IO.stdin`, `IO.stdout`, `IO.stderr` |
| `Env` | `Env.read`, `Env.write` |
| — | `Proc`, `Time`, `Rand`, `Fan`, `Abort` |

- **Only these four parents exist.** Everything else is a leaf.
- Inference and `[permissions]` work in leaves. A `pub` boundary may declare
  a parent (`{FS}`) as a coarse upper bound, so a later `FS.write` inside it
  is not an interface change.
- A parent means the leaves of the registry version it was written
  against. **A leaf added later does not join an existing parent
  automatically**; it has to be granted explicitly.
- Categories are assigned **per operation**, not per module: `url.parse`
  and path manipulation are empty, `http.get` is `Net.fetch`.
- `Abort` is `panic` / failing `assert*` / `process.exit` only. A `Result`
  error is not `Abort`, and fallibility stays the separate axis of
  ADR-0002. `panic` carries `Abort` and `IO.stderr` both.
- This supersedes the roadmap's `IO = FS.read + FS.write + IO.std*`
  shorthand: `IO` is the three standard streams, file access is `FS`.

### D3. Inferred ⊆ manifest ⊆ host is one chain (○ 2026-10-03)

```
inferred set   ⊆   [permissions].allow   ⊆   capabilities the host grants
(compiler)         (almide.toml)              (wasm imports / Porta)
```

- **wasm**: imports outside the inferred set are pruned from the artifact,
  so an ungranted operation is absent, not merely forbidden.
- **Public API**: widening a `pub` function's effect upper bound is classified
  **breaking** by `scripts/check-interface-diff.sh`, and goes through the
  existing `@deprecated` / epoch procedure.
- **Dependencies** (later stage): each dependency's effect summary is
  recorded in the lock; moving to a version whose summary grew requires an
  explicit acknowledgement.
- Order: pruning and the interface diff first; the lock last.
- The static chain is not a runtime grant. Which files, hosts or processes
  a run may reach stays the host's decision; an effect summary is never an
  authorization token.

## Rejected

1. **User-defined effects and handlers** (Koka, Effekt, Unison, OCaml 5,
   Flix). Multi-shot handlers conflict with reference counting and regions,
   and they hide control flow from the reader we optimise for. If ever
   added, one-shot only, in a new ADR.
2. **Surface effect variables** (`(A -> B) effects E`, Koka rows, Rust
   `?effect`). ADR-0009 already rejected row polymorphism for its error
   messages; D1 gets the same expressiveness with none of the syntax.
3. **Second-class functions** (Effekt). Breaks storing and returning closures,
   which existing code does.
4. **Capabilities as values in the surface** (Scala capture checking).
   Annotation-heavy and still experimental; D3 takes the idea at the host
   boundary instead.
5. **Mandatory effect annotation on every function and closure** (Vera), and
   **per-method granularity** (Aver). Too much burden / too fine; 14 leaves
   under 4 parents is the chosen point.
6. **Effects absent from types** (OCaml 5) and **runtime-only permissions**
   (Deno). Neither can say why or by which path.

## Open — not decided by this ADR

- **D4, diagnostics**: a violation names the boundary, the added category,
  the causing operation and the call path (`main → load_cfg →
  fs.read_text`), with fix-its in the order "take it as a value / move the
  operation to the caller / widen the bound"; plus `almide why-effect <cat>`
  and a JSON form. To be ruled separately.
- **Surface syntax** for a declared upper bound and for an explicitly empty
  set (the proposal's `effects {}`).
- **The compatibility bound of a plain `fn`**: ADR-0022 admits output and
  abort builtins, so a plain `fn` is `{IO.stdout, IO.stderr, Abort}` at most,
  not `{}`. Whether a strict empty bound is offered, and how it is spelled.
- **Manifest versioning**: today an empty `allow` means "everything". A
  versioned manifest must tell omitted / empty / all apart and reject
  unknown names (proposal EFF 03, EFF 18) without flipping today's meaning
  unannounced.
- **Language-defined traps** (overflow, bounds) and whether they are `Abort`.
- **Extern and native-shim contracts**, and how trusted dependencies are
  reported (proposal EFF 14, EFF 15).

## Consequences

- `Ty::Fn` gains a category set next to the effect bit (the roadmap's
  Phase 4 is unblocked by design, still unscheduled). The intent is that
  categories are erased after checking, so monomorphisation does not
  multiply by sets (not yet verified).
- Category assignment moves from module names to a single versioned registry
  of per-operation contracts, from which the checker, manifest validation,
  diagnostics and docs read.
- The current CLI names (`IO` covering files) need a migration path to D2's
  names.

## Falsifier

1. **Dojo shows that transparent sets make LLM edits fail more often** than
   explicit bounds (e.g. because an effect "appears" at a call site the edit
   did not touch) — retreat to explicit slots on `pub` HOFs.
2. **Writers reach for parents everywhere and the leaves never get
   narrowed**, so `pub` bounds stop carrying information — restrict parents to
   manifests.
3. **Import pruning or the interface-diff classification produces a false
   breaking verdict on a release** that a reviewer has to override — the
   chain is checking the wrong thing and is re-specified.

## References

- Flix, effect polymorphism: https://doc.flix.dev/effect-polymorphism.html
- Flix, "Fighting Supply Chain Attacks with Effect Systems" (OOPSLA 2026):
  https://2026.splashcon.org/details/oopsla-2026/120/Fighting-Supply-Chain-Attacks-with-Effect-Systems
- Scala 3 capture checking:
  https://scala-lang.org/api/3.x/docs/experimental/capture-checking/overview.html
- Roc functions: https://roc-lang.org/docs/main/langref/functions/
- Local: `../almide-references/wado/docs/spec.md` (Effect System),
  `../almide-references/aver/docs/language.md`,
  `../almide-references/jacquard/README.md`,
  `../almide-references/research-2026/agent-verify-memory.md`
- Internal: #3238, `crates/almide-ir/src/effect.rs`,
  `crates/almide-codegen/src/pass_effect_inference.rs`,
  `src/cli/mod.rs::check_permissions`, `docs/wasm/capability-system.md`
