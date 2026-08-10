#!/usr/bin/env python3
"""Enumerate the scalar-read audit units (#1183 class, aviation-quality Stage 1).

An audit unit is a FUNCTION in crates/almide-mir/src/lower/ that emits a raw
memory read (`PrimKind::Load` or the `load_at_offset` helper). Each unit must
carry a classification in proofs/scalar-read-audit.toml; the gate
(scripts/check-scalar-read-audit.sh) diffs this enumeration against the ledger,
so the SAME scan feeds both the audit and the gate — one instrument, both
sides (the #1176 lesson).

Output: one line per unit, `file :: fn :: load_count`, sorted.
"""
import glob
import re
import sys

FN_RE = re.compile(r"\s*(?:pub(?:\(crate\))?\s+)?fn\s+([A-Za-z_0-9]+)")


def enumerate_units(root: str):
    units = {}
    for path in sorted(glob.glob(f"{root}/crates/almide-mir/src/lower/*.rs")):
        fn = None
        for line in open(path, encoding="utf-8"):
            m = FN_RE.match(line)
            if m:
                fn = m.group(1)
            if "PrimKind::Load" in line or "load_at_offset" in line:
                if "fn load_at_offset" in line:
                    continue
                key = (path.split("/")[-1], fn or "?")
                units[key] = units.get(key, 0) + 1
    return units


if __name__ == "__main__":
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    for (f, fn), n in sorted(enumerate_units(root).items()):
        print(f"{f} :: {fn} :: {n}")
