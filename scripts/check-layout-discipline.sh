#!/usr/bin/env bash
# LAYOUT DISCIPLINE GATE (#1316).
#
# Every NEW long-lived data structure in the compile pipeline is flat arrays +
# u32 ids + interned atoms, or it carries a written deviation note. The rule and
# its measured justification: crates/CLAUDE.md § "Layout discipline (new
# long-lived structures)". The ledger: proofs/layout-ledger.toml.
#
# WHAT THIS GATE CHECKS (all of it re-derived from source on every run, by the
# shared scan in scripts/lib/layout-enumerate.py — the gate and the ledger
# cannot drift apart):
#
#   * a type in a `[scope] globs` file, or a serde-deriving type outside a
#     [[frozen]] file, with no ledger row              -> UNREGISTERED
#   * a ledger row naming a type that no longer exists -> STALE
#   * a row that says FLAT while the type holds a banned field construct
#     (String / &str / Box / Rc / Arc / RefCell / lock / HashMap / HashSet /
#     Vec<Vec<..>> / a reference / a pre-existing pointer-rich house tree)
#                                                      -> the row lies
#   * a row that says DEVIATION while the type is now clean
#                                                      -> reclassify + ratchet
#   * a DEVIATION row without a real paragraph in `why`
#   * a [[frozen]] file whose measured serde-type count moved (a new long-lived
#     type parked in a pre-existing file, or a frozen row gone stale)
#   * the DEVIATION count against `deviation_ceiling` — both directions, so the
#     debt is a number in the ledger and may only be moved deliberately
#   * a scan that found no types at all (blind-gate guard, #976 class)
#
# WHAT THIS GATE CANNOT CHECK — say it out loud, because a gate trusted beyond
# its reach is worse than no gate:
#
#   * SoA vs AoS. `Vec<Row>` of flat rows passes and is array-of-structs. The
#     split into parallel columns is a review call, not a mechanical one.
#   * test/example/bench code. `tests/`, `examples/` and `benches/` directories
#     are skipped: a fixture type is not long-lived, and forcing rows for them
#     would train everyone to rubber-stamp the ledger.
#   * function bodies. A HashMap<String, _> built at runtime inside a fn, or a
#     process-wide cache keyed by pointer (crates/almide-syntax/src/parse_cache.rs),
#     is invisible here — only type DECLARATIONS are scanned.
#   * type resolution. Field types are read as WRITTEN. An alias, a generic
#     parameter, or a struct from another crate is opaque and passes; only the
#     pinned house trees (Ty / IrExpr / IrStmt / IrPattern / Program / Decl) are
#     recognised by name.
#   * whether a u32 id indexes the table it claims to, whether an interner is
#     actually used at the construction sites, or whether the format is
#     genuinely memcpy-loadable. Those are review, and the deviation note is
#     where the reviewer's reasoning gets written down.
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

python3 - "$ROOT" <<'EOF'
import fnmatch
import importlib.util
import os
import re
import sys
import tomllib

root = sys.argv[1]
ledger_path = os.path.join(root, "proofs/layout-ledger.toml")

spec = importlib.util.spec_from_file_location(
    "layout_enum", os.path.join(root, "scripts/lib/layout-enumerate.py"))
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
types = mod.enumerate_types(root)

with open(ledger_path, "rb") as f:
    ledger = tomllib.load(f)

ceiling = None
for line in open(ledger_path, encoding="utf-8"):
    m = re.match(r'#\s*deviation_ceiling\s*=\s*"(\d+)"', line.strip())
    if m:
        ceiling = int(m.group(1))

VOCAB = {"FLAT", "DEVIATION"}
errs = []

# blind-gate guard: a scan that matches nothing must not pass green.
if len(types) < 100:
    errs.append(f"the source scan found only {len(types)} type declaration(s) — "
                f"the enumerator is broken or the tree moved; refusing to pass")
if ceiling is None:
    errs.append('ledger header is missing `# deviation_ceiling = "N"`')

frozen_rows = ledger.get("frozen", [])
frozen_paths = {r["path"] for r in frozen_rows}
globs = ledger.get("scope", {}).get("globs", [])
if not globs:
    errs.append("[scope] globs is empty — the tripwire for predictable module "
                "names (cache, clif) must name at least one pattern")

# ── 1. frozen files: the measured serde-type count must still match ──
measured_serde = {}
for (rel, name), info in types.items():
    if info["serde"]:
        measured_serde[rel] = measured_serde.get(rel, 0) + 1

