#!/usr/bin/env python3
"""Where does the address of each load and store come from? (untrusted: it only measures)

  classify.py <dir of modules> [--list <list.tsv>] [--out <dir>] [--limit N]

<dir> holds <name>.wasm and, when the cutter's proposals were kept beside it, <name>.windows,
<name>.fresh and <name>.loops. Without them every access counts as not proposed.

Per module, every value is given one of two kinds by a fixpoint over the module's functions:
  whole   it was never computed by arithmetic: a parameter that only ever receives whole values,
          the result of a call that only returns whole values, a 32- or 64-bit word read from
          memory, a constant, a global, or a local only ever assigned such values (wrap and
          extend keep the kind)
  arith   anything else
The address of an access is `root + constant` (the constant: what i32.add adds, plus the
access's own offset). Classes:
  hdr       whole root, constant + width <= 12
  fix       whole root, constant >= 12
  straddle  whole root, starts below 12 and ends past it
  idx       the root is `whole + arith` computed in place (an indexed address)
  cursor    the root is a local of kind arith
  arith     any other arith root
The whole roots are split by where the value came from: param, local, loaded, call, const, global.

Tables: the classes over all accesses and over the accesses nothing proposes; the functions
whose every access is proposed, or proposed-or-hdr, and what stops the rest.
"""
import collections
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wasm_read as W

KEEP = (0xA7, 0xAC, 0xAD)          # i32.wrap_i64, i64.extend_i32_s, i64.extend_i32_u


def proposals(stem):
    """{function: {access instruction: rule}}; a later rule does not replace an earlier one."""
    out = collections.defaultdict(dict)
    for ext, rule in ((".windows", "window"), (".fresh", "fresh"), (".loops", "loop")):
        p = stem + ext
        if not os.path.exists(p):
            continue
        for line in open(p, encoding="utf-8", errors="replace"):
            t = line.split("|")[0].split()
            if len(t) >= 3 and t[0] in ("WINDOW", "FRESH", "LOOP"):
                out[int(t[1])].setdefault(int(t[2]), rule)
    return out


def routines(stem, m):
    """({function index: name} of the runtime routines the anchor file names and the kernel reads
    against the proved trees, the allocator's function index or None, how many of them are not
    the module's bytes at that index)."""
    named, alloc, off = {}, None, 0
    if os.path.exists(stem + ".proved"):
        for line in open(stem + ".proved", encoding="utf-8", errors="replace"):
            t = line.split()
            if len(t) >= 3 and t[0] == "SHIPPED":
                idx = int(t[2])
                named[idx] = t[1]
                k = idx - m.nimp
                code = bytes(int(x) for x in t[3:])
                if not (0 <= k < len(m.bodies)) or code not in m.bytes[m.bodies[k][1] - 8:m.bodies[k][2]] and m.bytes[m.bodies[k][1]:m.bodies[k][2]] not in code:
                    off += 1
    if os.path.exists(stem + ".fresh"):
        for line in open(stem + ".fresh", encoding="utf-8", errors="replace"):
            t = line.split()
            if len(t) == 2 and t[0] == "ALLOCATOR":
                alloc = int(t[1])
    return named, alloc, off


