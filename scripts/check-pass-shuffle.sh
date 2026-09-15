#!/usr/bin/env bash
# PASS-ORDER SHUFFLE GATE (#2186 step 4).
#
# The native nanopass pipeline runs in the order `target.rs` spells, and for a
# long time that order was the ONLY record of which pass needs which: 37
# `.add(` calls, "(order matters!)" comments, and 17 declared edges. Every
# pass now declares its `depends_on` / `run_before` edges (and the three
# representation boundaries — `UnifyVarTables`, `ConcretizeTypes`,
# `IrLinkFlatten` — are barriers), and this gate checks that the declared
# edges are the WHOLE truth: under `ALMIDE_SHUFFLE_PASSES=<seed>` the pipeline
# runs in a random order that respects only the declared edges, and the Rust
# it emits must be byte-identical to the declared order's, file for file.
#
# A divergence names the file and the order that produced it; the pair the
# divergence needs declared is found by `scripts/pass-shuffle-bisect.py`
# (delta-debugging over the inverted pairs with `ALMIDE_PASS_EDGES`).
#
# Corpus: every `.almd` under spec/lang and spec/wasm_cross (the language
# and cross-target fixtures — ~950 programs), or the list in `$CORPUS` (the
# negative controls hand a short one to their stand-in compilers); seeds:
# 1..$SEEDS (default 3, fixed, so the gate is deterministic). `ALMIDE_BIN`
# is the compiler.
set -uo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

ALMIDE="${ALMIDE:-${ALMIDE_BIN:-almide}}"
SEEDS="${SEEDS:-3}"
JOBS="${JOBS:-8}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

if [ -n "${CORPUS:-}" ]; then
  cp "$CORPUS" "$tmp/corpus"
  floor=1
else
  find spec/lang spec/wasm_cross -name '*.almd' | sort > "$tmp/corpus"
  floor=100
fi
count=$(wc -l < "$tmp/corpus" | tr -d ' ')
[ "$count" -ge "$floor" ] || { echo "::error::pass-shuffle: only $count corpus file(s) found — the corpus moved and the gate went blind"; exit 1; }

# One emission per file, hashed; `$1` = seed ("" for the declared order).
emit_all() {
  local seed="$1" out="$2"
  mkdir -p "$out"
  export ALMIDE seed out
  # shellcheck disable=SC2016
  xargs -P "$JOBS" -I{} bash -c '
    f="$1"; key="$(printf "%s" "$f" | tr "/" "_")"
    if [ -n "$seed" ]; then
      ALMIDE_SHUFFLE_PASSES="$seed" "$ALMIDE" "$f" --target rust > "$out/$key.rs" 2> "$out/$key.err" < /dev/null
    else
      "$ALMIDE" "$f" --target rust > "$out/$key.rs" 2> "$out/$key.err" < /dev/null
    fi
    echo $? > "$out/$key.code"
  ' _ {} < "$tmp/corpus"
  ( cd "$out" && for f in *.rs; do printf '%s  %s\n' "$(shasum -a 256 < "$f" | cut -c1-16)" "$f"; done ) > "$out/.hashes"
}

emit_all "" "$tmp/declared"
fail=0
for seed in $(seq 1 "$SEEDS"); do
  emit_all "$seed" "$tmp/seed$seed"
  # A pass that panics under the shuffle (an undeclared `depends_on` the
  # runtime validator caught, or an ICE) is a failure in its own right.
  panics=$(grep -l "panicked" "$tmp/seed$seed"/*.err 2>/dev/null | wc -l | tr -d ' ')
  if [ "$panics" != 0 ]; then
    echo "::error::pass-shuffle: seed $seed — $panics file(s) panicked under the shuffled order:"
    grep -l "panicked" "$tmp/seed$seed"/*.err | head -5 | while read -r e; do echo "    $(basename "$e" .err): $(grep -m1 panicked "$e")"; done
    fail=1
  fi
  diverged=$(diff "$tmp/declared/.hashes" "$tmp/seed$seed/.hashes" | grep '^<' | awk '{print $3}')
  if [ -n "$diverged" ]; then
    n=$(printf '%s\n' "$diverged" | wc -l | tr -d ' ')
    echo "::error::pass-shuffle: seed $seed — $n file(s) emit different Rust under a pass order the declared edges permit (a dependency is missing its declaration; bisect with scripts/pass-shuffle-bisect.py):"
    first=$(printf '%s\n' "$diverged" | head -1)
    echo "    order: $(grep -m1 ' order: ' "$tmp/seed$seed/${first%.rs}.err" | sed 's/.* order: //')"
    printf '%s\n' "$diverged" | head -8 | sed 's/^/    /'
    diff "$tmp/declared/$first" "$tmp/seed$seed/$first" | head -6 | sed 's/^/    | /'
    fail=1
  fi
done

if [ "$fail" = 0 ]; then
  echo "pass-shuffle OK: $count file(s) emit byte-identical Rust under $SEEDS shuffled pass order(s)"
fi
exit $fail
