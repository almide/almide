#!/usr/bin/env bash
# The reads a loop's exit test guards, in SHIPPED modules, proven inside the
# block whose count the loop captured, from the module's own bytes — checked
# by the Rocq kernel, read by read.
#
# A `for` over a list or a string reads the count ONCE, before the loop, into
# a local, and every turn tests the counter against that local. No single
# straight-line run holds the length read and the access, so
# check-shipped-windows.sh reaches none of these; the bounds witness records
# them, and almide-anchor-check (unverified) holds the events to the shipped
# instructions by shape. GuardLoop.v judges them from four byte strings: the
# capture (`local.get b ; i32.load 4 ; [i32.const s ; i32.div_u ;] local.set
# n`), everything up to the exit test, the test and the access, and
# everything from there to the `end` of the outermost block opened since the
# capture. `loop_core` runs GuardWindow's check on the capture's instructions
# followed by those of the test and the access, and scans every other
# instruction: none assigns `b` or `n`, and the nesting closes what it opened.
# `loop_sound` then puts the access inside the payload in every run of the
# test and the access from locals in which `n` holds the captured count, with
# 32-bit wrap-around. `loop_class` adds what stands behind the length word
# still being the one the capture read: nothing in between writes memory or
# calls (pure), the loop holds its own share of the block (held), or the
# block's local is read nowhere but for the access (kept). No witness is
# read; the anchor file gives one number, the increment routine's index.
#
#   proofs/check-shipped-loops.sh <dump> <work-dir> <leg>
#
# <dump> is the concatenation, per module, of a line `MODULE <label>` and the
# output of `almide-anchor-check --loops <artifact.wasm> <anchors>` (`LOOP
# <function> <access instruction> <increment> | <hold> | <capture> | <up to
# the test> | <test and access> | <the rest>` per proposal, then `INCREMENT
# <index | ->`). The cutter only proposes: every access addressed by the
# captured block's local and the tested counter, with no bound, stride,
# offset or width tested. The kernel is asked once per distinct proposal for
# its class, and every class read is then a goal `loop_class .. = <class>`
# the kernel proves. <leg> names the row of proofs/shipped-loops-floor.txt.
#
# Fails when a module's dump has no `INCREMENT` line (the sweep went blind),
# when fewer reads are accepted (class above 4) than the leg's floor, when
# more reads than the leg's first ceiling have a core that holds and nothing
# behind the length word (class 4: a count captured before a loop that
# nothing protects), or when more reads than the leg's second ceiling have a
# core the kernel REFUSES (class 0: an access through the looped block by the
# tested counter that the kernel cannot place inside it). The corpus holds
# refused reads that are in bounds for a reason the rule does not know: a
# loop that counts BYTES and reads 4- or 8-byte elements stays inside only
# because the length and the counter are multiples of the element size. They
# are counted, not hidden: the cutter tests no bound, so a read one slot too
# far is refused the same way and takes the count over its ceiling. Lower a
# floor or raise a ceiling in its own commit and say why.
# Writes <work-dir>/verdicts.tsv (label, function, access, class).
#
# The floor counts the leg's WHOLE list, so it is judged on a full run only.
# Under a CI slice (ALMIDE_CI_SLICE=k > 1, a PR into develop: one fixture in
# k) the kernel still classes every proposal the slice holds, and the two
# ceilings are still judged: a slice is part of the list, so a slice over a
# ceiling puts the whole list over it. The accepted count is printed with no
# verdict against the floor, and the last line says so.
#
# Needs coqc and the compiled spine (proofs/check.sh, or at least
# GuardWindow.vo, GuardFresh.vo and GuardLoop.vo). That the function at the
# increment's index is the proven routine is proofs/check-shipped-routines.sh's
# verdict on the same artifact, not this script's.
set -euo pipefail
export LC_ALL=C
HERE="$(cd "$(dirname "$0")" && pwd)"
DUMP="${1:?usage: proofs/check-shipped-loops.sh <dump> <work-dir> <leg>}"
WORK="${2:?usage: proofs/check-shipped-loops.sh <dump> <work-dir> <leg>}"
LEG="${3:?usage: proofs/check-shipped-loops.sh <dump> <work-dir> <leg>}"
FLOORS="${SHIPPED_LOOPS_FLOOR:-$HERE/shipped-loops-floor.txt}"
mkdir -p "$WORK"
WORK="$(cd "$WORK" && pwd)"
COQC="${COQC:-$(command -v coqc)}"
[ -f "$HERE/GuardLoop.vo" ] || { echo "FAIL: proofs/GuardLoop.vo is not built (run proofs/check.sh first)"; exit 1; }

