#!/usr/bin/env bash
# Probe (not a gate): a control on proofs/check-shipped-routines.sh.
#
# That gate has the Rocq kernel decode the runtime routines of every shipped
# module to the proven trees. This asks whether the decode discriminates: of
# ONE shipped module's four routines, every byte is changed in every way that
# adds one, subtracts one or flips one bit, one byte per variant, and the
# gate's own goal for the module (all four routines, with the one global the
# two count routines must share and the free routine the release must call)
# is put to Rocq for each variant. A variant the goal still holds for is one
# the gate would accept.
#
#   scripts/probe-routine-control.sh <out-dir>
#
# Needs target/release/{almide,almide-anchor-check,examples/emit_structural_bundle}
# and coqc. Writes <out-dir>/dump (the module's routines as shipped),
# <out-dir>/variants.tsv (variant, routine, position, old byte, new byte) and
# <out-dir>/verdicts.tsv (variant, OK | NO); the unchanged module is variant
# v000000 and must read OK.
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$1"
OUT="$(cd "$1" && pwd)"
TD="$ROOT/target/release"
COQC="${COQC:-$(command -v coqc)}"
cd "$ROOT"

# The proof files the shipped-form decoders need, dependencies first.
python3 -I - "$ROOT/proofs" StructuralShipped > "$OUT/order.txt" <<'PY'
import os, re, sys
d, top = sys.argv[1], sys.argv[2]
seen, order = set(), []
def visit(m):
    if m in seen:
        return
    seen.add(m)
    p = os.path.join(d, m + ".v")
    if not os.path.exists(p):
        return
    for line in re.findall(r"From\s+AlmideTrust\s+Require\s+(?:Import\s+|Export\s+)?([^.]+)\.", open(p, encoding="utf-8").read()):
        for dep in line.split():
            visit(dep)
    order.append(m)
visit(top)
print("\n".join(order))
PY
t0=$(date +%s)
while IFS= read -r m; do
  [ -f "proofs/$m.vo" ] || (cd proofs && "$COQC" -Q . AlmideTrust "$m.v" > "$OUT/build-$m.log" 2>&1) || { echo "the proof file $m.v did not compile"; tail -n 20 "$OUT/build-$m.log"; exit 1; }
done < "$OUT/order.txt"
echo "proof files: $(wc -l < "$OUT/order.txt" | tr -d ' ') compiled in $(( $(date +%s) - t0 )) s"

# The first certified fixture whose module names all four routines.
: > "$OUT/dump"
grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt | head -n "${PROBE_TRY:-40}" | while IFS= read -r f; do
  "$TD/almide" build "$f" --target wasm -o "$OUT/m.wasm" > "$OUT/build.err" 2>&1 < /dev/null || continue
  "$TD/examples/emit_structural_bundle" "$f" --artifact "$OUT/m.wasm" --anchors "$OUT/m.anchors" > "$OUT/m.bundle" 2> "$OUT/prod.err" < /dev/null || continue
  "$TD/almide-anchor-check" --proved-bytes "$OUT/m.wasm" "$OUT/m.anchors" > "$OUT/one.txt" 2> /dev/null || continue
  if [ "$(grep -c -E '^SHIPPED (inc|dec|free|alloc) ' "$OUT/one.txt")" = 4 ]; then
    { printf 'MODULE %s\n' "$f"; cat "$OUT/one.txt"; } > "$OUT/dump"
    break
  fi
done || true
[ -s "$OUT/dump" ] || { echo "no fixture among the first ${PROBE_TRY:-40} names all four routines"; exit 1; }
head -n 1 "$OUT/dump"
awk '$1 == "SHIPPED" { print "  " $2 ": function " $3 ", " NF - 3 " byte(s)" } $1 == "EXIT" { print "  exit import: " $2 }' "$OUT/dump"

# One goal per variant, in chunks; the goal is the gate's own.
python3 -I - "$OUT/dump" "$OUT" "${PROBE_CHUNK:-150}" <<'PY'
import sys
dump, out, chunk = sys.argv[1], sys.argv[2], int(sys.argv[3])
r, ex = {}, None
for line in open(dump, encoding="utf-8"):
    t = line.split()
    if t and t[0] == "SHIPPED":
        r[t[1]] = (int(t[2]), [int(b) for b in t[3:]])
    elif t and t[0] == "EXIT":
        ex = None if t[1] == "-" else int(t[1])

def zl(bs):
    return "[" + ";".join(str(b) for b in bs) + "]"

