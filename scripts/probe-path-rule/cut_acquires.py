#!/usr/bin/env python3
"""Propose, per recorded frame, the instruction that performs each named acquire (untrusted:
it only proposes; proofs/GuardAcquire.v's `acquire_ok` is evaluated by the kernel on what
this prints).

  cut_acquires.py <module.wasm> <anchors>

The anchor file's `rcfns` line names the rc_inc routine first; its `rc` line holds, per frame
(a position among the defined functions), the recorded events. A named acquire is `a<n>`:
n a local, `0xA0000000 | local` the occupant of the cell that local holds, `0xC0000000 |
global` a global. For every call of the rc_inc routine in the frame's function, the longest
run of bytes before it that GuardPath's `pdec` reads is run as `prun` runs it, and what the
call is handed is compared with what each local holds there. Acquires are then matched to
calls, one call each (a maximum matching).

Prints, per frame that has a named acquire,

  FRAME <position> <function> <allocator> <rc_inc> <placed>/<acquires> <calls> | <certificates> | <not placed> | <body bytes>

<calls> being how many calls of the rc_inc routine the body holds (4294967295 stands for a
module with no rc_inc routine: no call is one of it), and a certificate being `L<local>:<start>:<length>`, `C<local>:<start>:<length>` (a cell's
occupant) or `G<global>:<start>:<length>`: the window's first byte and length in the body.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wasm_read as W
import cut_paths as CP

GLOBAL_HOLDER, CELL_HOLDER = 0xC0000000, 0xA0000000


def read_anchors(path):
    inc, alloc, frames = None, None, {}
    for line in open(path, encoding="utf-8"):
        t = line.rstrip("\n").split(" ", 1)
        rest = t[1] if len(t) > 1 else ""
        if t[0] == "rcfns" and rest.split():
            first = rest.split()[0]
            inc = None if first == "-" else int(first)
        elif t[0] == "alloc" and rest.split() and rest.split()[0].isdigit():
            alloc = int(rest.split()[0])
        elif t[0] == "rc" and rest:
            for part in rest.split(";"):
                if "|" in part:
                    pos, evs = part.split("|", 1)
                    frames[int(pos)] = evs
    return inc, alloc, frames


def acquires(evs):
    out = []
    for t in evs.split():
        if t[0] == "a" and len(t) > 1:
            name = t[1:].split("@")[0]
            if name.isdigit():
                out.append(int(name))
    return out


def holders(run, top):
    """What the call's operand is, as holders: `L<x>` when local x holds it there, `C<x>`
    when it is a word loaded through the address local x holds, `G<g>` a global."""
    out = set()
    now = {x: run.look(x) for x in run.seen}
    for x, t in now.items():
        if t == top:
            out.add(f"L{x}")
    if top[0] == "M" and top[2] == 0x28:
        for x, t in now.items():
            if t == top[4]:
                out.add(f"C{x}")
    if top[0] == "G":
        out.add(f"G{top[2]}")
    return out


def wanted(n):
    """The holders an acquire named n may be placed on (the byte-level check's `through`)."""
    if n & GLOBAL_HOLDER == GLOBAL_HOLDER:
        return {f"G{n & 0x3FFFFFFF}"}
    if n & CELL_HOLDER == CELL_HOLDER:
        return {f"C{n & 0x1FFFFFFF}"}
    return {f"L{n}", f"C{n}"}


def match(want, nodes):
    owner = {}

    def augment(e, seen):
        for i, (_, hs) in enumerate(nodes):
            if want[e] & hs and i not in seen:
                seen.add(i)
                if i not in owner or augment(owner[i], seen):
                    owner[i] = e
                    return True
        return False

    for e in range(len(want)):
        augment(e, set())
    return {e: i for i, e in owner.items()}


def main():
    m = W.Module(sys.argv[1])
    inc, alloc, frames = read_anchors(sys.argv[2])
    alloc = CP.NONE if alloc is None else alloc
    for pos in sorted(frames):
        names = acquires(frames[pos])
        if not names or pos >= len(m.bodies):
            continue
        try:
            ins = W.instructions(m, pos)
        except W.Unread as e:
            print(f"UNREAD {pos} {e}")
            continue
        sh = CP.Shape(ins)
        base = ins[0].at
        nodes = []
        for j, x in enumerate(ins):
            if x.op != 0x10 or x.imm != inc or inc is None:
                continue
            us = CP.units(ins, sh, j, alloc, 200)
            # the window stays in the block the call stands in: `acquire_ok` is given no bytes
            # after the call, so a loop entered on the way could not be closed
            for n, (first, _, _) in enumerate(us):
                if ins[first].op in (0x02, 0x03, 0x04) and sh.end.get(first, -1) >= j:
                    us = us[:n]
                    break
            best = None
            for cut in range(len(us), 0, -1):
                depth, ok = 0, True
                for first, eff, need in reversed(us[:cut]):
                    if depth < need:
                        ok = False
                        break
                    depth += eff
                if ok and depth >= 1:
                    best = us[cut - 1][0]
                    break
            if best is None:
                continue
            run = CP.Run(ins, sh, best, j, alloc)
            nodes.append(((ins[best].at - base, x.end - ins[best].at), holders(run, run.stk[-1])))
        want = [wanted(n) for n in names]
        got = match(want, nodes)
        certs, left = [], []
        for e, n in enumerate(names):
            if e in got:
                (start, length), hs = nodes[got[e]]
                h = sorted(want[e] & hs)[0]
                certs.append(f"{h}:{start}:{length}")
            else:
                left.append(str(n))
        body = m.bytes[base:ins[-1].end]
        ncalls = sum(1 for x in ins if x.op == 0x10 and x.imm == inc and inc is not None)
        print(f"FRAME {pos} {pos + m.nimp} {alloc} {CP.NONE if inc is None else inc} {len(certs)}/{len(names)} {ncalls} | "
              f"{' '.join(certs) or '-'} | {' '.join(left) or '-'} | {' '.join(str(b) for b in body)}")


if __name__ == "__main__":
    main()