for r in frozen_rows:
    p = r["path"]
    if not os.path.exists(os.path.join(root, p)):
        errs.append(f"{p}: [[frozen]] row points at a file that no longer exists")
        continue
    want, got = r.get("serde_types"), measured_serde.get(p, 0)
    if got != want:
        errs.append(
            f"{p}: FROZEN DRIFT — {got} serde-deriving type(s), ledger says {want}. "
            f"If the new type is an AST/IR node under the #1316 pre-existing "
            f"exemption, bump the count here in the SAME commit. If it is a new "
            f"long-lived artifact (a cache, an index, a CLIF structure), it does "
            f"not belong in a frozen file — give it its own module, where the "
            f"layout discipline applies.")

# ── 2. the in-scope population ──
registered = {}
for r in ledger.get("type", []):
    key = (r["path"], r["name"])
    if key in registered:
        errs.append(f"{r['path']} :: {r['name']}: duplicate ledger row")
    registered[key] = r

in_scope = set(registered)
for (rel, name), info in types.items():
    if any(fnmatch.fnmatch(rel, g) for g in globs):
        in_scope.add((rel, name))
    elif info["serde"] and rel not in frozen_paths:
        in_scope.add((rel, name))

for key in sorted(in_scope):
    rel, name = key
    info = types.get(key)
    row = registered.get(key)
    if info is None:
        errs.append(f"{rel} :: {name}: STALE ledger row — no such type declaration")
        continue
    if row is None:
        why_scope = ("is declared in a file matching a [scope] glob"
                     if any(fnmatch.fnmatch(rel, g) for g in globs)
                     else "derives Serialize/Deserialize outside a [[frozen]] file, "
                          "so it crosses a process boundary")
        errs.append(
            f"{rel}:{info['line']} :: {name}: UNREGISTERED long-lived type — it "
            f"{why_scope}. Add a [[type]] row to proofs/layout-ledger.toml: "
            f"class = \"FLAT\" if it is flat arrays + u32 ids + interned atoms, "
            f"or class = \"DEVIATION\" with a `why` paragraph saying what it "
            f"costs and what would remove it (and raise deviation_ceiling).")
        continue
    cls = row.get("class", "")
    banned = info["banned"]
    if cls not in VOCAB:
        errs.append(f"{rel} :: {name}: unknown class {cls!r} (expected one of {sorted(VOCAB)})")
        continue
    if cls == "FLAT" and banned:
        detail = "; ".join(f"`{f}: {t}` — {w}" for f, t, w in banned)
        errs.append(f"{rel}:{info['line']} :: {name}: row says FLAT but the type "
                    f"holds {detail}. Either flatten the field or move the row to "
                    f"DEVIATION with a `why` paragraph and raise deviation_ceiling.")
    if cls == "DEVIATION":
        if not banned:
            errs.append(f"{rel} :: {name}: row says DEVIATION but every field is now "
                        f"flat — reclassify as FLAT and ratchet deviation_ceiling DOWN.")
        why = (row.get("why") or "").strip()
        if len(why) < 120:
            errs.append(f"{rel} :: {name}: DEVIATION needs the one-paragraph note "
                        f"#1316 requires (what deviates, what it costs — measured — "
                        f"and what would remove it); got {len(why)} char(s).")

deviations = sum(1 for r in registered.values() if r.get("class") == "DEVIATION")
if ceiling is not None:
    if deviations > ceiling:
        errs.append(f"DEVIATION count {deviations} exceeds deviation_ceiling {ceiling} — "
                    f"a new deviation lands with its note AND a deliberate raise here")
    elif deviations < ceiling:
        errs.append(f"DEVIATION count {deviations} is BELOW deviation_ceiling {ceiling} — "
                    f"ratchet the header down (the debt may only shrink, and the "
                    f"ledger must say so)")

if errs:
    for e in errs:
        print(f"::error::layout-discipline: {e}")
    print(f"layout discipline FAILED — {len(errs)} problem(s).", file=sys.stderr)
    sys.exit(1)

flat = sum(1 for r in registered.values() if r.get("class") == "FLAT")
print(f"layout discipline OK — {len(types)} type declaration(s) scanned, "
      f"{len(frozen_rows)} file(s) frozen at their measured serde count, "
      f"{len(registered)} registered ({flat} FLAT / {deviations} DEVIATION, "
      f"ceiling {ceiling}).")
EOF
