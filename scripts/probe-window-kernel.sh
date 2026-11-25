#!/usr/bin/env bash
# A measurement, not a gate: the Rocq kernel's verdict on every window the
# checker cut out of a shipped module (proofs/GuardWindow.v, `window_ok`).
#
#   scripts/probe-window-kernel.sh <dump> <work-dir>
#
# <dump> is, per module, a line `MODULE <leg> <file>` and one line
# `WINDOWHEX <function> <access instruction> <bytes...>` per window; the bytes
# are the module's own. Writes <work-dir>/verdicts.tsv (leg, file, function,
# access, bytes, ok | REFUSED) and prints the totals.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
DUMP="${1:?usage: scripts/probe-window-kernel.sh <dump> <work-dir>}"
WORK="${2:?usage: scripts/probe-window-kernel.sh <dump> <work-dir>}"
mkdir -p "$WORK"
WORK="$(cd "$WORK" && pwd)"
COQC="${COQC:-$(command -v coqc)}"

if ! (cd "$ROOT/proofs" && "$COQC" -Q . AlmideTrust GuardWindow.v > "$WORK/GuardWindow.log" 2>&1); then
  echo "window kernel check: proofs/GuardWindow.v does not compile"
  head -20 "$WORK/GuardWindow.log"
  exit 1
fi
grep -m1 'Closed under the global context' "$WORK/GuardWindow.log" || { echo "window kernel check: the theorem is not closed"; head -20 "$WORK/GuardWindow.log"; }

python3 -I - "$DUMP" "$WORK" "${WINDOW_CHUNK:-150}" <<'PY'
import sys
dump, work, chunk = sys.argv[1], sys.argv[2], int(sys.argv[3])
wins, cur = [], None
for line in open(dump, encoding="utf-8", errors="replace"):
    t = line.split()
    if not t:
        continue
    if t[0] == "MODULE":
        cur = (t[1], " ".join(t[2:]))
    elif t[0] == "WINDOWHEX" and cur is not None:
        wins.append((cur[0], cur[1], t[1], t[2], t[3:]))
HEAD = """From AlmideTrust Require Import GuardWindow.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""
def example(name, bs):
    return f"Example {name} : window_ok [{';'.join(bs)}] = true.\nProof. vm_compute. reflexivity. Qed.\n"
with open(f"{work}/index.tsv", "w", encoding="utf-8") as idx:
    for i, (leg, label, func, acc, bs) in enumerate(wins):
        k, name = i // chunk, f"w{i:06d}"
        with open(f"{work}/Win_{k:04d}.v", "a", encoding="utf-8") as f:
            if i % chunk == 0:
                f.write(HEAD)
            f.write("\n" + example(name, bs))
        with open(f"{work}/one_{name}.v", "w", encoding="utf-8") as f:
            f.write(HEAD + "\n" + example(name, bs))
        idx.write(f"Win_{k:04d}\t{name}\t{leg}\t{label}\t{func}\t{acc}\t{len(bs)}\n")
print(f"{len(wins)} window(s) in the dump")
PY

export COQC ROOT WORK
find "$WORK" -maxdepth 1 -name 'Win_*.v' | LC_ALL=C sort \
  | xargs -P "${WINDOW_JOBS:-4}" -I{} bash -c 'c="$(basename "$1" .v)"; if "$COQC" -Q "$ROOT/proofs" AlmideTrust "$1" > "$WORK/$c.log" 2>&1; then echo 0 > "$WORK/$c.rc"; else echo 1 > "$WORK/$c.rc"; fi' _ {}

: > "$WORK/verdicts.tsv"
for v in "$WORK"/Win_*.v; do
  [ -e "$v" ] || continue
  c="$(basename "$v" .v)"
  if [ "$(cat "$WORK/$c.rc" 2>/dev/null)" = 0 ]; then
    awk -F'\t' -v c="$c" '$1 == c { print $3 "\t" $4 "\t" $5 "\t" $6 "\t" $7 "\tok" }' "$WORK/index.tsv" >> "$WORK/verdicts.tsv"
  else
    # the chunk holds a window the kernel refuses: ask window by window
    while IFS=$'\t' read -r chunk name leg label func acc n; do
      [ "$chunk" = "$c" ] || continue
      if "$COQC" -Q "$ROOT/proofs" AlmideTrust "$WORK/one_$name.v" > "$WORK/one_$name.log" 2>&1; then
        printf '%s\t%s\t%s\t%s\t%s\tok\n' "$leg" "$label" "$func" "$acc" "$n" >> "$WORK/verdicts.tsv"
      else
        printf '%s\t%s\t%s\t%s\t%s\tREFUSED\n' "$leg" "$label" "$func" "$acc" "$n" >> "$WORK/verdicts.tsv"
        cp "$WORK/one_$name.v" "$WORK/refused_$name.v"
      fi
    done < "$WORK/index.tsv"
  fi
done

python3 -I - "$WORK/verdicts.tsv" <<'PY'
import collections, sys
rows = [l.rstrip("\n").split("\t") for l in open(sys.argv[1], encoding="utf-8")]
for leg in sorted(set(r[0] for r in rows)):
    R = [r for r in rows if r[0] == leg]
    by = collections.Counter(r[5] for r in R)
    mods = len(set(r[1] for r in R if r[5] == "ok"))
    sizes = sorted(int(r[4]) for r in R)
    print(f"window kernel check [{leg}]: {len(R)} window(s): {dict(by)}; accepted windows in {mods} module(s); bytes per window min {sizes[0]} median {sizes[len(sizes)//2]} max {sizes[-1]}")
PY
