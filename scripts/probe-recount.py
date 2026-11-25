#!/usr/bin/env python3
"""Recount a module's memory accesses with a decoder that shares no code with the census.

  recount.py <a.wasm> <a.anchors> [--fn <a.fn> | --fnrows <fnrows.tsv> --file <path> [--leg L]] [--label L] [--rows]

The census (the byte-level checker's `--census`) is built on a third-party WebAssembly parser and
on that parser's validator, whose operand-stack height it shadows. It puts every memory access of
a module into one class:

   located   an event of the anchor file names the instruction, or names the call right after it
             (a verified linear certificate also locates accesses; this script does not read one)
   bulk      memory.copy or memory.fill, not located
   header    static offset below 12 and the address pushed directly by a local.get, not located
   other     the rest: a load, a store, or a byte-wide access

where an access is one of the 14 scalar loads, the 9 scalar stores, v128.load, v128.store,
memory.copy and memory.fill.

This script reads the same two files with its own decoder of the binary format and its own
operand stack (written from the specification; no parser library), and prints one line per
module. Given the census's rows for the module it compares function by function:

   accesses               must be equal
   located                must equal the census's located minus its linear
   header, bulk, other    must be equal when the function has no linear access; otherwise the
                          census's are smaller by exactly the function's linear count in total

It also counts what the census's definition of an access leaves out (memory.init, atomic
accesses, the other vector loads and stores), and the header count under a second reading of
"pushed by a local.get" in which a value that leaves a block through its `end` keeps the
instruction that pushed it (the census reads such a value as pushed by the `end`).
"""
import sys

args = sys.argv[1:]


def opt(name, flag=False):
    if name in args:
        i = args.index(name)
        if flag:
            del args[i]
            return True
        v = args[i + 1]
        del args[i:i + 2]
        return v
    return False if flag else None


fn_path, fnrows_path, file_key, leg = opt("--fn"), opt("--fnrows"), opt("--file"), opt("--leg") or "fixtures"
label, show_rows = opt("--label"), opt("--rows", True)
wasm_path, anchors_path = args[0], args[1]
label = label or wasm_path


class Refuse(Exception):
    pass


def uleb(b, i):
    r = s = 0
    while True:
        x = b[i]
        i += 1
        r |= (x & 0x7F) << s
        s += 7
        if x < 0x80:
            return r, i


def sleb(b, i):
    r = s = 0
    while True:
        x = b[i]
        i += 1
        r |= (x & 0x7F) << s
        s += 7
        if x < 0x80:
            if x & 0x40:
                r -= 1 << s
            return r, i


def name(b, i):
    n, i = uleb(b, i)
    return i + n


def limits(b, i):
    flags, i = uleb(b, i)
    _, i = uleb(b, i)
    if flags & 1:
        _, i = uleb(b, i)
    return flags, i


VALTYPES = {0x7F, 0x7E, 0x7D, 0x7C, 0x7B, 0x70, 0x6F}

# ---- the module: types, the function index space, the code section
b = open(wasm_path, "rb").read()
if b[:8] != b"\x00asm\x01\x00\x00\x00":
    print(f"RECOUNT\t{label}\tnot a version-1 module")
    sys.exit(0)
types, func_types, n_imported, bodies, mem_flags = [], [], 0, [], []
i = 8
while i < len(b):
    sec = b[i]
    size, i = uleb(b, i + 1)
    end = i + size
    j = i
    if sec == 1:
        n, j = uleb(b, j)
        for _ in range(n):
            if b[j] != 0x60:
                raise SystemExit(f"{label}: type form {b[j]:#x} is not a function type")
            np_, j = uleb(b, j + 1)
            j += np_
            nr, j = uleb(b, j)
            j += nr
            types.append((np_, nr))
    elif sec == 2:
        n, j = uleb(b, j)
        for _ in range(n):
            j = name(b, name(b, j))
            kind = b[j]
            j += 1
            if kind == 0:
                t, j = uleb(b, j)
                func_types.append(t)
                n_imported += 1
            elif kind == 1:
                j += 1
                _, j = limits(b, j)
            elif kind == 2:
                f, j = limits(b, j)
                mem_flags.append(f)
            elif kind == 3:
                j += 2
            elif kind == 4:
                _, j = uleb(b, j + 1)
            else:
                raise SystemExit(f"{label}: import kind {kind}")
    elif sec == 3:
        n, j = uleb(b, j)
        for _ in range(n):
            t, j = uleb(b, j)
            func_types.append(t)
    elif sec == 5:
        n, j = uleb(b, j)
        for _ in range(n):
            f, j = limits(b, j)
            mem_flags.append(f)
    elif sec == 10:
        n, j = uleb(b, j)
        for _ in range(n):
            size2, j = uleb(b, j)
            bodies.append((j, j + size2))
            j += size2
    i = end

