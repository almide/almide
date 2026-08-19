# SPIKE S2a — sema-as-queries mechanics: VERDICT

Date: 2026-08-19. Gates (d)(e)(f) of ARCHITECTURE.md §6.5. Harness:
`cargo run --release -p almide-spine --bin s2_bench` (crates/almide-spine/src/s2.rs).

## The firewall chain measured

```
SourceFile.text → parse_decls(file) → decl_fp(key) → check_decl(key)
                        ↘ project_symbols → symbol_fp(name) ↗
```

Fingerprints are hashes of each decl's JSON — spans and expression ids never
reach JSON in the ported AST (`#[serde(skip)]` throughout), so span-independence
falls out of the serialization boundary instead of needing rust-analyzer's
AstId maps or MoonBit's relative locations. Interface = decl JSON minus
`body`/`value`; deps are name-level and overapproximate (every string in the
body JSON naming a project symbol).

## Numbers (spec/ corpus + 10 injected `__s2_base_k`/`__s2_user_k` pairs)

Cold: 1,098 files, **6,973 decls, 5,408 symbols, 15,673 dep edges**, 344.7 ms.

| gate | edit | re-checks | verdict |
|---|---|---|---|
| (d) | body of `__s2_base_k` (digit toggle; interface unchanged) | **max 1** over 20 rounds; dependents never re-ran | **PASS** |
| (e) | interface (rename `__s2_base_k` ⇄ `__s2_basex_k`) | **exactly 2** (min=max over 10 rounds): the decl + its one true dependent | **PASS** |
| (f) | span-only (blank line prepended) | **0** over 20 rounds | **PASS** |

Warm re-derive incl. iterating all 6,973 memoized checks: ~1.1 ms (span edit)
/ ~7 ms (body edit). Informational — cost gate (g) needs the real checker.

## A finding worth keeping: the gate caught a test-design bug, not a machinery bug

The first (e) run used ONE shared pair name across all 10 victim files and
"failed" with re-checks min 1 / max 11. That was the machinery being MORE
precise than the test: renaming one copy while 9 identical-signature shadows
remained left the name's interface fingerprint unchanged (1 re-check —
correct); deleting the last copy flipped `symbol_fp` Some→None and exactly
the 10 true dependents plus the renamed decl re-ran (11 — correct). Unique
names fixed the EXPERIMENT and (e) became exactly 2. Lesson recorded: global
name-keyed interfaces deduplicate identical signatures for free.

## What remains open (honest)

- **(g)** — warm full-loop ≥10x with the REAL checker cost — is unit 4 proper:
  port the checker into this query shape. S2a proves the invalidation
  geometry; it says nothing about check-phase cost.
- Deps are name-level overapprox; the real checker will read true resolution.
- Symbol keys are frozen at setup in the spike; production needs dynamic
  symbol interning (standard salsa interned structs).

## Decision

(d)(e)(f) all PASS on top of S1's (a)(b)(c) → **unit 4 (sema port into the
firewall shape) is GO**, with gate (g) as its acceptance bar alongside corpus
diagnostics parity (§5) and the Zig-style incremental diagnostic scenarios.
