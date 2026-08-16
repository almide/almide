#!/usr/bin/env bash
# Allocation-ceiling gate — ONE limit, six spellings, machine-checked.
#
# WHAT THIS ENFORCES
# ------------------
# Every allocation ceiling in the tree denotes the SAME 2 GiB, expressed in
# whatever unit its module counts in:
#
#   runtime/rs/src/string.rs  ALMIDE_REPEAT_MAX_BYTES       1 << 31   bytes  x1
#   runtime/rs/src/bytes.rs   ALMIDE_BYTES_MAX_BYTES        1 << 31   bytes  x1
#   runtime/rs/src/list.rs    ALMIDE_LIST_REPEAT_MAX_ELEMS  (1<<31)/8 slots  x8
#   runtime/rs/src/matrix.rs  ALMIDE_MATRIX_MAX_ELEMS       1 << 28   f64    x8
#   stdlib/*.almd             a literal in a `prim.die` guard
#
# The number matters: it is sized BELOW the wasm 4 GiB address space so a size
# one leg can satisfy and the other cannot never becomes a native success against
# a wasm out-of-memory. If one of these drifts, the two legs disagree again for
# exactly the inputs the ceiling exists to make agree — and they disagree
# SILENTLY, because each leg is internally consistent.
#
# WHY A GATE AND NOT A COMMENT
# ----------------------------
# Every one of these sites already carried a "keep in sync with X" comment. That
# is the mechanism that was in place while `bytes.new(u32::MAX)` allocated 4 GiB
# natively and aborted on wasm, and while sixteen array readers sat outside the
# ceiling their own precedent defined. A comment asking a future reader to
# remember is not a mechanism. This is the same lesson `proofs/domain-edges.toml`
# records at the value level, applied to the constants.
#
# THE SECOND HALF, which is the half that catches the NEXT one: an abort guard in
# the stdlib may only compare against a DECLARED ceiling value. A new allocator
# that invents its own threshold fails here rather than shipping a sixth spelling
# nobody knows to keep in sync. (Shape borrowed from Rust `tidy`'s target_policy:
# walk the implementation, subtract what is declared, fail on the remainder.)

set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

python3 - "$REPO" <<'PY'
import re, sys, pathlib

repo = pathlib.Path(sys.argv[1])
LIMIT = 1 << 31          # 2 GiB — the one true ceiling, in bytes
fail = []

def ev(expr):
    """Evaluate a Rust integer-literal expression like `(1 << 31) / 8`."""
    if not re.fullmatch(r"[\d_()<>/*+\- ]+", expr):
        return None
    try:
        return eval(expr.replace("_", ""), {"__builtins__": {}}, {})
    except Exception:
        return None

# ── 1. the named native constants, each converted to bytes ──────────────────
NATIVE = [
    ("runtime/rs/src/string.rs", "ALMIDE_REPEAT_MAX_BYTES",      1),
    ("runtime/rs/src/bytes.rs",  "ALMIDE_BYTES_MAX_BYTES",       1),
    ("runtime/rs/src/list.rs",   "ALMIDE_LIST_REPEAT_MAX_ELEMS", 8),
    ("runtime/rs/src/matrix.rs", "ALMIDE_MATRIX_MAX_ELEMS",      8),
]
declared = set()
for rel, name, width in NATIVE:
    p = repo / rel
    if not p.exists():
        fail.append(f"{rel}: missing — the ceiling roster names it")
        continue
    m = re.search(rf"const\s+{name}\s*:\s*i64\s*=\s*([^;]+);", p.read_text())
    if not m:
        fail.append(f"{rel}: `{name}` not found (renamed? then update this roster)")
        continue
    v = ev(m.group(1).strip())
    if v is None:
        fail.append(f"{rel}: `{name}` is not a literal expression this gate can evaluate")
        continue
    declared.add(v)
    if v * width != LIMIT:
        fail.append(f"{rel}: {name} = {m.group(1).strip()} = {v} x {width}B = {v*width} bytes, "
                    f"but the shared ceiling is {LIMIT} bytes")

