#!/usr/bin/env python3
"""Probe (gates nothing): the Rocq kernel's time and peak memory on routine
certificates, per part size, to size the structural wall's kernel parts."""
import os, subprocess, time
ROOT = os.getcwd()
TD = os.environ.get("CARGO_TARGET_DIR", ROOT + "/target")
ALMIDE, PRODUCER = TD + "/debug/almide", TD + "/debug/examples/emit_structural_bundle"
LINPROVE = TD + "/release/almide-lin-prove"
OUT = ROOT + "/kp"
os.makedirs(OUT, exist_ok=True)
fx = [l.strip() for l in open("crates/almide-wasm/tests/golden/witness-fixtures.txt") if l.strip() and not l.startswith("#")]
step = max(1, len(fx) // 120)
certs = []
for k, f in enumerate(fx[::step]):
    w, a = f"{OUT}/{k}.wasm", f"{OUT}/{k}.anchors"
    if subprocess.run([ALMIDE, "build", f, "--target", "wasm", "-o", w], capture_output=True).returncode:
        continue
    if subprocess.run([PRODUCER, f, "--artifact", w, "--anchors", a], capture_output=True).returncode:
        continue
    r = subprocess.run([LINPROVE, "--routines", w, a, f"{OUT}/{k}.rsel"], capture_output=True, text=True)
    if r.returncode == 0 and r.stdout.strip():
        certs.append(r.stdout.strip())
total = sum(len(c) for c in certs)
print(f"== {len(certs)} routine certificates, {total} bytes, mean {total // max(1, len(certs))}, max {max(map(len, certs), default=0)}")
lit = lambda s: '"' + s.replace('"', '""') + '"'
HDR = ("From AlmideTrust Require Import OwnershipChecker NameTotality CapabilityBound CapabilityReach CallModes HelperSig BoundsCert InitCert FieldCert RoutineBoundsCert RoutineProcsCert RoutineLinCert.\n"
       "From Stdlib Require Import String List.\nImport ListNotations.\nOpen Scope string_scope.\n")
def goals(ws, limit=60000):
    out, chunk, size = [], [], 0
    for w in ws + [None]:
        if w is None or (chunk and size + len(w) > limit):
            if chunk:
                out.append("Goal forallb check_lbounds_cert [%s] = true.\nProof. vm_compute. reflexivity. Qed.\n" % ";".join(lit(x) for x in chunk))
            chunk, size = [], 0
        if w is not None:
            chunk.append(w); size += len(w)
    return out
for part in (250000, 1000000, 2000000, 4000000):
    sel, size = [], 0
    for c in sorted(certs, key=len):
        if size + len(c) > part and sel:
            break
        sel.append(c); size += len(c)
    name = f"KProbe_{part}"
    open(f"{ROOT}/proofs/{name}.v", "w").write(HDR + "".join(goals(sel)))
    t = time.time()
    r = subprocess.run(["/usr/bin/time", "-f", "%M", "coqc", "-Q", ".", "AlmideTrust", name + ".v"], cwd=ROOT + "/proofs", capture_output=True, text=True)
    rss = r.stderr.strip().splitlines()[-1] if r.stderr.strip() else "?"
    print(f"  part {part:>8} B literal ({size} B, {len(sel)} certs): rc {r.returncode}, {time.time() - t:.0f} s, peak {rss} KiB", flush=True)
    if r.returncode:
        print("   ", r.stderr[-400:])
