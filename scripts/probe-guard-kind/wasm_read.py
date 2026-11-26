#!/usr/bin/env python3
"""A reader of shipped modules for trying rule candidates (untrusted: it only measures).

Module(path) gives:
  .types        [(params, results)]            the type section
  .ftype        [type index]                   of every function, imported ones first
  .nimp         number of imported functions
  .imports      [(module, name)]               of the imported functions
  .names        {function index: name}         from the name section, when there is one
  .table_funcs  the function indices an element segment holds (None: a segment this reader
                does not read, so any function may be called indirectly)
  .exports      {function index} exported
  .bodies       [(locals, start, end)]         per defined function: declared local count
                                               (parameters not included), instruction bytes
  .bytes

instructions(m, k) -> [Ins] for defined function k, every instruction in order (block, else and
end included), each with .at .end .op .sub .imm.

origins(m, k) -> (ins, addr, local_assign, call_args, returns):
  addr[i]          for a load or store at instruction i: the origin term of its address operand
  local_assign[x]  the origin terms ever stored into local x (local.set, local.tee)
  call_args        [(i, callee or None, [origin terms of the arguments])]
  returns          [origin terms of values that leave the function]
An origin term is a tuple:
  ("L", x)                 local.get x
  ("K", value)             a constant
  ("G", g)                 global.get g
  ("M", op, off, term)     the result of a load
  ("C", callee, i)         the result of the call at instruction i (callee None: indirect)
  ("B", op, a, b)          a binary operation
  ("U", op, a)             a unary operation or conversion
  ("S", a, b)              select
  ("P", (terms...))        one of several values: the result of a block, an `if` or a `br`'s
                           target, by whichever path the run took
  ("J", i)                 a value this reader does not follow (instruction i)
"""
import sys


class Unread(Exception):
    pass


def uleb(b, p):
    r = s = 0
    while True:
        if p >= len(b):
            raise Unread("a number runs past the end")
        x = b[p]
        p += 1
        r |= (x & 0x7F) << s
        s += 7
        if x < 0x80:
            return r, p


def sleb(b, p):
    r = s = 0
    while True:
        x = b[p]
        p += 1
        r |= (x & 0x7F) << s
        s += 7
        if x < 0x80:
            if x & 0x40:
                r -= 1 << s
            return r, p


VALTYPES = (0x7F, 0x7E, 0x7D, 0x7C, 0x7B, 0x70, 0x6F)


