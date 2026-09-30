#!/usr/bin/env bash
# STRUCTURAL CORPUS WALL (#2759, #1696 step 4): the corpus sweep of the leg
# that ships. corpus-wall.sh drives the MIR lowering, which no wasm artifact
# comes from since #2761; this twin drives the STRUCTURAL build — the
# product route `almide build --target wasm` takes (`almide::wasm_route`) —
# over every fixture golden/witness-fixtures.txt lists as fully certified
# (#2754: every frame of its shipped pass carries an ownership witness).
#
# Per fixture, the producer (crates/almide-wasm/examples/
# emit_structural_bundle.rs) writes one certificate bundle:
#   ownership        one witness per frame, recorded as the RC instructions
#                    were emitted (witness.rs)
#   call-modes       the program's call sites against their callees' frame
#                    conventions (witness_modes.rs)
#   names            per index space of the stock-WASI bytes the build ships,
#                    and per function's locals (cert_project.rs)
#   caps             per source-declared function: declared | transitive reach
#   caps-transitive  the whole call graph, the reach computed IN the proof
#
# Every witness is judged by THREE independent verdicts, and all three must
# agree: the checker extracted from the Coq proof (proofs/checker), the Rocq
# KERNEL itself (a generated file of vm_compute goals, coqc'd), and
# almide-verify (the portable re-checker a binary distribution runs). The
# report gives acceptance per property. A witness may REJECT only if
# proofs/structural-wall-exceptions.txt names it (a shrink-only ledger: a
# listed witness that now accepts is stale and fails the gate).
set -euo pipefail
cd "$(dirname "$0")"
ROOT="$(cd .. && pwd)"
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1

FIXTURES="$ROOT/crates/almide-wasm/tests/golden/witness-fixtures.txt"
EXCEPTIONS="$ROOT/proofs/structural-wall-exceptions.txt"
COQC="${COQC:-$(command -v coqc || true)}"
[ -n "$COQC" ] || { echo "STRUCTURAL WALL FAIL: coqc is not on PATH (the kernel verdict needs it)" >&2; exit 1; }

echo "== build the bundle producer, almide-verify and the kernel-proven checker =="
(cd "$ROOT" && cargo build -q -p almide-wasm --example emit_structural_bundle && cargo build -q -p almide-verify)
TD="${CARGO_TARGET_DIR:-$ROOT/target}/debug"
PRODUCER="$TD/examples/emit_structural_bundle"
VERIFY="$TD/almide-verify"
./build-checker.sh >/dev/null

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT

echo "== produce one bundle per certified fixture (product route, build form) =="
N=0
while IFS= read -r f; do
  case "$f" in ''|'#'*) continue ;; esac
  N=$((N + 1))
  if ! (cd "$ROOT" && "$PRODUCER" "$f") > "$OUT/$N.bundle" 2> "$OUT/$N.err"; then
    echo "STRUCTURAL WALL FAIL: the producer refused a certified fixture: $f" >&2
    sed 's/^/  /' "$OUT/$N.err" >&2
    exit 1
  fi
  printf '%s\t%s\n' "$N" "$f" >> "$OUT/index"
  # Verdict 3: almide-verify, one run per bundle (its per-witness lines are
  # read back in bundle order below).
  set +e
  "$VERIFY" bundle "$OUT/$N.bundle" > "$OUT/$N.verify" 2>&1
  set -e
done < "$FIXTURES"
[ "$N" -gt 0 ] || { echo "STRUCTURAL WALL FAIL: no certified fixture listed in $FIXTURES" >&2; exit 1; }
echo "  $N fixture bundle(s)"

echo "== the extracted checker judges every witness; the kernel file is generated =="
KGEN="$(mktemp /tmp/KernelStructural_XXXXXX).v"
set +e
python3 - "$OUT" "$ROOT/proofs/checker" "$EXCEPTIONS" "$KGEN" <<'PYEOF'
import os, subprocess, sys
out, checker, exceptions_path, kgen = sys.argv[1:5]
FN = {"ownership": "check_xc", "names": "check_names_cert", "caps": "check_caps_cert",
      "caps-transitive": "check_prog_cert", "call-modes": "check_modes_cert"}

def parse(path):
    b = open(path, "rb").read()
    i, recs, uncert = b.index(b"\n") + 1, [], []
    while i < len(b):
        j = b.index(b"\n", i)
        line = b[i:j].decode()
        i = j + 1
        if line.startswith("witness "):
            _, prop, n, fn = line.split(" ", 3)
            n = int(n)
            recs.append((prop, fn, b[i:i + n].decode()))
            i += n + 1
        elif line.startswith("uncertified "):
            uncert.append(line)
    return recs, uncert

def verify_lines(path):
    got = []
    for line in open(path).read().splitlines():
        parts = line.split(None, 2)
        if len(parts) == 3 and parts[0] in ("ACCEPT", "REJECT"):
            got.append((parts[1], parts[2], parts[0] == "ACCEPT"))
    return got

exceptions = set()
if os.path.exists(exceptions_path):
    for line in open(exceptions_path):
        line = line.strip()
        if line and not line.startswith("#"):
            fx, prop, fn = line.split("\t")[:3]
            exceptions.add((fx, prop, fn))

