#!/usr/bin/env bash
# IMPLEMENTATION-COUNT GATE.
#
# A stdlib hole (`fn f(...) = _`) is one operation whose body a backend
# supplies. Tonight (2026-09-21) three of the four defects the 0.63.0-rc3 soak
# found were the same shape: one operation implemented several times — a native
# intrinsic, hand-written structural-wasm lowerings, shape-monomorphic Almide
# copies on the prim floor — and a rule that landed in one copy never reached
# the others. `list.fold` had ten. A fixture cannot catch the copy that does not
# take its shape, and a sweep keyed on a guard's spelling cannot see the copy
# that predates the spelling ("0 matches in this file" is the signature of an
# unconverted copy, not a clean one). What catches it is the COUNT.
#
# This gate pins, per hole, (native, emitter, selfhost) as re-derived from source
# by scripts/lib/impl-count-enumerate.py, in proofs/impl-count-ledger.toml.
#
#   - a hole whose total GREW is a failure: a new copy must add itself to the
#     ledger in the same change, so the reviewer sees "fold: 3 emitter arms → 4"
#     as a line in the diff rather than as a blocker nine weeks later;
#   - a hole whose total SHRANK is also a failure — "run with UPDATE=1" — so the
#     ledger never sits above reality in the flattering direction;
#   - a new hole, or a hole that disappeared, likewise.
#
# The goal is the fold, not the pin: docs/roadmap/active/wasm-ownership-emit-
# mechanization.md (2026-09-21 section) takes `list.fold` from ten to one body
# per target and works down list.almd's holes. Every step LOWERS a row here.
#
# UPDATE=1 rewrites the ledger from the current tree.
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER="$ROOT/proofs/impl-count-ledger.toml"
ENUM="$ROOT/scripts/lib/impl-count-enumerate.py"

if [ "${UPDATE:-0}" = "1" ]; then
  python3 "$ENUM" "$ROOT" --toml > "$LEDGER"
  echo "impl-count: ledger rewritten — $(grep -c '^\[\[hole\]\]' "$LEDGER") holes"
  exit 0
fi

[ -f "$LEDGER" ] || { echo "::error::$LEDGER missing — run UPDATE=1 $0"; exit 1; }

python3 - "$ROOT" "$LEDGER" "$ENUM" <<'PY'
import subprocess, sys, tomllib
root, ledger, enum = sys.argv[1:4]
with open(ledger, "rb") as f:
    pinned = {(h["module"], h["name"]): h for h in tomllib.load(f)["hole"]}
now = {}
for line in subprocess.check_output([sys.executable, enum, root], text=True).splitlines():
    m, n, native, e, s, t = line.split("\t")
    now[(m, n)] = dict(native=int(native), emitter=int(e), selfhost=int(s), total=int(t))

bad = []
for key, cur in now.items():
    p = pinned.get(key)
    if p is None:
        bad.append(f"NEW hole {key[0]}.{key[1]} (total {cur['total']}) is not in the ledger")
        continue
    if cur["total"] > p["total"]:
        bad.append(f"{key[0]}.{key[1]}: total {p['total']} -> {cur['total']} GREW "
                   f"(native {p['native']}->{cur['native']}, emitter {p['emitter']}->{cur['emitter']}, "
                   f"selfhost {p['selfhost']}->{cur['selfhost']}) — a new copy of one operation; "
                   f"fold it into an existing body, or add the row with UPDATE=1 and say why in the PR")
    elif cur["total"] < p["total"] or any(cur[c] != p[c] for c in ("native", "emitter", "selfhost")):
        bad.append(f"{key[0]}.{key[1]}: ledger says {p['total']} "
                   f"({p['native']}/{p['emitter']}/{p['selfhost']}), tree has {cur['total']} "
                   f"({cur['native']}/{cur['emitter']}/{cur['selfhost']}) — run UPDATE=1 to record the shrink")
for key in pinned:
    if key not in now:
        bad.append(f"ledger row {key[0]}.{key[1]} no longer a hole — run UPDATE=1")

holes = len(now)
impls = sum(c["total"] for c in now.values())
worst = sorted(now.items(), key=lambda kv: -kv[1]["total"])[:3]
worst_s = ", ".join(f"{m}.{n}={c['total']}" for (m, n), c in worst)
if bad:
    for b in bad:
        print(f"::error::impl-count: {b}")
    print(f"impl-count: FAILED — {len(bad)} drift(s) against {ledger}")
    sys.exit(1)
print(f"impl-count OK: {holes} holes, {impls} implementations pinned (worst: {worst_s})")
PY
