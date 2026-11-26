#!/usr/bin/env python3
"""Cut the copy windows of a module: for every `memory.copy`, the longest straight-line run
before it that proofs/GuardCopy.v's `pdec` reads (untrusted: it only proposes; the kernel
decodes the bytes again and proves or does not prove each statement).

  cut_copies.py <module.wasm> <allocator index>

The allocator index is the anchor file's `alloc` line (4294967295 for none): a window may
call that function and no other. Prints, per copy,

  COPY <function> <instruction> <allocator> | <written> | <read> | <bytes>

where <written> and <read> are the cutter's guesses at the block each side lies in, zero or
more of `L<local>.<v>` (the block that local holds at the copy, by the length word read
after the window's v-th allocation) and `F<k>` (the block the k-th allocation returned), or
`-` for no guess. A copy with no window of three operands is `NOWINDOW <function> <instruction>`.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wasm_read as W

NONE = 4294967295
MAX_GUESSES = 4


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


def aborting_arm(ins, i):
    """`ins[i]` is `if` (empty block type): the index of its `end` when the arm is
    `... unreachable end` with only instructions `darm` reads, else None."""
    k = i + 1
    while k < len(ins) and arm_ok(ins[k]):
        k += 1
    if k + 1 < len(ins) and ins[k].op == 0x00 and ins[k + 1].op == 0x0B:
        return k + 1
    return None


def units(ins, j, alloc):
    """The window's units before instruction j, nearest first: (first, last, effect, need),
    where effect is the stack change and need the depth it must find. Stops at anything
    `pdec` does not read."""
    arms = {}
    for i, x in enumerate(ins[:j]):
        if x.op == 0x04 and x.imm == 0x40:
            e = aborting_arm(ins, i)
            if e is not None and e < j:
                arms[e] = i
    out, k = [], j - 1
    while k >= 0:
        x = ins[k]
        if x.op == 0x0B:
            if k not in arms:
                break
            out.append((arms[k], k, -1, 1))
            k = arms[k] - 1
            continue
        if x.op == 0x04:
            if x.imm != 0x40:
                break
            out.append((k, k, -1, 1))
        elif x.op in (0x20, 0x23, 0x41, 0x42):
            out.append((k, k, 1, 0))
        elif x.op in (0x21, 0x1A, 0x0D):
            out.append((k, k, -1, 1))
        elif x.op == 0x22:
            out.append((k, k, 0, 1))
        elif x.op == 0x1B:
            out.append((k, k, -2, 3))
        elif x.op == 0x10 and x.imm == alloc:
            out.append((k, k, 0, 1))
        elif 0x28 <= x.op <= 0x35 or is_un(x.op):
            out.append((k, k, 0, 1))
        elif is_bin(x.op):
            out.append((k, k, -1, 2))
        else:
            break
        k -= 1
    return out


def term_run(ins, first, j):
    """The symbolic run of instructions first..j-1 as `prun` does it:
    (stack, env, conditions, allocations made)."""
    env, stk, conds, v = {}, [], [], 0
    k = first
    while k < j:
        x = ins[k]
        if x.op == 0x20:
            stk.append(env.get(x.imm, ("L", x.imm)))
        elif x.op == 0x21:
            env[x.imm] = stk.pop()
        elif x.op == 0x22:
            env[x.imm] = stk[-1]
        elif x.op == 0x23:
            stk.append(("G", v, x.imm))
        elif x.op in (0x41, 0x42):
            stk.append(("K", x.imm))
        elif x.op == 0x1A:
            stk.pop()
        elif x.op == 0x1B:
            c, b, a = stk.pop(), stk.pop(), stk.pop()
            stk.append(("S", a, b, c))
        elif x.op == 0x0D:
            conds.append(stk.pop())
        elif x.op == 0x04:
            conds.append(stk.pop())
            e = aborting_arm(ins, k)
            if e is not None and e < j:
                k = e
        elif x.op == 0x10:
            conds.append(stk.pop())
            stk.append(("A", v))
            v += 1
        elif 0x28 <= x.op <= 0x35:
            stk.append(("M", v, x.op, x.imm, stk.pop()))
        elif is_un(x.op):
            stk.append(("U", x.op, stk.pop()))
        else:
            b, a = stk.pop(), stk.pop()
            stk.append(("B", x.op, a, b))
        k += 1
    return stk, env, conds, v


def addends(t):
    if t[0] == "B" and t[1] == 0x6A:
        return addends(t[2]) + addends(t[3])
    return [t]


def length_reads(t, out):
    """Every (block term, version) whose length word (`i32.load` at + 4) is read in `t`."""
    if t[0] == "M":
        if t[2] == 0x28 and t[3] == 4:
            out.add((t[4], t[1]))
        length_reads(t[4], out)
    elif t[0] == "U":
        length_reads(t[2], out)
    elif t[0] == "B":
        length_reads(t[2], out)
        length_reads(t[3], out)
    elif t[0] == "S":
        for u in t[1:]:
            length_reads(u, out)


def guesses(addr, env, seen, reads, version):
    """The blocks one side may lie in, as `F<k>` and `L<local>.<v>`."""
    out = []
    now = {x: env.get(x, ("L", x)) for x in seen}
    for a in addends(addr):
        if a[0] == "K":
            continue
        if a[0] == "A":
            out.append(f"F{a[1]}")
            continue
        holders = sorted(x for x in now if now[x] == a)
        if not holders:
            continue
        vs = sorted(v for (b, v) in reads if b == a) or [version]
        for v in vs:
            out.append(f"L{holders[0]}.{v}")
    return out[:MAX_GUESSES]


def main():
    m = W.Module(sys.argv[1])
    alloc = int(sys.argv[2])
    for k in range(len(m.bodies)):
        try:
            ins = W.instructions(m, k)
        except W.Unread as e:
            print(f"UNREAD {k + m.nimp} {e}")
            continue
        for j, x in enumerate(ins):
            if not (x.op == 0xFC and x.sub == 10):
                continue
            us = units(ins, j, alloc)
            # the longest suffix of units that runs from an empty stack and leaves three
            best = None
            for cut in range(len(us), 0, -1):
                depth, ok = 0, True
                for first, last, eff, need in reversed(us[:cut]):
                    if depth < need:
                        ok = False
                        break
                    depth += eff
                if ok and depth >= 3:
                    best = us[cut - 1][0]
                    break
            if best is None:
                print(f"NOWINDOW {k + m.nimp} {j}")
                continue
            stk, env, conds, version = term_run(ins, best, j)
            seen = {y.imm for y in ins[best:j] if y.op in (0x20, 0x21, 0x22)}
            reads = set()
            for t in stk + conds + list(env.values()):
                length_reads(t, reads)
            wr = guesses(stk[-3], env, seen, reads, version)
            rd = guesses(stk[-2], env, seen, reads, version)
            raw = m.bytes[ins[best].at:x.end]
            print(f"COPY {k + m.nimp} {j} {alloc} | {' '.join(wr) or '-'} | {' '.join(rd) or '-'} | "
                  f"{' '.join(str(b) for b in raw)}")


if __name__ == "__main__":
    main()
