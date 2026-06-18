#!/usr/bin/env bash
# make-verify demo: when an AI modification is WRONG, does the language make it
# CLEAR and RECOVERABLE — or ship the bug silently?
set -u
here="$(cd "$(dirname "$0")" && pwd)"
echo "########################################################################"
echo "# 1. The correct program — Almide compiles, runs, and the trust spine"
echo "#    re-verifies it (ownership / names / capabilities) per build."
echo "########################################################################"
almide run "$here/shape.almd"
echo
echo "########################################################################"
echo "# 2. An AI was asked to add a Triangle variant. It updated the TYPE but"
echo "#    forgot to handle it in area() — the canonical modification mistake."
echo "#"
echo "#    ALMIDE: caught at compile, with the exact missing case AND the fix."
echo "########################################################################"
almide run "$here/shape_buggy.almd"
echo
echo "########################################################################"
echo "# 3. The SAME mistake in mainstream Python: it compiles, runs, exits 0,"
echo "#    and silently returns None. The bug ships to production."
echo "########################################################################"
echo "\$ python3 shape_buggy.py"
python3 "$here/shape_buggy.py"
echo "(exit $?)"
