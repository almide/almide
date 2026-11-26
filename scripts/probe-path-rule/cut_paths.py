#!/usr/bin/env python3
"""Cut the paths of a module: for every load, store, `memory.copy` and `memory.fill`, the
longest run of the function's bytes before it that proofs/GuardPath.v's `pdec` reads
(untrusted: it only proposes; the kernel decodes the bytes again and proves or does not prove
each statement).

  cut_paths.py <module.wasm> <allocator index or -> [--max <instructions>] [--only <function>]

The allocator index is the anchor file's `alloc` line: a path may call that function and no
other. Prints, per access,

  ACCESS <function> <instruction> <kind> <allocator> <signature> <first> | <written> | <read> | <bytes> | <rest> | <events>

<kind> is load, store, copy or fill; <signature> is a hash of the access and the eight
instructions before it (to find the same access in another build of the same source); <first>
is the instruction the path starts at;
<written> and <read> are the cutter's guesses at the block each range lies in, zero or more of
`L<local>.<v>` (the block that local holds at the access, by the length word in memory number
v) and `F<k>` (the block allocation number k returned), `-` for no guess, `.` for a range the
access does not have, `h` for an access to a block's first twelve bytes (not this rule's).
<rest> is the bytes after the access up to the `end` of the outermost loop the path entered
(`-` when it entered none). <events> is what may have written memory on the way, each as
`<v>:<what><instruction>`: memory has number v after it; `a` a call of the allocator, `w` a
store or a bulk write, `x` anything else that may write (another call, `memory.grow`,
`memory.init`); the ones inside a block the path passes or a loop it enters all carry the
number that block or loop gives memory. A statement about a length word in memory v is about
the access's memory only if every `w` with a larger number is itself proved to lie in a
block's payload and no `x` has a larger number. An access with no path is
`NOPATH <function> <instruction> <kind>`.
"""
import hashlib
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wasm_read as W

NONE = 4294967295
MAX_GUESSES = 3
VAL = (0x7F, 0x7E, 0x7D, 0x7C)


def is_un(op):
    return op in (0x45, 0x50) or 0x67 <= op <= 0x69 or 0x79 <= op <= 0x7B or 0x8B <= op <= 0x91 \
        or 0x99 <= op <= 0x9F or 0xA7 <= op <= 0xC4


def is_bin(op):
    return 0x46 <= op <= 0x4F or 0x51 <= op <= 0x66 or 0x6A <= op <= 0x78 or 0x7C <= op <= 0x8A \
        or 0x92 <= op <= 0x98 or 0xA0 <= op <= 0xA6


def arm_ok(x):
    """An instruction GuardWindow's `darm` reads inside an aborting arm."""
    return x.op in (0x41, 0x42, 0x10, 0x1A) or 0x20 <= x.op <= 0x24 or is_un(x.op) or is_bin(x.op) \
        or 0x28 <= x.op <= 0x3E


def bt_ok(x):
    return x.imm == 0x40 or x.imm in VAL


class Shape:
    """Where each block of a function body closes."""

    def __init__(self, ins):
        self.end, self.els, self.opener = {}, {}, {}
        stack = []
        for i, x in enumerate(ins):
            if x.op in (0x02, 0x03, 0x04):
                stack.append(i)
            elif x.op == 0x05:
                self.els[stack[-1]] = i
                self.opener[i] = stack[-1]
            elif x.op == 0x0B:
                if stack:
                    o = stack.pop()
                    self.end[o] = i
                    self.opener[i] = o


def aborting(ins, i):
    """`ins[i]` is `if` with no result: the index of its `end` when the arm is
    `... unreachable end` with only instructions `darm` reads, else None."""
    if ins[i].imm != 0x40:
        return None
    k = i + 1
    while k < len(ins) and arm_ok(ins[k]):
        k += 1
    if k + 1 < len(ins) and ins[k].op == 0x00 and ins[k + 1].op == 0x0B:
        return k + 1
    return None


