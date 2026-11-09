# Per-function body sizes of a wasm module (name section names), sorted.
import sys
def uleb(b, p):
    r = s = 0
    while True:
        x = b[p]; p += 1; r |= (x & 0x7f) << s; s += 7
        if x < 0x80: return r, p
b = open(sys.argv[1], 'rb').read(); p = 8
nimp = 0; bodies = []; names = {}
while p < len(b):
    sid = b[p]; p += 1; size, p = uleb(b, p); end = p + size
    if sid == 2:
        n, q = uleb(b, p)
        for _ in range(n):
            l, q = uleb(b, q); q += l; l, q = uleb(b, q); q += l
            k = b[q]; q += 1
            if k == 0: _, q = uleb(b, q); nimp += 1
            elif k == 1: q += 1; f = b[q]; q += 1; _, q = uleb(b, q); q = (uleb(b, q)[1] if f & 1 else q)
            elif k == 2: f = b[q]; q += 1; _, q = uleb(b, q); q = (uleb(b, q)[1] if f & 1 else q)
            elif k == 3: q += 2
    elif sid == 10:
        n, q = uleb(b, p)
        for _ in range(n):
            l, q2 = uleb(b, q); bodies.append(l + (q2 - q)); q = q2 + l
    elif sid == 0:
        l, q = uleb(b, p); nm = b[q:q + l].decode(); q += l
        if nm == 'name':
            while q < end:
                sub = b[q]; q += 1; sl, q = uleb(b, q); se = q + sl
                if sub == 1:
                    n, r = uleb(b, q)
                    for _ in range(n):
                        i, r = uleb(b, r); l2, r = uleb(b, r); names[i] = b[r:r + l2].decode('utf-8', 'replace'); r += l2
                q = se
    p = end
rows = [(sz, names.get(nimp + i, '#%d' % (nimp + i))) for i, sz in enumerate(bodies)]
print("total code bytes", sum(s for s, _ in rows), "functions", len(rows), "file", len(b))
for sz, nm in sorted(rows, reverse=True):
    print(sz, nm)
