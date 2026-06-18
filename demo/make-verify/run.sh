#!/usr/bin/env bash
# make-verify demo: when an AI modification is WRONG, does the language make the
# failure CLEAR and RECOVERABLE at author/CI time — or pass its compile gate and
# defer the failure to runtime (a silent wrong value, or a crash)?
set -u
here="$(cd "$(dirname "$0")" && pwd)"
sep() { printf '\n########################################################################\n# %s\n########################################################################\n' "$1"; }

sep "1. NON-EXHAUSTIVE MATCH — AI added a Triangle variant, forgot the area arm"
echo "--- Almide: caught at compile, with the exact missing case AND the fix ---"
almide run "$here/shape_buggy.almd" 2>&1 | sed -n '1,9p'
echo "--- Python: compiles, runs, exits 0, prints 'None' (a SILENT WRONG VALUE to the user) ---"
python3 "$here/shape_buggy.py"

sep "2. MISSED CALL SITE — AI added a discount param, updated 1 of 2 call sites"
echo "--- Almide: caught at compile (arity), with the full corrected signature ---"
almide run "$here/cart_buggy.almd" 2>&1 | sed -n '1,9p'
echo "--- Python: py_compile is GREEN (silent at CI); the bug is a deferred runtime crash ---"
python3 -m py_compile "$here/cart_buggy.py" && echo "py_compile: OK (no error — ships)"
python3 "$here/cart_buggy.py" 2>&1 | tail -1

sep "3. NONE MISHANDLE — AI used absent-able lookup results without handling none"
echo "--- Almide: caught at compile (Option[Int] is not a number) ---"
almide run "$here/lookup_buggy.almd" 2>&1 | sed -n '1,7p'
echo "--- Python: py_compile GREEN; deferred runtime TypeError on the absent path ---"
python3 -m py_compile "$here/lookup_buggy.py" && echo "py_compile: OK (no error — ships)"
python3 "$here/lookup_buggy.py" 2>&1 | tail -1

printf '\n========================================================================\n'
echo "Almide caught ALL three at COMPILE (before any run), each with an actionable"
echo "diagnostic. Python's compile gate caught NONE — one shipped a silently wrong"
echo "value, two deferred to runtime. That is the trust difference, independent of"
echo "which model wrote the change."
