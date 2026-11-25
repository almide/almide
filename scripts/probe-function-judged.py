#!/usr/bin/env python3
"""Probe only: whole function bodies in which the kernel judges every load and
store (proofs/GuardFunction.v).

  probe-function-judged.py gen <modules.tsv> <work-dir> <chunk>
  probe-function-judged.py controls <work-dir> <how many>
  probe-function-judged.py join <work-dir> <out-dir>

<modules.tsv>: label, module (.wasm), fresh-block proposals (the output of
`almide-anchor-check --fresh`, or `-`), window proposals (the output of
`--windows`, or `-`), one module per line.

gen: reads each module's code section with a reader of its own (untrusted: it
only chooses what to ASK), and for every function that has a load or a store,
no bulk memory instruction, and a proposal ending at each of its loads and
stores, writes a goal `judged <allocator> [body] [claims]`. A claim gives a
position in the body and lengths; the kernel cuts the strings out of the body
itself. Goals are asked once per distinct (allocator, body, claims).
  <work>/functions.tsv   one row per function with a load or a store
  <work>/modules.tsv     one row per module
  <work>/goals.tsv       goal name, chunk
  <work>/Ask_<k>.v       `Eval vm_compute in (judged ...)` per goal
controls: from goals the kernel answered `true`, variants that must be refused.
join: reads the answers (<work>/Ask_<k>.out) and writes the tables.
"""
import os
import sys

