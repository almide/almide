# Fan floor ledger runner (docs/benchmarks/fan-floor.md): every probe x
# iters x implementation, the per-offer cost as t(big offers) - t(small
# offers) in us, min of REPS with the implementations interleaved. Prints one
# line per (probe, iters). Run from this directory after ./build.sh;
# ALMIDE = the almide binary under test, ALMIDE_DEV = an optional second one
# (the `al-n-dev` column).
import subprocess, time, os, sys
A = os.path.expanduser(os.environ.get('ALMIDE', 'almide'))
OLD = os.environ.get('ALMIDE_DEV')
REPS = int(os.environ.get('REPS', '7'))
PROBES = {  # probe: (iters list, small offers, big offers, offers per unit for us/offer)
    'sweep': ([0, 100, 300, 1000, 3000, 10000, 30000], 10, 1010),
    'nest':  ([0, 100, 1000, 10000], 10, 210),
    'busy':  ([0, 100, 1000, 10000], 10, 510),
}
ONLY = sys.argv[1:] or list(PROBES)
os.makedirs('gen', exist_ok=True)
def src(probe, it, off):
    f = f'gen/{probe}_{it}_{off}.almd'
    if not os.path.exists(f):
        open(f, 'w').write(open(f'almide/{probe}.almd.tmpl').read().replace('__IT__', str(it)).replace('__OFF__', str(off)))
    return f
def nbin(probe, it, off, al=None):
    al = al or A
    b = f'gen/n_{probe}_{it}_{off}' + ('_dev' if al == OLD else '')
    if not os.path.exists(b):
        r = subprocess.run([al, 'build', '--release', src(probe, it, off), '-o', b], capture_output=True, text=True)
        if r.returncode: print('BUILD FAIL', b, r.stderr[-400:], flush=True); open(b, 'w').write('#!/bin/sh\nexit 1\n'); os.chmod(b, 0o755)
    return b
SEQ_N = {'ALMIDE_FAN_SEQUENTIAL': '1'}; SEQ_W = {'ALMIDE_FAN_PAR_OFF': '1'}; PAR = {'ALMIDE_FAN_COST_OFF': '1'}
def impls(probe, it, off):
    return [
        ('al-n-seq',   [nbin(probe, it, off)], SEQ_N),
        ('al-n',       [nbin(probe, it, off)], {}),
    ] + ([('al-n-dev', [nbin(probe, it, off, OLD)], {})] if OLD else []) + [
        ('al-n-par',   [nbin(probe, it, off)], PAR),
        ('al-w-seq',   [A, 'run', src(probe, it, off), '--target', 'wasm'], SEQ_W),
        ('al-w',       [A, 'run', src(probe, it, off), '--target', 'wasm'], {}),
        ('al-w-par',   [A, 'run', src(probe, it, off), '--target', 'wasm'], PAR),
        ('rayon-seq',  ['rust/target/release/floor', 'seq', probe, str(it), str(off)], {}),
        ('rayon',      ['rust/target/release/floor', 'par', probe, str(it), str(off)], {}),
        ('go-seq',     ['go/floor', 'seq', probe, str(it), str(off)], {}),
        ('go',         ['go/floor', 'par', probe, str(it), str(off)], {}),
        ('zig-seq',    ['zig/floor', 'seq', probe, str(it), str(off)], {}),
        ('zig',        ['zig/floor', 'par', probe, str(it), str(off)], {}),
    ]
def once(cmd, env):
    s = time.perf_counter(); r = subprocess.run(cmd, env=dict(os.environ, **env), capture_output=True, text=True)
    return time.perf_counter() - s, (r.stdout.strip() if r.returncode == 0 else 'ERR ' + r.stderr.strip()[-120:])
for probe in ONLY:
    its, lo, hi = PROBES[probe]
    for it in its:
        names = [n for n, _, _ in impls(probe, it, lo)]
        best = {(n, o): 1e9 for n in names for o in (lo, hi)}; outs = {o: {} for o in (lo, hi)}
        for _ in range(REPS):
            for o in (lo, hi):
                for n, cmd, env in impls(probe, it, o):
                    d, out = once(cmd, env); best[(n, o)] = min(best[(n, o)], d); outs[o][n] = out
        us = {n: (best[(n, hi)] - best[(n, lo)]) / (hi - lo) * 1e6 for n in names}
        agree = all(len(set(outs[o].values())) == 1 for o in (lo, hi))
        bad = '' if agree else ' MISMATCH ' + repr(outs[hi])
        print(f'floor {probe:5} iters={it:<6} ' + ' '.join(f'{n}={us[n]:8.2f}' for n in names) + f' agree={agree}{bad}', flush=True)
