#!/usr/bin/env bash
# Guarded accesses of SHIPPED modules, proven in bounds from their own bytes —
# checked by the Rocq kernel, window by window.
#
# The bounds witness is judged over a trace model, and almide-anchor-check
# (unverified) holds its events to the shipped instructions. This script asks
# for less trust on the part where less is needed: a WINDOW is a straight-line
# run of a function's shipped instructions that ends in a load or a store
# (the compare, its aborting arm, the address arithmetic, the access).
# GuardWindow.v decodes a window's bytes, runs them symbolically and accepts
# when the access is `base + 12 + idx * S + o` under a compare that left
# `idx <u len(base) / S`; `window_sound` then puts the access inside that
# block's payload in every run, with 32- and 64-bit wrap-around. No witness
# and no anchor file is read.
#
#   proofs/check-shipped-windows.sh <dump> <work-dir> <leg>
#
# <dump> is the concatenation, per module, of a line `MODULE <label>` and the
# output of `almide-anchor-check --windows <artifact.wasm>` (`WINDOW
# <function> <access instruction> <bytes...>` per proposed window, then
# `ACCESSES <loads and stores swept>`). The cutter only proposes; each window
# is a goal `window_ok [bytes] = true` the kernel computes. <leg> names the
# row of proofs/shipped-windows-floor.txt the count is held to.
#
# Fails when a proposed window is REFUSED by the kernel (the cutter and the
# proof drifted apart), when a module's dump has no `ACCESSES` line (the
# sweep went blind), or when fewer windows are proven than the leg's floor
# (evidence was lost: restore it, or lower the floor in its own commit and
# say why). Writes <work-dir>/verdicts.tsv (label, function, access, bytes,
# ok | REFUSED).
#
# The floor counts the leg's WHOLE list, so it is judged on a full run only.
# Under a CI slice (ALMIDE_CI_SLICE=k > 1, a PR into develop: one fixture in
# k) every proposal the slice holds is still judged by the kernel and a
# refusal still fails, but the count is printed with no verdict against the
# floor, and the last line says so.
#
# Needs coqc and the compiled spine (proofs/check.sh, or at least
# GuardWindow.vo).
set -euo pipefail
export LC_ALL=C
HERE="$(cd "$(dirname "$0")" && pwd)"
DUMP="${1:?usage: proofs/check-shipped-windows.sh <dump> <work-dir> <leg>}"
WORK="${2:?usage: proofs/check-shipped-windows.sh <dump> <work-dir> <leg>}"
LEG="${3:?usage: proofs/check-shipped-windows.sh <dump> <work-dir> <leg>}"
FLOORS="${SHIPPED_WINDOWS_FLOOR:-$HERE/shipped-windows-floor.txt}"
mkdir -p "$WORK"
WORK="$(cd "$WORK" && pwd)"
COQC="${COQC:-$(command -v coqc)}"
[ -f "$HERE/GuardWindow.vo" ] || { echo "FAIL: proofs/GuardWindow.vo is not built (run proofs/check.sh first)"; exit 1; }

# One .v per chunk of windows: `<work>/Win_<k>.v`, and `<work>/index.tsv`
# (chunk, example name, label, function, access, bytes).
python3 -I - "$DUMP" "$WORK" "${WINDOW_CHUNK:-200}" <<'PY'
import sys
dump, work, chunk = sys.argv[1], sys.argv[2], int(sys.argv[3])
mods, wins, cur = [], [], None
for line in open(dump, encoding="utf-8"):
    t = line.split()
    if not t:
        continue
    if t[0] == "MODULE":
        cur = {"label": " ".join(t[1:]), "accesses": None, "other": []}
        mods.append(cur)
    elif cur is None:
        sys.exit(f"FAIL: the dump starts with `{line.strip()[:60]}`, not a MODULE line")
    elif t[0] == "WINDOW":
        wins.append((cur["label"], t[1], t[2], t[3:]))
    elif t[0] == "ACCESSES":
        cur["accesses"] = int(t[1])
    else:
        cur["other"].append(line.rstrip("\n"))
blind = [m for m in mods if m["accesses"] is None]
if blind:
    for m in blind[:5]:
        print(f"FAIL: {m['label']}: no ACCESSES line: {' | '.join(m['other'])[:200]}")
    sys.exit(1)
if not mods:
    sys.exit("FAIL: the dump names no module")

