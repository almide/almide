# ADR-0027: The effect surface — `effect[...]` bounds, `pure fn`, a versioned manifest, declared-only `Abort`, fail-closed externs, and one category registry

- **Status**: Proposed (draft for review, 2026-10-03). Nothing here is
  implemented. Each section ends with a **Ruling** line for the reviewer.
- **Date**: 2026-10-03
- **Scope**: the six questions ADR-0026 left open — §1 surface syntax for a
  bound, §2 the plain-`fn` bound and a strict empty bound, §3 manifest
  versioning, §4 language-defined traps, §5 extern contracts and trust, §6 one
  category vocabulary.
- **Builds on**: [ADR-0026](./0026-effect-categories-ride-the-transparent-fn-type.md)
  (categories, D1–D4), [ADR-0002](./0002-fallibility-effect-orthogonal.md) §D6
  (effect implies fallible), [ADR-0009](./0009-fn-type-quadrant-transparency.md)
  (fn-type slots), [ADR-0022](./0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)
  (output and abort in a plain `fn`), [ADR-0017](./0017-flight-profile-is-a-normative-subset.md)
  (bounded profile).
- **Method**: four parallel surveys on 2026-10-03 — bound syntax (13
  languages), permission manifests (10 systems), traps and foreign calls (12 +
  9 systems), and an in-repo inventory of every place that spells a category.
  Facts marked *(memory)* were not checked against a source in that session.

## Findings that stand on their own

These were found while surveying and are defects today, independent of how
the questions below are ruled. Each deserves an issue.