HEAD = """From AlmideTrust Require Import GuardWindow GuardFresh GuardFunction.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""


def uleb(b, p):
    r = s = 0
    while True:
        x = b[p]
        p += 1
        r |= (x & 0x7F) << s
        s += 7
        if x < 0x80:
            return r, p


def skip_leb(b, p):
    while b[p] >= 0x80:
        p += 1
    return p + 1


def bodies(b):
    """(number of imported functions, [(start, end) of each body's instructions])."""
    p, imports, out = 8, 0, []
    while p < len(b):
        sid = b[p]
        size, p = uleb(b, p + 1)
        end = p + size
        if sid == 2:
            n, q = uleb(b, p)
            for _ in range(n):
                for _ in range(2):
                    ln, q = uleb(b, q)
                    q += ln
                kind = b[q]
                q += 1
                if kind == 0:
                    q = skip_leb(b, q)
                    imports += 1
                elif kind in (1, 2):
                    if kind == 1:
                        q += 1
                    fl, q = uleb(b, q)
                    q = skip_leb(b, q)
                    if fl & 1:
                        q = skip_leb(b, q)
                elif kind == 3:
                    q += 2
                else:
                    raise ValueError("an import kind this reader does not know")
        if sid == 10:
            n, q = uleb(b, p)
            for _ in range(n):
                sz, q = uleb(b, q)
                fe = q + sz
                nl, r = uleb(b, q)
                for _ in range(nl):
                    r = skip_leb(b, r)
                    r += 1
                out.append((r, fe))
                q = fe
        p = end
    return imports, out


def instrs(b, start, end):
    """[(start, end, opcode, is bulk)] with offsets relative to `start`; None if not read."""
    p, out = start, []
    try:
        while p < end:
            at, op = p, b[p]
            p += 1
            bulk = False
            if op in (0x02, 0x03, 0x04):
                bt = b[p]
                p = p + 1 if (bt == 0x40 or bt >= 0x6F) else skip_leb(b, p)
            elif op in (0x0C, 0x0D, 0x10, 0x12, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0xD0, 0xD2):
                p = skip_leb(b, p)
            elif op == 0x13:
                p = skip_leb(b, skip_leb(b, p))
            elif op == 0x0E:
                n, p = uleb(b, p)
                for _ in range(n + 1):
                    p = skip_leb(b, p)
            elif op == 0x11:
                p = skip_leb(b, skip_leb(b, p))
            elif op == 0x1C:
                n, p = uleb(b, p)
                p += n
            elif 0x28 <= op <= 0x3E:
                p = skip_leb(b, skip_leb(b, p))
            elif op in (0x3F, 0x40):
                p += 1
            elif op in (0x41, 0x42):
                p = skip_leb(b, p)
            elif op == 0x43:
                p += 4
            elif op == 0x44:
                p += 8
            elif op == 0xFC:
                sub, p = uleb(b, p)
                if sub == 8:
                    p = skip_leb(b, p) + 1
                    bulk = True
                elif sub in (9, 13, 15, 16, 17):
                    p = skip_leb(b, p)
                elif sub == 10:
                    p += 2
                    bulk = True
                elif sub == 11:
                    p += 1
                    bulk = True
                elif sub in (12, 14):
                    p = skip_leb(b, skip_leb(b, p))
                elif sub > 7:
                    return None
            elif op >= 0xC5 and op not in (0xD1,):
                return None
            out.append((at - start, p - start, op, bulk))
    except IndexError:
        return None
    if p != end:
        return None
    return out


def read_props(path, kind):
    """{function index: [(access instruction, [byte strings])]}, and the allocator."""
    props, alloc = {}, None
    if path == "-" or not os.path.isfile(path):
        return props, alloc
    for line in open(path, encoding="utf-8", errors="replace"):
        t = line.split()
        if not t:
            continue
        if kind == "F" and t[0] == "FRESH" and line.count("|") == 3:
            head, pre, mid, tail = [x.split() for x in line.split("|")]
            if len(head) != 4 or not all(x.isdigit() for x in head[1:] + pre + mid + tail):
                continue
            parts = [bytes(int(x) for x in s) for s in (pre, mid, tail)]
            props.setdefault(int(head[1]), []).append((int(head[2]), parts))
        elif kind == "F" and t[0] == "ALLOCATOR" and len(t) == 2 and t[1].isdigit():
            alloc = int(t[1])
        elif kind == "W" and t[0] == "WINDOW" and len(t) > 3 and all(x.isdigit() for x in t[1:]):
            props.setdefault(int(t[1]), []).append((int(t[2]), [bytes(int(x) for x in t[3:])]))
    return props, alloc


def place(body, ins, kind, k, parts):
    """Positions in `body` where the proposal's strings sit and end with a load or a store."""
    blob = b"".join(parts)
    n = len(blob)
    ends = {e: i for i, (_, e, _, _) in enumerate(ins)}
    starts = {s for (s, _, _, _) in ins}
    found = []
    if 0 <= k < len(ins) and 0x28 <= ins[k][2] <= 0x3E:
        e = ins[k][1]
        if e - n >= 0 and body[e - n:e] == blob and (e - n) in starts:
            found.append(e - n)
    if not found:
        p = body.find(blob)
        while p >= 0:
            e = p + n
            if p in starts and e in ends and 0x28 <= ins[ends[e]][2] <= 0x3E:
                found.append(p)
            p = body.find(blob, p + 1)
    if kind == "F":
        return [("F", s, len(parts[0]), len(parts[1]), len(parts[2])) for s in found]
    return [("W", s, n) for s in found]


def tee_claims(b, start, ins, alloc):
    """Claims for the accesses that may address the value `local.tee x` leaves on
    the stack right after `i32.const n ; call alloc ; local.tee x`: the next
    few loads and stores of the straight-line run that follows. Which of them
    do address it is the kernel's verdict."""
    out = []
    stop = {0x02, 0x03, 0x04, 0x05, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13}
    for j in range(2, len(ins)):
        if ins[j][2] != 0x22 or ins[j - 1][2] != 0x10 or ins[j - 2][2] != 0x41:
            continue
        callee, _ = uleb(b, start + ins[j - 1][0] + 1)
        if callee != alloc:
            continue
        s, lp, seen = ins[j - 2][0], ins[j][1] - ins[j - 2][0], 0
        for k in range(j + 1, min(len(ins), j + 61)):
            if ins[k][2] in stop or ins[k][3]:
                break
            if 0x28 <= ins[k][2] <= 0x3E:
                out.append(("T", s, lp, ins[k][1] - ins[j][1]))
                seen += 1
                if seen == 3:
                    break
    return out


def claim_text(c):
    if c[0] == "F":
        return f"CFresh {c[1]} {c[2]} {c[3]} {c[4]}"
    if c[0] == "T":
        return f"CTee {c[1]} {c[2]} {c[3]}"
    return f"CWindow {c[1]} {c[2]}"


def claim_end(c):
    return c[1] + sum(c[2:])


def goal_text(alloc, body, claims):
    return f"(judged {alloc} [{';'.join(str(x) for x in body)}] [{'; '.join(claim_text(c) for c in claims)}])"


def gen(listing, work, chunk):
    os.makedirs(work, exist_ok=True)
    names, order = {}, []
    fn_rows, mod_rows = [], []
    for line in open(listing, encoding="utf-8"):
        c = line.rstrip("\n").split("\t")
        if len(c) < 4:
            continue
        label, module, fresh, windows = c[:4]
        try:
            b = open(module, "rb").read()
            imports, fs = bodies(b)
        except (OSError, ValueError, IndexError) as e:
            mod_rows.append([label, "unreadable", str(e)[:80]] + ["-"] * 10)
            continue
        fprops, alloc = read_props(fresh, "F")
        wprops, _ = read_props(windows, "W")
        swept = "-"
        if windows != "-" and os.path.isfile(windows):
            for l in open(windows, encoding="utf-8", errors="replace"):
                if l.startswith("ACCESSES "):
                    swept = l.split()[1]
        a = 4294967295 if alloc is None else alloc
        tot = dict(functions=len(fs), with_access=0, accesses=0, bulk=0, unread=0, candidates=0, cand_accesses=0, unplaced=0, proposals=0)
        decoded = [instrs(b, s, e) for (s, e) in fs]

        def props_of(i, shift):
            return [("F", k, parts) for k, parts in fprops.get(i + shift, [])] + [("W", k, parts) for k, parts in wprops.get(i + shift, [])]

        def placed(shift):
            n = 0
            for i, (s, e) in enumerate(fs):
                if decoded[i] is not None:
                    n += sum(1 for kind, k, parts in props_of(i, shift) if place(bytes(b[s:e]), decoded[i], kind, k, parts))
            return n

        # The cutters number a function by its place among the module's own
        # bodies; a dump that counts the imports too is read as well.
        shift = 0 if imports == 0 or placed(0) >= placed(imports) else imports
        for i, (s, e) in enumerate(fs):
            fidx = imports + i
            ins = decoded[i]
            props = props_of(i, shift)
            tot["proposals"] += len(props)
            if ins is None:
                tot["unread"] += 1
                fn_rows.append([label, str(fidx), str(e - s), "-", "-", str(len(props)), "-", "unread", "-"])
                continue
            acc = [x for x in ins if 0x28 <= x[2] <= 0x3E]
            bulk = any(x[3] for x in ins)
            if not acc and not bulk:
                continue
            tot["with_access"] += 1
            tot["accesses"] += len(acc)
            body = bytes(b[s:e])
            claims = set()
            for kind, k, parts in props:
                got = place(body, ins, kind, k, parts)
                if not got:
                    tot["unplaced"] += 1
                claims.update(got)
            tees = tee_claims(b, s, ins, a)
            tot["tee_claims"] = tot.get("tee_claims", 0) + len(tees)
            claims.update(tees)
            claims = sorted(claims)
            ends = {claim_end(c) for c in claims}
            if bulk:
                tot["bulk"] += 1
                state = "bulk"
            elif acc and all(x[1] in ends for x in acc):
                state = "asked"
            else:
                state = "not proposed"
            name = "-"
            if state == "asked":
                tot["candidates"] += 1
                tot["cand_accesses"] += len(acc)
                key = (a, body, tuple(claims))
                if key not in names:
                    names[key] = f"g{len(names):06d}"
                    order.append(key)
                name = names[key]
            fn_rows.append([label, str(fidx), str(e - s), str(len(ins)), str(len(acc)), str(len(props)), str(len(claims)), state, name])
        mod_rows.append([label, "read", str(a), swept] + [str(tot.get(k, 0)) for k in ("functions", "with_access", "accesses", "bulk", "unread", "candidates", "cand_accesses", "unplaced", "tee_claims")])
    with open(f"{work}/functions.tsv", "w", encoding="utf-8") as f:
        f.write("module\tfunction\tbody_bytes\tinstructions\taccesses\tproposals\tclaims\tstate\tgoal\n")
        for r in fn_rows:
            f.write("\t".join(r) + "\n")
    with open(f"{work}/modules.tsv", "w", encoding="utf-8") as f:
        f.write("module\tstate\tallocator\tcutter_swept\tfunctions\twith_access\taccesses\tbulk\tunread\tasked\tasked_accesses\tunplaced_proposals\ttee_claims\n")
        for r in mod_rows:
            f.write("\t".join(r) + "\n")
    write_goals(work, "Ask", [(names[k], goal_text(*k)) for k in order], chunk)
    with open(f"{work}/goal-keys.tsv", "w", encoding="utf-8") as f:
        for k in order:
            f.write(f"{names[k]}\t{k[0]}\t{';'.join(str(x) for x in k[1])}\t{'|'.join(' '.join(str(x) for x in c) for c in k[2])}\n")
    print(f"{len(mod_rows)} module(s), {sum(1 for r in fn_rows if r[7] != 'unread')} function(s) with a load or a store, {sum(1 for r in fn_rows if r[7] == 'asked')} asked as {len(order)} distinct goal(s)")


def write_goals(work, stem, goals, chunk):
    with open(f"{work}/{stem.lower()}-goals.tsv", "w", encoding="utf-8") as idx:
        for i, (name, text) in enumerate(goals):
            k = i // chunk
            with open(f"{work}/{stem}_{k:04d}.v", "a" if i % chunk else "w", encoding="utf-8") as f:
                if i % chunk == 0:
                    f.write(HEAD)
                f.write(f"\nEval vm_compute in {text}.\n")
            idx.write(f"{name}\t{stem}_{k:04d}\n")


def answers(work, stem):
    """{goal name: 'true' | 'false' | '?'} from the compiler's output, in the order asked."""
    by_chunk = {}
    for line in open(f"{work}/{stem.lower()}-goals.tsv", encoding="utf-8"):
        name, chunk = line.rstrip("\n").split("\t")
        by_chunk.setdefault(chunk, []).append(name)
    out = {}
    for chunk, ns in by_chunk.items():
        vals = []
        p = f"{work}/{chunk}.out"
        if os.path.isfile(p):
            for l in open(p, encoding="utf-8", errors="replace"):
                t = l.strip()
                if t in ("= true", "= false"):
                    vals.append(t[2:])
        for i, n in enumerate(ns):
            out[n] = vals[i] if len(vals) == len(ns) else "?"
    return out


def read_keys(work):
    keys = {}
    for line in open(f"{work}/goal-keys.tsv", encoding="utf-8"):
        name, alloc, body, claims = line.rstrip("\n").split("\t")
        cs = []
        for c in claims.split("|") if claims else []:
            t = c.split()
            cs.append(tuple([t[0]] + [int(x) for x in t[1:]]))
        keys[name] = (int(alloc), [int(x) for x in body.split(";")] if body else [], cs)
    return keys


def controls(work, many):
    ans, keys = answers(work, "Ask"), read_keys(work)
    goals, rows = [], []
    for name in sorted(keys):
        if ans.get(name) != "true" or len(goals) >= 3 * many:
            continue
        alloc, body, cs = keys[name]
        ends = [claim_end(c) for c in cs]
        # one claim that alone ends where it ends: without it an access is uncovered
        only = [j for j in range(len(cs)) if ends.count(ends[j]) == 1]
        if not only:
            continue
        j = only[0]
        c, before, after = cs[j], cs[:j], cs[j + 1:]
        variants = [
            ("claim left out", before + after),
            # the claim starts one byte late (and so ends one byte late)
            ("start moved by one byte", before + [tuple([c[0], c[1] + 1] + list(c[2:]))] + after),
            # the claim is one byte short at its end, its start unchanged
            ("one byte short", before + [tuple(list(c[:-1]) + [c[-1] - 1])] + after),
        ]
        for what, v in variants:
            cname = f"c{len(goals):05d}"
            goals.append((cname, goal_text(alloc, body, v)))
            rows.append((cname, name, what))
    write_goals(work, "Control", goals, 50)
    with open(f"{work}/controls.tsv", "w", encoding="utf-8") as f:
        for r in rows:
            f.write("\t".join(r) + "\n")
    print(f"{len(goals)} control goal(s) from {len({r[1] for r in rows})} accepted goal(s)")


def checks(work, chunk):
    """Each answer again, as a statement the kernel checks: `judged ... = <answer>`."""
    keys, ans = read_keys(work), answers(work, "Ask")
    asked = [n for n in sorted(keys) if ans.get(n) in ("true", "false")]
    for i, name in enumerate(asked):
        k = i // chunk
        with open(f"{work}/Check_{k:04d}.v", "a" if i % chunk else "w", encoding="utf-8") as f:
            if i % chunk == 0:
                f.write(HEAD)
            f.write(f"\nExample {name} : {goal_text(*keys[name])} = {ans[name]}.\nProof. vm_compute. reflexivity. Qed.\n")
    print(f"{len(asked)} answer(s) to check, {len(keys) - len(asked)} goal(s) without an answer")


def join(work, out):
    os.makedirs(out, exist_ok=True)
    ans = answers(work, "Ask")
    rows = [l.rstrip("\n").split("\t") for l in open(f"{work}/functions.tsv", encoding="utf-8")][1:]
    per = {}
    with open(f"{out}/functions.tsv", "w", encoding="utf-8") as f:
        f.write("module\tfunction\tbody_bytes\tinstructions\taccesses\tproposals\tclaims\tstate\tgoal\tkernel\n")
        for r in rows:
            v = ans.get(r[8], "-") if r[7] == "asked" else "-"
            f.write("\t".join(r + [v]) + "\n")
            m = per.setdefault(r[0], dict(judged=0, judged_accesses=0, refused=0, unanswered=0))
            if v == "true":
                m["judged"] += 1
                m["judged_accesses"] += int(r[4])
            elif v == "false":
                m["refused"] += 1
            elif v == "?":
                m["unanswered"] += 1
    mods = [l.rstrip("\n").split("\t") for l in open(f"{work}/modules.tsv", encoding="utf-8")]
    tot = {}
    with open(f"{out}/modules.tsv", "w", encoding="utf-8") as f:
        f.write("\t".join(mods[0] + ["judged", "judged_accesses", "refused", "unanswered"]) + "\n")
        for r in mods[1:]:
            m = per.get(r[0], dict(judged=0, judged_accesses=0, refused=0, unanswered=0))
            f.write("\t".join(r + [str(m[k]) for k in ("judged", "judged_accesses", "refused", "unanswered")]) + "\n")
            if r[1] != "read":
                tot["unreadable modules"] = tot.get("unreadable modules", 0) + 1
                continue
            for k, v in zip(mods[0][4:], r[4:]):
                tot[k] = tot.get(k, 0) + int(v)
            if r[3].isdigit():
                tot["cutter_swept"] = tot.get("cutter_swept", 0) + int(r[3])
            for k in ("judged", "judged_accesses", "refused", "unanswered"):
                tot[k] = tot.get(k, 0) + m[k]
            tot["modules"] = tot.get("modules", 0) + 1
            tot["modules with a judged function"] = tot.get("modules with a judged function", 0) + (m["judged"] > 0)
    dist = {}
    for v in ans.values():
        dist[v] = dist.get(v, 0) + 1
    with open(f"{out}/summary.txt", "w", encoding="utf-8") as f:
        for k in sorted(tot):
            f.write(f"{k}\t{tot[k]}\n")
        f.write(f"distinct goals\t{len(ans)}\t{' '.join(f'{k}={v}' for k, v in sorted(dist.items()))}\n")
        if os.path.isfile(f"{work}/controls.tsv"):
            cans = answers(work, "Control")
            kinds = {}
            for line in open(f"{work}/controls.tsv", encoding="utf-8"):
                cname, _, what = line.rstrip("\n").split("\t")
                k = kinds.setdefault(what, {})
                k[cans.get(cname, "?")] = k.get(cans.get(cname, "?"), 0) + 1
            for what in sorted(kinds):
                f.write(f"control: {what}\t{' '.join(f'{k}={v}' for k, v in sorted(kinds[what].items()))}\n")
    sys.stdout.write(open(f"{out}/summary.txt", encoding="utf-8").read())


if sys.argv[1] == "gen":
    gen(sys.argv[2], sys.argv[3], int(sys.argv[4]))
elif sys.argv[1] == "controls":
    controls(sys.argv[2], int(sys.argv[3]))
elif sys.argv[1] == "checks":
    checks(sys.argv[2], int(sys.argv[3]))
elif sys.argv[1] == "join":
    join(sys.argv[2], sys.argv[3])
else:
    sys.exit(__doc__)
