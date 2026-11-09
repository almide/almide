#!/usr/bin/env python3
"""#4005 A4 probe (gates nothing): witness mutation lab.

1. Build the first LAB_N certified fixtures, produce each bundle, judge every
   witness with the extracted checker; list the 64 smallest accepted
   witnesses with their decoded op count (ownership) / token shape.
2. Mutation matrix: witnesses with >= 2 RC ops (ownership) and with numbered
   accesses (bounds); each real mutation judged by the extracted checker,
   almide-verify and the Rocq kernel (a `= false` goal, its own coqc run);
   a mutation any judge accepts is also put back into its bundle and the
   anchor check judged on the shipped bytes.
"""
import os
import re
import subprocess
import sys

ROOT = os.getcwd()
TD = os.environ.get("CARGO_TARGET_DIR", os.path.join(ROOT, "target")) + "/debug"
ALMIDE, PRODUCER = TD + "/almide", TD + "/examples/emit_structural_bundle"
VERIFY, ANCHOR = TD + "/almide-verify", TD + "/almide-anchor-check"
CHECKER = os.path.join(ROOT, "proofs", "checker")
OUT = os.path.join(ROOT, "mut-out")
os.makedirs(OUT, exist_ok=True)
N = int(os.environ.get("LAB_N", "200"))
FN = {"ownership": "check_xc", "names": "check_names_cert", "caps": "check_caps_cert",
      "caps-transitive": "check_prog_cert", "call-modes": "check_modes_cert",
      "bounds": "check_lbounds_cert", "init": "check_init_cert", "fields": "check_fields_cert", "rc-sig": "check_sig_cert"}
OPS = set("iadmrbsIADMRBS")


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def parse(path):
    b = open(path, "rb").read()
    i, recs = b.index(b"\n") + 1, []
    while i < len(b):
        j = b.index(b"\n", i)
        line = b[i:j].decode()
        start = j + 1
        i = start
        if line.startswith("witness "):
            _, prop, n, fn = line.split(" ", 3)
            n = int(n)
            recs.append((prop, fn, b[start:start + n].decode(), j, start, n))
            i += n + 1
    return b, recs


def judge_checker(prop, w):
    p = os.path.join(OUT, "one.w")
    open(p, "w").write(w)
    return run([CHECKER, prop, p]).returncode


def judge_verify(prop, w):
    p = os.path.join(OUT, "one.w")
    open(p, "w").write(w)
    return run([VERIFY, prop, p]).returncode


def judge_kernel(prop, w, tag):
    """0 = the kernel proves `FN w = false` (rejects); 1 = it does not."""
    name = "MutK_" + re.sub(r"[^A-Za-z0-9_]", "_", tag)
    path = os.path.join(ROOT, "proofs", name + ".v")
    lit = '"' + w.replace('"', '""') + '"'
    open(path, "w").write(
        "From AlmideTrust Require Import OwnershipChecker NameTotality CapabilityBound CapabilityReach CallModes HelperSig BoundsCert InitCert FieldCert RoutineBoundsCert RoutineProcsCert RoutineLinCert.\n"
        "From Stdlib Require Import String List.\nOpen Scope string_scope.\n"
        f"Goal {FN[prop]} {lit} = false.\nProof. vm_compute. reflexivity. Qed.\n")
    r = run(["coqc", "-Q", ".", "AlmideTrust", name + ".v"], cwd=os.path.join(ROOT, "proofs"))
    for ext in (".v", ".vo", ".vos", ".vok", ".glob"):
        try:
            os.remove(os.path.join(ROOT, "proofs", name + ext))
        except OSError:
            pass
    if r.returncode != 0 and "Error" in r.stderr and "reflexivity" not in r.stderr and "Unable to unify" not in r.stderr:
        print(f"    kernel error ({tag}): {r.stderr.strip()[:200]}")
    return 0 if r.returncode == 0 else 1


fixtures = [l.strip() for l in open("crates/almide-wasm/tests/golden/witness-fixtures.txt") if l.strip() and not l.startswith("#")][:N]
built = []
for k, f in enumerate(fixtures):
    w, a, bnd = (os.path.join(OUT, f"{k}.{e}") for e in ("wasm", "anchors", "bundle"))
    if run([ALMIDE, "build", f, "--target", "wasm", "-o", w]).returncode != 0:
        continue
    r = run([PRODUCER, f, "--artifact", w, "--anchors", a])
    if r.returncode != 0:
        continue
    open(bnd, "w").write(r.stdout)
    built.append((k, f))