# The distinct proposals, one `Eval` each: `<work>/Class_<k>.v`; and
# `<work>/index.tsv` (number of the distinct proposal, label, function,
# access), one line per proposal.
python3 -I - "$DUMP" "$WORK" "${LOOPS_CHUNK:-400}" <<'PY'
import sys
dump, work, chunk = sys.argv[1], sys.argv[2], int(sys.argv[3])
mods, props, cur = [], [], None
for line in open(dump, encoding="utf-8"):
    t = line.split()
    if not t:
        continue
    if t[0] == "MODULE":
        cur = {"label": " ".join(t[1:]), "inc": None, "other": []}
        mods.append(cur)
    elif cur is None:
        sys.exit(f"FAIL: the dump starts with `{line.strip()[:60]}`, not a MODULE line")
    elif t[0] == "LOOP" and line.count("|") == 5:
        parts = [p.split() for p in line.split("|")]
        head = parts[0]
        if len(head) != 4 or not all(x.isdigit() for p in parts for x in (p[1:] if p is head else p)):
            sys.exit(f"FAIL: {cur['label']}: a LOOP line is not numbers: {line.strip()[:120]}")
        props.append((cur["label"], head[1], head[2], (head[3],) + tuple(";".join(p) for p in parts[1:])))
    elif t[0] == "INCREMENT" and len(t) == 2:
        cur["inc"] = t[1]
    else:
        cur["other"].append(line.rstrip("\n"))
blind = [m for m in mods if m["inc"] is None]
if blind:
    for m in blind[:5]:
        print(f"FAIL: {m['label']}: no INCREMENT line: {' | '.join(m['other'])[:200]}")
    sys.exit(1)
if not mods:
    sys.exit("FAIL: the dump names no module")
HEAD = """From AlmideTrust Require Import GuardWindow GuardFresh GuardLoop.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""
ids = {}
with open(f"{work}/goals.tsv", "w", encoding="utf-8") as goals:
    for _, _, _, g in props:
        if g in ids:
            continue
        i = len(ids)
        ids[g] = i
        with open(f"{work}/Class_{i // chunk:04d}.v", "a" if i % chunk else "w", encoding="utf-8") as f:
            if i % chunk == 0:
                f.write(HEAD)
            f.write(f"Eval vm_compute in (loop_class {g[0]} [{g[1]}] [{g[2]}] [{g[3]}] [{g[4]}] [{g[5]}]).\n")
        goals.write("\t".join((str(i),) + g) + "\n")
with open(f"{work}/index.tsv", "w", encoding="utf-8") as idx:
    for label, func, acc, g in props:
        idx.write(f"{ids[g]}\t{label}\t{func}\t{acc}\n")
with open(f"{work}/swept.txt", "w", encoding="utf-8") as f:
    f.write(f"{len(mods)} {sum(1 for m in mods if m['inc'] != '-')} {len(props)} {len(ids)}\n")
print(f"{len(mods)} module(s), {len(props)} read(s) proposed, {len(ids)} distinct")
PY

export COQC HERE WORK
find "$WORK" -maxdepth 1 -name 'Class_*.v' | LC_ALL=C sort \
  | xargs -P "${LOOPS_JOBS:-4}" -I{} bash -c 'c="$(basename "$1" .v)"; "$COQC" -Q "$HERE" AlmideTrust "$1" > "$WORK/$c.out" 2> "$WORK/$c.err" || { echo "FAIL: $c did not compile: $(head -3 "$WORK/$c.err")"; exit 255; }' _ {}

# Every class read is now a goal the kernel proves: `<work>/Proved_<k>.v`.
python3 -I - "$WORK" "${LOOPS_CHUNK:-400}" <<'PY'
import glob, sys
work, chunk = sys.argv[1], int(sys.argv[2])
goals = [l.rstrip("\n").split("\t") for l in open(f"{work}/goals.tsv", encoding="utf-8")]
vals = []
for p in sorted(glob.glob(f"{work}/Class_*.out")):
    vals += [l.split("=", 1)[1].strip() for l in open(p, encoding="utf-8") if l.strip().startswith("= ")]
if len(vals) != len(goals) or not all(v.isdigit() for v in vals):
    sys.exit(f"FAIL: {len(vals)} class(es) read for {len(goals)} distinct proposal(s)")
HEAD = """From AlmideTrust Require Import GuardWindow GuardFresh GuardLoop.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""
with open(f"{work}/classes.tsv", "w", encoding="utf-8") as out:
    for (i, inc, hold, pre, mid, ht, rest), v in zip(goals, vals):
        i = int(i)
        with open(f"{work}/Proved_{i // chunk:04d}.v", "a" if i % chunk else "w", encoding="utf-8") as f:
            if i % chunk == 0:
                f.write(HEAD)
            f.write(f"\nExample l{i:06d} : loop_class {inc} [{hold}] [{pre}] [{mid}] [{ht}] [{rest}] = {v}.\nProof. vm_compute. reflexivity. Qed.\n")
        out.write(f"{i}\t{v}\n")
