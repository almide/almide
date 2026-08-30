#!/usr/bin/env bash
# MC/DC DECISION LEDGER (#566 rung 2, CG-2): rustc's MC/DC instrumentation was
# REMOVED upstream (rust-lang/rust#144999, Aug 2025; 1.96.1 accepts only
# block|branch|condition — probed 2026-08-26), so MC/DC-grade evidence for the
# SAFETY SET is a PER-DECISION argument: every boolean-operator site (&&/|| in
# real boolean position) in the safety files carries a ledger row that is
# either RESOLVED — `vectors` naming the unit tests that toggle each operand
# independently with a flipped outcome (the MC/DC independence pair), or
# `refactored` (the site no longer scans) — or `pending` under a shrink-only
# ceiling. `condition`-level llvm-cov instrumentation is the measured backstop
# (the coverage-condition CI job), not the argument itself.
#
#   bash proofs/mcdc-ledger.sh            # the gate (CI)
#   bash proofs/mcdc-ledger.sh --write    # re-scan; add pending rows for new
#                                         # sites, refresh line numbers, keep
#                                         # hand-written fields by id
#   bash proofs/mcdc-ledger.sh --emit-sites  # JSON per scanned site (id, file,
#                                         # byte offset, op) — the ONE scanner,
#                                         # consumed by proofs/mcdc-mutation.sh
set -uo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
python3 - "${1:---check}" <<'PY'
import hashlib, re, sys

LEDGER = "proofs/mcdc-ledger.toml"
SAFETY_SET = [
    "crates/almide-codegen/src/perceus_verified.rs",
    "crates/almide-mir/src/certificate.rs",
    "crates/almide-mir/src/certificate_b.rs",
    "crates/almide-mir/src/certificate_b_tail.rs",
    "crates/almide-mir/src/certificate_c.rs",
    "crates/almide-mir/src/certificate_p2.rs",
    "crates/almide-mir/src/translation_validation.rs",
    "crates/almide-frontend/src/check/calls.rs",
    "crates/almide-frontend/src/check/infer_calls_closures.rs",
    "crates/almide-codegen/src/pass_effect_inference.rs",
]
RESOLUTIONS = ("pending", "vectors", "refactored")

def strip_noise(src):
    """Blank out comments, string/char literals — keep byte offsets stable."""
    out = list(src); i = 0; n = len(src)
    def blank(a, b):
        for k in range(a, min(b, n)):
            if out[k] not in "\n": out[k] = " "
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i); j = n if j < 0 else j; blank(i, j); i = j
        elif src.startswith("/*", i):
            j = src.find("*/", i); j = n if j < 0 else j + 2; blank(i, j); i = j
        elif c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            blank(i, j + 1); i = j + 1
        elif c == "'" and i + 2 < n and (src[i+1] == "\\" or src[i+2] == "'"):
            j = i + 1
            while j < n and src[j] != "'":
                j += 2 if src[j] == "\\" else 1
            blank(i, j + 1); i = j + 1
        else:
            i += 1
    return "".join(out)

def scan(path):
    """Boolean &&/|| operator sites: preceding non-space char is a value end
    (alnum, ), ], _, ?), not a delimiter — which excludes closure heads
    `(|| …`, `, |x| …` and reference sigils. Sites past the file's first
    `#[cfg(test)]` are the tests themselves, not production decisions, and are
    not ledgered (Rust convention keeps the test mod at the tail; the scanner
    cut is byte-positional, so a production fn moved below it would vanish
    from the scan — keep test mods last)."""
    raw = open(path, encoding="utf-8").read()
    cut = raw.find("#[cfg(test)]")
    if cut >= 0: raw = raw[:cut]
    src = strip_noise(raw)
    raw_lines = raw.splitlines()
    sites, counts = [], {}
    for m in re.finditer(r'&&|\|\|', src):
        k = src[:m.start()].rstrip()
        prev = k[-1] if k else ""
        if not (prev.isalnum() or prev in ")]_?"):
            continue
        line = src.count("\n", 0, m.start()) + 1
        excerpt = raw_lines[line - 1].strip()[:120]
        norm = re.sub(r'\s+', ' ', excerpt)
        dup = counts.get((path, norm), 0); counts[(path, norm)] = dup + 1
        sid = hashlib.sha256(f"{path}|{norm}|{dup}".encode()).hexdigest()[:10]
        sites.append({"id": sid, "file": path, "line": line, "op": m.group(0), "excerpt": excerpt, "offset": m.start()})
    return sites

sites = [s for p in SAFETY_SET for s in scan(p)]
if not sites:
    print("::error::mcdc-ledger: zero boolean sites scanned across the safety set — a broken instrument is not a pass"); sys.exit(1)

