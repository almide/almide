# Stdlib Excellence — the content-quality program (ratified 2026-08-19)

The port doctrine carries incumbent content bug-compatibly; the user has
directed that CONTENT (stdlib first) must instead land at world-best quality.
This ledger is that directive made executable. It does not weaken the parity
net — it adds the protocol for changing what the net protects.

## The intentional-change protocol

Every content improvement follows four steps, in one commit:
1. **Justify** — cite the audit/survey evidence (which weakness, vs which
   world reference).
2. **Classify** — `additive` (new surface; existing goldens must stay green),
   `behavioral` (existing outputs change; the parity diff is REVIEWED
   file-by-file, then goldens regenerate with the reason recorded here), or
   `removal` (pre-release only; after first release it needs the deprecation
   window per the incumbent's interface-diff doctrine).
3. **Gate** — new surface ships with executable evidence: run-fixtures through
   the interpreter (`s5::run_file`) at minimum; family-complete per the
   incumbent's matrix rule (a family ships whole, never point-wise).
4. **Record** — one row in the table below flips to DONE with the commit.

## Work queue (audit-derived, ranked; to be re-based on the 9-compiler stdlib survey once RESEARCH-stdlib.md lands)

| # | item | evidence | class | status |
|---|---|---|---|---|
| 1 | Remove `openai_streaming_call` from core `http` | audit: vendor API in stdlib is a layering violation with a short shelf life | removal (pre-release) | **DONE 2026-08-19** |
| 2 | `url` module — parse/build/percent-encode | audit: no URL handling; the incumbent's own effect-capability table names a phantom `url` module | additive | queued — survey-first: the 9-compiler stdlib survey (RESEARCH-stdlib.md) sets the design bar before any new module is authored |
| 3 | Opaque `net` handles — `TcpStream`/`TcpListener` types instead of raw `Int` | audit: "indefensible in a language that made `Int` illegal as a *time*" | behavioral | queued |
| 4 | Interp bridge coverage burn-down (the 138 `Unsupported` skips: `prim.handle` slices #1226, `prim.alloc_*`) | unit-3 gate histogram; world-best = the executable spec covers its own contract corpus | additive (oracle-side) | queued, ceiling 138 shrink-only |
| 5 | `log` module (leveled, structured) | audit: phantom module in the capability table; no logging story | additive | queued |
| 6 | `uuid` module (v4 + v7) | audit gap; needs a CSPRNG stance first (see #8) | additive | queued |
| 7 | `time` module or capability-table fix | audit: capability table names `time`, stdlib has only `datetime` | additive/doc | queued |
| 8 | Crypto stance: HMAC-SHA256 + CSPRNG doctrine | audit: `hash.sha256` + `fnv1a32` only; no HMAC, no crypto-grade random | additive, needs design ratification (○×) | queued |
| 9 | `Ord` auto-derive parity with `Eq`/`Hash` | audit: inconsistency an LLM trips on; `<` stays hardwired to 4 types | behavioral (checker) | queued |
| 10 | Retire hand-monomorphized `fan_map_*` type-pair family | falls out of R3 (dictionary ABI) — do not hand-fix | blocked on unit 6+ | — |
| 11 | Stdlib fmt canonicalization (legacy fn-slot spellings) | audit: stdlib not formatted to its own canon | mechanical | blocked on almide-tools port |

Rules: the queue is append-only; a DONE row names its commit; `behavioral`
rows may not land while their parity impact is unreviewed.