class Module:
    def __init__(self, path):
        b = self.bytes = open(path, "rb").read()
        if b[:8] != b"\0asm\x01\0\0\0":
            raise Unread("not a version-1 module")
        self.types, self.ftype, self.imports, self.bodies, self.names = [], [], [], [], {}
        self.nimp = 0
        self.globals_imported = 0
        self.table_funcs, self.exports = set(), set()
        p = 8
        while p < len(b):
            sid = b[p]
            size, p = uleb(b, p + 1)
            end = p + size
            if sid == 1:
                n, q = uleb(b, p)
                for _ in range(n):
                    if b[q] != 0x60:
                        raise Unread(f"a type that is not a function type (0x{b[q]:02x})")
                    np_, q = uleb(b, q + 1)
                    ps = list(b[q:q + np_]); q += np_
                    nr, q = uleb(b, q)
                    rs = list(b[q:q + nr]); q += nr
                    self.types.append((ps, rs))
            elif sid == 2:
                n, q = uleb(b, p)
                for _ in range(n):
                    ln, q = uleb(b, q); mod = b[q:q + ln].decode("utf-8", "replace"); q += ln
                    ln, q = uleb(b, q); nm = b[q:q + ln].decode("utf-8", "replace"); q += ln
                    kind = b[q]; q += 1
                    if kind == 0:
                        t, q = uleb(b, q)
                        self.ftype.append(t); self.imports.append((mod, nm)); self.nimp += 1
                    elif kind == 1:
                        q += 1
                        fl = b[q]; _, q = uleb(b, q + 1)
                        if fl & 1:
                            _, q = uleb(b, q)
                    elif kind == 2:
                        fl = b[q]; _, q = uleb(b, q + 1)
                        if fl & 1:
                            _, q = uleb(b, q)
                    elif kind == 3:
                        q += 2; self.globals_imported += 1
                    elif kind == 4:
                        _, q = uleb(b, q + 1)
                    else:
                        raise Unread(f"an import of kind {kind}")
            elif sid == 3:
                n, q = uleb(b, p)
                for _ in range(n):
                    t, q = uleb(b, q)
                    self.ftype.append(t)
            elif sid == 7:
                n, q = uleb(b, p)
                for _ in range(n):
                    ln, q = uleb(b, q); q += ln
                    kind = b[q]
                    idx, q = uleb(b, q + 1)
                    if kind == 0:
                        self.exports.add(idx)
            elif sid == 9:
                try:
                    n, q = uleb(b, p)
                    for _ in range(n):
                        fl, q = uleb(b, q)
                        if fl not in (0, 1, 2, 3):
                            raise Unread("an element segment of expressions")
                        if fl == 2:
                            _, q = uleb(b, q)
                        if fl in (0, 2):
                            while b[q] != 0x0B:      # the offset expression: constants and global.get
                                if b[q] in (0x41, 0x42):
                                    _, q = sleb(b, q + 1)
                                elif b[q] == 0x23:
                                    _, q = uleb(b, q + 1)
                                else:
                                    raise Unread("an offset expression")
                            q += 1
                        if fl in (1, 2, 3):
                            q += 1                   # elemkind
                        cnt, q = uleb(b, q)
                        for _ in range(cnt):
                            f, q = uleb(b, q)
                            if self.table_funcs is not None:
                                self.table_funcs.add(f)
                except (Unread, IndexError):
                    self.table_funcs = None
            elif sid == 10:
                n, q = uleb(b, p)
                for _ in range(n):
                    sz, q = uleb(b, q)
                    fend = q + sz
                    nl, r = uleb(b, q)
                    locs = 0
                    for _ in range(nl):
                        c, r = uleb(b, r)
                        locs += c
                        r += 1
                    self.bodies.append((locs, r, fend))
                    q = fend
            elif sid == 0:
                ln, q = uleb(b, p)
                if b[q:q + ln] == b"name":
                    q += ln
                    try:
                        while q < end:
                            sub = b[q]
                            ssz, q = uleb(b, q + 1)
                            send = q + ssz
                            if sub == 1:
                                n, r = uleb(b, q)
                                for _ in range(n):
                                    idx, r = uleb(b, r)
                                    ln2, r = uleb(b, r)
                                    self.names[idx] = b[r:r + ln2].decode("utf-8", "replace")
                                    r += ln2
                            q = send
                    except (IndexError, Unread):
                        pass
            p = end

    def sig(self, f):
        return self.types[self.ftype[f]]

    def nparams(self, k):
        return len(self.sig(self.nimp + k)[0])


class Ins:
    __slots__ = ("at", "end", "op", "sub", "imm")

    def __init__(self, at, end, op, sub, imm):
        self.at, self.end, self.op, self.sub, self.imm = at, end, op, sub, imm


def instructions(m, k):
    b = m.bytes
    _, p, e = m.bodies[k]
    out = []
    while p < e:
        at = p
        op = b[p]
        p += 1
        sub, imm = None, None
        if op in (0x02, 0x03, 0x04):
            if b[p] == 0x40 or b[p] in VALTYPES:
                imm = b[p]; p += 1
            else:
                t, p = sleb(b, p)
                imm = ("type", t)
        elif op in (0x0C, 0x0D, 0x10, 0x12, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0xD2):
            imm, p = uleb(b, p)
        elif op == 0x0E:
            n, p = uleb(b, p)
            ls = []
            for _ in range(n + 1):
                x, p = uleb(b, p)
                ls.append(x)
            imm = ls
        elif op in (0x11, 0x13):
            t, p = uleb(b, p)
            tb, p = uleb(b, p)
            imm = (t, tb)
        elif op == 0x1C:
            n, p = uleb(b, p)
            p += n
        elif 0x28 <= op <= 0x3E:
            al, p = uleb(b, p)
            if al & 0x40:
                raise Unread("a memory index on an access")
            off, p = uleb(b, p)
            imm = off
        elif op in (0x3F, 0x40):
            p += 1
        elif op == 0x41:
            imm, p = sleb(b, p)
            imm &= 0xFFFFFFFF
        elif op == 0x42:
            imm, p = sleb(b, p)
            imm &= 0xFFFFFFFFFFFFFFFF
        elif op == 0x43:
            p += 4
        elif op == 0x44:
            p += 8
        elif op == 0xD0:
            p += 1
        elif op == 0xFC:
            sub, p = uleb(b, p)
            if sub <= 7:
                pass
            elif sub == 8:
                _, p = uleb(b, p); p += 1
            elif sub in (9, 13, 15, 16, 17):
                _, p = uleb(b, p)
            elif sub == 10:
                p += 2
            elif sub == 11:
                p += 1
            elif sub in (12, 14):
                _, p = uleb(b, p); _, p = uleb(b, p)
            else:
                raise Unread(f"0xfc {sub}")
        elif op in (0xFD, 0xFE) or op in (0x06, 0x07, 0x08, 0x09, 0x0A, 0x14, 0x15, 0x18, 0x19, 0x1F):
            raise Unread(f"opcode 0x{op:02x}")
        elif op > 0xC4 and op not in (0xD0, 0xD1, 0xD2):
            raise Unread(f"opcode 0x{op:02x}")
        out.append(Ins(at, p, op, sub, imm))
    if p != e:
        raise Unread("the body's last instruction runs past its end")
    return out


