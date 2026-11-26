#!/usr/bin/env python3
"""Reference for the kind pass a kernel rule would run (untrusted: it measures and proposes).

  kinds_ref.py <dir of modules> [--limit N] [--out <dir>]
  kinds_ref.py goals <dir of modules> <work dir (new)> [--limit N]
      one Rocq file per module, <work>/K_<n>.v: the module's tables and claims as a `ctx` of
      proofs/GuardKind.v, and per function the goal that `header_ends` of its body IS the list
      this script places in a header; <work>/index.tsv (file, module, functions, header accesses).

Every value on the operand stack is one of
  ("W", c)   a WHOLE value plus the constant c: a value no arithmetic of this function made
             (a whole parameter or local, a 32- or 64-bit word read from memory, the result of
             a call whose result is whole, a global), with constants added to it since
  ("K", c)   a constant
  "A"        anything else
A CLAIM SET for a module says which parameters (function, index), which results (function),
which locals (function, index) are whole, and the same per function type for indirect calls.
The pass walks one body with the stack discipline of validation and FAILS a claim when

  - a local claimed whole is assigned something that is not ("W", 0) or a constant;
  - an argument at a whole parameter's position is not ("W", 0) or a constant;
  - a value returned by a function whose result is claimed whole is not ("W", 0) or a constant;
  - a global claimed whole is set to something that is not ("W", 0) or a constant.

The result of a block, an `if` or a loop is "A" (nothing is joined), and so is every float.
The chooser starts from every claim and drops the ones that fail until none does (the greatest
consistent set). A header access is then one whose address is ("W", c) with
c + offset + width <= 12.

Prints how many accesses the pass places in a header, and how many functions have every load
and store either proposed to one of the three kernel rules or placed in a header.
"""
import collections
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wasm_read as W
from classify import proposals, routines

KEEP = (0xA7, 0xAC, 0xAD)
A = "A"
W0 = ("W", 0)
M32 = 0xFFFFFFFF


def whole(v):
    return v == W0 or (v != A and v[0] == "K")