def pure_arm(ins, a, b):
    """Instructions a..b-1 only compute and leave one value (GuardPath's `parm`)."""
    depth = 0
    for x in ins[a:b]:
        if x.op in (0x20, 0x23, 0x41, 0x42):
            depth += 1
        elif is_un(x.op) or 0x28 <= x.op <= 0x35:
            if depth < 1:
                return False
        elif is_bin(x.op):
            if depth < 2:
                return False
            depth -= 1
        else:
            return False
    return depth == 1


def region(ins, a, b):
    """What instructions a..b-1 may change: the locals assigned, memory, the globals."""
    xs, m, g = set(), False, False
    for x in ins[a:b]:
        if x.op in (0x21, 0x22):
            xs.add(x.imm)
        elif 0x36 <= x.op <= 0x3E or x.op == 0x40 or (x.op == 0xFC and x.sub in (8, 10, 11)):
            m = True
        elif x.op in (0x10, 0x11):
            m = g = True
        elif x.op == 0x24:
            g = True
    return xs, m, g


def units(ins, sh, j, alloc, cap):
    """The path's units before instruction j, nearest first: (first, effect, need), where
    effect is the stack change and need the depth it must find. Stops at anything `pdec`
    does not read."""
    out, k = [], j - 1
    while k >= 0 and j - k <= cap:
        x = ins[k]
        op = x.op
        if op == 0x0B:
            o = sh.opener.get(k)
            if o is None or not bt_ok(ins[o]):
                break
            val = 1 if ins[o].imm in VAL else 0
            if ins[o].op == 0x04:
                if o not in sh.els and val:
                    break
                out.append((o, val - 1, 1))
            else:
                out.append((o, val, 0))
            k = o - 1
            continue
        if op == 0x05:
            o = sh.opener.get(k)
            if o is None or not bt_ok(ins[o]):
                break
            out.append((o, -1, 1))
            k = o - 1
            continue
        if op == 0x04:
            if not bt_ok(x):
                break
            out.append((k, -1, 1))
        elif op in (0x02, 0x03):
            if not bt_ok(x):
                break
            out.append((k, 0, 0))
        elif op == 0x01:
            out.append((k, 0, 0))
        elif op in (0x20, 0x23, 0x41, 0x42, 0x43, 0x44):
            out.append((k, 1, 0))
        elif op in (0x21, 0x1A, 0x0D, 0x24):
            out.append((k, -1, 1))
        elif op == 0x22:
            out.append((k, 0, 1))
        elif op == 0x1B:
            out.append((k, -2, 3))
        elif op == 0x10:
            if x.imm != alloc:
                break
            out.append((k, 0, 1))
        elif 0x28 <= op <= 0x35 or is_un(op):
            out.append((k, 0, 1))
        elif is_bin(op):
            out.append((k, -1, 2))
        elif 0x36 <= op <= 0x3E:
            out.append((k, -2, 2))
        elif op == 0xFC and x.sub in (10, 11):
            out.append((k, -3, 3))
        else:
            break
        k -= 1
    return out


