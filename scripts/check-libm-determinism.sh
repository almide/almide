#!/usr/bin/env bash
# The libm determinism gate (#1197). Almide vendors musl-libm so a
# transcendental is bit-identical native <-> wasm AND across host platforms;
# a call to Rust's own `f64::exp`/`powf`/... escapes that contract. Every such
# call site must carry a verdict in proofs/libm-determinism-audit.toml:
#
#   VENDORED         the site routes to `almide_rt_libm_*` (the contract's own
#                    path) — scanned only because the name matches.
#   IEEE_EXACT       correctly-rounded / exact by IEEE-754 (e.g. `powi`, which
#                    is compiler-expanded multiplication, not a libm call).
#   UNREACHABLE      the fn exists but NO `@intrinsic` in stdlib/*.almd reaches
#                    it, so no Almide program can observe it. The entry names
#                    what would make it reachable — a loaded gun, disarmed.
#   DECLARED_APPROX  a DELIBERATE approximation (a SIMD fast-path). The entry
#                    MUST cite a contract or issue (`ref = "..."`) — an
#                    accuracy/speed trade is a product decision that has to be
#                    written down, which is precisely what #1197 found missing.
#   TEST_ONLY        inside a #[cfg(test)] / test helper — never shipped.
#
# PLATFORM is the failing verdict: a reachable, undeclared platform call.
# The gate also fails on an unclassified site (new call added), a stale entry,
# a changed call count (the site changed — re-audit it), or a DECLARED_APPROX
# without a `ref`. Enumeration: scripts/lib/libm-call-enumerate.py, shared with
# the audit so the two cannot drift (the #1176 one-instrument rule).
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER="$ROOT/proofs/libm-determinism-audit.toml"

python3 - "$ROOT" "$LEDGER" <<'EOF'
import importlib.util
import re
import sys

root, ledger_path = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_file_location(
    "enum", f"{root}/scripts/lib/libm-call-enumerate.py")
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
tree = mod.enumerate_units(root)

VOCAB = {"VENDORED", "IEEE_EXACT", "UNREACHABLE", "DECLARED_APPROX", "TEST_ONLY"}

ledger, cur = {}, {}
for raw in open(ledger_path, encoding="utf-8"):
    line = raw.strip()
    if line == "[[site]]":
        if cur:
            ledger[(cur.get("file"), cur.get("fn"))] = cur
        cur = {}
    m = re.match(r'(\w+)\s*=\s*"?([^"]*)"?\s*$', line)
    if m and cur is not None:
        k, v = m.group(1), m.group(2)
        if k == "calls":
            cur[k] = int(v)
        elif k in ("file", "fn", "verdict", "why", "ref"):
            cur[k] = v
if cur:
    ledger[(cur.get("file"), cur.get("fn"))] = cur

errors = []
for key, n in sorted(tree.items()):
    e = ledger.get(key)
    if e is None:
        errors.append(
            f"UNCLASSIFIED platform-libm call: {key[0]} :: {key[1]} ({n} call(s)) — "
            f"add a verdict to proofs/libm-determinism-audit.toml"
        )
        continue
    if e.get("calls") != n:
        errors.append(
            f"SITE CHANGED: {key[0]} :: {key[1]} has {n} call(s), ledger says "
            f"{e.get('calls')} — re-audit and update the entry"
        )
    v = e.get("verdict", "")
    if v == "PLATFORM":
        errors.append(
            f"REACHABLE PLATFORM libm: {key[0]} :: {key[1]} — route it through "
            f"almide_rt_libm_* (the #1197 fix) or declare the approximation"
        )
    elif v not in VOCAB:
        errors.append(f"BAD VERDICT '{v}' on {key[0]} :: {key[1]} — vocabulary: {sorted(VOCAB)}")
    if v == "DECLARED_APPROX" and not e.get("ref"):
        errors.append(
            f"DECLARED_APPROX without a contract/issue ref: {key[0]} :: {key[1]} — "
            f"an accuracy/speed trade must be written down (#1197's whole finding)"
        )
for key in sorted(ledger):
    if key not in tree:
        errors.append(f"STALE ledger entry: {key[0]} :: {key[1]} no longer calls a platform libm — remove it")

if errors:
    for e in errors:
        print(f"::error::{e}")
    print(f"libm-determinism FAILED — {len(errors)} problem(s).")
    sys.exit(1)
counts = {}
for e in ledger.values():
    counts[e["verdict"]] = counts.get(e["verdict"], 0) + 1
summary = ", ".join(f"{k}={v}" for k, v in sorted(counts.items()))
print(f"libm-determinism OK — {len(tree)} site(s) all classified ({summary}); zero reachable-undeclared.")
EOF
