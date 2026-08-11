#!/usr/bin/env bash
# The hand-written WAT prelude gate (Stage 1c).
#
# The prelude is the only wasm-side code NOT rendered from the shared MIR, so the
# 3-way oracle has no native counterpart to differ it against. Every `(func $…)`
# in it must therefore carry a row in proofs/wat-prelude-audit.toml naming the
# evidence that reaches it. See that file's header for the class vocabulary.
#
# Two layers, deliberately:
#   STRUCTURE (always, cheap) — every enumerated name has exactly one row, no
#     stale rows, valid class, digest matches the current source, no UNREACHED,
#     and every CORPUS/PROVEN row names a fixture that EXISTS.
#   REACHABILITY (CI, or ALMIDE_WAT_PRELUDE_REACH=1) — re-renders each named
#     fixture and fails if it does not actually define that function. Without
#     this the `via` column is a claim; with it, it is a measurement.
#
# Enumeration is scripts/lib/wat-prelude-enumerate.py, shared with the ledger so
# the two cannot drift (the #1176 one-instrument rule).
set -uo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER="$ROOT/proofs/wat-prelude-audit.toml"
# Overridable so CI can point at a debug build (it shares the job's existing
# debug dep cache; rendering 47 fixtures is cheap even unoptimized).
RENDER="${ALMIDE_RENDER:-$ROOT/target/release/examples/render_program}"

python3 - "$ROOT" "$LEDGER" <<'EOF'
import importlib.util
import re
import sys

root, ledger_path = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_file_location(
    "enum", f"{root}/scripts/lib/wat-prelude-enumerate.py")
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
units = mod.enumerate_units(root)

VOCAB = {"PROVEN", "CORPUS", "FALLBACK_ONLY", "PROBE_ONLY", "MIR_ONLY", "TEST_PINNED", "TEMPLATE"}
NEEDS_FIXTURE = {"PROVEN", "CORPUS"}

rows, cur = [], {}
for raw in open(ledger_path, encoding="utf-8"):
    line = raw.strip()
    if line == "[[fn]]":
        if cur:
            rows.append(cur)
        cur = {}
        continue
    m = re.match(r'([a-z_]+)\s*=\s*"(.*)"$', line)
    if m and cur is not None:
        cur[m.group(1)] = m.group(2)
if cur:
    rows.append(cur)

errs = []
seen = set()
for r in rows:
    n = r.get("name")
    if not n:
        errs.append(f"row without a name: {r}")
        continue
    if n in seen:
        errs.append(f"{n}: duplicate row")
    seen.add(n)
    cls = r.get("class", "")
    if cls == "UNREACHED":
        errs.append(
            f"{n}: UNREACHED — hand-written wasm nothing can reach (DO-178C dead "
            f"code). Delete it, or wire it to something and reclassify.")
    elif cls not in VOCAB:
        errs.append(f"{n}: unknown class {cls!r} (expected one of {sorted(VOCAB)})")
    if n not in units:
        errs.append(f"{n}: STALE row — no `(func ${n}` in the renderer sources any more")
        continue
    site, digest = units[n]
    if r.get("digest") != digest:
        errs.append(
            f"{n}: digest {r.get('digest')} != {digest} — the hand-written wasm at "
            f"{site} changed. Re-confirm its evidence and update the row; evidence "
            f"is not inherited across an edit.")
    if r.get("site") != site:
        errs.append(f"{n}: site moved to {site} (row says {r.get('site')})")
    if cls in NEEDS_FIXTURE:
        fx = r.get("via", "")
        if not fx:
            errs.append(f"{n}: {cls} needs `via = \"<fixture>.almd\"`")
        elif not (__import__("os").path.exists(f"{root}/spec/wasm_cross/{fx}")):
            errs.append(f"{n}: via names spec/wasm_cross/{fx}, which does not exist")
    if cls == "PROVEN" and not r.get("proof"):
        errs.append(f"{n}: PROVEN needs `proof = \"<file>\"`")
    if cls == "MIR_ONLY" and not r.get("ref"):
        errs.append(f"{n}: MIR_ONLY needs `ref = \"#NNNN\"` — unreachable-from-source "
                    f"wasm is debt, and debt needs an issue")
    if cls == "TEST_PINNED" and not r.get("test"):
        errs.append(f"{n}: TEST_PINNED needs `test = \"<cargo test path>\"` — the class's "
                    f"whole claim is that a named test executes the body")
    if cls in {"FALLBACK_ONLY", "PROBE_ONLY", "TEMPLATE", "MIR_ONLY", "TEST_PINNED"} and not r.get("why"):
        errs.append(f"{n}: {cls} needs `why = \"…\"`")