# The statement proofs/check-shipped-routines.sh writes for a module.
def statement(r):
    e = -1 if ex is None else ex
    fr = r["free"][0]
    goals = [f"sdecode_bytes inc_naming {zl(r['inc'][1])} = Some (inc_body, mkRoles [g] [] [])",
             f"dec_verdict ({fr}) ({e}) {zl(r['dec'][1])} = Some (dec_body, [g], true)",
             f"sdecode_bytes free_naming {zl(r['free'][1])} = Some (free_body 48, no_roles)",
             f"alloc_verdict ({e}) {zl(r['alloc'][1])} = Some (alloc_body 48, [h;h;h;h], true)"]
    return "exists g h,\n  " + " /\\\n  ".join(goals)

HEAD = """From AlmideTrust Require Import StructuralRuntime StructuralAlloc StructuralDecode StructuralShipped.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""

def goal(name, r):
    return (f"Goal True.\ntryif (assert ({statement(r)}) by (do 2 eexists; repeat split; reflexivity))\n"
            f"then idtac \"VERDICT {name} OK\" else idtac \"VERDICT {name} NO\".\nexact I.\nQed.\n")

variants = [("v000000", "-", -1, -1, -1, r)]
for name in ("inc", "dec", "free", "alloc"):
    idx, bs = r[name]
    for p, b in enumerate(bs):
        news = sorted({(b + 1) % 256, (b - 1) % 256} | {b ^ (1 << k) for k in range(8)})
        for nb in news:
            rr = dict(r)
            rr[name] = (idx, bs[:p] + [nb] + bs[p + 1:])
            variants.append((f"v{len(variants):06d}", name, p, b, nb, rr))
with open(f"{out}/variants.tsv", "w", encoding="utf-8") as f:
    f.write("variant\troutine\tposition\told\tnew\n")
    for v in variants:
        f.write("\t".join(str(x) for x in v[:5]) + "\n")
for k in range(0, len(variants), chunk):
    with open(f"{out}/Ctl_{k // chunk:04d}.v", "w", encoding="utf-8") as f:
        f.write(HEAD)
        for v in variants[k:k + chunk]:
            f.write("\n" + goal(v[0], v[5]))
print(f"{len(variants) - 1} variant(s) of {sum(len(r[n][1]) for n in r)} byte(s), in {(len(variants) + chunk - 1) // chunk} file(s)")
PY

t0=$(date +%s)
export COQC ROOT OUT
find "$OUT" -maxdepth 1 -name 'Ctl_*.v' | sort \
  | xargs -P "${PROBE_JOBS:-4}" -I{} bash -c 'c="$(basename "$1" .v)"; "$COQC" -Q "$ROOT/proofs" AlmideTrust "$1" > "$OUT/$c.log" 2>&1 || echo "$c" >> "$OUT/failed-files"' _ {}
echo "kernel time (wall): $(( $(date +%s) - t0 )) s"
if [ -s "$OUT/failed-files" ]; then
  echo "file(s) that did not compile: $(tr '\n' ' ' < "$OUT/failed-files")"
  tail -n 5 "$OUT/$(head -n 1 "$OUT/failed-files").log"
fi
cat "$OUT"/Ctl_*.log | sed -n 's/^VERDICT \(v[0-9]*\) \(OK\|NO\)$/\1\t\2/p' | sort > "$OUT/verdicts.tsv"

python3 -I - "$OUT/variants.tsv" "$OUT/verdicts.tsv" <<'PY'
import collections, sys
vs = [l.rstrip("\n").split("\t") for l in open(sys.argv[1], encoding="utf-8")][1:]
vd = dict(l.rstrip("\n").split("\t") for l in open(sys.argv[2], encoding="utf-8"))
print(f"control (the unchanged module): {vd.get('v000000', 'NO VERDICT')}")
by = collections.Counter()
for v in vs[1:]:
    by[(v[1], vd.get(v[0], "none"))] += 1
for name in ("inc", "dec", "free", "alloc"):
    print(f"  {name}: refused {by[(name, 'NO')]}, accepted {by[(name, 'OK')]}, no verdict {by[(name, 'none')]}")
acc = [v for v in vs[1:] if vd.get(v[0]) == "OK"]
print(f"variants: {len(vs) - 1}; refused {sum(1 for v in vs[1:] if vd.get(v[0]) == 'NO')}, accepted {len(acc)}, no verdict {sum(1 for v in vs[1:] if v[0] not in vd)}")
for v in acc[:60]:
    print(f"  accepted: {v[1]} byte {v[2]}: {v[3]} -> {v[4]}")
sys.exit(0 if vd.get("v000000") == "OK" else 1)
PY