PY
find "$WORK" -maxdepth 1 -name 'Proved_*.v' | LC_ALL=C sort \
  | xargs -P "${LOOPS_JOBS:-4}" -I{} bash -c 'c="$(basename "$1" .v)"; "$COQC" -Q "$HERE" AlmideTrust "$1" > "$WORK/$c.log" 2>&1 || { echo "FAIL: the kernel does not prove the classes of $c: $(head -3 "$WORK/$c.log")"; exit 255; }' _ {}

row="$(awk -v leg="$LEG" '$1 == leg && NF == 4 { print $2, $3, $4 }' "$FLOORS")"
[ -n "$row" ] || { echo "FAIL: $FLOORS has no row of four fields for the leg \`$LEG\`"; exit 1; }
fail=0
# shellcheck source=../scripts/lib/ci-slice.sh
. "$HERE/../scripts/lib/ci-slice.sh"
slice="$(ci_slice_k)"
# shellcheck disable=SC2086
python3 -I - "$WORK" "$LEG" $row "$slice" <<'PY' || fail=1
import sys
work, leg, floor, ceiling, refusals, slice_k = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5]), int(sys.argv[6])
sliced = slice_k > 1
cls = dict(l.split() for l in open(f"{work}/classes.tsv", encoding="utf-8"))
rows = [l.rstrip("\n").split("\t") for l in open(f"{work}/index.tsv", encoding="utf-8")]
mods, named, proposed, distinct = (int(x) for x in open(f"{work}/swept.txt", encoding="utf-8").read().split())
with open(f"{work}/verdicts.tsv", "w", encoding="utf-8") as out:
    for i, label, func, acc in rows:
        out.write(f"{label}\t{func}\t{acc}\t{cls[i]}\n")
c = [int(cls[r[0]]) for r in rows]
ok = sum(1 for v in c if v > 4)
bare = [r for r, v in zip(rows, c) if v == 4]
bad = [r for r, v in zip(rows, c) if v == 0]
with_one = len({r[1] for r, v in zip(rows, c) if v > 4})
print(f"shipped loop reads [{leg}]: {mods} module(s), {named} naming an increment routine, {proposed} read(s) proposed as {distinct} distinct: {ok} accepted, {len(bare)} with nothing behind the length word, {len(bad)} refused")
floor_says = f"a 1-in-{slice_k} CI slice: the floor of {floor} counts the whole list and is NOT judged here; a full run judges it" if sliced else f"floor {floor}"
print(f"  accepted: pure {sum(1 for v in c if v > 4 and v & 1)}, held {sum(1 for v in c if v > 4 and v & 2)}, kept {sum(1 for v in c if v > 4 and v & 8)} (a read may be more than one), in {with_one} module(s) ({floor_says}); nothing behind: {len(bare)} (ceiling {ceiling}); refused: {len(bad)} (ceiling {refusals})")
failed = False
if len(bad) > refusals:
    for i, label, func, acc in bad[:20]:
        print(f"  refused: {label}: the read at function {func}, instruction {acc} addresses the looped block by the tested counter and the kernel cannot place it inside (goal l{int(i):06d} of {work})")
    print(f"FAIL: {len(bad)} read(s) are refused by the kernel, above the ceiling of {refusals} for `{leg}`: a loop reads through its block by its counter where the rule cannot place the read. Fix the read, or raise proofs/shipped-loops-floor.txt in its own commit and say why it is in bounds.")
    failed = True
if not sliced and ok < floor:
    print(f"FAIL: {ok} read(s) accepted, below the floor of {floor} for `{leg}`: evidence was lost. Restore it, or lower proofs/shipped-loops-floor.txt in its own commit and say why.")
    failed = True
if len(bare) > ceiling:
    for i, label, func, acc in bare[:10]:
        print(f"  nothing behind the length word: {label}, function {func}, instruction {acc}")
    print(f"FAIL: {len(bare)} read(s) rest on a count captured before a loop that nothing protects, above the ceiling of {ceiling} for `{leg}`. Protect the loop, or raise proofs/shipped-loops-floor.txt in its own commit and say why.")
    failed = True
# The three notes below ask for a row to be moved to this run's count, which
# only the whole list's count can justify: a slice prints none of them.
if not sliced and ok > floor:
    print(f"  note: {ok - floor} above the floor; raise the `{leg}` row of proofs/shipped-loops-floor.txt to {ok} to keep them.")
if not sliced and len(bare) < ceiling:
    print(f"  note: {ceiling - len(bare)} below the ceiling; lower the `{leg}` row's first ceiling to {len(bare)} to keep it.")
if not sliced and len(bad) < refusals:
    print(f"  note: {refusals - len(bad)} below the refusal ceiling; lower the `{leg}` row's second ceiling to {len(bad)} to keep it.")
sys.exit(1 if failed else 0)
PY
[ "$fail" -eq 0 ] || exit 1
if [ "$slice" -gt 1 ]; then
  echo "ok   every proposed loop read is classed by the kernel, and the counts hold their ceilings (a CI slice: the floor is judged on a full run, not here)"
else
  echo "ok   every proposed loop read is classed by the kernel, and the counts hold their floor and ceilings"
fi
