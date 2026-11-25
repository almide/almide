#!/usr/bin/env python3
"""Probe only: negative controls of the fresh-block rule and the loop rule.

  probe-rule-controls.py make <fresh-dump> <fresh-verdicts.tsv> <loop-dump> <loop-verdicts.tsv> <work-dir>
  probe-rule-controls.py read <work-dir>

`make` takes every DISTINCT proposal the kernel accepted and writes, per
proposal, a handful of deliberately wrong variants as goals for the kernel
(`<work-dir>/Ctl_<k>.v`, one `Eval vm_compute` each) with an index
(`<work-dir>/index.tsv`: rule, kind). `read` pairs the kernel's answers
(`Ctl_<k>.out`) with the index and prints, per rule and kind, how many
variants the kernel refuses and how many it still accepts.

The fresh-block rule (`fresh_ok alloc pre mid tail`):
  past      the access's memarg offset moved 12 + n further (past the block)
  empty     the allocation's size set to 0 (only header words still fit)
  callee    the allocation's callee is another function
  assigned  `i32.const 0 ; local.set x` put in front of what lies between

The loop rule (`loop_core pre mid ht rest`):
  next      the access's memarg offset moved one slot further
  test      the exit test's `i32.ge_u` turned into `i32.gt_u`
  base      the capture reads another local's block
  stride    the capture counts slots of twice the size (when it divides)
  open      the last `end` taken off the scanned bytes
  set-base  `i32.const 0 ; local.set b` put right after the access
  set-count `i32.const 0 ; local.set n` put right after the access
"""
import collections
import glob
import sys


def uleb(bs, p):
    r = s = 0
    while True:
        x = bs[p]
        p += 1
        r |= (x & 0x7F) << s
        s += 7
        if x < 0x80:
            return r, p


def sleb(bs, p):
    r = s = 0
    while True:
        x = bs[p]
        p += 1
        r |= (x & 0x7F) << s
        s += 7
        if x < 0x80:
            if x & 0x40:
                r -= 1 << s
            return r, p


def enc_u(v):
    out = []
    while True:
        b = v & 0x7F
        v >>= 7
        if v:
            out.append(b | 0x80)
        else:
            out.append(b)
            return out


def enc_s(v):
    out = []
    while True:
        b = v & 0x7F
        v >>= 7
        if (v == 0 and not b & 0x40) or (v == -1 and b & 0x40):
            out.append(b)
            return out
        out.append(b | 0x80)


def last_access(bs):
    """(start, opcode, align, offset) of the memory access a run ends in: the
    last position from which opcode, alignment and offset read to the end."""
    for p in range(len(bs) - 3, -1, -1):
        if 40 <= bs[p] <= 62:
            try:
                al, q = uleb(bs, p + 1)
                off, q = uleb(bs, q)
            except IndexError:
                continue
            if q == len(bs) and al < 64:
                return p, bs[p], al, off
    return None


WIDTH = {40: 4, 41: 8, 42: 4, 43: 8, 44: 1, 45: 1, 46: 2, 47: 2, 48: 1, 49: 1, 50: 2, 51: 2, 52: 4, 53: 4,
         54: 4, 55: 8, 56: 4, 57: 8, 58: 1, 59: 2, 60: 1, 61: 2, 62: 4}


def fresh_variants(alloc, pre, mid, tail):
    out = []
    try:
        assert pre[0] == 65
        n, p = sleb(pre, 1)
        assert pre[p] == 16
        a, q = uleb(pre, p + 1)
        st = pre[q]
        x, _ = uleb(pre, q + 1)
    except (AssertionError, IndexError):
        return out
    acc = last_access(tail)
    if acc:
        at, op, al, off = acc
        out.append(("past", alloc, pre, mid, tail[:at] + [op] + enc_u(al) + enc_u(off + 12 + max(n, 0))))
    out.append(("empty", alloc, [65] + enc_s(0) + pre[p:], mid, tail))
    out.append(("callee", alloc, pre[:p] + [16] + enc_u(a + 1) + pre[q:], mid, tail))
    out.append(("assigned", alloc, pre, [65, 0, 33] + enc_u(x) + mid, tail))
    return out


