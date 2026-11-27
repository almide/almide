#!/usr/bin/env bash
# Accesses into a block just allocated, in SHIPPED modules, proven inside that
# block from the module's own bytes — checked by the Rocq kernel, access by
# access.
#
# The init and fields witnesses are judged over a trace model, and
# almide-anchor-check (unverified) holds their events to the shipped
# instructions by shape. For the commonest of those accesses less trust is
# needed: where a function allocates a constant size into a local
# (`i32.const n ; call $alloc ; local.set x`), leaves that local alone, and
# then reads or writes at `x` plus a constant, GuardFresh.v reads the three
# byte strings itself — the allocation, the instructions in between (scanned
# with every instruction's length: none assigns `x`, the nesting ends where
# it began), and the access's own straight-line run — and `fresh_sound` puts
# the access inside `[x, x + 12 + n)` in every run, with 32-bit wrap-around.
# No witness is read; the anchor file gives one number, the allocator's index.
#
#   proofs/check-shipped-fresh.sh <dump> <work-dir> <leg>
#
# <dump> is the concatenation, per module, of a line `MODULE <label>` and the
# output of `almide-anchor-check --fresh <artifact.wasm> <anchors>` (`FRESH
# <function> <access instruction> <allocator> | <bytes> | <bytes> | <bytes>`
# per proposal, then `ALLOCATOR <index>`). The cutter only proposes, and tests
# no bound; each proposal is a goal `fresh_ok <allocator> [..] [..] [..] =
# true` the kernel computes, once per distinct triple. <leg> names the row of
# proofs/shipped-fresh-floor.txt the count is held to.
#
# Fails when a proposal is REFUSED by the kernel (an access through a pointer
# just allocated lies outside its block, or the cutter and the proof drifted
# apart), when a module's dump has no `ALLOCATOR` line (the sweep went
# blind), or when fewer accesses are proven than the leg's floor (evidence
# was lost: restore it, or lower the floor in its own commit and say why).
# Writes <work-dir>/verdicts.tsv (label, function, access, ok | REFUSED).
#
# The floor counts the leg's WHOLE list, so it is judged on a full run only.
# Under a CI slice (ALMIDE_CI_SLICE=k > 1, a PR into develop: one fixture in
# k) every proposal the slice holds is still judged by the kernel and a
# refusal still fails, but the count is printed with no verdict against the
# floor, and the last line says so.
#
# Needs coqc and the compiled spine (proofs/check.sh, or at least
# GuardWindow.vo and GuardFresh.vo). That the function at the allocator's
# index is the proven allocator is proofs/check-shipped-routines.sh's verdict
# on the same artifact, not this script's.
set -euo pipefail
export LC_ALL=C
HERE="$(cd "$(dirname "$0")" && pwd)"
DUMP="${1:?usage: proofs/check-shipped-fresh.sh <dump> <work-dir> <leg>}"
WORK="${2:?usage: proofs/check-shipped-fresh.sh <dump> <work-dir> <leg>}"
LEG="${3:?usage: proofs/check-shipped-fresh.sh <dump> <work-dir> <leg>}"
FLOORS="${SHIPPED_FRESH_FLOOR:-$HERE/shipped-fresh-floor.txt}"
mkdir -p "$WORK"
WORK="$(cd "$WORK" && pwd)"
COQC="${COQC:-$(command -v coqc)}"
[ -f "$HERE/GuardFresh.vo" ] || { echo "FAIL: proofs/GuardFresh.vo is not built (run proofs/check.sh first)"; exit 1; }

