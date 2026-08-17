# xtarget-fuzz — Almide generative fuzzer

Stage 3 of the completeness roadmap: the machine that hunts the
composition space continuously for **observable divergences** and
compiler failures.

It generates well-typed Almide programs (synthesis, corpus mutation, and
the self-checking identity family), runs each through an oracle ladder
that compiles and executes it on both targets, and judges the results.
Every program is reproducible from a `(seed, index, family)` triple.
Findings are delta-debugged to a minimal repro and written to
`findings/`.

## Which oracle judges what

A *differential* fuzzer has no oracle for a bug the two legs SHARE.
native-v1 and wasm share the whole frontend, `almide-mir` and the linked
IR, so a miscompile there makes both legs identically wrong and the vote
comes back unanimous — #1322 is exactly that: a scalar let-alias
miscompile that lived from v0.57.0 while all 394 `spec/wasm_cross`
parity fixtures stayed green.

| oracle | what it can convict | coverage |
|---|---|---|
| native ⇄ wasm differential | a divergence between the two backends | every program |
| `almide-interp` (third judge, #516) | one backend, when the interpreter can run the program | abstains freely — it is a vote, not a verdict |
| **by construction** (identity family, #1332) | **either leg, or BOTH at once** | only the identity family, which is Int scalars and their control flow |

The third row is the one the other two cannot supply: an identity-family
program's expected stdout is a literal in its own source, so a leg is
judged **alone** and agreement proves nothing.

This is a standalone crate (its own `[workspace]`, like
`tools/wasmgen-harness`). It path-deps the parent `almide` crate for the
AST / parser / formatter, and **shells out** to the freshly built
`almide` binary for the oracle ladder — so a compiler ICE crashes a
child process we can observe, not the fuzzer.

## Build & run

```bash
# 1. Build the compiler (the fuzzer drives this binary).
cargo build --release --bin almide          # from the repo root

# 2. Build the fuzzer.
cd tools/xtarget-fuzz && cargo build --release

# 3. Run a campaign (time-budgeted or fixed count).
./target/release/xtarget-fuzz run --minutes 60          # 60-minute hunt
./target/release/xtarget-fuzz run --count 200 --jobs 8  # 200 programs, 8 workers

# A campaign run ENTIRELY under the by-construction oracle (#1332).
./target/release/xtarget-fuzz run --minutes 30 --family identity

# Inspect / reproduce one program deterministically.
./target/release/xtarget-fuzz gen    --seed 100 --index 42   # print source
./target/release/xtarget-fuzz replay --seed 100 --index 42   # re-run the ladder

# Re-judge a saved repro. An identity repro carries its own oracle in
# `// @expect` header lines, so this needs no seed and no family.
./target/release/xtarget-fuzz ladder findings/<dir>/repro.almd

# Catalogue / corpus sizes.
./target/release/xtarget-fuzz stats
```

`--family all|identity|synthesis` (default `all`) selects which families
a campaign draws from. **`(seed, index)` only reproduces a program under
the same `--family`** — `all` spends one draw on the family roll before
generating and `identity` spends none, so the two disagree from the first
byte at the same index. Every finding's `meta.txt` records the family and
the `reproduce` line spells it out.

`wasmtime` must be on `PATH` for the WASM execution rung. The repo root
and the `almide` binary are autodetected; override with `--repo` /
`--almide`.

## The generator

Three families (`SYNTHESIS_WEIGHT` / `MUTATION_WEIGHT` /
`IDENTITY_WEIGHT` in `src/generator/mod.rs`): the identity family takes
`3/13` ≈ 23% of a mixed campaign, and the remaining 77% splits 7:3
between synthesis and mutation as before.

The identity decision is drawn from a **separate** RNG sub-stream
(`IDENTITY_STREAM_SALT`) rather than the program's main stream. Any draw
added to the main stream would re-key it, and an archived finding's
`(seed, index)` would stop regenerating the program it was minimized
from; splitting the decision off keeps the pre-#1332 synthesis and
mutation streams byte-identical.

### The identity family (~23%) — the by-construction oracle (#1332)

`src/generator/identity.rs`. Builds each program **backwards from its
answer**, the Rustlantis move. Every accumulator starts at a literal `K`,
and every statement group between the initializer and the `println` is an
**identity transformer** — so the program must print `K`, a literal
visible in its own source, and no second execution is needed to judge it.

```almide
// @expect a0=-3728
fn main() -> Unit = {
  var a0 = -3728
  let snap2 = a0          // ← #1322's shape: sound only if this COPIES
  a0 = a0 + 1796
  var it8 = 0
  while it8 < 3 {         // ← loop-carried scalar state
    let cur9 = a0
    a0 = cur9 + 896
    it8 = it8 + 1
  }
  a0 = a0 - 2688
  a0 = snap2
  println("a0=${a0}")
}
```

Soundness is structural, not measured:

1. **Every block is an identity by algebra.** It carries its own inverse
   (`+n`/`-n`, `*m`/`/m`, `xor n`/`xor n`, swap/swap, snapshot/restore),
   or is compensated by a constant the generator computed while emitting
   (`while` trips × step, `0..<t`'s triangular number, a list whose
   elements sum to zero), or is balanced across **both** arms of a branch
   so the taken arm cannot matter. A block's body is itself a list of
   identity blocks, so nesting composes.
2. **No arithmetic can overflow or truncate.** The generator carries a
   conservative bound on `|acc|` and refuses any block that would push it
   past `2^40`. Integer division appears only as the closing half of a
   `*m` / `/m` pair, where the dividend is exactly `m ×` the opening
   value. (Almide's `+`/`-` lower to `wrapping_*`, so the additive
   inverses would hold even without the bound; the bound is what makes
   `/` and the compensated loops exact.)

Neither property depends on the compiler being correct, which is the
point. Block weights are biased toward the three shapes #1332 names —
loop-carried scalar state, let-of-var aliasing, branch-arm assignment.

**Deliberately NOT covered**: strings, floats, Unicode, collections as
data, effects, generics. None of those has a cheap by-construction
inverse; they stay under the differential oracle in the other two
families. A narrow family with a real oracle beats a broad one with none.

**Minimization is structural** (`minimize::minimize_plan`): identity
findings shrink through the *plan*, never the text. Almost every line is
one half of an inverse pair, so deleting a line changes the value the
program is supposed to print — a text-level shrink would "still
reproduce" for a reason unrelated to the bug and quietly turn a
miscompile into a generator artifact.

### The other two families

- **Type-directed synthesis (~54%)** — `src/generator/{term,program}.rs`.
  Builds programs well-typed *by construction* from a typed term grammar:
  pick a goal type, generate an expression of that type from literals,
  in-scope variables, stdlib calls whose return type unifies with the
  goal, inline lambdas for HOF arguments, and `if` arms. Fuel-bounded for
  termination. Ambiguous literals (`[]`, `none`, `ok`/`err`) are hoisted
  into annotated `let` bindings so they type-check.

- **Stdlib catalogue** — `src/generator/catalogue.rs`. Signatures are
  extracted from a **machine source**: the bundled `stdlib/*.almd`
  declaration files, parsed with the real Almide parser. A curated
  *weight table* overlays the parsed surface to bias selection toward the
  historic divergence clusters (string/Unicode, float formatting,
  closures/HOFs).

- **Value pools** — `src/generator/pools.rs`. Named, commented tables of
  divergence-prone literals: multibyte strings (`日本語`, emoji, `é`,
  combining marks, `ß`), float boundaries (`-0.0`, `5e-324`, `1e300`,
  `0.1+0.2` shapes), int extremes (`i64::MIN/MAX`, width boundaries).

- **Mutation (~23%)** — `src/generator/mutate.rs`. Parses the
  `main`-bearing corpus (`spec/wasm_cross`, `examples`, …), strips `test`
  blocks, and applies type-preserving AST mutations (literal
  perturbation from the pools, equal-kind subexpression swap, statement
  duplication). `// wasm:skip` files are excluded (known divergences).

Determinism: `SplitMix64` (`src/rng.rs`) seeded `for_program(seed, index)`
for the program itself, plus the salted sub-stream above for the family
roll. No wall-clock / fs / process calls appear in generated programs
(effects whitelist: `println`/`print`).

## The oracle ladder

`src/oracle/` — cheap→expensive, first failure classifies the program:

| Rung | Check | Failure means |
|------|-------|---------------|
| a | `almide check` accepts | **generator bug** (we promised well-typed) — counted, not a finding |
| b | `parse∘fmt` is idempotent | formatter instability |
| c | native build + run (no ICE) | native codegen failure |
| d | wasm build + validate | wasm codegen failure |
| e | **self-check** — each leg vs its `@expect` output | **miscompile**, attributable to `native` / `wasm` / **`both legs`** |
| f | run both, byte-compare stdout/exit | **divergence** (or a hang) |
| g | both vs the reference interpreter | one backend wrong, when the interpreter votes |

Rung (e) fires only for programs that carry a by-construction oracle (the
identity family). It runs **before** the differential comparison because
it is the only rung that can convict two legs at once — but **after** the
resource-limit skips (C-196 stack, C-197 wasm32 memory), so a wasm OOM is
still a skip rather than a bogus miscompile.

## Minimizer

`src/minimize.rs` — delta-debugging: statement removal then expression
simplification, re-running the ladder and keeping a shrink only if the
same finding kind reproduces. Identity findings take the structural path
(`minimize_plan`, see above) instead. Output lands in `findings/<kind>/`:
`repro.almd`, `original.almd`, `meta.txt` (seed/index/family/replay
command), `native.out`, `wasm.out`. Findings are deduplicated by
`(kind, summary)`.

## Throughput

The WASM build + native cargo build are the bottleneck. Workers each own
an **isolated** build scratch dir (`ALMIDE_RUN_PROJECT_DIR`), so the
shared-`/tmp` build flock never serializes them — throughput scales with
cores. Measured locally (Apple M-series, see the campaign summary line):
roughly **40–120 programs/min** depending on `--jobs` and the
native-build cache warmth (the per-program cost is dominated by the
native `cargo` rebuild; warm caches are ~0.25 s/program).

## Nightly CI

`.github/workflows/fuzz-nightly.yml` runs a time-budgeted campaign at
03:00 UTC (and on manual dispatch), uploads `findings/` as an artifact,
and opens/updates a `fuzz`-labelled tracking issue when findings > 0. PR
CI is intentionally left untouched — the fuzzer is nightly only.

The nightly needs no change to pick up the identity family: it runs the
default `--family all`, whose weight table now includes it, so roughly a
quarter of every night is judged by the by-construction oracle. The
campaign summary reports the exact share as `self-checked = N (P%)`.
`SelfCheckFailure` is a correctness class, so a night that finds one goes
red and files the tracking issue like any other miscompile.

A future refinement (not wired): give one matrix shard
`--family identity` so a fixed fraction of the night's coverage is
oracle-backed regardless of how the weights are retuned.
