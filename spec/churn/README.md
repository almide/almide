# Frees churn gates (ALMIDE_WASM_FREES=1)

Long-loop reclamation fixtures for the real-RC wasm mode. NOT part of the
default corpus (millions of iterations); run via:

```
scripts/check-frees-churn.sh
```

Pass bar per fixture: byte-identical stdout to native, exit 0, and flat RSS
(within 2x of the frees-OFF baseline). The corpus alone passed builds with
live double-frees twice — these shapes are the gate the corpus cannot be.

- record_churn.almd — 2M iterations of construct+drop with a fresh string
  field (free-list reuse, the sentinel/resurrection belt).
- tco_loop_churn.almd — 200k loop-variant TCO calls (the shape that exposed
  the M2 managed-param OOB; acceptance test for per-iteration TCO reclaim).
