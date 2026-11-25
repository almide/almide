#!/usr/bin/env bash
# The runtime routines of SHIPPED modules decode to the proven trees — checked
# by the Rocq kernel, module by module.
#
# check-structural-bytes.sh holds the bytes the EMITTER writes for `$inc` /
# `$dec_flat` / `$free` / `$alloc` to the lists StructuralDecode.v proves
# about. A module ships after two more passes, which renumber indices and put
# an exit call in front of every trap, so the shipped bodies are not those
# lists. This script takes the bodies as they ship and has the kernel check,
# for every module, that StructuralShipped.v's decoders read them as the SAME
# trees, with:
#   - `$inc` and `$dec_flat` reading one and the same global as `$line_end`;
#   - `$dec_flat` freeing through the function whose own bytes decode to
#     `free_body` (the module's free routine);
#   - every exit call in front of a trap naming the module's `proc_exit`
#     import;
#   - `$alloc` reading one global as the bump frontier, and one callee and
#     one message address at each of its three dies.
#
#   proofs/check-shipped-routines.sh <dump> [<work-dir>]
#
# <dump> is the concatenation, per module, of a line `MODULE <label>` and the
# output of `almide-anchor-check --proved-bytes <artifact.wasm> <anchors>`
# (`SHIPPED <routine> <function index> <bytes...>` per named routine, then
# `EXIT <import index | ->`). The bytes are the module's own; the indices are
# hints the kernel re-derives. Writes <work-dir>/verdicts.tsv (label,
# routines, ok | FAILED) and exits 1 if any module fails.
#
# Needs coqc and the compiled spine (proofs/check.sh, or at least
# StructuralShipped.vo and what it requires).
set -euo pipefail
export LC_ALL=C
HERE="$(cd "$(dirname "$0")" && pwd)"
DUMP="${1:?usage: proofs/check-shipped-routines.sh <dump> [<work-dir>]}"
WORK="${2:-$(mktemp -d)}"
mkdir -p "$WORK"
COQC="${COQC:-$(command -v coqc)}"
[ -f "$HERE/StructuralShipped.vo" ] || { echo "FAIL: proofs/StructuralShipped.vo is not built (run proofs/check.sh first)"; exit 1; }

# One .v per chunk of modules: `<work>/Shipped_<k>.v`, and `<work>/index.tsv`
# (chunk, example name, label, routines).
python3 -I - "$DUMP" "$WORK" "${SHIPPED_CHUNK:-100}" <<'PY'
import sys
dump, work, chunk = sys.argv[1], sys.argv[2], int(sys.argv[3])
mods, cur = [], None
for line in open(dump, encoding="utf-8"):
    t = line.split()
    if not t:
        continue
    if t[0] == "MODULE":
        cur = {"label": " ".join(t[1:]), "r": {}, "exit": None}
        mods.append(cur)
    elif t[0] == "SHIPPED" and cur is not None:
        cur["r"][t[1]] = (int(t[2]), [int(b) for b in t[3:]])
    elif t[0] == "EXIT" and cur is not None:
        cur["exit"] = None if t[1] == "-" else int(t[1])
    elif cur is not None:
        cur.setdefault("other", []).append(line.rstrip("\n"))

def zl(bs):
    return "[" + ";".join(str(b) for b in bs) + "]"