class Pass:
    def __init__(self, m, alloc):
        self.m, self.alloc = m, alloc
        self.ins = {}
        self.unread = {}
        for k in range(len(m.bodies)):
            try:
                self.ins[k] = W.instructions(m, k)
            except W.Unread as e:
                self.unread[k] = str(e)
        nf = len(m.ftype)
        self.not_local = set()      # (f, x) dropped
        self.not_param = set()      # (f, j)
        self.not_result = set()     # f
        self.not_tparam = set()     # (type key, j)
        self.not_tresult = set()    # type key
        self.not_global = set()     # g: some global.set gives it a value that is not whole
        self.tkey = {f: (tuple(m.types[m.ftype[f]][0]), tuple(m.types[m.ftype[f]][1])) for f in range(nf)}
        table = range(nf) if m.table_funcs is None else sorted(x for x in m.table_funcs if x < nf)
        self.table = set(table)
        # imported functions: nothing is known of what they return
        for f in range(m.nimp):
            self.not_result.add(f)
        for f in range(m.nimp, nf):
            if len(m.sig(f)[1]) != 1:
                self.not_result.add(f)     # the kernel pass looks at one returned value
        self.rounds = 0
        while True:
            self.rounds += 1
            before = (len(self.not_local), len(self.not_param), len(self.not_result), len(self.not_tparam), len(self.not_tresult), len(self.not_global))
            for k in self.ins:
                self.walk(k, None)
            # a function in the table answers to its type's claims
            for g in self.table:
                tk = self.tkey[g]
                for j in range(len(tk[0])):
                    if (g, j) in self.not_param:
                        self.not_tparam.add((tk, j))
                    if (tk, j) in self.not_tparam:
                        self.not_param.add((g, j))
                if g in self.not_result:
                    self.not_tresult.add(tk)
            after = (len(self.not_local), len(self.not_param), len(self.not_result), len(self.not_tparam), len(self.not_tresult), len(self.not_global))
            if after == before:
                break

    def is_whole_local(self, f, x):
        np_ = len(self.m.sig(f)[0])
        if x < np_:
            return (f, x) not in self.not_param and (f, x) not in self.not_local
        return (f, x) not in self.not_local

    def walk(self, k, accesses):
        """One body. With `accesses` a dict, fills it: instruction -> the address value."""
        m = self.m
        f = m.nimp + k
        ins = self.ins[k]
        nres = len(m.sig(f)[1])
        stack = []
        frames = [[0, nres, False, True]]       # base, results, unreachable, is the function

        def pop():
            fr = frames[-1]
            if len(stack) > fr[0]:
                return stack.pop()
            if fr[2]:
                return W0
            raise W.Unread("stack")

        def bt(imm):
            if imm == 0x40:
                return 0, 0
            if isinstance(imm, tuple):
                ps, rs = m.types[imm[1]]
                return len(ps), len(rs)
            return 0, 1

        def dead():
            fr = frames[-1]
            del stack[fr[0]:]
            fr[2] = True

        def returned():
            # the values a return here would carry
            if not nres or f in self.not_result:
                return
            have = len(stack) - frames[-1][0]
            for j in range(nres):
                v = stack[len(stack) - nres + j] if have >= nres - j else W0
                if not whole(v):
                    self.not_result.add(f)

        def branch(d):
            if frames[-1 - d][3]:
                returned()

        def assign(x, v):
            if self.is_whole_local(f, x) and not whole(v):
                self.not_local.add((f, x))

        for i, x in enumerate(ins):
            op = x.op
            if op == 0x00:
                dead()
            elif op == 0x01:
                pass
            elif op in (0x02, 0x03, 0x04):
                if op == 0x04:
                    pop()
                np_, nr = bt(x.imm)
                base = max(len(stack) - np_, frames[-1][0])
                for j in range(base, len(stack)):
                    stack[j] = A
                frames.append([base, nr, False, False])
            elif op == 0x05:
                fr = frames[-1]
                del stack[fr[0]:]
                fr[2] = False
            elif op == 0x0B:
                fr = frames.pop()
                if fr[3]:
                    if not fr[2]:
                        frames.append(fr); returned(); frames.pop()
                    break
                del stack[fr[0]:]
                stack.extend([A] * fr[1])
            elif op == 0x0C:
                branch(x.imm); dead()
            elif op == 0x0D:
                pop(); branch(x.imm)
            elif op == 0x0E:
                pop()
                for d in x.imm:
                    branch(d)
                dead()
            elif op == 0x0F:
                returned(); dead()
            elif op in (0x10, 0x12):
                g = x.imm
                ps, rs = m.sig(g)
                args = [pop() for _ in ps][::-1]
                for j, v in enumerate(args):
                    if not whole(v):
                        self.not_param.add((g, j))
                res = W0 if (g == self.alloc or g not in self.not_result) else A
                if op == 0x12:
                    if nres and f not in self.not_result and res == A:
                        self.not_result.add(f)
                    dead()
                else:
                    stack.extend([res] * len(rs))
            elif op in (0x11, 0x13):
                pop()
                ty = m.types[x.imm[0]]
                tk = (tuple(ty[0]), tuple(ty[1]))
                args = [pop() for _ in ty[0]][::-1]
                for j, v in enumerate(args):
                    if not whole(v):
                        self.not_tparam.add((tk, j))
                res = A if tk in self.not_tresult else W0
                if op == 0x13:
                    if nres and f not in self.not_result and res == A:
                        self.not_result.add(f)
                    dead()
                else:
                    stack.extend([res] * len(ty[1]))
            elif op == 0x1A:
                pop()
            elif op in (0x1B, 0x1C):
                pop()
                b_ = pop()
                a_ = pop()
                stack.append(a_ if a_ == b_ else A)
            elif op == 0x20:
                stack.append(W0 if self.is_whole_local(f, x.imm) else A)
            elif op == 0x21:
                assign(x.imm, pop())
            elif op == 0x22:
                v = pop()
                assign(x.imm, v)
                stack.append(v)
            elif op == 0x23:
                stack.append(A if x.imm in self.not_global else W0)
            elif op == 0x24:
                if not whole(pop()):
                    self.not_global.add(x.imm)
            elif op == 0x25:
                pop(); stack.append(A)
            elif op == 0x26:
                pop(); pop()
            elif W.is_load(op):
                a = pop()
                if accesses is not None:
                    accesses[i] = a
                stack.append(W0 if op in (0x28, 0x29) else A)
            elif W.is_store(op):
                pop()
                a = pop()
                if accesses is not None:
                    accesses[i] = a
            elif op == 0x3F:
                stack.append(A)
            elif op == 0x40:
                pop(); stack.append(A)
            elif op in (0x41, 0x42):
                stack.append(("K", x.imm))
            elif op in (0x43, 0x44):
                stack.append(A)
            elif op in (0x45, 0x50) or 0x67 <= op <= 0x69 or 0x79 <= op <= 0x7B or 0x8B <= op <= 0x91 \
                    or 0x99 <= op <= 0x9F or 0xA7 <= op <= 0xC4 or op == 0xD1:
                v = pop()
                stack.append(v if op in KEEP and v != A and v[0] == "W" else A)
            elif 0x46 <= op <= 0x4F or 0x51 <= op <= 0x66 or 0x6A <= op <= 0x78 or 0x7C <= op <= 0x8A \
                    or 0x92 <= op <= 0x98 or 0xA0 <= op <= 0xA6:
                b_ = pop()
                a_ = pop()
                r = A
                if op == 0x6A and a_ != A and b_ != A:
                    if a_[0] == "W" and b_[0] == "K":
                        r = ("W", (a_[1] + b_[1]) & M32)
                    elif a_[0] == "K" and b_[0] == "W":
                        r = ("W", (a_[1] + b_[1]) & M32)
                    elif a_[0] == "K" and b_[0] == "K":
                        r = ("K", (a_[1] + b_[1]) & M32)
                stack.append(r)
            elif op in (0xD0, 0xD2):
                stack.append(A)
            elif op == 0xFC:
                s = x.sub
                if s <= 7:
                    pop(); stack.append(A)
                elif s in (8, 10, 11, 12, 14, 17):
                    pop(); pop(); pop()
                elif s == 15:
                    pop(); pop(); stack.append(A)
                elif s == 16:
                    stack.append(A)
            else:
                raise W.Unread(f"opcode 0x{op:02x}")


