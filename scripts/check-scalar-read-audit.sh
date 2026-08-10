#!/usr/bin/env bash
# The scalar-read audit gate (#1183 class, aviation-quality Stage 1).
#
# Every function in crates/almide-mir/src/lower/ that emits a raw memory read
# (PrimKind::Load / load_at_offset) is an AUDIT UNIT and must carry a verdict
# in proofs/scalar-read-audit.toml:
#
#   TAG_DISPATCHED    payload read guarded by a tag test in the same fn
#   MASKED            unconditional read discarded on the wrong-tag path by
#                     arithmetic masking (the branchless eq pattern)
#   CALLER_DISPATCHED runs only under a caller that already dispatched the tag
#                     (the ledger entry names the caller)
#   NOT_SUM_READ      the load reads a non-sum layout (field slot, verified
#                     index, length, global, closure env, prim floor)
#   WALLS             the path declines before a wrong-typed read can flow
#
# UNGUARDED_SUSPECT is a WORKING verdict for an audit in progress — the gate
# FAILS on it: a suspect must be fixed (tag dispatch / wall) or reclassified
# with evidence before landing. The gate also fails when:
#   - a unit exists in the tree but not in the ledger (new arm, unclassified);
#   - a ledger entry names a unit that no longer exists (stale ledger);
#   - a unit's LOAD COUNT changed vs the ledger (the arm changed — re-audit it);
#   - a verdict is outside the vocabulary above.
#
# The enumeration is scripts/lib/scalar-read-enumerate.py — the SAME scan the
# audit used, so the gate and the ledger cannot drift apart.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER="$ROOT/proofs/scalar-read-audit.toml"

python3 - "$ROOT" "$LEDGER" <<'EOF'
import re
import sys
import importlib.util

root, ledger_path = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_file_location(
    "enum", f"{root}/scripts/lib/scalar-read-enumerate.py")
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
tree = mod.enumerate_units(root)

VOCAB = {"TAG_DISPATCHED", "MASKED", "CALLER_DISPATCHED", "NOT_SUM_READ", "WALLS"}

ledger = {}
cur = {}
for raw in open(ledger_path, encoding="utf-8"):
    line = raw.strip()
    if line == "[[arm]]":
        if cur:
            ledger[(cur["file"], cur["fn"])] = cur
        cur = {}
    m = re.match(r'(\w+)\s*=\s*"?([^"]*)"?\s*$', line)
    if m and cur is not None:
        k, v = m.group(1), m.group(2)
        if k in ("file", "fn", "verdict", "why", "caller"):
            cur[k] = v
        elif k == "loads":
            cur[k] = int(v)
if cur:
    ledger[(cur["file"], cur["fn"])] = cur

errors = []
for key, n in sorted(tree.items()):
    e = ledger.get(key)
    if e is None:
        errors.append(f"UNCLASSIFIED arm: {key[0]} :: {key[1]} ({n} load(s)) — add a verdict to proofs/scalar-read-audit.toml")
        continue
    if e.get("loads") != n:
        errors.append(f"ARM CHANGED: {key[0]} :: {key[1]} has {n} load(s), ledger says {e.get('loads')} — re-audit and update the entry")
    v = e.get("verdict", "")
    if v == "UNGUARDED_SUSPECT":
        errors.append(f"UNGUARDED arm: {key[0]} :: {key[1]} — fix (tag dispatch / wall) or reclassify with evidence before landing")
    elif v not in VOCAB:
        errors.append(f"BAD VERDICT '{v}' on {key[0]} :: {key[1]} — vocabulary: {sorted(VOCAB)} ")
    if v == "CALLER_DISPATCHED" and not e.get("caller"):
        errors.append(f"CALLER_DISPATCHED without a named caller: {key[0]} :: {key[1]}")
for key in sorted(ledger):
    if key not in tree:
        errors.append(f"STALE ledger entry: {key[0]} :: {key[1]} no longer emits loads — remove it")

if errors:
    for e in errors:
        print(f"::error::{e}")
    print(f"scalar-read audit FAILED — {len(errors)} problem(s).")
    sys.exit(1)
counts = {}
for e in ledger.values():
    counts[e["verdict"]] = counts.get(e["verdict"], 0) + 1
summary = ", ".join(f"{k}={v}" for k, v in sorted(counts.items()))
print(f"scalar-read audit OK — {len(tree)} arm(s) all classified ({summary}); zero unguarded.")
EOF
