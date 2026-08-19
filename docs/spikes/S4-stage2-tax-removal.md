# Unit 4 stage 2 — removing the check taxes, adjudicated by parity

Date: 2026-08-19. Probe: `s4_probe`; bench: `s3_bench`; referee: the
1,062-file oracle parity manifest (all three variants must hash-match it).

## Where a per-file check actually went (s4_probe, 211-file sample)

| phase | avg | share |
|---|---|---|
| parse | 0.05 ms | 1.1% |
| resolve (+bundled stdlib parse) | 0.10 ms | 2.3% |
| canonicalize + from_env + refresh | 1.09 ms | 25.5% |
| **infer_program (the user's code!)** | **0.25 ms** | **5.7%** |
| **#862 bundled-stdlib re-inference** | **2.71 ms** | **63.3%** |
| lower + unused warnings | 0.09 ms | 2.0% |

**89% of every check was stdlib tax.** Per-decl decomposition of the entry
(5.7%) was the planned stage-2 move — the probe redirected the effort.

## The three variants, each parity-adjudicated (1,062/1,062 byte-identical)

| variant | change | batch (1,062 files) | warm 1-file edit |
|---|---|---|---|
| v1 | faithful cmd_check_json | 6,807 ms* | 4.42 ms |
| v2 | drop the #862 loop for stdlib-only entries | 2,516 ms | 1.78 ms |
| v3 | + per-import-set env template (module-half canonicalize + from_env + refresh computed once, cloned per file) | **924 ms** | **0.62 ms** |

*v1 batch predates the unused-warning stage; diagnostics counts differ for
that reason only (100 vs 125); the parity manifest is the authority.

- v2's justification is semantic AND empirical: for a stdlib-only entry the
  loop emits nothing (bundled modules are "compiled in and CI-gated" — the
  incumbent's own words), its mutations land after the entry's diagnostics
  are taken, and the parity gate proves the one remaining coupling candidate
  (env/type_map read by the unused pass) does not observably fire.
- v3 required stage-2 STRUCTURAL changes (new code, not ports):
  `canonicalize_modules_env` / `canonicalize_entry_onto` split in
  almide-frontend (the verbatim `canonicalize_program` is untouched; the
  split's validity domain is stdlib-only entries, stated at the definition),
  plus `#[derive(Clone)]` on `Checker` / `TypeEnv` / `Constraint` — the
  entire cascade was three derives.

## Where this leaves the §6.5 / §5 gates

- (g): PASS at **1499x** (and the batch baseline itself is now 7.4x cheaper
  than the incumbent model). Keystroke latency: **0.62 ms median**,
  independent of project size.
- Invalidation exactness unchanged: max 1 check/edit, 0 on no-edit.
- **Still open (honest):** per-DECL granularity inside a file. Entry
  inference is 0.25 ms on the average file, so the median payoff is small,
  but the largest corpus files still pay whole-file inference on every
  body edit; the S2a firewall shape stands ready. Also open: incremental
  diagnostic scenarios, E1 wiring.
