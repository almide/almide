#!/usr/bin/env python3
"""Probe (gates nothing): where the Rocq kernel's time and memory go on routine
certificates, and what three spellings of the same goals cost.

  base    Goal forallb f ["lit";...] = true. Proof. vm_compute. reflexivity. Qed.
  cast    the same statement, proved by `vm_cast_no_check (eq_refl true)` (the
          kernel computes once, at Qed)
  shared  the cast proof, each literal written as a concatenation of chunks
          defined once per file (the statement still names the same string)
  defs    only `Definition w := "lit".` per witness (the literal's own cost)
"""
import glob, os, re, subprocess, sys, tarfile, time
ROOT = os.getcwd()
P = ROOT + "/proofs"
tarfile.open(ROOT + "/lab/rcerts-fx.tar.xz").extractall(ROOT + "/lab")
certs = sorted((open(f).read().rstrip("\n") for f in glob.glob(ROOT + "/lab/fx/*.rcert")), key=lambda c: (len(c), c))
print(f"== {len(certs)} certificates, {sum(map(len, certs))} bytes, max {len(certs[-1])}", flush=True)
lit = lambda s: '"' + s.replace('"', '""') + '"'
HDR = ("From AlmideTrust Require Import OwnershipChecker NameTotality CapabilityBound CapabilityReach CallModes HelperSig BoundsCert InitCert FieldCert RoutineBoundsCert RoutineProcsCert RoutineLinCert.\n"
       "From Stdlib Require Import String List.\nImport ListNotations.\nOpen Scope string_scope.\n")
BASE = "Proof. vm_compute. reflexivity. Qed.\n"
CAST = "Proof. vm_cast_no_check (eq_refl true). Qed.\n"
SPLIT = re.compile(r"(?:(?<=Q)|(?<=C))\d+")
MIN = 64

def chunks(ws, limit=60000):
    out, chunk, size = [], [], 0
    for w in ws + [None]:
        if w is None or (chunk and size + len(w) > limit):
            if chunk:
                out.append(chunk)
            chunk, size = [], 0
        if w is not None:
            chunk.append(w); size += len(w)
    return out

def plain(ws, proof):
    return HDR + "".join("Goal forallb check_lbounds_cert [%s] = true.\n%s" % (";".join(lit(x) for x in c), proof) for c in chunks(ws))

def shared(ws, proof):
    defs, order, goals = {}, [], []
    def expr(w):
        parts, i = [], 0
        for m in SPLIT.finditer(w):
            parts.append(w[i:m.start()]); parts.append(m.group()); i = m.end()
        parts.append(w[i:])
        out, pend = [], ""
        for p in parts:
            if len(p) >= MIN:
                if pend:
                    out.append(lit(pend)); pend = ""
                if p not in defs:
                    defs[p] = "k%d" % len(defs); order.append(p)
                out.append(defs[p])
            else:
                pend += p
        if pend or not out:
            out.append(lit(pend))
        # re-expand: the expression must be the witness
        back = "".join(order[int(x[1:])] if x[0] == "k" else x[1:-1].replace('""', '"') for x in out)
        assert back == w
        return "(" + " ++ ".join(out) + ")"
    for c in chunks(ws):
        goals.append("Goal forallb check_lbounds_cert [%s] = true.\n%s" % (";".join(expr(x) for x in c), proof))
    body = "".join("Definition %s : string := %s.\n" % (defs[p], lit(p)) for p in order)
    print(f"    shared: {len(order)} chunk definition(s), {sum(map(len, order))} literal bytes for {sum(map(len, ws))} witness bytes", flush=True)
    return HDR + body + "".join(goals)

def defs_only(ws):
    return HDR + "".join("Definition w%d : string := %s.\n" % (i, lit(w)) for i, w in enumerate(ws))

def run(name, text):
    open(f"{P}/{name}.v", "w").write(text)
    t = time.time()
    r = subprocess.run(["/usr/bin/time", "-f", "%M", "coqc", "-Q", ".", "AlmideTrust", name + ".v"], cwd=P, capture_output=True, text=True)
    rss = r.stderr.strip().splitlines()[-1] if r.stderr.strip() else "?"
    print(f"  {name:<22} rc {r.returncode}  {time.time() - t:7.1f} s  peak {rss} KiB  ({len(text)} B file)", flush=True)
    if r.returncode:
        print("   ", r.stderr[-600:], flush=True)
    for e in (".v", ".vo", ".vos", ".vok", ".glob"):
        try: os.remove(f"{P}/{name}{e}")
        except OSError: pass

# A: a medium sample — every 12th certificate below 60 KB.
med = [c for c in certs if len(c) < 60000][::12]
print(f"== A: {len(med)} medium certificates, {sum(map(len, med))} bytes", flush=True)
run("KA_base", plain(med, BASE)); run("KA_cast", plain(med, CAST)); run("KA_shared", shared(med, CAST)); run("KA_defs", defs_only(med))
# B: one certificate near 170-200 KB.
mid = [c for c in certs if 150000 < len(c) < 300000][:1]
if mid:
    print(f"== B: one certificate of {len(mid[0])} bytes", flush=True)
    run("KB_base", plain(mid, BASE)); run("KB_cast", plain(mid, CAST)); run("KB_shared", shared(mid, CAST)); run("KB_defs", defs_only(mid))
# C: the largest certificate.
big = certs[-1:]
print(f"== C: the largest certificate, {len(big[0])} bytes", flush=True)
run("KC_cast", plain(big, CAST)); run("KC_shared", shared(big, CAST)); run("KC_defs", defs_only(big)); run("KC_base", plain(big, BASE))
# D: the three largest in one file, shared (do repeated big bodies cost once?).
big3 = certs[-3:]
print(f"== D: the three largest, {sum(map(len, big3))} bytes", flush=True)
run("KD_shared", shared(big3, CAST))
