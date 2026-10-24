#!/usr/bin/env python3
"""Distance from the shipped stdlib surface to the redesign's target surface.

The target is docs/roadmap/stdlib-redesign/target-surface.json (generated with
target-surface.md from the L2 verdicts). The shipped surface is the
machine-owned signature index in docs/stdlib/*.md, which CI keeps fresh from
the compiler's own module interface. Comparing the two needs no build.

Four counts, each a ratchet in proofs/stdlib-target-distance.toml that may
only go DOWN:

  missing     a target function the stdlib does not have yet
  shape       a target function present under its name with another
              signature (arity, parameter names, types, effect)
  undeprecated  a function the redesign removes or renames that is still
              public without `@deprecated` (epoch 15 opens the window)
  lang_wait   unshipped target functions whose target form needs a protocol
              feature (#4000) — a built-in type conforming to a protocol
              (`list.sum[A: Numeric]`). Counted, never failed: R-L0-37 makes
              the pyramid an assumed design. Once shipped, a function is
              measured like any other.

usage: check-stdlib-target.py [--list missing|shape|undeprecated|lang_wait] [--write] [--self-test]
"""
import json, os, re, sys, glob, collections

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = os.path.join(ROOT, 'docs/roadmap/stdlib-redesign/target-surface.json')
RATCHET = os.path.join(ROOT, 'proofs/stdlib-target-distance.toml')
# protocols a built-in type would have to conform to; the language cannot say that yet (#4000)
LANG_WAIT = {'Numeric', 'Integer', 'Text', 'FloatingPoint', 'Ord', 'Hash', 'Parse'}

def split_top(s):
    out, dep, cur = [], 0, ''
    for c in s:
        dep += c in '([{'; dep -= c in ')]}'
        if c == ',' and dep == 0: out.append(cur.strip()); cur = ''
        else: cur += c
    if cur.strip(): out.append(cur.strip())
    return out

def opt(t):
    # Option[X] and X? are one type (R-L0-24); compare the short form
    t = t.strip()
    while True:
        m = re.search(r'Option\[', t)
        if not m: return t
        i = m.end(); dep = 1; k = i
        while dep:
            dep += t[k] == '['; dep -= t[k] == ']'; k += 1
        inner = t[i:k - 1]
        if '->' in inner: inner = f'({inner})'
        t = t[:m.start()] + inner + '?' + t[k:]

SIG = re.compile(r'^(?P<eff>effect\s+)?(?:fn\s+)?(?P<mod>[a-z][a-z0-9_]*)\.(?P<name>[a-z_][a-z0-9_]*)(?P<gen>\[[^\]]*\])?\((?P<params>.*)\)\s*->\s*(?P<ret>.+?)\s*(?P<dep>\(deprecated[^)]*\))?$')

def shape(m):
    ps = []
    for p in split_top(m['params']):
        mm = re.match(r'^(mut\s+)?(\w+)\s*:\s*(.*)$', p)
        ps.append((bool(mm and mm[1]), mm[2] if mm else p, opt(mm[3]) if mm else p))
    ret = opt(m['ret'])
    # an effect fn's index line may spell the Result it declares; the target spells the success type
    if m['eff']:
        r = re.match(r'^Result\[(.*), String\]$', ret)
        if r: ret = r[1]
    return dict(effect=bool(m['eff']), params=ps, ret=ret)

def shipped():
    out = {}
    for f in sorted(glob.glob(os.path.join(ROOT, 'docs/stdlib/*.md'))):
        inblk = False
        for ln in open(f):
            if 'BEGIN GENERATED SIGNATURE INDEX' in ln: inblk = True; continue
            if 'END GENERATED SIGNATURE INDEX' in ln: inblk = False; continue
            if not inblk: continue
            m = SIG.match(ln.strip())
            if m: out[(m['mod'], m['name'])] = dict(shape(m), deprecated=bool(m['dep']), line=ln.strip())
    return out