HEAD = """From AlmideTrust Require Import GuardWindow.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""

def example(name, label, bs):
    return f"(* {label} *)\nExample {name} : window_ok [{';'.join(bs)}] = true.\nProof. vm_compute. reflexivity. Qed.\n"

with open(f"{work}/index.tsv", "w", encoding="utf-8") as idx:
    for i, (label, func, acc, bs) in enumerate(wins):
        k, name = i // chunk, f"w{i:06d}"
        with open(f"{work}/Win_{k:04d}.v", "a" if i % chunk else "w", encoding="utf-8") as f:
            if i % chunk == 0:
                f.write(HEAD)
            f.write("\n" + example(name, label, bs))
        idx.write(f"Win_{k:04d}\t{name}\t{label}\t{func}\t{acc}\t{len(bs)}\n")
with open(f"{work}/swept.txt", "w", encoding="utf-8") as f:
    f.write(f"{len(mods)} {sum(m['accesses'] for m in mods)} {len(wins)}\n")
print(f"{len(mods)} module(s), {sum(m['accesses'] for m in mods)} load(s) and store(s) swept, {len(wins)} window(s) proposed")
PY

: > "$WORK/verdicts.tsv"
fail=0
# The chunks are independent: compile them side by side, then read each one's
# exit code in order.
export COQC HERE WORK
find "$WORK" -maxdepth 1 -name 'Win_*.v' | LC_ALL=C sort \
  | xargs -P "${WINDOW_JOBS:-4}" -I{} bash -c 'c="$(basename "$1" .v)"; if "$COQC" -Q "$HERE" AlmideTrust "$1" > "$WORK/$c.log" 2>&1; then echo 0 > "$WORK/$c.rc"; else echo 1 > "$WORK/$c.rc"; fi' _ {}
for v in "$WORK"/Win_*.v; do
  [ -e "$v" ] || continue
  c="$(basename "$v" .v)"
  if [ "$(cat "$WORK/$c.rc" 2>/dev/null)" = 0 ]; then
    awk -F'\t' -v c="$c" '$1 == c { print $3 "\t" $4 "\t" $5 "\t" $6 "\tok" }' "$WORK/index.tsv" >> "$WORK/verdicts.tsv"
  else
    # the chunk holds a window the kernel refuses: ask window by window
    while IFS=$'\t' read -r chunk name label func acc n; do
      [ "$chunk" = "$c" ] || continue
      {
        printf 'From AlmideTrust Require Import GuardWindow.\nFrom Stdlib Require Import ZArith.\nFrom Stdlib Require Import List.\nImport ListNotations.\nOpen Scope Z_scope.\n\n'
        awk -v n="$name" '$1 == "Example" && $2 == n { on = 1 } on { print } on && /Qed\./ { exit }' "$WORK/$c.v"
      } > "$WORK/one_$name.v"
      if "$COQC" -Q "$HERE" AlmideTrust "$WORK/one_$name.v" > "$WORK/one_$name.log" 2>&1; then
        printf '%s\t%s\t%s\t%s\tok\n' "$label" "$func" "$acc" "$n" >> "$WORK/verdicts.tsv"
      else
        printf '%s\t%s\t%s\t%s\tREFUSED\n' "$label" "$func" "$acc" "$n" >> "$WORK/verdicts.tsv"
        echo "FAIL: $label: the window at function $func, instruction $acc is proposed by the cutter and refused by the kernel ($WORK/one_$name.v)"
        fail=1
      fi
    done < "$WORK/index.tsv"
  fi
done

floor="$(awk -v leg="$LEG" '$1 == leg { print $2 }' "$FLOORS")"
[ -n "$floor" ] || { echo "FAIL: $FLOORS has no row for the leg \`$LEG\`"; exit 1; }
# shellcheck source=../scripts/lib/ci-slice.sh
. "$HERE/../scripts/lib/ci-slice.sh"
slice="$(ci_slice_k)"
python3 -I - "$WORK/verdicts.tsv" "$WORK/swept.txt" "$LEG" "$floor" "$slice" <<'PY' || fail=1
import collections, sys
rows = [l.rstrip("\n").split("\t") for l in open(sys.argv[1], encoding="utf-8")]
mods, swept, proposed = (int(x) for x in open(sys.argv[2], encoding="utf-8").read().split())
leg, floor, slice_k = sys.argv[3], int(sys.argv[4]), int(sys.argv[5])
by = collections.Counter(r[4] for r in rows)
ok = by.get("ok", 0)
with_one = len({r[0] for r in rows if r[4] == "ok"})
print(f"shipped windows [{leg}]: {mods} module(s), {swept} load(s) and store(s) swept, {proposed} window(s) proposed: {dict(by)}")
if slice_k > 1:
    print(f"  proven in bounds from the bytes alone: {ok} access(es) in {with_one} module(s) (a 1-in-{slice_k} CI slice: the floor of {floor} counts the whole list and is NOT judged here; a full run judges it)")
    sys.exit(0)
print(f"  proven in bounds from the bytes alone: {ok} access(es) in {with_one} module(s) (floor {floor})")
if ok < floor:
    print(f"FAIL: {ok} window(s) proven, below the floor of {floor} for `{leg}`: evidence was lost. Restore it, or lower proofs/shipped-windows-floor.txt in its own commit and say why.")
    sys.exit(1)
if ok > floor:
    print(f"  note: {ok - floor} above the floor; raise the `{leg}` row of proofs/shipped-windows-floor.txt to {ok} to keep them.")
PY
[ "$fail" -eq 0 ] || exit 1
if [ "$slice" -gt 1 ]; then
  echo "ok   every proposed window is accepted by the kernel (a CI slice: the floor is judged on a full run, not here)"
else
  echo "ok   every proposed window is accepted by the kernel, and the count holds its floor"
fi
