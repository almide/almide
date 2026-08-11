#!/usr/bin/env python3
"""Enumerate the HAND-WRITTEN WAT functions of the wasm prelude (Stage 1c).

Almide's wasm leg is rendered from the same MIR as the native leg, so the 3-way
oracle (native <-> wasm <-> interp) differs the two against each other on every
corpus fixture. The prelude is the exception: `$alloc`, `$rc_dec`, the free-list,
the WASI shims and the rest are hand-written wasm with NO native counterpart to
be differed against. Whatever assurance they carry has to be named explicitly —
which is what `proofs/wat-prelude-audit.toml` does, one row per function.

A unit is a `(func $NAME` occurrence in a renderer source string. Scanning is by
TEXT, over every non-test `crates/almide-mir/src/*.rs`, so a prelude function
added in a new file is picked up without touching this script. Over-inclusion is
safe (a `(func $NAME` in a doc comment or an assertion just earns a row saying
so); under-inclusion is the dangerous direction, and a fixed file list is exactly
how you get it.

`digest` is a hash of the function's source text (from `(func $` to its matching
close paren). It changes iff the hand-written wasm changes, which forces the
row's evidence to be re-confirmed rather than inherited — the same "count
changed => re-audit" trick as the libm ledger.

Output: `name :: file:line :: digest`, sorted.
"""
import glob
import hashlib
import re
import sys

FUNC = re.compile(r"\(func \$([A-Za-z_][A-Za-z0-9_]*)")


def scanned_files(root: str):
    """The wasm renderer sources. `render_wasm/` is the unit-test tree — its
    `(func $…)` occurrences are expectation strings, not emitted wasm."""
    out = []
    for p in sorted(glob.glob(f"{root}/crates/almide-mir/src/*.rs")):
        if p.split("/")[-1].startswith("tests"):
            continue
        out.append(p)
    return out


def _digest(text: str, start: int) -> str:
    """Hash `text[start..]` up to the paren matching the one at `start`. WAT
    comments can hold unbalanced parens, so `;;`-to-newline is skipped; an
    unterminated block (a `(func $…)` mentioned in prose) hashes to the rest of
    the line, which is stable and still change-detecting."""
    depth, i, n = 0, start, len(text)
    while i < n:
        c = text[i]
        if c == ";" and text.startswith(";;", i):
            j = text.find("\n", i)
            if j < 0:
                break
            i = j
            continue
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                body = text[start : i + 1]
                return hashlib.sha256(" ".join(body.split()).encode()).hexdigest()[:12]
        i += 1
    line_end = text.find("\n", start)
    body = text[start : line_end if line_end > 0 else n]
    return hashlib.sha256(" ".join(body.split()).encode()).hexdigest()[:12]


def enumerate_units(root: str):
    """{name: (rel_path:line, digest)}. A name defined more than once keeps the
    FIRST site and folds the rest into the digest, so a duplicate definition is
    visible as a digest change rather than silently shadowing."""
    units = {}
    for path in scanned_files(root):
        rel = path[len(root) + 1 :]
        text = open(path, encoding="utf-8").read()
        for m in FUNC.finditer(text):
            name = m.group(1)
            line = text.count("\n", 0, m.start()) + 1
            d = _digest(text, m.start())
            if name in units:
                prev_site, prev_d = units[name]
                units[name] = (
                    prev_site,
                    hashlib.sha256(f"{prev_d}+{d}".encode()).hexdigest()[:12],
                )
            else:
                units[name] = (f"{rel}:{line}", d)
    return units


if __name__ == "__main__":
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    for name, (site, d) in sorted(enumerate_units(root).items()):
        print(f"{name} :: {site} :: {d}")