class Run:
    """The symbolic run of instructions s..j-1 as `pdec` and `prun` do it."""

    def __init__(self, ins, sh, s, j, alloc):
        self.env, self.stk, self.reads = {}, [], set()
        self.ver = self.mem = self.glo = 0
        self.loop = None
        self.seen = set()
        self.alloc = alloc
        self.events = []
        i = s
        while i < j:
            i = self.step(ins, sh, i, j, alloc)

    def look(self, x):
        return self.env.get(x, ("L", x))

    def havoc(self, ins, a, b, value):
        xs, m, g = region(ins, a, b)
        for x in xs:
            self.env[x] = ("H", self.ver, x)
        if value:
            self.stk.append(("J", self.ver))
        self.ver += 1
        if m:
            self.mem = self.ver
            i = a
            while i < b:
                y = ins[i]
                # an arm that aborts does not come back: what it calls is not on the way
                e = aborting(ins, i) if y.op == 0x04 else None
                if e is not None and e < b:
                    i = e + 1
                    continue
                if 0x36 <= y.op <= 0x3E or (y.op == 0xFC and y.sub in (10, 11)):
                    self.events.append((self.ver, "w", i))
                elif y.op == 0x10 and y.imm == self.alloc:
                    self.events.append((self.ver, "a", i))
                elif y.op in (0x10, 0x11, 0x40) or (y.op == 0xFC and y.sub == 8):
                    self.events.append((self.ver, "x", i))
                i += 1
        if g:
            self.glo = self.ver

    def arm(self, ins, a, b):
        st = []
        for x in ins[a:b]:
            if x.op == 0x20:
                st.append(self.look(x.imm))
            elif x.op == 0x23:
                st.append(("G", self.glo, x.imm))
            elif x.op in (0x41, 0x42):
                st.append(("K", x.imm))
            elif is_un(x.op):
                st.append(("U", x.op, st.pop()))
            elif is_bin(x.op):
                q, p = st.pop(), st.pop()
                st.append(("B", x.op, p, q))
            else:
                st.append(self.load(x, st.pop()))
        return st[0]

    def load(self, x, a):
        if x.op == 0x28 and x.imm == 4:
            self.reads.add((a, self.mem))
        return ("M", self.mem, x.op, x.imm, a)

    def step(self, ins, sh, i, j, alloc):
        x = ins[i]
        op, stk = x.op, self.stk
        if op in (0x20, 0x21, 0x22):
            self.seen.add(x.imm)
        if op == 0x01:
            pass
        elif op == 0x20:
            stk.append(self.look(x.imm))
        elif op == 0x21:
            self.env[x.imm] = stk.pop()
        elif op == 0x22:
            self.env[x.imm] = stk[-1]
        elif op == 0x23:
            stk.append(("G", self.glo, x.imm))
        elif op == 0x24:
            stk.pop()
            self.ver += 1
            self.glo = self.ver
        elif op in (0x41, 0x42, 0x43, 0x44):
            stk.append(("K", x.imm))
        elif op in (0x1A, 0x0D):
            stk.pop()
        elif op == 0x1B:
            c, b, a = stk.pop(), stk.pop(), stk.pop()
            stk.append(("S", a, b, c))
        elif op == 0x10:
            stk.pop()
            stk.append(("A", self.ver))
            self.ver += 1
            self.mem = self.glo = self.ver
            self.events.append((self.ver, "a", i))
        elif op == 0x02:
            e = sh.end[i]
            if e < j:
                self.havoc(ins, i + 1, e, x.imm in VAL)
                return e + 1
        elif op == 0x03:
            e = sh.end[i]
            if e < j:
                self.havoc(ins, i + 1, e, x.imm in VAL)
                return e + 1
            self.havoc(ins, i + 1, e, False)
            if self.loop is None:
                self.loop = i
        elif op == 0x04:
            e = aborting(ins, i)
            if e is not None and e < j:
                stk.pop()
                return e + 1
            e, el = sh.end[i], sh.els.get(i)
            if e < j:
                if el is not None and x.imm in VAL and pure_arm(ins, i + 1, el) and pure_arm(ins, el + 1, e):
                    a, b = self.arm(ins, i + 1, el), self.arm(ins, el + 1, e)
                    stk.append(("S", a, b, stk.pop()))
                else:
                    stk.pop()
                    self.havoc(ins, i + 1, e, x.imm in VAL)
                return e + 1
            stk.pop()
            if el is not None and el < j:
                return el + 1
        elif 0x28 <= op <= 0x35:
            stk.append(self.load(x, stk.pop()))
        elif is_un(op):
            stk.append(("U", op, stk.pop()))
        elif is_bin(op):
            b, a = stk.pop(), stk.pop()
            stk.append(("B", op, a, b))
        elif 0x36 <= op <= 0x3E:
            stk.pop()
            stk.pop()
            self.ver += 1
            self.mem = self.ver
            self.events.append((self.ver, "w", i))
        else:
            stk.pop()
            stk.pop()
            stk.pop()
            self.ver += 1
            self.mem = self.ver
            self.events.append((self.ver, "w", i))
        return i + 1