# One .v per chunk of DISTINCT triples: `<work>/Fresh_<k>.v`; `<work>/goals.tsv`
# (chunk, example name, the goal's four arguments) and `<work>/index.tsv`
# (example name, label, function, access), one line per proposal.
#
# A chunk ends at FRESH_CHUNK goals or at FRESH_CHUNK_TEXT characters of goal
# text, whichever comes first. A function that fills one block with a
# thousand stores proposes a thousand triples whose middle string grows with
# each store; 400 of those in one file is megabytes of byte lists, and the
# kernel's memory for a file grows with its text (about 1.4 MB per 1,000
# characters, measured), so a count alone lets one file take the machine.
python3 -I - "$DUMP" "$WORK" "${FRESH_CHUNK:-400}" "${FRESH_CHUNK_TEXT:-300000}" <<'PY'
import sys
dump, work, chunk, budget = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
mods, props, cur = [], [], None
for line in open(dump, encoding="utf-8"):
    t = line.split()
    if not t:
        continue
    if t[0] == "MODULE":
        cur = {"label": " ".join(t[1:]), "alloc": None, "other": []}
        mods.append(cur)
    elif cur is None:
        sys.exit(f"FAIL: the dump starts with `{line.strip()[:60]}`, not a MODULE line")
    elif t[0] == "FRESH" and line.count("|") == 3:
        head, pre, mid, tail = [p.split() for p in line.split("|")]
        if len(head) != 4 or not all(x.isdigit() for x in head[1:] + pre + mid + tail):
            sys.exit(f"FAIL: {cur['label']}: a FRESH line is not numbers: {line.strip()[:120]}")
        props.append((cur["label"], head[1], head[2], (head[3], ";".join(pre), ";".join(mid), ";".join(tail))))
    elif t[0] == "ALLOCATOR" and len(t) == 2:
        cur["alloc"] = t[1]
    else:
        cur["other"].append(line.rstrip("\n"))
blind = [m for m in mods if m["alloc"] is None]
if blind:
    for m in blind[:5]:
        print(f"FAIL: {m['label']}: no ALLOCATOR line: {' | '.join(m['other'])[:200]}")
    sys.exit(1)
if not mods:
    sys.exit("FAIL: the dump names no module")

