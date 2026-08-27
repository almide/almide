# Update README Stats

The README no longer carries hand-written counts: every number is either
rendered by a script from a source in the tree, or carries the date it was
measured, and `scripts/check-readme-numbers.sh` fails CI on a bare one. So
"updating the stats" means regenerating, and measuring only where a stamped
baseline is the source.

## Regenerate (no measurement — derives from the tree)

```bash
bash scripts/gen-readme-stats.sh        # stdlib functions/modules (from docs/stdlib signature indexes),
                                        # spec/ test files, contract count; renders the Hello, world
                                        # size table from docs/benchmarks/wasm-size.txt
bash scripts/gen-claims.sh              # contract-ledger claims block + proofs/STAGE-STATUS.md
almide run tools/almide-gates/src/main.almd -- bench docs/benchmarks --readme
                                        # build-speed block from docs/benchmarks/build-speed.txt
```

The lefthook pre-commit hooks run the first two whenever their inputs change;
running them by hand is for a fresh checkout or after a stdlib change
(regenerate the signature indexes first: `make stdlib-docs`).

## Re-measure (only when the compiler changed the thing being measured)

```bash
bash scripts/gen-readme-stats.sh --measure     # Hello, world on both wasm legs → docs/benchmarks/wasm-size.txt
almide run tools/almide-gates/src/main.almd -- bench docs/benchmarks
                                               # build-speed baseline → docs/benchmarks/build-speed.txt
```

Both baselines carry the binary version and the date; CI rebuilds Hello,
world and demands the stamped bytes, so a changed emitter is re-stamped here,
never edited in README.md.

## MSR

Measured by [almide-dojo](https://github.com/almide/almide-dojo), not here.
When a new run lands, update the scorecard table and its date in README.md
(the line must keep a `YYYY-MM-DD`, or the numbers gate rejects it).

## Check

```bash
bash scripts/gen-readme-stats.sh --check && bash scripts/check-readme-numbers.sh && bash scripts/gen-claims.sh --check
```

Commit as one line, e.g. `Regenerate README stats after the stdlib change`.
