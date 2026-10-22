#!/usr/bin/env python3
"""The memory-unsafe stdlib surface stays closed to `ffi` (R-L0-45, #4008, #4013).

R-L0-45: the only stdlib functions that read or write memory outside an
Almide value are `ffi.read` and `ffi.write`, and no public function outside
`ffi` has `RawPtr` as a parameter or return type. This gate reads the
machine-owned signature index in docs/stdlib/*.md (CI keeps it fresh from the
compiler's own module interface) and asserts:

  1. every public function with `RawPtr` in its signature is in `ffi`, or in
     ALLOWLIST — the deprecated `bytes` spellings R-ffi-01 moves to `ffi`
     (removed at dialect epoch 16). The allowlist only shrinks: an entry that
     is gone from the index, or no longer mentions `RawPtr`, fails until it is
     deleted here.
  2. `ffi`'s public surface is exactly FFI_SURFACE, the four functions of
     R-ffi-01. Adding one to `ffi` fails until this list and R-L0-45's list of
     memory-touching functions are updated in the same PR.

usage: check-rawptr-surface.py [--self-test]
"""
import os, re, sys, glob

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Shrink-only. The fifth deprecated spelling, `bytes.data_ptr`, returns Int and
# so never matches; `--profile critical` refuses all five (#4008).
ALLOWLIST = {'bytes.as_ptr', 'bytes.as_mut_ptr', 'bytes.from_raw_ptr', 'bytes.copy_to_ptr'}

# R-ffi-01. `read` / `write` touch memory (R-L0-45 rule 1); `address` / `to_int`
# build an address as a value.
FFI_SURFACE = {'ffi.address', 'ffi.to_int', 'ffi.read', 'ffi.write'}

SIG = re.compile(r'^(?:effect\s+)?(?:fn\s+)?(?P<mod>[a-z][a-z0-9_]*)\.(?P<name>[a-z_][a-z0-9_]*)(?:\[[^\]]*\])?\((?P<params>.*)\)\s*->\s*(?P<ret>.+?)\s*(?:\(deprecated[^)]*\))?$')
RAWPTR = re.compile(r'\bRawPtr\b')


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
        if not RAWPTR.search(line) or name.startswith('ffi.'): continue
        if name not in ALLOWLIST:
            bad.append(f'{name}: RawPtr outside ffi (R-L0-45) — {line}')
    for name in sorted(ALLOWLIST):
        if name not in have or not RAWPTR.search(have[name]):
            bad.append(f'{name}: allowlisted but no longer a RawPtr signature — delete it from ALLOWLIST (shrink-only)')
    ffi = {n for n in have if n.startswith('ffi.')}
    for name in sorted(ffi - FFI_SURFACE):
        bad.append(f'{name}: new ffi function — add it to FFI_SURFACE and to R-L0-45 rule 1 in the same PR')
    for name in sorted(FFI_SURFACE - ffi):
        bad.append(f'{name}: missing from the signature index (R-ffi-01)')
    return bad


def self_test(have):
    base = len(violations(have))
    assert base == 0, 'self-test needs a clean surface'
    pure = next(n for n, l in have.items() if not n.startswith('ffi.') and n not in ALLOWLIST and not RAWPTR.search(l))
    forged = dict(have); forged['string.leak_ptr'] = 'string.leak_ptr(s: String) -> RawPtr'
    assert len(violations(forged)) == 1, 'a RawPtr return outside ffi was not refused'
    forged = dict(have); forged[pure] = f'{pure}(p: RawPtr) -> Int'
    assert len(violations(forged)) == 1, 'a RawPtr parameter outside ffi was not refused'
    dropped = sorted(ALLOWLIST)[0]
    forged = dict(have); del forged[dropped]
    assert len(violations(forged)) == 1, 'a removed allowlisted fn did not demand the allowlist shrink'
    forged = dict(have); forged['ffi.peek'] = 'ffi.peek(ptr: RawPtr) -> Int'
    assert len(violations(forged)) == 1, 'a new ffi fn was not refused'
    forged = dict(have); del forged['ffi.read']
    assert len(violations(forged)) == 1, 'a missing ffi fn was not refused'
    print('self-test: 5 forged surfaces each refused exactly once (RawPtr return / parameter outside ffi, '
          'allowlist entry gone, new ffi fn, missing ffi fn)')
    return 0


def main():
    have = shipped()
    if not have:
        print('FAIL: no signature index read from docs/stdlib/*.md'); return 2
    if '--self-test' in sys.argv: return self_test(have)
    bad = violations(have)
    for b in bad: print('FAIL:', b)
    if bad: return 1
    n = sum(1 for l in have.values() if RAWPTR.search(l))
    print(f'rawptr surface: OK — {n} RawPtr signatures, {len(FFI_SURFACE)} in ffi, '
          f'{len(ALLOWLIST)} deprecated bytes spellings allowlisted (shrink-only)')
    return 0


sys.exit(main())