# ── 2. every stdlib abort guard must compare against a DECLARED value ───────
# A ceiling is exactly "a number that gates an abort", which is why this looks
# for `prim.die` rather than for the literal: an i32 range bound like 2147483648
# in `__sext32` is the same number and is NOT a ceiling.
seen_selfhost = 0
for p in sorted((repo / "stdlib").glob("*.almd")):
    for i, line in enumerate(p.read_text().splitlines(), 1):
        if "prim.die" not in line:
            continue
        for lit in re.findall(r"[<>]=?\s*(\d{6,})", line):
            seen_selfhost += 1
            v = int(lit)
            if v not in declared:
                fail.append(
                    f"{p.relative_to(repo)}:{i}: abort guard compares against {v}, which is not a "
                    f"declared ceiling {sorted(declared)} — name it in the roster or use the shared value")

# ── 3. no native abort may compare against an unnamed magic literal ─────────
for rel, _, _ in NATIVE:
    src = (repo / rel).read_text().splitlines()
    for i, line in enumerate(src, 1):
        if "process::exit" not in line:
            continue
        for back in src[max(0, i - 6):i]:
            for lit in re.findall(r"[<>]=?\s*(\d{6,})\b", back):
                fail.append(f"{rel}:{i}: abort guarded by the bare literal {lit} — "
                            f"give it a name so this gate can check it")

# ── 4. CROSS-LEG PAIRS: a limit written once per leg must be the same number ──
# Not every shared limit is the 2 GiB ceiling. `read_n_bytes` clamps to a per-call
# maximum that is NOT an allocation ceiling (it is what the wasm host-floor buffer can
# take, measured), but it IS written twice — once as a native constant, once as an .almd
# literal — and that is the same drift shape wearing different clothes. Each pair below
# is checked for EQUALITY only; what the number means is the pair's own business.
PAIRS = [
    # read_n_bytes: the CHUNK the self-host reads in, not a cap on the answer. The
    # answer is min(n, what stdin has) on both legs; the chunk only bounds the wasm
    # buffer. It was briefly a cap, and capping the ANSWER silently truncated a
    # caller's data — the pair is rostered so the two halves cannot drift, and the
    # name says chunk so nobody re-reads it as a limit.
    ("runtime/rs/src/io.rs", "ALMIDE_IO_READ_CHUNK_BYTES",
     "stdlib/io_read_n_bytes.almd", r"remaining > (\d{6,})"),
]
for rs_rel, name, almd_rel, almd_pat in PAIRS:
    rs_p, almd_p = repo / rs_rel, repo / almd_rel
    if not rs_p.exists() or not almd_p.exists():
        fail.append(f"{rs_rel} / {almd_rel}: one half of a declared pair is missing")
        continue
    m = re.search(rf"const\s+{name}\s*:\s*i64\s*=\s*([^;]+);", rs_p.read_text())
    if not m:
        fail.append(f"{rs_rel}: `{name}` not found (renamed? then update the PAIRS roster)")
        continue
    native = ev(m.group(1).strip())
    lits = {int(x) for x in re.findall(almd_pat, almd_p.read_text())}
    if native is None:
        fail.append(f"{rs_rel}: `{name}` is not a literal expression this gate can evaluate")
    elif not lits:
        fail.append(f"{almd_rel}: no clamp literal matched — the self-host half of `{name}` is gone or respelled")
    elif lits != {native}:
        fail.append(f"{name} = {native} but {almd_rel} clamps to {sorted(lits)} — the two legs "
                    f"would read different amounts for the same call")

if fail:
    print(f"::error::alloc-ceiling gate FAILED ({len(fail)} problem(s)):")
    for f in fail:
        print(f"    {f}")
    sys.exit(1)

print(f"alloc-ceilings: OK — {len(NATIVE)} named constants and {seen_selfhost} self-host "
      f"abort guard(s) all denote {LIMIT} bytes (2 GiB, below the wasm address space); "
      f"{len(PAIRS)} cross-leg pair(s) agree.")
PY