print(f"== built {len(built)} of {len(fixtures)} fixtures")

allw = []
for k, f in built:
    _, recs = parse(os.path.join(OUT, f"{k}.bundle"))
    for idx, (prop, fn, w, *_rest) in enumerate(recs):
        allw.append((k, f, idx, prop, fn, w))

print("== the 64 smallest accepted witnesses (property, length, ops, text)")
acc = []
for rec in sorted(allw, key=lambda r: len(r[5])):
    k, f, idx, prop, fn, w = rec
    if not w.strip() or prop not in FN:
        continue
    if judge_checker(prop, w) == 0:
        acc.append(rec)
    if len(acc) == 64:
        break
for k, f, idx, prop, fn, w in acc:
    ops = sum(1 for c in w if c in OPS) if prop == "ownership" else "-"
    print(f"  {prop:12} len={len(w):3} ops={ops} {w!r:.70} [{os.path.basename(f)} {fn[:30]}]")


def mutations(prop, w):
    out = []
    if prop == "ownership":
        if "i" in w:
            out.append(("flip i->d", w.replace("i", "d", 1)))
        if "d" in w:
            j = w.index("d")
            out.append(("drop a d", w[:j] + w[j + 1:]))
        letters = [j for j, c in enumerate(w) if c in OPS]
        for x, y in zip(letters, letters[1:]):
            if w[x] != w[y]:
                s = list(w)
                s[x], s[y] = s[y], s[x]
                out.append(("swap two ops", "".join(s)))
                break
    if prop == "bounds":
        m = re.search(r"([acw])(\d+)", w)
        if m:
            out.append(("N+1", w[:m.start(2)] + str(int(m.group(2)) + 1) + w[m.end(2):]))
    for ch, name in ((")", "drop )"), ("|", "drop |"), ("}", "drop }")):
        if ch in w:
            j = w.index(ch)
            out.append((name, w[:j] + w[j + 1:]))
    return out


print("== mutation matrix: mutation x judge (R = reject, A = ACCEPT)")
pick = {"ownership": [], "bounds": []}
for rec in sorted(allw, key=lambda r: len(r[5])):
    k, f, idx, prop, fn, w = rec
    if prop not in pick or len(pick[prop]) >= 6:
        continue
    if prop == "ownership" and sum(1 for c in w if c in OPS) < 2:
        continue
    if prop == "bounds" and not re.search(r"[acw]\d+", w):
        continue
    if judge_checker(prop, w) != 0:
        continue
    # prefer variety: one with `(`, one with `{`, then any
    pick[prop].append(rec)
accepts = 0
for prop, recs in pick.items():
    for k, f, idx, _, fn, w in recs:
        for name, mw in mutations(prop, w):
            if mw == w:
                continue
            c = judge_checker(prop, mw)
            v = judge_verify(prop, mw)
            kr = judge_kernel(prop, mw, f"{k}_{idx}_{name}")
            cells = ["R" if c == 1 else ("A" if c == 0 else f"?{c}"), "R" if v == 1 else ("A" if v == 0 else f"?{v}"), "R" if kr == 0 else "A"]
            anchor = "-"
            if "A" in cells:
                accepts += 1
                b, recs2 = parse(os.path.join(OUT, f"{k}.bundle"))
                _, _, _, hdr_end, start, n = recs2[idx]
                hdr_start = b.rindex(b"\n", 0, hdr_end) + 1
                line = b[hdr_start:hdr_end].decode().split(" ", 3)
                line[2] = str(len(mw.encode()))
                nb = b[:hdr_start] + " ".join(line).encode() + b"\n" + mw.encode() + b[start + n:]
                mp = os.path.join(OUT, f"{k}.mut.bundle")
                open(mp, "wb").write(nb)
                ar = run([ANCHOR, os.path.join(OUT, f"{k}.wasm"), mp, os.path.join(OUT, f"{k}.anchors")])
                anchor = "R" if ar.returncode != 0 else "A"
            print(f"  {prop:9} {name:12} checker={cells[0]} verify={cells[1]} kernel={cells[2]} anchor={anchor}  {w!r:.40} -> {mw!r:.40} [{os.path.basename(f)} {fn[:24]}]")
print(f"== {accepts} mutation(s) some judge accepted")
