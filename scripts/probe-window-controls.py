#!/usr/bin/env python3
"""Probe only: two controls on the window checker of proofs/GuardWindow.v.

  probe-window-controls.py mutants <windows.txt> <out-dir>
  probe-window-controls.py verdicts <out-dir>
  probe-window-controls.py judge <out-dir> <jobs> <memory KB> <seconds per 2000>
  probe-window-controls.py vectors <out-dir>
  probe-window-controls.py compare <out-dir>

NEGATIVE CONTROL. <windows.txt> holds one accepted window per line, as decimal
bytes. `mutants` writes every string that differs from a window in ONE byte
(the byte plus one, minus one, and with each single bit flipped) as
`Eval vm_compute in (window_ok [...])` goals, <out-dir>/mut/Mut_NNNN.v, and
<out-dir>/mutants.tsv (code, window line, position, new byte, and what the
changed byte is in the ORIGINAL window, read by the small decoder below).
The caller compiles the files, keeping each one's output beside it; `verdicts`
then reads them and reports, per role of the changed byte, how many mutants
the kernel still accepts.

DIFFERENTIAL CONTROL. `vectors` writes operand vectors for the nine
instructions the proof interprets, the goals that make the proof assistant
compute `usem` / `bsem` on them (<out-dir>/sem/Sem.v), and the same vectors
for an independent WebAssembly engine (<out-dir>/sem/vectors.json).
`compare` reads both engines' answers.
"""
import json
import os
import random
import sys