def measure(t, have):
    res = collections.defaultdict(list)
    for r in t['after']:
        key = (r['mod'], r['name'])
        m = SIG.match(r['sig'])
        gen = (m['gen'] or '') if m else ''
        bounds = set(re.findall(r':\s*(\w+)', gen))
        # waiting on the language only while it is not shipped: a shipped
        # protocol-generic function is measured like any other
        if bounds & LANG_WAIT and key not in have:
            res['lang_wait'].append(f"{r['mod']}.{r['name']}  {r['sig']}"); continue
        if key not in have:
            res['missing'].append(f"{r['mod']}.{r['name']}  [{r['kind']}]  {r['sig']}"); continue
        if not m: continue
        want, got = shape(m), have[key]
        diff = []
        if want['effect'] != got['effect']: diff.append('effect')
        if len(want['params']) != len(got['params']): diff.append('arity')
        else:
            for (wm, wn, wt), (gm, gn, gt) in zip(want['params'], got['params']):
                if wm != gm: diff.append(f'mut {wn}')
                if wn != gn: diff.append(f'name {gn}->{wn}')
                if wt.replace(' ', '') != gt.replace(' ', ''): diff.append(f'type {wn}: {gt}->{wt}')
        if want['ret'].replace(' ', '') != got['ret'].replace(' ', ''): diff.append(f"ret {got['ret']}->{want['ret']}")
        if diff: res['shape'].append(f"{r['mod']}.{r['name']}  " + '; '.join(diff))
    target_keys = {(r['mod'], r['name']) for r in t['after']}
    for mod, rows in t['removed'].items():
        for x in rows:
            key = (mod, x['name'])
            if key in have and key not in target_keys and not have[key]['deprecated']:
                res['undeprecated'].append(f"{mod}.{x['name']}  → {x['to']}")
    return res

def self_test(t, have):
    # negative controls: each forged surface must raise exactly the count it forges
    base = {k: len(v) for k, v in measure(t, have).items()}
    def count(h, k): return len(measure(t, h)[k])
    ok_key = next(k for k in have if any((r['mod'], r['name']) == k for r in t['after']) and not any(k[0] + '.' + k[1] + ' ' in x for x in measure(t, have)['shape']))
    forged = dict(have); del forged[ok_key]
    assert count(forged, 'missing') == base.get('missing', 0) + 1, 'a deleted target fn was not counted missing'
    forged = dict(have); g = dict(have[ok_key]); g['effect'] = not g['effect']; forged[ok_key] = g
    assert count(forged, 'shape') == base.get('shape', 0) + 1, 'a flipped effect was not counted as a shape difference'
    rm = next((m, x['name']) for m, xs in t['removed'].items() for x in xs if (m, x['name']) not in have)
    forged = dict(have); forged[rm] = dict(effect=False, params=[], ret='Unit', deprecated=False, line='')
    assert count(forged, 'undeprecated') == base.get('undeprecated', 0) + 1, 'a removed fn without @deprecated was not counted'
    forged[rm] = dict(forged[rm], deprecated=True)
    assert count(forged, 'undeprecated') == base.get('undeprecated', 0), 'a deprecated removed fn was counted'
    print('self-test: 4 forged surfaces each moved exactly their count (deleted fn, flipped effect, removed fn undeprecated / deprecated)')
    return 0

def main():
    t = json.load(open(TARGET))
    have = shipped()
    if not have: print('FAIL: no signature index read from docs/stdlib/*.md'); return 2
    if '--self-test' in sys.argv: return self_test(t, have)
    res = measure(t, have)
    counts = {k: len(res[k]) for k in ('missing', 'shape', 'undeprecated', 'lang_wait')}
    if '--list' in sys.argv:
        for x in res[sys.argv[sys.argv.index('--list') + 1]]: print(x)
        return 0
    print('stdlib target distance:', ', '.join(f'{k} {v}' for k, v in counts.items()), f"(target {len(t['after'])} functions)")
    if '--write' in sys.argv:
        open(RATCHET, 'w').write('# Shrink-only distance from docs/stdlib/*.md to the stdlib redesign target (scripts/check-stdlib-target.py).\n'
                                 + ''.join(f'{k} = {v}\n' for k, v in counts.items()))
        return 0
    if not os.path.exists(RATCHET):
        print('no ratchet file; run with --write'); return 1
    base = dict(re.findall(r'^(\w+) = (\d+)$', open(RATCHET).read(), re.M))
    bad = [k for k in ('missing', 'shape', 'undeprecated') if counts[k] > int(base.get(k, 0))]
    for k in ('missing', 'shape', 'undeprecated'):
        if counts[k] < int(base.get(k, 0)):
            print(f'  {k} went down {base[k]} -> {counts[k]}: lower the ratchet (--write)')
    if bad:
        print('FAIL: grew past the ratchet:', ', '.join(f'{k} {base.get(k)} -> {counts[k]}' for k in bad)); return 1
    return 0

sys.exit(main())