HEAD = """From AlmideTrust Require Import StructuralRuntime StructuralAlloc StructuralDecode StructuralShipped.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""

def example(name, m):
    r = m["r"]
    ex = -1 if m["exit"] is None else m["exit"]
    fr = r["free"][0] if "free" in r else -1
    goals, ev = [], []
    if "inc" in r:
        goals.append(f"sdecode_bytes inc_naming {zl(r['inc'][1])} = Some (inc_body, mkRoles [g] [] [])")
    if "dec" in r:
        goals.append(f"dec_verdict ({fr}) ({ex}) {zl(r['dec'][1])} = Some (dec_body, [g], true)")
    if "inc" in r or "dec" in r:
        ev.append("g")
    if "free" in r:
        goals.append(f"sdecode_bytes free_naming {zl(r['free'][1])} = Some (free_body 48, no_roles)")
    if "alloc" in r:
        goals.append(f"alloc_verdict ({ex}) {zl(r['alloc'][1])} = Some (alloc_body 48, [h;h;h;h], true)")
        ev.append("h")
    if not goals:
        return None
    head = ("exists " + " ".join(ev) + ",\n  ") if ev else ""
    intro = ("do %d eexists. " % len(ev)) if ev else ""
    return f"(* {m['label']} *)\nExample {name} : {head}" + " /\\\n  ".join(goals) + f".\nProof. {intro}repeat split; reflexivity. Qed.\n"

with open(f"{work}/index.tsv", "w", encoding="utf-8") as idx:
    k, n, out = 0, 0, None
    for i, m in enumerate(mods):
        name = f"m{i:05d}"
        text = example(name, m)
        if text is None:
            idx.write(f"-\t{name}\t{m['label']}\t-\n")
            continue
        if out is None or n == chunk:
            if out:
                out.close()
            k, n = k + 1, 0
            out = open(f"{work}/Shipped_{k:03d}.v", "w", encoding="utf-8")
            out.write(HEAD)
        out.write("\n" + text)
        n += 1
        idx.write(f"Shipped_{k:03d}\t{name}\t{m['label']}\t{','.join(sorted(m['r']))}\n")
    if out:
        out.close()
    # each module again as its own file, for naming the one a failed chunk holds
    for i, m in enumerate(mods):
        text = example(f"m{i:05d}", m)
        if text is not None:
            with open(f"{work}/one_m{i:05d}.v", "w", encoding="utf-8") as f:
                f.write(HEAD + "\n" + text)
print(f"{len(mods)} module(s) in the dump")
PY

: > "$WORK/verdicts.tsv"
fail=0
# The chunks are independent: compile them side by side, then read each one's
# exit code in order.
export COQC HERE WORK
find "$WORK" -maxdepth 1 -name 'Shipped_*.v' | LC_ALL=C sort \
  | xargs -P "${SHIPPED_JOBS:-4}" -I{} bash -c 'c="$(basename "$1" .v)"; if "$COQC" -Q "$HERE" AlmideTrust "$1" > "$WORK/$c.log" 2>&1; then echo 0 > "$WORK/$c.rc"; else echo 1 > "$WORK/$c.rc"; fi' _ {}
for v in "$WORK"/Shipped_*.v; do
  [ -e "$v" ] || continue
  c="$(basename "$v" .v)"
  if [ "$(cat "$WORK/$c.rc" 2>/dev/null)" = 0 ]; then
    awk -F'\t' -v c="$c" '$1 == c { print $3 "\t" $4 "\tok" }' "$WORK/index.tsv" >> "$WORK/verdicts.tsv"
  else
    # the chunk holds a module the kernel refuses: ask module by module
    while IFS=$'\t' read -r chunk name label routines; do
      [ "$chunk" = "$c" ] || continue
      if "$COQC" -Q "$HERE" AlmideTrust "$WORK/one_$name.v" > "$WORK/one_$name.log" 2>&1; then
        printf '%s\t%s\tok\n' "$label" "$routines" >> "$WORK/verdicts.tsv"
      else
        printf '%s\t%s\tFAILED\n' "$label" "$routines" >> "$WORK/verdicts.tsv"
        echo "FAIL: $label ($routines): $(grep -m1 -E 'Error|Unable|not convertible' "$WORK/one_$name.log" | cut -c1-200)"
        fail=1
      fi
    done < "$WORK/index.tsv"
  fi
done
awk -F'\t' '$1 == "-" { print $3 "\t-\tnone named" }' "$WORK/index.tsv" >> "$WORK/verdicts.tsv"

python3 -I - "$WORK/verdicts.tsv" <<'PY'
import collections, sys
rows = [l.rstrip("\n").split("\t") for l in open(sys.argv[1], encoding="utf-8")]
by = collections.Counter(r[2] for r in rows)
routines = collections.Counter()
for r in rows:
    if r[2] == "ok":
        for x in r[1].split(","):
            routines[x] += 1
print(f"shipped routines: {len(rows)} module(s): {dict(by)}")
print(f"  decoded to the proven trees: {dict(sorted(routines.items()))}, {sum(routines.values())} routine(s)")
PY
[ "$fail" -eq 0 ] || exit 1
echo "ok   every named runtime routine of every module decodes, as shipped, to its proven tree"
