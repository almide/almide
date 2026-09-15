#!/usr/bin/env python3
"""Name the pass-order edges a shuffle divergence needs declared (#2186 step 4).

usage: scripts/pass-shuffle-bisect.py <almide-binary> <file.almd> <seed>

The declared order D comes from `target.rs`; the shuffled order O from the
compiler's own `[almide] ALMIDE_SHUFFLE_PASSES=<seed> order:` line. The
inverted pairs are the (a, b) with a before b in D but after it in O. Forcing
every inverted pair as an extra edge (`ALMIDE_PASS_EDGES`) must restore
byte-identity — else the divergence is not an ordering effect — and a greedy
delta-debugging pass then drops every pair that is not needed, leaving the
minimal set to declare (as `depends_on` / `run_before` on the passes named).
"""
import hashlib
import os
import re
import subprocess
import sys


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    almide, path, seed = sys.argv[1:4]
    root = subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True).stdout.strip()

    def emit(env):
        e = dict(os.environ)
        e.update(env)
        r = subprocess.run([almide, path, "--target", "rust"], capture_output=True, text=True, env=e)
        order = [l for l in r.stderr.splitlines() if " order: " in l]
        order = order[0].split(" order: ", 1)[1].split() if order else None
        return hashlib.sha256(r.stdout.encode()).hexdigest(), order

    base, _ = emit({})
    shuffled, order = emit({"ALMIDE_SHUFFLE_PASSES": seed})
    if shuffled == base:
        print(f"no divergence under seed {seed}")
        return 0
    if order is None:
        print("the compiler printed no shuffled order — is ALMIDE_SHUFFLE_PASSES honoured by this binary?", file=sys.stderr)
        return 1
    src = open(os.path.join(root, "crates/almide-codegen/src/target.rs"), encoding="utf-8").read()
    rust_arm = src[src.index("Target::Rust =>"):src.index("Target::Wgsl =>")]
    declared = [m.group(1) for m in re.finditer(r"\.add\(([A-Za-z]+)Pass\)", rust_arm)]
    pos_d = {p: i for i, p in enumerate(declared)}
    pos_o = {p: i for i, p in enumerate(order)}
    inverted = [(a, b) for a in order for b in order
                if a in pos_d and b in pos_d and pos_d[a] < pos_d[b] and pos_o[a] > pos_o[b]]
    print(f"{len(inverted)} inverted pair(s) under seed {seed}")

    def ok(pairs):
        h, _ = emit({"ALMIDE_SHUFFLE_PASSES": seed, "ALMIDE_PASS_EDGES": ",".join(f"{a}<{b}" for a, b in pairs)})
        return h == base

    if not ok(inverted):
        print("forcing every inverted pair did not restore identity — the divergence is not an ordering effect", file=sys.stderr)
        return 1
    keep = list(inverted)
    changed = True
    while changed:
        changed = False
        for p in list(keep):
            trial = [q for q in keep if q != p]
            if ok(trial):
                keep = trial
                changed = True
    print("declare:", ", ".join(f"{a} before {b}" for a, b in keep))
    return 0


if __name__ == "__main__":
    sys.exit(main())