def addends(t):
    if t[0] == "B" and t[1] == 0x6A:
        return addends(t[2]) + addends(t[3])
    return [t]


def guesses(run, addr, word_off):
    """The blocks a range may lie in, as `F<k>` and `L<local>.<v>`; `h` for a word in the
    first twelve bytes of whatever the address is."""
    ads = addends(addr)
    if word_off is not None and word_off < 12 and len(ads) == 1:
        return ["h"]
    out = []
    now = {x: run.look(x) for x in run.seen}
    for a in ads:
        if a[0] == "K":
            continue
        if a[0] == "A":
            out.append(f"F{a[1]}")
            continue
        holders = sorted(x for x in now if now[x] == a)
        if not holders:
            continue
        vs = sorted(v for (b, v) in run.reads if b == a) or sorted({0, run.mem})
        for v in vs:
            out.append(f"L{holders[0]}.{v}")
    return out[:MAX_GUESSES]


def kind_of(x):
    if 0x28 <= x.op <= 0x35:
        return "load", 1
    if 0x36 <= x.op <= 0x3E:
        return "store", 2
    if x.op == 0xFC and x.sub == 10:
        return "copy", 3
    if x.op == 0xFC and x.sub == 11:
        return "fill", 3
    return None, 0


def text(x):
    return f"{x.op}.{x.sub}.{x.imm}"


def main():
    args = sys.argv[1:]
    cap, only = 400, None
    if "--max" in args:
        k = args.index("--max")
        cap = int(args[k + 1])
        del args[k:k + 2]
    if "--only" in args:
        k = args.index("--only")
        only = int(args[k + 1])
        del args[k:k + 2]
    m = W.Module(args[0])
    alloc = NONE if args[1] == "-" else int(args[1])
    for k in range(len(m.bodies)):
        if only is not None and k + m.nimp != only:
            continue
        try:
            ins = W.instructions(m, k)
        except W.Unread as e:
            print(f"UNREAD {k + m.nimp} {e}")
            continue
        sh = Shape(ins)
        for j, x in enumerate(ins):
            kind, operands = kind_of(x)
            if kind is None:
                continue
            us = units(ins, sh, j, alloc, cap)
            best = None
            for cut in range(len(us), 0, -1):
                depth, ok = 0, True
                for first, eff, need in reversed(us[:cut]):
                    if depth < need:
                        ok = False
                        break
                    depth += eff
                if ok and depth >= operands:
                    best = us[cut - 1][0]
                    break
            if best is None:
                print(f"NOPATH {k + m.nimp} {j} {kind}")
                continue
            run = Run(ins, sh, best, j, alloc)
            stk = run.stk
            wr = rd = ["."]
            if kind == "load":
                rd = guesses(run, stk[-1], x.imm)
            elif kind == "store":
                wr = guesses(run, stk[-2], x.imm)
            elif kind == "copy":
                wr, rd = guesses(run, stk[-3], None), guesses(run, stk[-2], None)
            else:
                wr = guesses(run, stk[-3], None)
            raw = m.bytes[ins[best].at:x.end]
            rest = m.bytes[x.end:ins[sh.end[run.loop]].end] if run.loop is not None else b""
            sig = hashlib.sha256(" ".join(text(y) for y in ins[max(0, j - 8):j + 1]).encode()).hexdigest()[:12]
            evs = " ".join(f"{v}:{tag}{i}" for v, tag, i in run.events) or "-"
            print(f"ACCESS {k + m.nimp} {j} {kind} {alloc} {sig} {best} | {' '.join(wr) or '-'} | {' '.join(rd) or '-'} | "
                  f"{' '.join(str(b) for b in raw)} | {' '.join(str(b) for b in rest) or '-'} | {evs}")


if __name__ == "__main__":
    main()