STOP = {0x02, 0x03, 0x04, 0x05, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13}


def tee_claims(ins, alloc):
    """The accesses the earlier whole-function probe also asked the fresh-block rule about: the
    first three loads or stores of the straight-line run after `i32.const n ; call alloc ;
    local.tee x` (which of them address the block is the kernel's verdict; here: asked)."""
    out = set()
    for j in range(2, len(ins)):
        if ins[j].op != 0x22 or ins[j - 1].op != 0x10 or ins[j - 1].imm != alloc or ins[j - 2].op != 0x41:
            continue
        seen = 0
        for k in range(j + 1, min(len(ins), j + 61)):
            x = ins[k]
            if x.op in STOP or (x.op == 0xFC and x.sub in (8, 10, 11)):
                break
            if 0x28 <= x.op <= 0x3E:
                out.add(k)
                seen += 1
                if seen == 3:
                    break
    return out


def zlist(xs):
    return "[" + "; ".join(str(x) for x in xs) + "]"


def goals(d, work, limit):
    os.makedirs(work)
    names = sorted(f[:-5] for f in os.listdir(d) if f.endswith(".wasm"))[:limit]
    with open(os.path.join(work, "index.tsv"), "w", encoding="utf-8") as idx:
        for n, name in enumerate(names):
            stem = os.path.join(d, name)
            m = W.Module(stem + ".wasm")
            _, alloc, _ = routines(stem, m)
            p = Pass(m, alloc)
            nf = len(m.ftype)
            sigs = [(len(m.sig(f)[0]), len(m.sig(f)[1])) for f in range(nf)]
            types = [(len(t[0]), len(t[1])) for t in m.types]
            tkey = [(tuple(t[0]), tuple(t[1])) for t in m.types]
            wpar = [(f, j) for f in range(m.nimp, nf) for j in range(sigs[f][0]) if (f, j) not in p.not_param]
            wres = [f for f in range(m.nimp, nf) if f not in p.not_result]
            tpar = [(t, j) for t in range(len(types)) for j in range(types[t][0]) if (tkey[t], j) not in p.not_tparam]
            tres = [t for t in range(len(types)) if tkey[t] not in p.not_tresult]
            globs = set()
            for ins in p.ins.values():
                globs |= {x.imm for x in ins if x.op in (0x23, 0x24)}
            wglob = sorted(g for g in globs if g not in p.not_global)
            out = ["From AlmideTrust Require Import GuardWindow GuardKind.",
                   "From Stdlib Require Import ZArith List.", "Import ListNotations.", "Open Scope Z_scope.", "",
                   f"(* {name} *)",
                   "Definition c : ctx := mkC",
                   "  " + zlist(f"({a}, {b})" for a, b in sigs),
                   "  " + zlist(f"({a}, {b})" for a, b in types),
                   f"  {alloc if alloc is not None else -1}",
                   "  " + zlist(f"({a}, {b})" for a, b in wpar),
                   "  " + zlist(wres),
                   "  " + zlist(f"({a}, {b})" for a, b in tpar),
                   "  " + zlist(tres),
                   "  " + zlist(wglob) + ".", ""]
            nfun = nhdr = 0
            for k, ins in sorted(p.ins.items()):
                f = m.nimp + k
                acc = {}
                p.walk(k, acc)
                locs, start, end = m.bodies[k]
                wl = [x for x in range(sigs[f][0] + locs) if p.is_whole_local(f, x)]
                ends = []
                for i, a in acc.items():
                    x = ins[i]
                    if a != A and a[0] == "W" and a[1] + x.imm + W.WIDTH[x.op] <= 12:
                        ends.append(x.end - start)
                ends.sort(reverse=True)
                body = list(m.bytes[start:end])
                out.append(f"Example f{f} : header_ends c {f} {zlist(wl)} {sigs[f][1]} {zlist(body)} = Some {zlist(ends)}.")
                out.append("Proof. vm_compute. reflexivity. Qed.")
                nfun += 1
                nhdr += len(ends)
            with open(os.path.join(work, f"K_{n:04d}.v"), "w", encoding="utf-8") as fh:
                fh.write("\n".join(out) + "\n")
            idx.write(f"K_{n:04d}\t{name}\t{nfun}\t{nhdr}\t{len(p.unread)}\n")
    print("modules", len(names))


