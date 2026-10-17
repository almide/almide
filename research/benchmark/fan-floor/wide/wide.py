# Fan floor, wide ledger runner (docs/benchmarks/fan-floor.md, "Wide shapes"):
# for each row (probe, n, iters) and each implementation, the per-map cost
# t(R reps) - t(1 rep) over R - 1, in us, min of REPS runs with the
# implementations interleaved; max RSS (KB, GNU time) of the R-rep run; and
# per implementation the startup cost (n=1, iters=0, one rep). Every
# implementation runs every element and keeps the lowest-index error, and
# all must print the same checksum (agree=). Run from this directory after
# ./build.sh; ALMIDE = the almide binary under test. Prints one line per row
# and appends a JSON record per row to wide-results.jsonl.
import subprocess, time, os, sys, json
A = os.path.expanduser(os.environ.get('ALMIDE', 'almide'))
REPS = int(os.environ.get('REPS', '7'))
WASM = os.environ.get('WASM', '1') == '1'
TARGET = float(os.environ.get('TARGET', '1.5e8'))  # work units per R-rep run
ROWS = [('size', n, 100) for n in (10, 100, 1000, 10**4, 10**5, 10**6)] + \
       [('size', n, 10000) for n in (10, 100, 1000, 10**4)] + \
       [('skew', n, 1000) for n in (1000, 10**5)] + \
       [('heap', n, 0) for n in (1000, 10**5)] + \
       [('fail', n, 1000) for n in (1000, 10**5)]
ONLY = sys.argv[1:]
os.makedirs('gen', exist_ok=True)
def reps_for(probe, n, it):
    units = n * (it * (2.9 if probe == 'skew' else 1) if probe != 'heap' else 300)
    return int(max(3, min(3000, TARGET / max(units, 1))))
def src(probe, n, it, r):
    f = f'gen/{probe}_{n}_{it}_{r}.almd'
    if not os.path.exists(f):
        t = open(f'almide/{probe}.almd.tmpl').read()
        open(f, 'w').write(t.replace('__N__', str(n)).replace('__IT__', str(it)).replace('__REPS__', str(r)))
    return f
def nbin(probe, n, it, r):
    b = f'gen/n_{probe}_{n}_{it}_{r}'
    if not os.path.exists(b):
        x = subprocess.run([A, 'build', '--release', src(probe, n, it, r), '-o', b], capture_output=True, text=True)
        if x.returncode: print('BUILD FAIL', b, x.stderr[-400:], flush=True); open(b, 'w').write('#!/bin/sh\nexit 1\n'); os.chmod(b, 0o755)
    return b
def impls(probe, n, it, r):
    a = [str(x) for x in (probe, n, it, r)]
    L = [('al-n-seq', [nbin(probe, n, it, r)], {'ALMIDE_FAN_SEQUENTIAL': '1'}),
         ('al-n',     [nbin(probe, n, it, r)], {})]
    if WASM:
        L.append(('al-w', [A, 'run', src(probe, n, it, r), '--target', 'wasm'], {}))
    for i in ('seq', 'rayon', 'tuned', 'scope'): L.append((f'rs-{i}', ['rust/target/release/wide', i] + a, {}))
    for i in ('seq', 'par', 'pool', 'errgroup'): L.append((f'go-{i}', ['go/wide', i] + a, {}))
    for i in ('seq', 'std', 'hand'): L.append((f'zig-{i}', ['zig/wide', i] + a, {}))
    return L
def once(cmd, env):
    s = time.perf_counter(); r = subprocess.run(cmd, env=dict(os.environ, **env), capture_output=True, text=True)
    return time.perf_counter() - s, (r.stdout.strip() if r.returncode == 0 else 'ERR ' + r.stderr.strip()[-120:])
def rss(cmd, env):
    r = subprocess.run((['/usr/bin/time', '-f', '%M'] if sys.platform != 'darwin' else ['/usr/bin/time', '-l']) + cmd, env=dict(os.environ, **env), capture_output=True, text=True)
    try: return int(r.stderr.strip().splitlines()[-1])
    except Exception: return -1
log = open('wide-results.jsonl', 'a')
if not ONLY or 'startup' in ONLY:
    names = [m for m, _, _ in impls('size', 1, 0, 1)]
    best = {m: 1e9 for m in names}
    for _ in range(REPS):
        for m, cmd, env in impls('size', 1, 0, 1):
            best[m] = min(best[m], once(cmd, env)[0])
    print('startup ' + ' '.join(f'{m}={best[m]*1e3:7.2f}ms' for m in names), flush=True)
    log.write(json.dumps({'row': 'startup', 'ms': {m: best[m] * 1e3 for m in names}}) + '\n'); log.flush()
for probe, n, it in ROWS:
    if ONLY and probe not in ONLY: continue
    R = reps_for(probe, n, it)
    names = [m for m, _, _ in impls(probe, n, it, 1)]
    best = {(m, r): 1e9 for m in names for r in (1, R)}; outs = {r: {} for r in (1, R)}
    for _ in range(REPS):
        for r in (1, R):
            for m, cmd, env in impls(probe, n, it, r):
                d, o = once(cmd, env); best[(m, r)] = min(best[(m, r)], d); outs[r][m] = o
    us = {m: (best[(m, R)] - best[(m, 1)]) / (R - 1) * 1e6 for m in names}
    mem = {m: rss(cmd, env) for m, cmd, env in impls(probe, n, it, R)}
    agree = all(len(set(outs[r].values())) == 1 for r in (1, R))
    bad = '' if agree else ' MISMATCH ' + repr(outs[R])
    print(f'wide {probe:4} n={n:<7} iters={it:<5} R={R:<5} ' + ' '.join(f'{m}={us[m]:10.2f}' for m in names) + f' agree={agree}{bad}', flush=True)
    print(f'rss  {probe:4} n={n:<7} iters={it:<5}         ' + ' '.join(f'{m}={mem[m]}' for m in names), flush=True)
    log.write(json.dumps({'row': [probe, n, it], 'R': R, 'us': us, 'rss_kb': mem, 'agree': agree}) + '\n'); log.flush()
