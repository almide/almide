#!/usr/bin/env python3
"""The memory surface of the public stdlib stays closed (R-L0-45, #4008, #4013, ADR-0031).

After ADR-0031 a pointer a program can read through carries its provenance:
the opaque foreign token `Handle` is only passed back to an `@extern` fn, and
the bounds-checked view `Region` belongs to `ffi`. This gate reads the
machine-owned signature index in docs/stdlib/*.md (CI keeps it fresh from the
compiler's own module interface) and asserts:

  1. no public stdlib function has `Handle` (or its removed spelling `RawPtr`)
     in its signature — not even in `ffi`: no stdlib function mints, reads
     through, or converts an address (ADR-0031 D1, D4, D7);
  2. every public function with `Region` in its signature is in `ffi`;
  3. `ffi`'s public surface is exactly FFI_SURFACE. Adding one to `ffi` fails
     until this list and R-L0-45's list of memory-touching functions are
     updated in the same PR.

usage: check-rawptr-surface.py [--self-test]
"""
import os, re, sys, glob

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# ADR-0031. `read` / `write` / `to_bytes` touch memory, after the region's
# bounds check (R-L0-45 rule 1); `region` / `alloc` mint one; `slice`
# narrows one inside its parent's bounds (the 2026-10-29 amendment) without
# touching memory; `len` / `is_writable` read its header.
FFI_SURFACE = {'ffi.region', 'ffi.alloc', 'ffi.slice', 'ffi.len', 'ffi.is_writable', 'ffi.read', 'ffi.write', 'ffi.to_bytes'}

SIG = re.compile(r'^(?:effect\s+)?(?:fn\s+)?(?P<mod>[a-z][a-z0-9_]*)\.(?P<name>[a-z_][a-z0-9_]*)(?:\[[^\]]*\])?\((?P<params>.*)\)\s*->\s*(?P<ret>.+?)\s*(?:\(deprecated[^)]*\))?$')
HANDLE = re.compile(r'\b(?:Handle|RawPtr)\b')
REGION = re.compile(r'\bRegion\b')


def shipped():
    out = {}
    for f in sorted(glob.glob(os.path.join(ROOT, 'docs/stdlib/*.md'))):
        inblk = False
        for ln in open(f):
            if 'BEGIN GENERATED SIGNATURE INDEX' in ln: inblk = True; continue
            if 'END GENERATED SIGNATURE INDEX' in ln: inblk = False; continue
            if not inblk: continue
            m = SIG.match(ln.strip())
            if m: out[f"{m['mod']}.{m['name']}"] = ln.strip()
    return out


def violations(have):
    bad = []
    for name, line in sorted(have.items()):
        if HANDLE.search(line):
            bad.append(f'{name}: a Handle in a public stdlib signature (ADR-0031 D1/D7) — {line}')
        if REGION.search(line) and not name.startswith('ffi.'):
            bad.append(f'{name}: a Region outside ffi (R-L0-45) — {line}')
    ffi = {n for n in have if n.startswith('ffi.')}
    for name in sorted(ffi - FFI_SURFACE):
        bad.append(f'{name}: new ffi function — add it to FFI_SURFACE and to R-L0-45 rule 1 in the same PR')
    for name in sorted(FFI_SURFACE - ffi):
        bad.append(f'{name}: missing from the signature index (R-ffi-01)')
    return bad


def self_test(have):
    base = len(violations(have))
    assert base == 0, 'self-test needs a clean surface'
    pure = next(n for n, l in have.items() if not n.startswith('ffi.') and not REGION.search(l))
    forged = dict(have); forged['string.leak_ptr'] = 'string.leak_ptr(s: String) -> Handle'
    assert len(violations(forged)) == 1, 'a Handle return outside ffi was not refused'
    forged = dict(have); forged['ffi.region'] = 'ffi.region(p: Handle, n: Int) -> Region'
    assert len(violations(forged)) == 1, 'a Handle inside ffi was not refused'
    forged = dict(have); forged[pure] = f'{pure}(r: Region) -> Int'
    assert len(violations(forged)) == 1, 'a Region parameter outside ffi was not refused'
    forged = dict(have); forged['ffi.peek'] = 'ffi.peek(r: Region) -> Int'
    assert len(violations(forged)) == 1, 'a new ffi fn was not refused'
    forged = dict(have); del forged['ffi.read']
    assert len(violations(forged)) == 1, 'a missing ffi fn was not refused'
    print('self-test: 5 forged surfaces each refused exactly once (Handle outside ffi, Handle in ffi, '
          'Region outside ffi, new ffi fn, missing ffi fn)')
    return 0


def main():
    have = shipped()
    if not have:
        print('FAIL: no signature index read from docs/stdlib/*.md'); return 2
    if '--self-test' in sys.argv: return self_test(have)
    bad = violations(have)
    for b in bad: print('FAIL:', b)
    if bad: return 1
    n = sum(1 for l in have.values() if REGION.search(l))
    print(f'memory surface: OK — 0 Handle signatures, {n} Region signatures, all {len(FFI_SURFACE)} in ffi')
    return 0


sys.exit(main())