| # | Finding | Where |
|---|---|---|
| F1 | A plain-`fn` `@extern` counts as pure in category inference; nothing in the pass reads `@extern`. `tests/licm_extern_test.rs` patched the same hole for LICM only. | `pass_effect_inference.rs`, `effect.rs` |
| F2 | `random.*` is never inferred: `module_to_effect` has no arm that yields `Rand`. `path` and `url` (pure) are classified `IO` / `Net`; `process` is classified `Env`. | `pass_effect_inference.rs:23-31` |
| F3 | An unknown name in `[permissions].allow` is dropped silently; a list of only unknown names (the doc comment's `"Log"`) enforces with nothing allowed. `--profile critical --allow` rejects unknown names — the two paths disagree. | `src/cli/mod.rs:69-78`, `src/cli/check.rs:372-381`, `src/project.rs:77-83`, `src/main.rs:892` |
| F4 | `io.read_byte`, `io.read_n_bytes` and `process.args` are plain `fn` but read stdin / argv. ADR-0022 admits writes and aborts in a plain `fn`, "never reads". `env.args` is `effect fn`; `process.args` is not. | `stdlib/io.almd:28,32`, `stdlib/process.almd:20`, `stdlib/env.almd:24` |
| F5 | `docs/specs/effect-system.md` lists a `Log` category backed by a module that does not exist; `docs/diagnostics/E085.md` points to a `--profile critical` section of `docs/specs/cli.md` that does not exist. | as named |
| F6 | `@pure` is in `KNOWN_ATTRS` with no semantics and no uses: a model that writes it today is silently unchecked. | `crates/almide-frontend/src/attr_vocab.rs:39` |

## 1. Surface syntax for a declared bound

**Constraints in the grammar today.** `effect` is a hard keyword; `with`,
`uses`, `effects`, `pure` are not keywords and have no code uses as
identifiers in `stdlib/`, `spec/` or the exercise corpus; `total` is used as
an identifier 143 times. `!` after the return type is the fallibility marker
(`-> T!`, `-> T!E`), so Aver's `! [Disk]` collides. `{` after the return type
already triggers the missing-`=` hint and reads as a record, so Flix/Unison's
`-> T {FS.read}` is risky. Nothing follows `effect` with `[` today.

| Language | Bound | Empty |
|---|---|---|
| Flix | `-> Unit \ {FsRead, IO}` | `\ {}` |
| Koka | `: <console,exn> int` | `total` |
| Unison | `->{IO, Exception}` | `->{}` |
| Effekt | `Double / { Exception }` | `/ {}` |
| Wado | `with (Stdout, FileSystem)`; `pub` explicit | no clause |
| Aver | `! [Disk.readText]`, `! [Disk]` | no clause |
| Scala CC | `A ->{c} B`; `=>` = any | `A -> B` |
| Vera | `effects(<Http>)`, mandatory | `effects(pure)` |
| Roc | `=>` vs `->` | `->` |

**Proposal: brackets on the keyword that already means "touches the world".**

```almide
pub effect[FS.read] fn load(p: String) -> String = fs.read_text(p)!
pub effect[FS] fn sync(dir: String) -> Unit = ...               // parent = coarse bound
effect fn helper() -> Unit = ...                                // no bound: inferred
effect fn serve(port: Int, f: effect[Net.listen] (Req) -> Resp!) -> Unit = ...
```

- One bracket pair, no new word. `[...]` already reads as "parameterised by"
  (`List[Int]`), and the same spelling serves declarations and fn-type slots
  (ADR-0009's `effect` prefix).
- Bare `effect fn` keeps its meaning: inferred, unbounded. No existing code
  changes. `effect[...]` still implies fallible (ADR-0002 §D6).
- A diagnostic prints the bound exactly as written (`effect[FS.read]`), so it
  can be copied back.
- `pub` functions are **not** required to carry a bound at first. Once the
  interface diff (ADR-0026 D3) records inferred sets, a missing bound is a
  diff-time finding, not a parse error.

**Rejected.** A trailing `uses FS.read` clause (splits effect information
across the signature; comma ambiguity inside parameter lists; new word). An
`@effects(...)` attribute (cannot attach to a fn-type slot, so a second
syntax would still be needed; attribute arguments do not accept dotted
names). `-> T {..}` / `-> T \ {..}` (collides with records and the
missing-`=` hint, or adds a sigil with no Almide precedent).

**Ruling**: ○ / × — `effect[...]` for declarations and slots; `pub` bound
optional until the interface diff exists.

## 2. The plain-`fn` bound, and a strict empty bound

**Plain `fn`** stays what ADR-0022 says: at most `{IO.stdout, IO.stderr,
Abort}`. That set is never spelled; it is the meaning of the keyword. The
readers in F4 are moved out of it (`io.read_*` and `process.args` become
`effect fn`, or are reclassified with a ruling of their own) — a plain `fn`
never reads.

**Proposal: a contextual `pure` modifier meaning exactly `{}`.**

```almide
pure fn area(w: Int, h: Int) -> Int = w * h              // {}: println / panic / assert rejected
fn area_dbg(w: Int, h: Int) -> Int = { println("${w}"); w * h }   // the ADR-0022 bound
```

- It joins the existing modifier family (`effect fn`, `scoped fn`), and like
  `scoped` it is contextual, so no identifier is reserved.
- It excludes `Abort` as well (Flix, Unison, Effekt), so the rule is one word
  with no carve-out list. §4 says what `{}` does *not* exclude.
- Writing `pure` where it is not needed fails safe: it only adds a check.
- `@pure` (F6) leaves `KNOWN_ATTRS` and gets a "did you mean `pure fn`"
  diagnostic, so two spellings never coexist.
- `pure (A) -> B` in a fn-type slot is **deferred**: ADR-0026 D1 makes bare
  slots transparent, so the demand is unproven. Declarations ship first.
- `pure` is independent of `@bounded` / `scoped`; none implies another.

**Rejected.** `effect[] fn` — `effect` implies fallible (ADR-0002 §D6), so it
would read "touches the world, but nothing" and still lift to `T!`. `total`
(Koka) — taken as an identifier 143 times.

**Ruling**: ○ / × — `pure fn` = `{}` including no `Abort`; F4 readers leave
the plain-`fn` bound; slots deferred.

## 3. Manifest versioning

| System | Default | Unknown name | Meaning pinned by |
|---|---|---|---|
| Deno | deny all | error | — |
| Go | `go` line sets GODEBUG defaults | **error** | the declared version |
| Android | old permission keeps old meaning | — | `targetSdk` |
| pnpm 10 | build scripts off | — | major version; per-package `allowBuilds` |
| Flix (OOPSLA 2026) | effect lock per dependency | — | the lock |
| LavaMoat | generated `policy.json` | — | regenerate + review the diff |

The precedents agree: a version number pins what names mean (Go, Android),
unknown names are errors (Go), per-dependency grants are an explicit map
(pnpm, LavaMoat), and growth is caught by diffing a recorded summary (Flix,
LavaMoat).

**Proposal: one integer key that pins the registry version.**

```toml
[permissions]
effects = 1                              # registry version; pins what each parent means
allow = ["FS.read", "Net", "IO.stdout"]  # [] = deny all, ["*"] = every leaf of version 1
```

- **No `effects` key = legacy**: today's meaning exactly (empty = everything,
  old names), plus a warning, and `almide fix` rewrites it by a fixed table
  (`IO` → `["FS", "IO"]`, `[]` → `["*"]`). Meaning changes only through a
  visible edit.
- **With `effects`**: omitting `allow` is an error; `[]` is deny all; `"*"`
  is all leaves of that version (one spelling — no `all = true`); an unknown
  or misspelt name is an error with a did-you-mean.
- Raising `effects` is a one-line edit and `check` prints the leaves each
  parent gained. A registry version bumps when a leaf is added under an
  existing parent.
- `--profile critical --allow` takes the same names (F3); `Process` is
  accepted as a legacy alias of `Proc` with a warning.
- **Later stage** (ADR-0026 D3 order): per-dependency grants and an effect
  summary in the lock.

```toml
[permissions.deps]          # each grant ⊆ the root allow; unlisted deps inherit it
"almide/http" = ["Net.fetch"]
"almide/csv"  = []

# almide.lock
[[package]]
name = "almide/http"
version = "0.4.1"
effects = 1
summary = ["Net.fetch", "IO.stderr"]
```

  `almide update` refuses a version whose summary grew, printing the new
  leaves with their D4 paths; accepting means widening `[permissions.deps]`.

**Rejected.** A version suffix on every name (`"FS.read@1"`) — models drop or
invent the suffix. Flipping `[]` to deny-all without a version key —
silently changes meaning.

**Ruling**: ○ / × — `effects = N` key; legacy kept + warned + `almide fix`;
deps and lock later.

## 4. Language-defined traps

**What Almide does today.** Integer `+ - *` wrap (C-170), so overflow is not
a trap. Division and remainder by zero, `MIN / -1`, an out-of-bounds `xs[i]`,
and a handful of domain errors (ALS-T4/T6) abort with `Error: …` and exit 1.
Stack exhaustion is a resource limit (C-196); out of memory is a defined but
target-dependent abort (C-197). The critical / bounded profile does not
reject indexing or division.

| Language | Div/0 | Bounds | In the effect type? |
|---|---|---|---|
| Koka | total (x/0 = 0) | `exn` | bounds yes |
| Lean | x/0 = 0 | `a[i]!` panics | no |
| Pony | 0, or partial `/?` | — | partial ops marked |
| Gleam | 0 | — | no |
| Rust, Swift, Haskell, Zig *(memory)* | trap / panic | trap / panic | no |
| SPARK *(memory)* | proof obligation | proof obligation | discharged statically |

Only Koka types a bounds failure, and even Koka makes division total rather
than typed. Nobody types stack or memory exhaustion.

**Proposal: `Abort` is declared aborts only; trap freedom is a profile
obligation, not a category.**

- `Abort` = `panic`, a failing `assert*`, `process.exit`. (`process.exit` is
  `Abort`, not `Proc`; `Proc` is starting another program.)
- Trap sites (ALS-T6 family) carry no category. If every `xs[i]` carried
  `Abort`, nearly every function would, and the category would stop telling
  anyone anything — and an edit that adds an index would change an
  interface.
- The meaning of `{}` / `pure` is written down precisely: *no observable
  effect and no declared abort; a language-defined trap (ALS-T6) or a
  resource limit (C-196, C-197) can still end the run.*
- Absence of runtime errors belongs to `--profile critical` / `@bounded` as a
  SPARK-style rule (each trap site discharged by a guard, `list.get`, a
  literal divisor, or a checked operation), listed by `almide check
  --effects` in the D4 path shape. That rule is a separate ADR; until it
  exists, no profile claims trap freedom.

**Rejected.** Every trap site carries `Abort` (sound, but the category stops
discriminating). Making division total (Koka, Lean, Gleam, Pony) — a change
to C-001 / ALS-T6 semantics, out of scope here.

**Ruling**: ○ / × — `Abort` = declared only; `{}` documented as above; trap
freedom deferred to a profile ADR.

## 5. Extern contracts and trust

**Today.** `@extern` declares an ABI only (module-system.md §11) and no
effect; F1 shows the consequence. Stdlib `@intrinsic("almide_rt_*")` (855
declarations) is restricted to the stdlib by E085, and effectful ones are
spelled `effect fn`. `docs/stdlib/semantics-manifest.toml` already holds 323
probe-backed per-function rows.

| System | Foreign call's effect | Asserting less | Trust report |
|---|---|---|---|
| Flix | every Java call is `IO` | `unsafe` block | effect lock |
| Rust 2024 | `unsafe extern` items unsafe by default | `safe fn` = author's assertion | cargo-geiger, cargo-vet |
| Koka | `extern` declares its effect | `unsafe-total` | — |
| Lean | logic sees the reference definition | trusted compiler step | `#print axioms` |
| Haskell, OCaml *(memory)* | the declared type | — | — |

**Proposal: fail closed, record every assumption, report it.**

```almide
@extern(rust, "crate::gpu", "push_u32")
effect fn push_u32(x: Int) -> Unit = _               // no bound: ⊤ (every leaf)

@extern(rust, "crate::log", "write")
effect[IO.stderr] fn log_line(s: String) -> Unit = _ // bound = recorded assumption

@extern(rust, "crate::hash", "fnv")
pure fn fnv(b: Bytes) -> Int = _                     // {} = recorded assumption
```

- An extern without a bound is ⊤ — all leaves, Flix's rule. A plain-`fn`
  extern without `pure` is a warning for one release, then an error (F1).
- A bound or `pure` on an extern is an **assumption**, never a proof.
  `almide check --effects --trusted` lists every one with its location, like
  Lean's `#print axioms`. The D3 lock records each dependency's assumption
  set; a grown set needs acknowledgement, as a grown summary does.
- Stdlib intrinsics take their contracts from the §6 registry, not from
  per-declaration syntax, and are reported the same way.

**Rejected.** Contracts only in a side file (`native/contracts.toml`) — two
places that drift. Trusting the declared type silently (Haskell, OCaml) — the
hole F1 already is.

**Ruling**: ○ / × — ⊤ by default; bounds and `pure` on externs are reported
assumptions; plain-`fn` extern warned then rejected.

## 6. One category vocabulary

**Today** there are five code vocabularies and three doc vocabularies, and
none matches ADR-0026:

| Spelling | Where | Problem |
|---|---|---|
| `IO Net Env Time Rand Fan` | `effect.rs`, inference, `[permissions]` match (two copies) | `IO` means files; `Rand` never inferred; unknown names dropped |
| `IO Net Env Time Rand Process` | `--profile critical --allow`, help, hint (`bounded.rs:118-125`) | `Process` vs `Proc`; grants by module (`duration` is pure but granted under Time) |
| `Stdout Entropy CliArgs FsRead FsWrite Clock Stdin` | MIR witness (`lib_b.rs:520-585`), `certificate.rs:477-494` | manifest `IO` maps to stdin+stdout+files; no stderr, Net, Fan |
| `STDOUT … STDIN NET FOREIGN` | wasm witness (`cert_project.rs:55-101`) | `PURE_BOUND = {STDOUT, STDIN}` contradicts ADR-0022 (F4) |
| 13 bits `FS.read=0 … IO.stderr=12` | `docs/wasm/capability-system.md`, roadmap | numbering conflicts with the witness; no `Abort`; `IO` includes files |
| seven incl. `Log` | `docs/specs/effect-system.md` | phantom module (F5) |

**Proposal: one versioned registry, everything else derived or gated.**

- One file — `stdlib/effects.toml` — holds the registry version, the leaves,
  the four parents and their leaves per version, and a **per-operation**
  contract row for every stdlib function that has a category. It sits next
  to, and is cross-checked against, the semantics manifest.
- `Effect` in `almide-ir`, the inference table, manifest validation, the
  critical-profile grants, the CLI help, the diagnostics and the docs table
  are generated from it or gated against it. The witness ids (MIR and wasm)
  stay internal numbers but carry a gated mapping to leaves.
- The leaf table is append-only per version (a ratchet like
  `proofs/dialect-epochs.toml`).
- Per-operation assignments for the ambiguous cases:

| Operation | Category |
|---|---|
| `env.args`, `process.args`, `env.get` | `Env.read` |
| `env.set` | `Env.write` |
| `env.millis`, `env.unix_timestamp`, `env.sleep_ms`, `datetime.now`, `monotonic_ns` | `Time` (rest of `datetime`, `duration`: `{}`) |
| `fs.exists`, `fs.stat`, `fs.read_*`, `fs.list_dir` | `FS.read` |
| `fs.write_*`, `fs.remove`, `fs.mkdir_p` | `FS.write`; `fs.copy`, `fs.rename`: both |
| `fs.temp_dir`, `env.temp_dir` | `Env.read` (reads `$TMPDIR`) |
| `io.read_*`, `process.stdin_lines` | `IO.stdin` |
| `io.write`, `println` | `IO.stdout`; `eprintln`: `IO.stderr` |
| `panic`, failing `assert*` | `Abort` + `IO.stderr`; `process.exit`: `Abort` |
| `http.get` / `post` / … | `Net.fetch`; `http.serve`: `Net.listen`; codecs: `{}` |
| `process.exec` / spawn | `Proc` |
| `random.*` | `Rand` |
| `path.*`, `url.*`, `zlib.*` | `{}` |
| `fan { }`, `fan.map` | `Fan` + the arms' sets |

**Ruling**: ○ / × — `stdlib/effects.toml` as the single source; the table
above as the first version.

## Order of work if accepted

1. F1–F6 as issues (independent of the rulings).
2. §6 registry + generation/gates, with today's six names mapped (no
   behaviour change).