# ---- instruction arities (pops, pushes) of the operators with a fixed one
FIXED = {0x01: (0, 0), 0x1A: (1, 0), 0x1B: (3, 1), 0x20: (0, 1), 0x21: (1, 0), 0x22: (1, 1), 0x23: (0, 1), 0x24: (1, 0),
         0x25: (1, 1), 0x26: (2, 0), 0x3F: (0, 1), 0x40: (1, 1), 0x41: (0, 1), 0x42: (0, 1), 0x43: (0, 1), 0x44: (0, 1),
         0x45: (1, 1), 0x50: (1, 1), 0xD0: (0, 1), 0xD1: (1, 1), 0xD2: (0, 1)}
for o in range(0x28, 0x36):
    FIXED[o] = (1, 1)                      # loads
for o in range(0x36, 0x3F):
    FIXED[o] = (2, 0)                      # stores
for lo, hi in ((0x46, 0x4F), (0x51, 0x5A), (0x5B, 0x60), (0x61, 0x66), (0x6A, 0x78), (0x7C, 0x8A), (0x92, 0x98), (0xA0, 0xA6)):
    for o in range(lo, hi + 1):
        FIXED[o] = (2, 1)                  # comparisons and binary operators
for lo, hi in ((0x67, 0x69), (0x79, 0x7B), (0x8B, 0x91), (0x99, 0x9F), (0xA7, 0xC4)):
    for o in range(lo, hi + 1):
        FIXED[o] = (1, 1)                  # unary operators, conversions, sign extensions
FC = {8: (3, 0), 9: (0, 0), 10: (3, 0), 11: (3, 0), 12: (3, 0), 13: (0, 0), 14: (3, 0), 15: (2, 1), 16: (0, 1), 17: (3, 0)}
for o in range(8):
    FC[o] = (1, 1)                         # saturating truncations
LOADS = set(range(0x28, 0x36))
STORES = set(range(0x36, 0x3F))
BYTE_LOADS = {0x2C, 0x2D, 0x30, 0x31}
BYTE_STORES = {0x3A, 0x3C}
LOCAL_GET = 0x20


def blocktype(i):
    x = b[i]
    if x == 0x40:
        return (0, 0), i + 1
    if x in VALTYPES:
        return (0, 1), i + 1
    t, i = sleb(b, i)
    return types[t], i


