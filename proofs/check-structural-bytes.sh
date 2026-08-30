#!/usr/bin/env bash
# #576 slice-3 GROUNDING gate — the structural twin of check-wasm-bytes.sh.
#
# StructuralDecode.v proves `decode(bytes) = the trees` for BYTE LISTS
# written in the .v source. Those theorems mean something only if the
# lists are the bytes the emitter actually produces. This gate closes
# that with ZERO hand-copied constants: it dumps the emitter's real
# `$inc`/`$dec_flat`/`$free`/`$alloc` code-section bodies (the same
# `dump_runtime_bytes` helper that produced the lists), strips the
# code-section wrapper and locals vector, parses the lists OUT of
# StructuralDecode.v, and diffs.
#
# Tool policy (as check-wasm-bytes.sh, #921): locally a missing tool is
# an honest skip; in CI it is a FAILURE.
set -euo pipefail
cd "$(dirname "$0")"
ROOT=".."

require_or_skip() {
  command -v "$1" >/dev/null 2>&1 && return 0
  if [ "${CI:-}" = "true" ]; then
    echo "check-structural-bytes: $1 not found — FAIL (CI must install it)"
    exit 1
  fi
  echo "check-structural-bytes: $1 not found — SKIP (grounding not re-checked here)"
  exit 0
}
require_or_skip cargo
require_or_skip python3

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

# 1. The emitter's real bytes, through its own dump helper.
( cd "$ROOT" && cargo test -q -p almide-wasm --lib dump_runtime_bytes -- --ignored --nocapture 2>/dev/null ) \
  | grep -E "^(inc|dec_flat|free|alloc): " > "$tmp/dump.txt" \
  || { echo "FAIL: the byte dump produced nothing"; exit 1; }

# 2. Strip wrappers, parse the .v lists, diff.
python3 - "$tmp/dump.txt" StructuralDecode.v <<'PY'
import re
import sys

dump_path, v_path = sys.argv[1], sys.argv[2]

def read_leb(bs, i):
    # unsigned LEB128 (lengths/counts can exceed one byte — $alloc's do)
    v, shift = 0, 0
    while True:
        b = bs[i]; i += 1
        v |= (b & 0x7F) << shift
        shift += 7
        if b < 128:
            return v, i

def strip_wrapper(bytes_):
    # [section_len, fn_count, body_len, decl_count, (count, type)*decls, body...]
    i = 0
    for _ in range(3):
        _, i = read_leb(bytes_, i)  # section_len, fn_count, body_len
    decls, i = read_leb(bytes_, i)
    for _ in range(decls):
        _, i = read_leb(bytes_, i)  # local-run count
        i += 1                      # value type
    return bytes_[i:]

emitted = {}
for line in open(dump_path):
    name, rest = line.split(":", 1)
    raw = [int(x) for x in re.findall(r"\d+", rest)]
    key = {"inc": "inc", "dec_flat": "dec", "free": "free", "alloc": "alloc"}[name]
    emitted[key] = strip_wrapper(raw)

v = open(v_path).read()
proven = {}
for name in ("inc", "dec", "free", "alloc"):
    m = re.search(rf"Definition {name}_bytes : list Z :=\s*\[([^]]*)\]", v)
    if not m:
        sys.exit(f"FAIL: {name}_bytes not found in StructuralDecode.v")
    proven[name] = [int(x) for x in re.findall(r"\d+", m.group(1))]

fail = 0
for name in ("inc", "dec", "free", "alloc"):
    if emitted[name] == proven[name]:
        print(f"ok   {name}: {len(proven[name])} bytes — emitter == StructuralDecode.v")
    else:
        print(f"FAIL {name}: emitter {emitted[name]} != proven {proven[name]}")
        fail = 1
sys.exit(fail)
PY

echo "STRUCTURAL BYTES OK: the decode theorems are about the emitter's real bytes."