mode = sys.argv[1]
HEAD = """From AlmideTrust Require Import GuardWindow.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""
CHUNK = 2000


def uleb(bs, i):
    """(value, next index) of an unsigned LEB128 at bs[i]."""
    v = s = 0
    while True:
        b = bs[i]
        v |= (b & 0x7F) << s
        s += 7
        i += 1
        if b < 0x80:
            return v, i


def roles(bs):
    """What each byte of an accepted window is: (role, in an aborting arm).
    Mirrors the shape `dec` accepts; a window it cannot walk gets `?`."""
    out = ["?"] * len(bs)
    try:
        i, arm = 0, False
        while i < len(bs):
            op = bs[i]
            tag = "arm:" if arm else ""
            if op in (0x20, 0x21, 0x22, 0x23, 0x24, 0x10):  # local.get/set/tee, global.get/set, call
                _, j = uleb(bs, i + 1)
                out[i] = tag + "opcode"
                for k in range(i + 1, j):
                    out[k] = tag + ("callee" if op == 0x10 else "variable index")
                i = j
            elif op in (0x41, 0x42):
                _, j = uleb(bs, i + 1)
                out[i] = tag + "opcode"
                for k in range(i + 1, j):
                    out[k] = tag + "constant"
                i = j
            elif op == 0x04 and not arm:
                out[i] = "if"
                out[i + 1] = "if block type"
                i += 2
                arm = True
            elif op == 0x00 and arm and bs[i + 1] == 0x0B:
                out[i] = "arm:unreachable"
                out[i + 1] = "arm:end"
                i += 2
                arm = False
            elif 0x28 <= op <= 0x3E:
                _, j = uleb(bs, i + 1)
                _, k2 = uleb(bs, j)
                last = k2 == len(bs) and not arm
                out[i] = tag + ("the access's opcode" if last else "load opcode")
                for k in range(i + 1, j):
                    out[k] = tag + ("the access's alignment hint" if last else "alignment hint")
                for k in range(j, k2):
                    out[k] = tag + ("the access's offset" if last else "load offset")
                i = k2
            else:
                out[i] = tag + "opcode"
                i += 1
    except IndexError:
        pass
    return out


if mode == "mutants":
    src, out = sys.argv[2], sys.argv[3]
    os.makedirs(os.path.join(out, "mut"), exist_ok=True)
    wins = [[int(x) for x in l.split()] for l in open(src, encoding="utf-8") if l.strip()]
    originals = {tuple(w) for w in wins}
    rows, seen = [], set()
    for n, w in enumerate(wins):
        r = roles(w)
        for i, b in enumerate(w):
            for nb in sorted({(b + 1) % 256, (b - 1) % 256} | {b ^ (1 << k) for k in range(8)}):
                m = tuple(w[:i] + [nb] + w[i + 1:])
                if m in seen or m in originals:
                    continue
                seen.add(m)
                rows.append((f"x{len(rows):06d}", n, i, nb, r[i], m))
    with open(os.path.join(out, "mutants.tsv"), "w", encoding="utf-8") as fh:
        for code, n, i, nb, role, m in rows:
            fh.write(f"{code}\t{n}\t{i}\t{nb}\t{role}\t{' '.join(map(str, m))}\n")
    for k in range(0, len(rows), CHUNK):
        with open(os.path.join(out, "mut", f"Mut_{k // CHUNK:04d}.v"), "w", encoding="utf-8") as f:
            f.write(HEAD)
            for code, _, _, _, _, m in rows[k:k + CHUNK]:
                f.write(f"Eval vm_compute in (window_ok [{';'.join(map(str, m))}]).\n")
    print(f"{len(wins)} window(s), {len(originals)} distinct; {len(rows)} distinct one-byte mutant(s)")
    sys.exit(0)

if mode == "verdicts":
    out = sys.argv[2]
    rows = [l.rstrip("\n").split("\t") for l in open(os.path.join(out, "mutants.tsv"), encoding="utf-8")]
    vals = []
    for k in range(0, len(rows), CHUNK):
        p = os.path.join(out, "mut", f"Mut_{k // CHUNK:04d}.out")
        got = [l.split("=", 1)[1].strip() for l in open(p, encoding="utf-8") if l.strip().startswith("= ")]
        if len(got) != len(rows[k:k + CHUNK]):
            sys.exit(f"{p}: {len(got)} value(s) for {len(rows[k:k + CHUNK])} mutant(s)")
        vals += got
    by = {}
    with open(os.path.join(out, "accepted.tsv"), "w", encoding="utf-8") as fh:
        for (code, n, i, nb, role, m), v in zip(rows, vals):
            t = by.setdefault(role, [0, 0])
            t[0] += 1
            if v == "true":
                t[1] += 1
                fh.write(f"{code}\t{n}\t{i}\t{nb}\t{role}\t{m}\n")
    tot = sum(t[0] for t in by.values())
    acc = sum(t[1] for t in by.values())
    print(f"{tot} one-byte mutant(s): the kernel refuses {tot - acc} and still accepts {acc}")
    print("role of the changed byte in the original window: mutants, still accepted")
    with open(os.path.join(out, "by-role.tsv"), "w", encoding="utf-8") as fh:
        for role in sorted(by, key=lambda r: (-by[r][1], r)):
            print(f"   {role:34s} {by[role][0]:8d} {by[role][1]:8d}")
            fh.write(f"{role}\t{by[role][0]}\t{by[role][1]}\n")
    sys.exit(0)

if mode == "judge":
    # The kernel's verdict on every mutant, each compiler run under a memory
    # and a time limit. A run that does not finish is split until the mutants
    # it could not judge stand alone: those get `limit` instead of a verdict.
    import concurrent.futures
    import itertools
    import subprocess
    import threading
    import time
    out, jobs, mem_kb, secs = sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5])
    rows = [l.rstrip("\n").split("\t") for l in open(os.path.join(out, "mutants.tsv"), encoding="utf-8")]
    work = os.path.join(out, "judge")
    os.makedirs(work, exist_ok=True)
    serial, lock = itertools.count(), threading.Lock()
    runs = {"finished": 0, "memory or error": 0, "time": 0}

    def group_rss_kb(pgid):
        """Resident memory of every process in the group, from /proc (0 where there is none)."""
        total = 0
        try:
            pids = [d for d in os.listdir("/proc") if d.isdigit()]
        except OSError:
            return 0
        for d in pids:
            try:
                t = open(f"/proc/{d}/stat", encoding="utf-8", errors="replace").read().rsplit(")", 1)[1].split()
            except (OSError, IndexError):
                continue
            if int(t[2]) == pgid:
                total += int(t[21]) * 4
        return total

    def run(goals, allow):
        """The values the compiler printed for `goals`, or (None, why).
        The compiler runs in its own process group; a watcher stops the group
        when its resident memory passes the limit or its time runs out."""
        with lock:
            n = next(serial)
        path = os.path.join(work, f"J_{n:06d}.v")
        with open(path, "w", encoding="utf-8") as f:
            f.write(HEAD)
            for g in goals:
                f.write(f"Eval vm_compute in ({g}).\n")
        why, t0 = "", time.time()
        with open(path + ".out", "w", encoding="utf-8") as fo, open(path + ".err", "w", encoding="utf-8") as fe:
            p = subprocess.Popen(["coqc", "-Q", "proofs", "AlmideTrust", path], stdout=fo, stderr=fe, start_new_session=True)
            while p.poll() is None:
                if time.time() - t0 > allow:
                    why = "time"
                elif mem_kb and group_rss_kb(p.pid) > mem_kb:
                    why = "memory"
                if why:
                    try:
                        os.killpg(p.pid, 9)
                    except OSError:
                        pass
                    break
                time.sleep(0.05)
            p.wait()
        got = [l.split("=", 1)[1].strip() for l in open(path + ".out", encoding="utf-8") if l.strip().startswith("= ")]
        if not why and (p.returncode != 0 or len(got) != len(goals)):
            why = "error: " + " ".join(open(path + ".err", encoding="utf-8").read().split())[:160]
        with lock:
            runs["finished" if not why else "time" if why == "time" else "memory or error"] += 1
        return (got, "") if not why else (None, why)

    def goal(i):
        return f"window_ok [{';'.join(rows[i][5].split())}]"

    def judge(batch):
        got, why = run([goal(i) for i in batch], max(30, secs * len(batch) // CHUNK))
        if got is not None:
            return {i: (v, "") for i, v in zip(batch, got)}
        if len(batch) == 1:
            return {batch[0]: ("limit", why)}
        step, res = max(1, (len(batch) + 7) // 8), {}
        for k in range(0, len(batch), step):
            res.update(judge(batch[k:k + step]))
        return res

    # The limits must not be what stops an ordinary run, and must stop one
    # that builds a number of three thousand million binary digits.
    got, why = run(["window_ok [0;1;2]", "window_ok []"], 60)
    if got is None:
        sys.exit(f"the limits stop an ordinary run ({why}); nothing was judged")
    got, why = run(["Z.eqb (2 ^ 3000000000) 1"], int(os.environ.get("CAL_SECS", "60")))
    print(f"calibration: a run that builds 2 ^ 3000000000 " + ("FINISHED" if got is not None else f"is stopped ({why})"))
    runs.update({"finished": 0, "memory or error": 0, "time": 0})
    t0 = time.time()
    verdict = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        for res in pool.map(judge, [list(range(k, min(k + CHUNK, len(rows)))) for k in range(0, len(rows), CHUNK)]):
            verdict.update(res)
    print(f"kernel time on the mutants: {time.time() - t0:.0f} s ({jobs} at a time; each run under {mem_kb // 1000} MB and {secs} s per {CHUNK})")
    print(f"compiler runs: {runs['finished']} finished, {runs['memory or error']} stopped by the memory limit or an error, {runs['time']} by the time limit")
    by = {}
    with open(os.path.join(out, "verdict-all.tsv"), "w", encoding="utf-8") as fa, \
         open(os.path.join(out, "accepted.tsv"), "w", encoding="utf-8") as fh, \
         open(os.path.join(out, "limit.tsv"), "w", encoding="utf-8") as fl:
        for i, (code, n, pos, nb, role, m) in enumerate(rows):
            v, why = verdict[i]
            t = by.setdefault(role, [0, 0, 0])
            t[0] += 1
            fa.write(f"{code}\t{v}\n")
            if v == "true":
                t[1] += 1
                fh.write(f"{code}\t{n}\t{pos}\t{nb}\t{role}\t{m}\n")
            elif v == "limit":
                t[2] += 1
                fl.write(f"{code}\t{n}\t{pos}\t{nb}\t{role}\t{why}\t{m}\n")
            elif v != "false":
                sys.exit(f"{code}: the compiler printed {v!r}")
    tot, acc, lim = (sum(t[k] for t in by.values()) for k in range(3))
    print(f"{tot} one-byte mutant(s): the kernel refuses {tot - acc - lim}, still accepts {acc}, and gives no verdict within the limits on {lim}")
    print("role of the changed byte in the original window: mutants, still accepted, no verdict")
    with open(os.path.join(out, "by-role.tsv"), "w", encoding="utf-8") as fh:
        for role in sorted(by, key=lambda r: (-by[r][1], -by[r][2], r)):
            print(f"   {role:34s} {by[role][0]:8d} {by[role][1]:8d} {by[role][2]:8d}")
            fh.write(f"{role}\t{by[role][0]}\t{by[role][1]}\t{by[role][2]}\n")
    sys.exit(0)

# ---- the nine interpreted instructions ---------------------------------------
# name, wasm opcode, operand widths, result width
OPS = [("i32.add", 106, (32, 32), 32), ("i32.mul", 108, (32, 32), 32), ("i32.shl", 116, (32, 32), 32),
       ("i32.div_u", 110, (32, 32), 32), ("i32.shr_u", 118, (32, 32), 32), ("i32.ge_u", 79, (32, 32), 32),
       ("i64.ge_u", 90, (64, 64), 32), ("i32.wrap_i64", 167, (64,), 32), ("i64.extend_i32_u", 173, (32,), 64)]


def edge(w):
    top = 1 << w
    return [0, 1, 2, 7, 8, 31, 32, 33, 63, 64, 255, 256, 65535, 65536, top // 2 - 1, top // 2, top // 2 + 1, top - 2, top - 1,
            (1 << 31) - 1 if w == 64 else 12, 1 << 31 if w == 64 else 16, (1 << 32) - 1 if w == 64 else 24, 1 << 32 if w == 64 else 4]


if mode == "vectors":
    out = sys.argv[2]
    os.makedirs(os.path.join(out, "sem"), exist_ok=True)
    rnd = random.Random(20261125)
    vec = []
    for name, op, ws, _ in OPS:
        pools = [edge(w) + [rnd.randrange(1 << w) for _ in range(40)] for w in ws]
        if len(ws) == 1:
            cases = [(a,) for a in pools[0]]
        else:
            cases = [(a, b) for a in pools[0][:23] for b in pools[1][:23]] + list(zip(pools[0][23:], pools[1][23:]))
        for c in cases:
            if name == "i32.div_u" and c[1] == 0:
                continue  # the instruction traps; the proof models a trap as "this run is not there"
            vec.append((name, op, c))
    with open(os.path.join(out, "sem", "vectors.json"), "w", encoding="utf-8") as fh:
        json.dump([[n, [str(x) for x in c]] for n, _, c in vec], fh)
    with open(os.path.join(out, "sem", "Sem.v"), "w", encoding="utf-8") as f:
        f.write(HEAD)
        for name, op, c in vec:
            if len(c) == 1:
                f.write(f"Eval vm_compute in (usem (fun _ _ => 0) {op} {c[0]}).\n")
            else:
                f.write(f"Eval vm_compute in (bsem (fun _ _ _ => 0) {op} {c[0]} {c[1]}).\n")
    print(f"{len(vec)} vector(s) over {len(OPS)} instruction(s)")
    sys.exit(0)

if mode == "compare":
    out = sys.argv[2]
    vec = json.load(open(os.path.join(out, "sem", "vectors.json"), encoding="utf-8"))
    proof = [l.split("=", 1)[1].strip() for l in open(os.path.join(out, "sem", "Sem.out"), encoding="utf-8") if l.strip().startswith("= ")]
    engine = [l.strip() for l in open(os.path.join(out, "sem", "engine.out"), encoding="utf-8") if l.strip()]
    if not (len(vec) == len(proof) == len(engine)):
        sys.exit(f"{len(vec)} vector(s), {len(proof)} answer(s) from the proof, {len(engine)} from the engine")
    bad = 0
    per = {}
    for (name, c), p, e in zip(vec, proof, engine):
        per[name] = per.get(name, 0) + 1
        if int(p) != int(e):
            bad += 1
            if bad <= 10:
                print(f"DIFFERS: {name} {c}: the proof's meaning gives {p}, the engine {e}")
    print("vectors per instruction: " + ", ".join(f"{k} {v}" for k, v in per.items()))
    print(f"{len(vec)} vector(s): {len(vec) - bad} agree, {bad} differ")
    sys.exit(1 if bad else 0)

sys.exit("unknown mode")