def walk(lo, hi, ftype):
    """One body: per instruction (opcode key, static offset or None, producer of the address
    operand under the census's reading, under the second reading)."""
    i = lo
    n, i = uleb(b, i)
    for _ in range(n):
        _, i = uleb(b, i)
        i += 1
    out = []
    stack = []      # (producer under the census's reading, under the second reading)
    ctrl = [["func", 0, (0, types[ftype][1]), False]]   # kind, height, (params, results), unreachable

    def pop(k):
        fr = ctrl[-1]
        got = []
        for _ in range(k):
            if len(stack) > fr[1]:
                got.append(stack.pop())
            elif fr[3]:
                got.append((None, None))
            else:
                raise Refuse(f"operand stack underflow at instruction {len(out)}")
        got.reverse()
        return got

    def push(k, at, keep=None):
        for n_ in range(k):
            stack.append((at, keep[n_][1] if keep else at))

    def dead():
        fr = ctrl[-1]
        del stack[fr[1]:]
        fr[3] = True

    def label(l):
        fr = ctrl[-1 - l]
        return fr[2][0] if fr[0] == "loop" else fr[2][1]

    while i < hi:
        at = len(out)
        o = b[i]
        i += 1
        key, off, addr = o, None, (None, None)
        if o in (0x02, 0x03, 0x04):
            bt, i = blocktype(i)
            if o == 0x04:
                pop(1)
            ps = pop(bt[0])
            ctrl.append([{0x02: "block", 0x03: "loop", 0x04: "if"}[o], len(stack), bt, False])
            push(bt[0], at, ps)
        elif o == 0x05:
            fr = ctrl[-1]
            pop(fr[2][1])
            del stack[fr[1]:]
            fr[3] = False
            fr[0] = "else"
            push(fr[2][0], at)
        elif o == 0x0B:
            fr = ctrl[-1]
            rs = pop(fr[2][1])
            del stack[fr[1]:]
            ctrl.pop()
            if ctrl:
                push(fr[2][1], at, rs)
        elif o == 0x0C:
            l, i = uleb(b, i)
            pop(label(l))
            dead()
        elif o == 0x0D:
            l, i = uleb(b, i)
            pop(1)
            vs = pop(label(l))
            push(len(vs), at, vs)
        elif o == 0x0E:
            n, i = uleb(b, i)
            ls = []
            for _ in range(n + 1):
                l, i = uleb(b, i)
                ls.append(l)
            pop(1)
            pop(label(ls[-1]))
            dead()
        elif o == 0x0F:
            pop(ctrl[0][2][1])
            dead()
        elif o == 0x00:
            dead()
        elif o in (0x10, 0x12):
            f, i = uleb(b, i)
            p, r = types[func_types[f]]
            pop(p)
            if o == 0x12:
                dead()
            else:
                push(r, at)
        elif o in (0x11, 0x13):
            t, i = uleb(b, i)
            _, i = uleb(b, i)
            p, r = types[t]
            pop(p + 1)
            if o == 0x13:
                dead()
            else:
                push(r, at)
        elif o == 0x1C:
            n, i = uleb(b, i)
            i += n
            pop(3)
            push(1, at)
        elif o == 0xFC:
            sub, i = uleb(b, i)
            if sub not in FC:
                raise Refuse(f"operator 0xFC {sub}")
            if sub in (8, 12, 14):
                _, i = uleb(b, i)
                _, i = uleb(b, i)
            elif sub in (9, 11, 13, 15, 16, 17):
                _, i = uleb(b, i)
            elif sub == 10:
                i += 2
            key = (0xFC, sub)
            vs = pop(FC[sub][0])
            if sub in (8, 10, 11):
                addr = vs[0]
            push(FC[sub][1], at)
        elif o == 0xFD:
            raise Refuse("vector instructions")
        elif o == 0xFE:
            raise Refuse("atomic instructions")
        elif o in FIXED:
            if o in LOADS or o in STORES:
                al, i = uleb(b, i)
                if al & 0x40:
                    _, i = uleb(b, i)
                off, i = uleb(b, i)
            elif o in (0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0xD2):
                _, i = uleb(b, i)
            elif o in (0x3F, 0x40):
                i += 1
            elif o in (0x41, 0x42):
                _, i = sleb(b, i)
            elif o == 0x43:
                i += 4
            elif o == 0x44:
                i += 8
            elif o == 0xD0:
                i += 1
            vs = pop(FIXED[o][0])
            if o in LOADS or o in STORES:
                addr = vs[0]
            push(FIXED[o][1], at)
        else:
            raise Refuse(f"operator {o:#x}")
        out.append((key, off, addr))
    if i != hi or ctrl:
        raise Refuse("the body does not end where its size says")
    return out


def events(text):
    """Every position:ordinal token of the init, bounds and fields lines."""
    lines = text.split("\n")
    if not lines or lines[0] != "almide-anchors 1":
        raise SystemExit(f"{label}: not an anchor file")
    got = set()
    for want, l in (("init", lines[2]), ("bounds", lines[3])):
        if l != want and not l.startswith(want + " "):
            raise SystemExit(f"{label}: expected the {want} line, read {l[:30]!r}")
    picked = [lines[2][4:], lines[3][6:]] + [l[6:] for l in lines[4:] if l.startswith("fields ")][:1]
    for l in picked:
        for t in l.replace(";", " ").replace("|", " ").split():
            if t == "-":
                continue
            f, o = t.split(":")
            got.add((int(f), int(o)))
    return got