def main():
    args = sys.argv[1:]
    if args and args[0] == "goals":
        limit = int(args[args.index("--limit") + 1]) if "--limit" in args else None
        goals(args[1], args[2], limit)
        return
    d = args[0]
    limit = int(args[args.index("--limit") + 1]) if "--limit" in args else None
    out = args[args.index("--out") + 1] if "--out" in args else None
    names = sorted(f[:-5] for f in os.listdir(d) if f.endswith(".wasm"))[:limit]
    tot = collections.Counter()
    left = collections.Counter()
    rows = []
    for n in names:
        stem = os.path.join(d, n)
        m = W.Module(stem + ".wasm")
        named, alloc, _ = routines(stem, m)
        p = Pass(m, alloc)
        props = proposals(stem)
        tot["modules"] += 1
        tot["rounds (sum)"] += p.rounds
        nf = len(m.ftype)
        tot["params"] += sum(len(m.sig(f)[0]) for f in range(m.nimp, nf))
        tot["params whole"] += sum(1 for f in range(m.nimp, nf) for j in range(len(m.sig(f)[0])) if (f, j) not in p.not_param)
        tot["functions with a result"] += sum(1 for f in range(m.nimp, nf) if m.sig(f)[1])
        tot["results whole"] += sum(1 for f in range(m.nimp, nf) if m.sig(f)[1] and f not in p.not_result)
        for k, ins in p.ins.items():
            acc = {}
            p.walk(k, acc)
            bulk = sum(1 for x in ins if x.op == 0xFC and x.sub in (8, 10, 11))
            if not acc and not bulk:
                continue
            f = m.nimp + k
            tot["functions with an access"] += 1
            tot["accesses"] += len(acc)
            if f in named:
                tot["fn: a routine read against its proved tree"] += 1
                tot["acc: in such routines"] += len(acc)
                continue
            pk = dict(props.get(k, {}))
            for i in tee_claims(ins, alloc):
                pk.setdefault(i, "tee")
            c = collections.Counter()
            for i, a in acc.items():
                x = ins[i]
                w = W.WIDTH[x.op]
                if i in pk:
                    c["proposed"] += 1
                    tot["acc: proposed (" + pk[i] + ")"] += 1
                    if a != A and a[0] == "W" and a[1] + x.imm + w <= 12:
                        tot["acc: proposed and also a header access"] += 1
                elif a != A and a[0] == "W" and a[1] + x.imm + w <= 12:
                    c["header"] += 1
                elif a != A and a[0] == "W":
                    c["whole + constant past the header"] += 1
                elif a != A:
                    c["constant address"] += 1
                else:
                    c["arith"] += 1
            for key, v in c.items():
                tot["acc: " + key] += v
            rest = {key for key in c if key not in ("proposed", "header")}
            if bulk:
                rest.add("bulk")
            if not rest:
                tot["fn: every access proposed or a header access"] += 1
                tot["acc: in such functions"] += len(acc)
                if "header" not in c:
                    tot["fn: (of those) every access proposed"] += 1
            else:
                left[" + ".join(sorted(rest))] += 1
            if not (rest - {"whole + constant past the header"}):
                tot["fn: ... or whole + constant past the header"] += 1
                tot["acc: in such functions (with past-the-header)"] += len(acc)
            if out is not None:
                rows.append((n, k, len(acc), bulk, c))
    for key in sorted(tot):
        print(f"   {key:58s} {tot[key]}")
    print("== what else the other functions hold (top 12)")
    for key, v in left.most_common(12):
        print(f"   {v:6d}  {key}")
    if out is not None:
        os.makedirs(out, exist_ok=True)
        with open(os.path.join(out, "functions-kinds.tsv"), "w", encoding="utf-8") as fh:
            fh.write("module\tfunction\taccesses\tbulk\tclasses\n")
            for n, k, a, bulk, c in rows:
                fh.write(f"{n}\t{k}\t{a}\t{bulk}\t" + "; ".join(f"{key}={v}" for key, v in sorted(c.items())) + "\n")


if __name__ == "__main__":
    main()
