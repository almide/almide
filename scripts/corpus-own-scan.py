import sys, collections
own = open(sys.argv[1]).read().split('\n')
names = open(sys.argv[2]).read().split('\n') if len(sys.argv) > 2 else []
def why(line):
    lp = cp = bp = False
    for c in line:
        if bp:
            if c == '}': bp = False
            continue
        if c in 'xXtT': return None  # existing poison, not new
        if cp:
            if c == ']': cp = False
            continue
        if c == '{': bp = True
        elif c == '[': cp = True
        elif c == '(':
            if lp: return 'reopened ('
            lp = True
        elif c == ')':
            if not lp: return 'stray )'
            lp = False
        elif c in '|}]': return 'stray ' + c
    if bp: return 'unterminated {'
    if cp: return 'unterminated ['
    if lp: return 'unterminated ('
    return None
cnt = collections.Counter(); ex = {}
for i, l in enumerate(own):
    w = why(l)
    if w:
        cnt[w] += 1
        ex.setdefault(w, []).append((l[:120], names[i] if i < len(names) else '?'))
print("lines", len(own), "newly poisoned", sum(cnt.values()), dict(cnt))
for k, v in ex.items():
    for l, n in v[:8]:
        print(" ", k, "|", repr(l), "|", n)