EV = events(open(anchors_path, encoding="utf-8").read())
mine, refused, left_out = {}, None, 0
try:
    if len(mem_flags) != 1 or mem_flags[0] & 6:
        raise Refuse("not exactly one unshared 32-bit memory")
    for pos, (lo, hi) in enumerate(bodies):
        ops = walk(lo, hi, func_types[n_imported + pos])
        r = dict(acc=0, loc=0, hdr=0, hdr2=0, bulk=0, ol=0, os=0, ob=0)
        for k, (key, off, addr) in enumerate(ops):
            bulk = key in ((0xFC, 10), (0xFC, 11))
            if key == (0xFC, 8):
                left_out += 1
            if not (bulk or key in LOADS or key in STORES):
                continue
            r["acc"] += 1
            nxt = ops[k + 1][0] if k + 1 < len(ops) else None
            if (pos, k) in EV or (nxt == 0x10 and (pos, k + 1) in EV):
                r["loc"] += 1
            elif bulk:
                r["bulk"] += 1
            else:
                direct = off < 12 and addr[0] is not None and ops[addr[0]][0] == LOCAL_GET
                through = off < 12 and addr[1] is not None and ops[addr[1]][0] == LOCAL_GET
                r["hdr2"] += through
                if direct:
                    r["hdr"] += 1
                elif key in BYTE_LOADS or key in BYTE_STORES:
                    r["ob"] += 1
                elif key in STORES:
                    r["os"] += 1
                else:
                    r["ol"] += 1
        if r["acc"]:
            mine[pos] = r
except Refuse as e:
    refused = str(e)

census = None
if fn_path:
    census = {}
    for l in open(fn_path, encoding="utf-8"):
        p = l.split()
        if len(p) == 11 and p[0] == "CENSUSFN":
            census[int(p[1])] = [int(x) for x in p[3:]]
elif fnrows_path:
    census = {}
    for l in open(fnrows_path, encoding="utf-8", errors="replace"):
        p = l.rstrip("\n").split("\t")
        if len(p) >= 15 and p[0] == leg and p[1] == file_key:
            census[int(p[5])] = [int(x) for x in p[7:15]]

T = {k: sum(r[k] for r in mine.values()) for k in ("acc", "loc", "hdr", "hdr2", "bulk", "ol", "os", "ob")}
cols = [label, "refused: " + refused if refused else "read", len(bodies), len(mine)] + [T[k] for k in ("acc", "loc", "hdr", "hdr2", "bulk", "ol", "os", "ob")] + [left_out]
if census is not None and not refused:
    same = differ = lin_fns = lin_ok = 0
    for pos in sorted(set(mine) | set(census)):
        m, c = mine.get(pos), census.get(pos)
        if m is None or c is None:
            differ += 1
            print(f"DIFFER\t{label}\t{pos}\tmine {m}\tcensus {c}")
            continue
        acc, loc, lin, hdr, bulk, ol, os_, ob = c
        ok = m["acc"] == acc and m["loc"] == loc - lin
        if lin == 0:
            ok = ok and (m["hdr"], m["bulk"], m["ol"], m["os"], m["ob"]) == (hdr, bulk, ol, os_, ob)
        else:
            lin_fns += 1
            d = [m["hdr"] - hdr, m["bulk"] - bulk, m["ol"] - ol, m["os"] - os_, m["ob"] - ob]
            ok = ok and min(d) >= 0 and sum(d) == lin
            lin_ok += ok
        if ok:
            same += 1
        else:
            differ += 1
            print(f"DIFFER\t{label}\t{pos}\tmine {m}\tcensus acc={acc} loc={loc} lin={lin} hdr={hdr} bulk={bulk} other={ol},{os_},{ob}")
    cols += [len(census), same, differ, lin_fns, sum(c[0] for c in census.values()), sum(c[3] for c in census.values())]
elif census is not None:
    cols += [len(census), 0, 0, 0, sum(c[0] for c in census.values()), sum(c[3] for c in census.values())]
print("RECOUNT\t" + "\t".join(str(x) for x in cols))
if show_rows:
    for pos, r in sorted(mine.items()):
        print(f"ROW\t{label}\t{pos}\t" + "\t".join(str(r[k]) for k in ("acc", "loc", "hdr", "hdr2", "bulk", "ol", "os", "ob")))
