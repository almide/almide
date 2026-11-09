#!/usr/bin/env python3
"""Write the sequential twin of a probe: every `let (a, b, ...) = fan { f()! ... }`
tuple block becomes the arms run one after another, each Result held, then
unwrapped in arm order -- the reading C-199 gives the block (every arm runs,
the first Err in list order surfaces). Arms are assumed to return
Result[<T>, String] where <T> is given by a `// @armty: T` line (default Int)."""
import re, sys

src = open(sys.argv[1]).read()
armty = (re.search(r"//\s*@armty:\s*(.+)", src) or [None, "Int"])[1].strip()
pat = re.compile(r"^(?P<ind>[ \t]*)let \((?P<names>[^)]*)\) = fan \{\n(?P<body>.*?)\n(?P=ind)\}", re.S | re.M)
n = [0]

def repl(m):
    ind = m["ind"]
    names = [x.strip() for x in m["names"].split(",")]
    arms = [a.strip() for a in m["body"].splitlines() if a.strip()]
    assert len(arms) == len(names), (names, arms)
    out = []
    held = []
    for a in arms:
        n[0] += 1
        v = f"__seq_r{n[0]}"
        held.append(v)
        call = a[:-1] if a.endswith("!") else a
        out.append(f"{ind}let {v}: Result[{armty}, String] = {call}")
    for name, v in zip(names, held):
        out.append(f"{ind}let {name} = {v}!")
    return "\n".join(out)

out = pat.sub(repl, src)
if out == src:
    sys.exit(f"{sys.argv[1]}: no tuple fan block found")
sys.stdout.write(out)