def loop_variants(pre, mid, ht, rest):
    out = []
    try:
        assert pre[0] == 32
        b, p = uleb(pre, 1)
        assert pre[p] == 40
        _, q = uleb(pre, p + 1)
        _, q = uleb(pre, q)
        stride = None
        if pre[q] == 65:
            stride, r = sleb(pre, q + 1)
            div = pre[r]
            setp = r + 1
        else:
            setp = q
        assert pre[setp] == 33
        n, _ = uleb(pre, setp + 1)
        assert ht[0] == 32
        _, t = uleb(ht, 1)
        assert ht[t] == 32
        _, t = uleb(ht, t + 1)
        assert ht[t] == 79
    except (AssertionError, IndexError):
        return out
    acc = last_access(ht)
    if acc:
        at, op, al, off = acc
        slot = stride if stride and div == 110 else WIDTH.get(op, 1)
        out.append(("next", pre, mid, ht[:at] + [op] + enc_u(al) + enc_u(off + slot), rest))
    out.append(("test", pre, mid, ht[:t] + [75] + ht[t + 1:], rest))
    out.append(("base", [32] + enc_u(b + 1) + pre[p:], mid, ht, rest))
    if stride and div == 110:
        out.append(("stride", pre[:q] + [65] + enc_s(stride * 2) + pre[r:], mid, ht, rest))
    if rest and rest[-1] == 11:
        out.append(("open", pre, mid, ht, rest[:-1]))
    out.append(("set-base", pre, mid, ht, [65, 0, 33] + enc_u(b) + rest))
    out.append(("set-count", pre, mid, ht, [65, 0, 33] + enc_u(n) + rest))
    return out


def lst(bs):
    return "[" + ";".join(str(x) for x in bs) + "]"


def ints(text):
    return [int(x) for x in text.split()]


HEAD = """From AlmideTrust Require Import GuardWindow GuardFresh GuardLoop.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""


def make(fdump, fverd, ldump, lverd, work, chunk=500):
    goals = []
    # fresh: the dump's FRESHHEX lines and the verdicts are in the same order
    props = []
    for line in open(fdump, encoding="utf-8", errors="replace"):
        if line.startswith("FRESHHEX "):
            head, pre, mid, tail = [p.strip() for p in line[len("FRESHHEX "):].split("|")]
            props.append((head.split()[2], pre, mid, tail))
    verd = [l.rstrip("\n").split("\t")[-1] for l in open(fverd, encoding="utf-8")]
    assert len(props) == len(verd), ("fresh", len(props), len(verd))
    seen = set()
    for key, v in zip(props, verd):
        if v != "true" or key in seen:
            continue
        seen.add(key)
        alloc, pre, mid, tail = key
        for kind, a, p, m, t in fresh_variants(alloc, ints(pre), ints(mid), ints(tail)):
            goals.append(("fresh", kind, f"fresh_ok {a} {lst(p)} {lst(m)} {lst(t)}"))
    nf = len(seen)
    props = []
    for line in open(ldump, encoding="utf-8", errors="replace"):
        if line.startswith("LOOPHEX "):
            head, hold, pre, mid, ht, rest = [p.strip() for p in line[len("LOOPHEX "):].split("|")]
            props.append((pre, mid, ht, rest))
    verd = [l.rstrip("\n").split("\t")[-1] for l in open(lverd, encoding="utf-8")]
    assert len(props) == len(verd), ("loop", len(props), len(verd))
    seen = set()
    for key, v in zip(props, verd):
        if int(v) < 4 or key in seen:
            continue
        seen.add(key)
        for kind, p, m, h, r in loop_variants(*[ints(x) for x in key]):
            goals.append(("loop", kind, f"loop_core {lst(p)} {lst(m)} {lst(h)} {lst(r)}"))
    with open(f"{work}/index.tsv", "w", encoding="utf-8") as idx:
        for i, (rule, kind, goal) in enumerate(goals):
            k = i // chunk
            with open(f"{work}/Ctl_{k:04d}.v", "a" if i % chunk else "w", encoding="utf-8") as f:
                if i % chunk == 0:
                    f.write(HEAD)
                f.write(f"Eval vm_compute in ({goal}).\n")
            idx.write(f"{rule}\t{kind}\n")
    print(f"controls: {nf} distinct accepted fresh-block proposal(s), {len(seen)} distinct loop proposal(s) whose core holds, {len(goals)} variant(s)")


def read(work):
    idx = [l.rstrip("\n").split("\t") for l in open(f"{work}/index.tsv", encoding="utf-8")]
    vals = []
    for p in sorted(glob.glob(f"{work}/Ctl_*.out")):
        vals += [l.split("=", 1)[1].strip() for l in open(p, encoding="utf-8") if l.strip().startswith("= ")]
    if len(vals) != len(idx):
        sys.exit(f"{len(vals)} verdict(s) for {len(idx)} variant(s)")
    by = collections.Counter((r[0], r[1], v) for r, v in zip(idx, vals))
    for rule, kind in sorted({(r[0], r[1]) for r in idx}):
        print(f"control [{rule}] {kind:9s}: refused {by[(rule, kind, 'false')]}, still accepted {by[(rule, kind, 'true')]}")


if sys.argv[1] == "make":
    make(*sys.argv[2:7])
elif sys.argv[1] == "read":
    read(sys.argv[2])
else:
    sys.exit("unknown mode")