3. §3 manifest key, unknown-name errors, `almide fix` migration.
4. ADR-0026 D4 query on `check --effects` (paths, `--json`).
5. §1 / §2 syntax and `Ty::Fn` sets (ADR-0026 D1).
6. §5 extern rule; then §3's deps and lock.

## Falsifier

1. Dojo shows `effect[...]` and `List[...]` being confused, or brackets
   dropped, at a measurable rate — move to a trailing clause.
2. `pure fn` is written on most functions by models "to be safe" and then
   loosened by edits — the modifier is adding churn, not information.
3. A real supply-chain incident in the ecosystem passes through a trap or an
   extern assumption this ADR chose not to type.

## References

- Flix: https://doc.flix.dev/effect-polymorphism.html,
  https://doc.flix.dev/calling-methods.html,
  https://2026.splashcon.org/details/oopsla-2026/120/Fighting-Supply-Chain-Attacks-with-Effect-Systems
- Effekt: https://effekt-lang.org/tour/effects
- Scala capture checking: https://docs.scala-lang.org/scala3/reference/experimental/cc.html
- Swift typed throws: https://forums.swift.org/t/se-0413-typed-throws/68507
- Rust external blocks: https://doc.rust-lang.org/reference/items/external-blocks.html
- Go GODEBUG: https://tip.golang.org/doc/godebug
- Deno 2.5 permissions: https://deno.com/blog/v2.5
- Node permissions: https://nodejs.org/api/permissions.html
- pnpm build settings: https://pnpm.io/settings/build
- LavaMoat: https://github.com/LavaMoat/LavaMoat
- Capslock: https://github.com/google/capslock
- Local: `../almide-references/{wado/docs/spec.md, aver/docs/language.md,
  jacquard/README.md, vera/README.md, koka/lib/std/core/*.kk,
  lean4/src/Init/Prelude.lean, roc/Glossary.md}`