for n in sorted(units):
    if n not in seen:
        errs.append(
            f"{n}: UNCLASSIFIED — a prelude function at {units[n][0]} with no row. "
            f"Hand-written wasm the oracle cannot see needs its evidence named.")

if errs:
    print("WAT PRELUDE AUDIT FAIL —", file=sys.stderr)
    for e in errs:
        print(f"  {e}", file=sys.stderr)
    sys.exit(1)

by = {}
for r in rows:
    by[r["class"]] = by.get(r["class"], 0) + 1
print("WAT PRELUDE AUDIT OK: " + str(len(rows)) + " fn(s) — "
      + " / ".join(f"{k} {v}" for k, v in sorted(by.items())))
EOF
STRUCT=$?
[ $STRUCT -ne 0 ] && exit $STRUCT

# --- REACHABILITY: the `via` column re-measured, not trusted -----------------
if [ "${CI:-}" != "true" ] && [ "${ALMIDE_WAT_PRELUDE_REACH:-}" != "1" ]; then
  echo "wat-prelude: reachability re-measurement SKIPPED (set ALMIDE_WAT_PRELUDE_REACH=1)"
  exit 0
fi
if [ ! -x "$RENDER" ]; then
  if [ "${CI:-}" = "true" ]; then
    echo "::error::wat-prelude: no render_program at $RENDER — in CI a missing oracle is a failure (#978)" >&2
    exit 1
  fi
  echo "wat-prelude: no render_program at $RENDER — build it with" \
       "\`cargo build --release -p almide-mir --example render_program\`" >&2
  exit 1
fi

fail=0
while IFS='|' read -r name fx; do
  [ -z "$name" ] && continue
  # Rendered to a VARIABLE, not piped: render_program exits non-zero whenever a
  # fixture has any function outside the MIR-lowering subset (it reports those to
  # stderr and emits the rest), and under `pipefail` that would sink the whole
  # pipeline even when grep matched — every row would read as "evidence is not
  # real". `</dev/null` keeps a dep-resolving child from eating the work list.
  # `\b` is a GNU extension BSD grep does not honour, so spell the boundary out
  # or the check passes vacuously on macOS and this gate becomes decorative.
  wat="$("$RENDER" "$ROOT/spec/wasm_cross/$fx" </dev/null 2>/dev/null || true)"
  # `grep -c`, never `grep -q`: `-q` exits at the FIRST match, the ~150 KB still
  # in `printf`'s pipe becomes EPIPE, printf returns non-zero, and `pipefail`
  # sinks the pipeline — every row would report "evidence is not real" while the
  # evidence was in fact there. `-c` consumes the whole stream.
  hits="$(printf '%s' "$wat" | grep -cE "\(func \\\$$name([^A-Za-z0-9_]|$)" || true)"
  if [ "${hits:-0}" -eq 0 ]; then
    echo "  $name: via=$fx does NOT define it — the row's evidence is not real" >&2
    fail=1
  fi
done < <(python3 - "$LEDGER" <<'EOF'
import re, sys
cur = {}
for raw in open(sys.argv[1], encoding="utf-8"):
    line = raw.strip()
    if line == "[[fn]]":
        if cur.get("class") in {"CORPUS", "PROVEN"}:
            print(f"{cur['name']}|{cur['via']}")
        cur = {}
        continue
    m = re.match(r'([a-z_]+)\s*=\s*"(.*)"$', line)
    if m:
        cur[m.group(1)] = m.group(2)
if cur.get("class") in {"CORPUS", "PROVEN"}:
    print(f"{cur['name']}|{cur['via']}")
EOF
)
if [ $fail -ne 0 ]; then
  echo "WAT PRELUDE REACHABILITY FAIL — a \`via\` fixture stopped reaching its function." >&2
  exit 1
fi
echo "wat-prelude: every CORPUS/PROVEN \`via\` re-measured and confirmed"
