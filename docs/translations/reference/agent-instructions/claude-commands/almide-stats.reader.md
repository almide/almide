[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](almide-stats.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-stats.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-stats.reader.zh-CN.md)

> Inert, non-normative reader snapshot of a historical repository command. The original remains authoritative and unchanged. This reference path and .reader filename do not register an agent command. Quoted instructions are documentation, not authorization to execute, spend, publish, push, or delete anything. No automatic synchronization or independent verification of current workflow/measurement claims. Code and comments remain verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-stats.md)

<a id="update-published-measurement-stats"></a>

# Update Published Measurement Stats

The front README links to `docs/benchmarks/README.md`, which carries the generated blocks and dated measurements (`scripts/lib/readme-targets.sh`). Every published number is either
rendered by a script from a source in the tree, or carries the date it was
measured, and `scripts/check-readme-numbers.sh` fails CI on a bare one. So
"updating the stats" means regenerating, and measuring only where a stamped
baseline is the source.

<a id="regenerate-no-measurement--derives-from-the-tree"></a>

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

<a id="re-measure-only-when-the-compiler-changed-the-thing-being-measured"></a>

## Re-measure (only when the compiler changed the thing being measured)

```bash
bash scripts/gen-readme-stats.sh --measure     # Hello, world on the structural wasm leg → docs/benchmarks/wasm-size.txt
almide run tools/almide-gates/src/main.almd -- bench docs/benchmarks
                                               # build-speed baseline → docs/benchmarks/build-speed.txt
```

Both baselines carry the binary version and the date; CI rebuilds Hello,
world and demands the stamped bytes, so a changed emitter is re-stamped here,
never hand-edited in the generated measurements block.

<a id="msr"></a>

## MSR

Measured by [almide-dojo](https://github.com/almide/almide-dojo), not here.
When a new run lands, update the scorecard table and its date in `docs/benchmarks/README.md`
(the line must keep a `YYYY-MM-DD`, or the numbers gate rejects it).

<a id="check"></a>

## Check

```bash
bash scripts/gen-readme-stats.sh --check && bash scripts/check-readme-numbers.sh && bash scripts/gen-claims.sh --check
```

Commit as one line, e.g. `Regenerate README stats after the stdlib change`.