HEAD = """From AlmideTrust Require Import GuardWindow GuardFresh.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""
names = {}
k, held, text, largest = 0, 0, 0, 0
with open(f"{work}/goals.tsv", "w", encoding="utf-8") as goals:
    for _, _, _, g in props:
        if g in names:
            continue
        name = f"f{len(names):06d}"
        names[g] = name
        goal = f"\nExample {name} : fresh_ok {g[0]} [{g[1]}] [{g[2]}] [{g[3]}] = true.\nProof. vm_compute. reflexivity. Qed.\n"
        if held and (held >= chunk or text + len(goal) > budget):
            k, held, text = k + 1, 0, 0
        with open(f"{work}/Fresh_{k:04d}.v", "a" if held else "w", encoding="utf-8") as f:
            if not held:
                f.write(HEAD)
            f.write(goal)
        held, text = held + 1, text + len(goal)
        largest = max(largest, text)
        goals.write(f"Fresh_{k:04d}\t{name}\n")
with open(f"{work}/index.tsv", "w", encoding="utf-8") as idx:
    for label, func, acc, g in props:
        idx.write(f"{names[g]}\t{label}\t{func}\t{acc}\n")
with open(f"{work}/swept.txt", "w", encoding="utf-8") as f:
    f.write(f"{len(mods)} {sum(1 for m in mods if m['alloc'] != '4294967295')} {len(props)} {len(names)}\n")
print(f"{len(mods)} module(s), {len(props)} access(es) proposed, {len(names)} distinct triple(s) in {k + 1 if names else 0} file(s), the largest {largest} characters of goals")
PY

fail=0
# The chunks are independent: compile them side by side, then read each one's
# exit code in order.
export COQC HERE WORK
find "$WORK" -maxdepth 1 -name 'Fresh_*.v' | LC_ALL=C sort \
  | xargs -P "${FRESH_JOBS:-4}" -I{} bash -c 'c="$(basename "$1" .v)"; if "$COQC" -Q "$HERE" AlmideTrust "$1" > "$WORK/$c.log" 2>&1; then echo 0 > "$WORK/$c.rc"; else echo 1 > "$WORK/$c.rc"; fi' _ {}
: > "$WORK/refused.txt"
for v in "$WORK"/Fresh_*.v; do
  [ -e "$v" ] || continue
  c="$(basename "$v" .v)"
  [ "$(cat "$WORK/$c.rc" 2>/dev/null)" = 0 ] && continue
  # the chunk holds a triple the kernel refuses: ask triple by triple
  while IFS=$'\t' read -r chunk name; do
    [ "$chunk" = "$c" ] || continue
    {
      printf 'From AlmideTrust Require Import GuardWindow GuardFresh.\nFrom Stdlib Require Import ZArith.\nFrom Stdlib Require Import List.\nImport ListNotations.\nOpen Scope Z_scope.\n\n'
      awk -v n="$name" '$1 == "Example" && $2 == n { on = 1 } on { print } on && /Qed\./ { exit }' "$WORK/$c.v"
    } > "$WORK/one_$name.v"
    if ! "$COQC" -Q "$HERE" AlmideTrust "$WORK/one_$name.v" > "$WORK/one_$name.log" 2>&1; then
      echo "$name" >> "$WORK/refused.txt"
    fi
  done < "$WORK/goals.tsv"
done

floor="$(awk -v leg="$LEG" '$1 == leg { print $2 }' "$FLOORS")"
[ -n "$floor" ] || { echo "FAIL: $FLOORS has no row for the leg \`$LEG\`"; exit 1; }
# shellcheck source=../scripts/lib/ci-slice.sh
. "$HERE/../scripts/lib/ci-slice.sh"
slice="$(ci_slice_k)"
python3 -I - "$WORK" "$LEG" "$floor" "$slice" <<'PY' || fail=1
import sys
work, leg, floor, slice_k = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
refused = {l.strip() for l in open(f"{work}/refused.txt", encoding="utf-8") if l.strip()}
rows = [l.rstrip("\n").split("\t") for l in open(f"{work}/index.tsv", encoding="utf-8")]
mods, named, proposed, distinct = (int(x) for x in open(f"{work}/swept.txt", encoding="utf-8").read().split())
bad = [r for r in rows if r[0] in refused]
with open(f"{work}/verdicts.tsv", "w", encoding="utf-8") as out:
    for name, label, func, acc in rows:
        out.write(f"{label}\t{func}\t{acc}\t{'REFUSED' if name in refused else 'ok'}\n")
ok = len(rows) - len(bad)
with_one = len({r[1] for r in rows if r[0] not in refused})
print(f"shipped fresh-block accesses [{leg}]: {mods} module(s), {named} naming an allocator, {proposed} access(es) proposed as {distinct} distinct triple(s): {ok} ok, {len(bad)} REFUSED")
sliced = slice_k > 1
if sliced:
    print(f"  proven inside the block just allocated, from the bytes alone: {ok} access(es) in {with_one} module(s) (a 1-in-{slice_k} CI slice: the floor of {floor} counts the whole list and is NOT judged here; a full run judges it)")
else:
    print(f"  proven inside the block just allocated, from the bytes alone: {ok} access(es) in {with_one} module(s) (floor {floor})")
failed = False
for name, label, func, acc in bad[:20]:
    print(f"FAIL: {label}: the access at function {func}, instruction {acc} goes through a pointer just allocated and the kernel refuses it ({work}/one_{name}.v)")
    failed = True
if not sliced and ok < floor:
    print(f"FAIL: {ok} access(es) proven, below the floor of {floor} for `{leg}`: evidence was lost. Restore it, or lower proofs/shipped-fresh-floor.txt in its own commit and say why.")
    failed = True
if not sliced and ok > floor:
    print(f"  note: {ok - floor} above the floor; raise the `{leg}` row of proofs/shipped-fresh-floor.txt to {ok} to keep them.")
sys.exit(1 if failed else 0)
PY
[ "$fail" -eq 0 ] || exit 1
if [ "$slice" -gt 1 ]; then
  echo "ok   every proposed access is accepted by the kernel (a CI slice: the floor is judged on a full run, not here)"
else
  echo "ok   every proposed access is accepted by the kernel, and the count holds its floor"
fi