class Kinds:
    """whole = False, arith = True, by a fixpoint over one module. The allocator's result is
    whole whatever its body does: it is where blocks come from."""

    def __init__(self, m, alloc=None):
        self.m = m
        self.alloc = alloc
        self.fn = {}
        self.arith = set()
        self.unread = {}
        for k in range(len(m.bodies)):
            try:
                self.fn[k] = W.origins(m, k)
            except W.Unread as e:
                self.unread[k] = str(e)
        self.by_type = collections.defaultdict(list)
        cands = range(len(m.ftype)) if m.table_funcs is None else sorted(m.table_funcs)
        for f in cands:
            if f < len(m.ftype):
                self.by_type[(tuple(m.types[m.ftype[f]][0]), tuple(m.types[m.ftype[f]][1]))].append(f)
        changed = True
        self.rounds = 0
        while changed:
            changed = False
            self.rounds += 1
            for k, (ins, addr, la, calls, rets) in self.fn.items():
                f = m.nimp + k
                for x, terms in la.items():
                    v = self.var(f, x)
                    if v not in self.arith and any(self.kind(f, ins, t) for t in terms):
                        self.arith.add(v); changed = True
                for i, callee, args in calls:
                    if callee is None:
                        ty = m.types[ins[i].imm[0]]
                        targets = self.by_type.get((tuple(ty[0]), tuple(ty[1])), [])
                    else:
                        targets = [callee]
                    ks = [self.kind(f, ins, a) for a in args]
                    for g in targets:
                        for j, kd in enumerate(ks):
                            if kd and ("p", g, j) not in self.arith:
                                self.arith.add(("p", g, j)); changed = True
                if f != self.alloc and ("r", f) not in self.arith and any(self.kind(f, ins, t) for t in rets):
                    self.arith.add(("r", f)); changed = True

    def var(self, f, x):
        return ("p", f, x) if x < len(self.m.sig(f)[0]) else ("l", f, x)

    def kind(self, f, ins, t):
        h = t[0]
        if h == "L":
            return self.var(f, t[1]) in self.arith
        if h in ("K", "G"):
            return False
        if h == "M":
            return t[1] not in (0x28, 0x29)
        if h == "C":
            if t[1] is not None:
                return ("r", t[1]) in self.arith
            ty = self.m.types[ins[t[2]].imm[0]]
            return any(("r", g) in self.arith for g in self.by_type.get((tuple(ty[0]), tuple(ty[1])), []))
        if h == "U":
            return True if t[1] not in KEEP else self.kind(f, ins, t[2])
        if h == "S":
            return self.kind(f, ins, t[1]) or self.kind(f, ins, t[2])
        if h == "P":
            return any(self.kind(f, ins, u) for u in t[1])
        return True

    def sources(self, k, t, seen=None):
        """Where a whole value can have come from, following locals through every assignment
        the function makes to them: alloc, call, loaded, param, const, global."""
        seen = set() if seen is None else seen
        h = t[0]
        f = self.m.nimp + k
        if h == "L":
            x = t[1]
            out = set()
            if x < len(self.m.sig(f)[0]):
                out.add("param")
            if x in seen:
                return out
            seen.add(x)
            for u in self.fn[k][2].get(x, []):
                out |= self.sources(k, u, seen)
            if not out:
                out.add("const")          # a local never assigned holds zero
            return out
        if h == "C":
            return {"alloc" if t[1] is not None and t[1] == self.alloc else "call"}
        if h == "U":
            return self.sources(k, t[2], seen) if t[1] in KEEP else {"conv"}
        if h in ("S", "P"):
            out = set()
            for u in (t[1:] if h == "S" else t[1]):
                out |= self.sources(k, u, seen)
            return out
        return {{"K": "const", "G": "global", "M": "loaded"}.get(h, "other")}

    def source(self, k, t):
        return "+".join(sorted(self.sources(k, t)))

    def classify(self, k, i):
        ins, addr = self.fn[k][0], self.fn[k][1]
        f = self.m.nimp + k
        x = ins[i]
        root, c = W.peel(addr[i])
        total = (c + x.imm) & 0xFFFFFFFF
        w = W.WIDTH[x.op]
        if not self.kind(f, ins, root):
            src = self.source(k, root)
            if total >= 0x80000000:
                return "below", src
            if total + w <= 12:
                return "hdr", src
            if total >= 12:
                return "fix", src
            return "straddle", src
        if root[0] == "L":
            return "cursor", "param" if root[1] < len(self.m.sig(f)[0]) else "local"
        if root[0] == "B" and root[1] == 0x6A:
            a, b = self.kind(f, ins, root[2]), self.kind(f, ins, root[3])
            if a != b:
                return "idx", f"off{total}" if total in (0, 12) else ("off<12" if total < 12 else "off>12")
            return "arith", "sum of two arith" if a else "sum of two whole"
        return "arith", {"B": "binary", "U": "unary", "S": "either", "P": "either", "J": "unfollowed", "M": "narrow load",
                         "C": "call"}.get(root[0], root[0])