WIDTH = {0x28: 4, 0x29: 8, 0x2A: 4, 0x2B: 8, 0x2C: 1, 0x2D: 1, 0x2E: 2, 0x2F: 2, 0x30: 1, 0x31: 1,
         0x32: 2, 0x33: 2, 0x34: 4, 0x35: 4, 0x36: 4, 0x37: 8, 0x38: 4, 0x39: 8, 0x3A: 1, 0x3B: 2,
         0x3C: 1, 0x3D: 2, 0x3E: 4}


def is_load(op):
    return 0x28 <= op <= 0x35


def is_store(op):
    return 0x36 <= op <= 0x3E


def origins(m, k, conds=None):
    """With `conds` a dict, it is filled: instruction of an `if` or a `br_if` -> its condition term."""
    ins = instructions(m, k)
    fidx = m.nimp + k
    nres_fn = len(m.sig(fidx)[1])
    addr, local_assign, call_args, returns = {}, {}, [], []
    stack = []
    # control frames: [kind, base height, results, unreachable, targeted, params]
    frames = [["fn", 0, nres_fn, False, False, 0, []]]

    def J(i):
        return ("J", i)

    def pop(i):
        fr = frames[-1]
        if len(stack) > fr[1]:
            return stack.pop()
        if fr[3]:
            return J(i)
        raise Unread(f"the stack is empty at instruction {i}")

    def bt(imm):
        if imm == 0x40:
            return 0, 0
        if isinstance(imm, tuple):
            ps, rs = m.types[imm[1]]
            return len(ps), len(rs)
        return 0, 1

    def leave(i):
        fr = frames[-1]
        del stack[fr[1]:]
        fr[3] = True

    def top(n, i):
        have = len(stack) - frames[-1][1]
        return [stack[len(stack) - n + j] if have >= n - j else J(i) for j in range(n)]

    def phi(rows, n, i):
        out = []
        for j in range(n):
            seen = []
            for r in rows:
                t = r[j]
                for u in (t[1] if t[0] == "P" else (t,)):
                    if u not in seen:
                        seen.append(u)
            out.append(seen[0] if len(seen) == 1 else (("P", tuple(seen)) if seen else J(i)))
        return out

    def branch_values(depth, i):
        fr = frames[-1 - depth]
        fr[4] = True
        if fr[0] == "fn":
            returns.extend(top(fr[2], i))
        elif fr[0] != "loop" and fr[2]:
            fr[6].append(top(fr[2], i))

    for i, x in enumerate(ins):
        op = x.op
        if op in (0x00,):
            leave(i)
        elif op == 0x01:
            pass
        elif op in (0x02, 0x03, 0x04):
            if op == 0x04:
                c_ = pop(i)
                if conds is not None:
                    conds[i] = c_
            np_, nr = bt(x.imm)
            base = len(stack) - np_
            if base < frames[-1][1]:
                if not frames[-1][3]:
                    raise Unread(f"block parameters missing at {i}")
                base = len(stack)
            frames.append([{0x02: "block", 0x03: "loop", 0x04: "if"}[op], base, nr, False, False, np_, []])
        elif op == 0x05:
            fr = frames[-1]
            if not fr[3] and fr[2]:
                fr[6].append(top(fr[2], i))
            fr[0] = "else"
            del stack[fr[1]:]
            stack.extend(J(i) for _ in range(fr[5]))
            fr[3] = False
        elif op == 0x0B:
            fr = frames.pop()
            kind, base, nr, unreach, targeted, _, incoming = fr
            if kind == "fn":
                if not unreach:
                    returns.extend(stack[len(stack) - nr:] if nr else [])
                break
            if not unreach and nr:
                frames.append(fr); incoming.append(top(nr, i)); frames.pop()
            if kind == "if" and nr:
                incoming.append([J(i)] * nr)      # an `if` with a result and no `else` does not validate
            vals = phi(incoming, nr, i) if nr else []
            del stack[base:]
            stack.extend(vals)
        elif op == 0x0C:
            branch_values(x.imm, i)
            leave(i)
        elif op == 0x0D:
            c_ = pop(i)
            if conds is not None:
                conds[i] = c_
            branch_values(x.imm, i)
        elif op == 0x0E:
            pop(i)
            for d in x.imm:
                branch_values(d, i)
            leave(i)
        elif op == 0x0F:
            n = nres_fn
            returns.extend(stack[len(stack) - n:] if n and len(stack) - frames[-1][1] >= n else [J(i)] * n)
            leave(i)
        elif op in (0x10, 0x12):
            ps, rs = m.sig(x.imm)
            args = [pop(i) for _ in ps][::-1]
            call_args.append((i, x.imm, args))
            if op == 0x12:
                returns.extend(("C", x.imm, i) for _ in rs)
                leave(i)
            else:
                stack.extend(("C", x.imm, i) for _ in rs)
        elif op in (0x11, 0x13):
            pop(i)
            ps, rs = m.types[x.imm[0]]
            args = [pop(i) for _ in ps][::-1]
            call_args.append((i, None, args))
            if op == 0x13:
                returns.extend(("C", None, i) for _ in rs)
                leave(i)
            else:
                stack.extend(("C", None, i) for _ in rs)
        elif op == 0x1A:
            pop(i)
        elif op in (0x1B, 0x1C):
            pop(i)
            b_ = pop(i)
            a_ = pop(i)
            stack.append(("S", a_, b_))
        elif op == 0x20:
            stack.append(("L", x.imm))
        elif op == 0x21:
            local_assign.setdefault(x.imm, []).append(pop(i))
        elif op == 0x22:
            v = pop(i)
            local_assign.setdefault(x.imm, []).append(v)
            stack.append(v)
        elif op == 0x23:
            stack.append(("G", x.imm))
        elif op == 0x24:
            pop(i)
        elif op == 0x25:
            pop(i); stack.append(J(i))
        elif op == 0x26:
            pop(i); pop(i)
        elif is_load(op):
            a = pop(i)
            addr[i] = a
            stack.append(("M", op, x.imm, a))
        elif is_store(op):
            pop(i)
            addr[i] = pop(i)
        elif op == 0x3F:
            stack.append(J(i))
        elif op == 0x40:
            pop(i); stack.append(J(i))
        elif op in (0x41, 0x42):
            stack.append(("K", x.imm))
        elif op in (0x43, 0x44):
            stack.append(J(i))
        elif op in (0x45, 0x50) or 0x67 <= op <= 0x69 or 0x79 <= op <= 0x7B or 0x8B <= op <= 0x91 \
                or 0x99 <= op <= 0x9F or 0xA7 <= op <= 0xC4 or op == 0xD1:
            stack.append(("U", op, pop(i)))
        elif 0x46 <= op <= 0x4F or 0x51 <= op <= 0x66 or 0x6A <= op <= 0x78 or 0x7C <= op <= 0x8A \
                or 0x92 <= op <= 0x98 or 0xA0 <= op <= 0xA6:
            b_ = pop(i)
            a_ = pop(i)
            stack.append(("B", op, a_, b_))
        elif op in (0xD0, 0xD2):
            stack.append(J(i))
        elif op == 0xFC:
            s = x.sub
            if s <= 7:
                stack.append(("U", 0xFC00 + s, pop(i)))
            elif s in (8, 10, 11, 12, 14, 17):
                pop(i); pop(i); pop(i)
            elif s == 15:
                pop(i); pop(i); stack.append(J(i))
            elif s == 16:
                stack.append(J(i))
        else:
            raise Unread(f"opcode 0x{op:02x} has no stack rule here")
    return ins, addr, local_assign, call_args, returns


def peel(t):
    """(root term, constant added to it), through i32.add with a constant on either side."""
    c = 0
    while t[0] == "B" and t[1] == 0x6A:
        if t[3][0] == "K":
            c += t[3][1]; t = t[2]
        elif t[2][0] == "K":
            c += t[2][1]; t = t[3]
        else:
            break
    return t, c & 0xFFFFFFFF


if __name__ == "__main__":
    m = Module(sys.argv[1])
    acc = 0
    for k in range(len(m.bodies)):
        ins, addr, la, ca, rets = origins(m, k)
        acc += len(addr)
    print(f"functions {len(m.bodies)} (+{m.nimp} imported), accesses {acc}, names {len(m.names)}")