def read_ledger():
    try: src = open(LEDGER, encoding="utf-8").read()
    except FileNotFoundError: return None, {}, {}
    hdr = dict(re.findall(r'^#\s*(\w+)\s*=\s*"([^"]*)"', src, re.M))
    rows = {}
    for block in re.split(r'^\[\[site\]\]', src, flags=re.M)[1:]:
        f = dict(re.findall(r'^(\w+)\s*=\s*"((?:[^"\\]|\\.)*)"', block, re.M))
        if "id" in f: rows[f["id"]] = f
    return hdr, rows, src

mode = sys.argv[1]
if mode == "--emit-sites":
    import json
    for s_ in sites:
        print(json.dumps(s_))
    sys.exit(0)
hdr, rows, _ = read_ledger()

if mode == "--write":
    by_id = {s["id"]: s for s in sites}
    kept = {i: r for i, r in rows.items() if i in by_id}
    pending = [s for s in sites if s["id"] not in kept or kept[s["id"]].get("resolution") == "pending"]
    n_pending = sum(1 for s in sites if kept.get(s["id"], {}).get("resolution", "pending") == "pending")
    with open(LEDGER, "w", encoding="utf-8") as f:
        f.write('''# MC/DC DECISION LEDGER (#566 rung 2) — every boolean-operator site in the
# safety set, each resolved or pending. Regenerate rows: proofs/mcdc-ledger.sh
# --write (hand-written resolution/tests/note fields survive by id). A
# `vectors` row names unit tests demonstrating each operand's INDEPENDENT
# effect on the outcome (the MC/DC pair); `refactored` rows disappear when the
# site stops scanning; `pending` is a shrink-only debt.
#
# pending_ceiling = "%d"
''' % n_pending)
        for s in sites:
            old = kept.get(s["id"], {})
            f.write('\n[[site]]\n')
            f.write(f'id = "{s["id"]}"\nfile = "{s["file"]}"\nline = "{s["line"]}"\nop = "{s["op"]}"\n')
            f.write(f'excerpt = "{s["excerpt"].replace(chr(92), chr(92)*2).replace(chr(34), chr(92)+chr(34))}"\n')
            f.write(f'resolution = "{old.get("resolution", "pending")}"\n')
            if old.get("tests"): f.write(f'tests = "{old["tests"]}"\n')
            if old.get("note"): f.write(f'note = "{old["note"]}"\n')
    dropped = [i for i in rows if i not in by_id]
    print(f"mcdc-ledger written: {len(sites)} site(s), pending {n_pending}" + (f", dropped stale {len(dropped)}" if dropped else ""))
    sys.exit(0)

errs = []
if hdr is None:
    print("::error::proofs/mcdc-ledger.toml missing — run --write"); sys.exit(1)
try: ceiling = int(hdr["pending_ceiling"])
except (KeyError, ValueError): errs.append('header missing # pending_ceiling = "N"'); ceiling = -1
scan_ids = {s["id"]: s for s in sites}
for sid, s in scan_ids.items():
    r = rows.get(sid)
    if not r:
        errs.append(f'{s["file"]}:{s["line"]}: boolean site {sid} has NO ledger row ({s["excerpt"][:60]!r}) — run --write and resolve or leave pending'); continue
    if r.get("resolution") not in RESOLUTIONS:
        errs.append(f'{sid}: unknown resolution {r.get("resolution")!r}')
    if r.get("resolution") == "vectors":
        tests = [t.strip() for t in r.get("tests", "").split(",") if t.strip()]
        if not tests:
            errs.append(f'{sid}: resolution=vectors but no tests named')
        for t in tests:
            if "::" not in t:
                errs.append(f'{sid}: test {t!r} must be <file>::<fn>'); continue
            tf, fn = t.rsplit("::", 1)
            try: tsrc = open(tf, encoding="utf-8").read()
            except FileNotFoundError: errs.append(f'{sid}: test file {tf} missing'); continue
            if f"fn {fn}(" not in tsrc:
                errs.append(f'{sid}: fn {fn} not found in {tf}')
    if r.get("resolution") == "refactored":
        errs.append(f'{sid}: marked refactored but the site still scans at {s["file"]}:{s["line"]}')
for sid, r in rows.items():
    if sid not in scan_ids:
        errs.append(f'{sid}: STALE row — site no longer scans ({r.get("file")}:{r.get("line")})')
n_pending = sum(1 for sid in scan_ids if rows.get(sid, {}).get("resolution") == "pending")
if ceiling >= 0:
    if n_pending > ceiling: errs.append(f'pending sites {n_pending} exceed ceiling {ceiling} — resolve the new site or raise the ceiling in its own justified commit')
    elif n_pending < ceiling: errs.append(f'pending sites {n_pending} BELOW ceiling {ceiling} — ratchet it down')
for e in errs: print(f"::error::{e}")
if errs: sys.exit(1)
n_vec = sum(1 for sid in scan_ids if rows.get(sid, {}).get("resolution") == "vectors")
print(f"mcdc-ledger OK: {len(sites)} boolean site(s) across {len(SAFETY_SET)} safety files — vectors {n_vec}, pending {n_pending} (ceiling {ceiling}, shrink-only).")
PY