def main():
    args = sys.argv[1:]
    d = args[0]
    limit = int(args[args.index("--limit") + 1]) if "--limit" in args else None
    out = args[args.index("--out") + 1] if "--out" in args else None
    names = sorted(f[:-5] for f in os.listdir(d) if f.endswith(".wasm"))[:limit]
    allc, unp = collections.Counter(), collections.Counter()
    tot = collections.Counter()
    stop = collections.Counter()
    stop1 = collections.Counter()
    rows = []
    for n in names:
        stem = os.path.join(d, n)
        try:
            m = W.Module(stem + ".wasm")
        except W.Unread as e:
            tot["modules not read"] += 1
            continue
        named, alloc, off = routines(stem, m)
        kd = Kinds(m, alloc)
        props = proposals(stem)
        tot["modules"] += 1
        tot["modules with an allocator named"] += alloc is not None
        tot["named routines that are not the bytes at their index"] += off
        tot["functions not read"] += len(kd.unread)
        for k, (ins, addr, la, calls, rets) in kd.fn.items():
            bulk = sum(1 for x in ins if x.op == 0xFC and x.sub in (8, 10, 11))
            if not addr and not bulk:
                continue
            tot["functions with an access"] += 1
            tot["accesses"] += len(addr)
            tot["bulk"] += bulk
            if m.nimp + k in named:
                tot["fn: a routine read against its proved tree"] += 1
                tot["acc: in such routines"] += len(addr)
                allc["proved routine", named[m.nimp + k]] += len(addr)
                continue
            pk = props.get(k, {})
            for i in pk:
                if i not in addr:
                    tot["proposals that end on no access"] += 1
            classes = collections.Counter()
            for i in addr:
                c, s = kd.classify(k, i)
                allc[c, s] += 1
                if i in pk:
                    tot["proposed " + pk[i]] += 1
                    classes["proposed"] += 1
                else:
                    unp[c, s] += 1
                    classes[c] += 1
            left = {c for c in classes if c != "proposed"}
            if bulk:
                left.add("bulk")
            if not left:
                tot["fn: every access proposed"] += 1
                tot["acc: in such functions"] += len(addr)
            if left <= {"hdr"}:
                tot["fn: every access proposed or hdr"] += 1
                tot["acc: in such functions (proposed or hdr)"] += len(addr)
            if left <= {"hdr", "fix"}:
                tot["fn: every access proposed, hdr or fix"] += 1
                tot["acc: in such functions (proposed, hdr or fix)"] += len(addr)
            rest = left - {"hdr"}
            if rest:
                stop[" + ".join(sorted(rest))] += 1
                if len(rest) == 1:
                    stop1[next(iter(rest))] += 1
            if out is not None:
                rows.append((n, k, m.names.get(m.nimp + k, "-"), len(addr), bulk, classes))
    print("== totals")
    for key in sorted(tot):
        print(f"   {key:48s} {tot[key]}")
    for title, c in (("all accesses", allc), ("accesses nothing proposes", unp)):
        total = sum(c.values())
        print(f"== {title}: {total}")
        byc = collections.Counter()
        for (cl, s), v in c.items():
            byc[cl] += v
        for cl, v in byc.most_common():
            subs = ", ".join(f"{s} {v2}" for (c2, s), v2 in sorted(c.items(), key=lambda kv: -kv[1]) if c2 == cl)
            print(f"   {cl:9s} {v:7d} {100 * v / max(total, 1):5.1f}%   {subs}")
    print("== functions not `proposed or hdr`: what else they hold (top 15)")
    for key, v in stop.most_common(15):
        print(f"   {v:6d}  {key}")
    print("== of those, held back by one class only")
    for key, v in stop1.most_common():
        print(f"   {v:6d}  {key}")
    if out is not None:
        os.makedirs(out, exist_ok=True)
        with open(os.path.join(out, "functions.tsv"), "w", encoding="utf-8") as fh:
            fh.write("module\tfunction\tname\taccesses\tbulk\tclasses\n")
            for n, k, name, a, bulk, classes in rows:
                fh.write(f"{n}\t{k}\t{name}\t{a}\t{bulk}\t" + " ".join(f"{c}={v}" for c, v in sorted(classes.items())) + "\n")


if __name__ == "__main__":
    main()