fail = []
stats = {p: [0, 0] for p in FN}
fixtures_ok = 0
rejected_seen = set()
accepts = {p: [] for p in FN}   # kernel: forallb f [...] = true
rejects = []                     # kernel: f w = false
one = os.path.join(out, "one.w")
for row in open(os.path.join(out, "index")):
    k, fx = row.rstrip("\n").split("\t")
    recs, uncert = parse(os.path.join(out, k + ".bundle"))
    if uncert:
        fail.append(f"{fx}: the bundle declares {len(uncert)} uncertified frame(s) — the fixture is listed as certified ({uncert[0]})")
    portable = verify_lines(os.path.join(out, k + ".verify"))
    if len(portable) != len(recs):
        fail.append(f"{fx}: almide-verify judged {len(portable)} of {len(recs)} witnesses")
        continue
    all_ok = True
    for (prop, fn, w), (vp, vf, vok) in zip(recs, portable):
        if (vp, vf) != (prop, fn):
            fail.append(f"{fx}: almide-verify's verdict order drifted at [{prop}] {fn}")
            break
        with open(one, "w") as f:
            f.write(w)
        rc = subprocess.run([checker, prop, one], capture_output=True).returncode
        if rc not in (0, 1):
            fail.append(f"{fx}: the extracted checker gave no verdict (exit {rc}) on [{prop}] {fn}")
            continue
        ok = rc == 0
        if ok != vok:
            fail.append(f"{fx}: [{prop}] {fn}: extracted checker {'ACCEPT' if ok else 'REJECT'}, almide-verify {'ACCEPT' if vok else 'REJECT'}")
        stats[prop][1] += 1
        key = (fx, prop, fn)
        if ok:
            stats[prop][0] += 1
            accepts[prop].append(w)
        else:
            all_ok = False
            rejects.append((prop, w))
            rejected_seen.add(key)
            if key not in exceptions:
                fail.append(f"{fx}: [{prop}] {fn} REJECTED and no exception names it: {w[:200]!r}")
    fixtures_ok += all_ok
for key in sorted(exceptions - rejected_seen):
    fail.append("STALE exception (no witness of that name rejects — remove the row): " + "\t".join(key))

def lit(s):
    return '"' + s.replace('"', '""') + '"'
with open(kgen, "w") as g:
    g.write("From AlmideTrust Require Import OwnershipChecker NameTotality CapabilityBound CapabilityReach CallModes.\n")
    g.write("From Stdlib Require Import String List.\nImport ListNotations.\nOpen Scope string_scope.\n")
    LIMIT = 60000  # literal bytes per goal: bounds the kernel's peak term
    for prop, ws in accepts.items():
        chunk, size = [], 0
        for w in ws + [None]:
            if w is None or (chunk and size + len(w) > LIMIT):
                if chunk:
                    g.write("Goal forallb %s [%s] = true.\nProof. vm_compute. reflexivity. Qed.\n" % (FN[prop], ";".join(lit(x) for x in chunk)))
                chunk, size = [], 0
            if w is not None:
                chunk.append(w)
                size += len(w)
    for prop, w in rejects:
        g.write("Goal %s %s = false.\nProof. vm_compute. reflexivity. Qed.\n" % (FN[prop], lit(w)))

for p, (a, t) in stats.items():
    print(f"  [{p}] {a}/{t} witness(es) accepted by the extracted checker and almide-verify")
n = sum(1 for _ in open(os.path.join(out, "index")))
print(f"  {fixtures_ok}/{n} fixture(s) accepted on every property; {len(rejects)} rejected witness(es), each named in the exception ledger")
for m in fail[:20]:
    print("  FAIL " + m)
sys.exit(1 if fail else 0)
PYEOF
PRC=$?
set -e
if [ "$PRC" -ne 0 ]; then
  rm -f "$KGEN"
  echo "STRUCTURAL WALL FAIL: a certified fixture's witness is rejected, or the verdicts disagree (above)." >&2
  exit 1
fi

echo "== KERNEL ORACLE: the Rocq kernel re-derives every verdict above =="
KSTART=$(date +%s)
if (cd "$ROOT/proofs" && "$COQC" -Q . AlmideTrust "$KGEN" >/dev/null 2>&1); then
  echo "  KERNEL OK: every structural witness verdict certified by the Rocq kernel in $(( $(date +%s) - KSTART ))s (vm_compute)"
  rm -f "$KGEN" "${KGEN%.v}.vo" "${KGEN%.v}.vos" "${KGEN%.v}.vok" "${KGEN%.v}.glob"
else
  echo "STRUCTURAL WALL FAIL: the KERNEL disagrees with the extracted checker on a structural witness (extraction divergence)." >&2
  rm -f "$KGEN"
  exit 1
fi

echo
echo "STRUCTURAL WALL OK: over the $N certified spec/wasm_cross fixtures, every ownership, call-mode,"
echo "name-totality, capability and call-graph witness of the structural build is accepted —"
echo "the extracted checker, the Rocq kernel and almide-verify agreeing on every verdict."
