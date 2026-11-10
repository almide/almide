#!/usr/bin/env python3
"""The structural wall's kernel oracle, split into parts (#4005 A4).

The kernel keeps every goal's statement, so one coqc file holding every
witness grows its memory with the corpus. The wall writes the goals into
parts of bounded literal size instead, each its own coqc run, and this
module keeps the split from dropping a goal.

A verdict is a fact about a text: `f w = true` holds for every artifact
whose witness is the bytes `w`. So the kernel judges each DISTINCT
(checker, verdict, text) once, and the manifest carries how many witnesses
the wall fed in beside how many distinct texts the parts hold. Two texts
that differ in one byte are two texts: the key is the whole text, and
`check` re-reads the part files to hold that — it expands every goal back
to the strings it states and requires their digests to be exactly the
digests of the witnesses fed in.

Inside a part a text is spelled as a concatenation of chunks defined once
(`Definition kN : string := "..."`): the runtime routines' procedures recur
across artifacts under different table positions, and a statement that
repeats them costs the kernel their size again. The goal still states the
same string — `check` expands the spelling before it compares digests.

  write(base, header, fed, part_bytes, limit)
      fed: (checker, accepted?, text) per witness, in the wall's order.
      Writes <base>_p<i>.v per part, <base>.fed (one line per witness fed:
      checker, verdict, SHA-256 of its text) and the manifest <base>.lst —
      `goals G`, `witnesses W`, `distinct D`, then `path<TAB>goals` per part.

  kernel_parts.py check <manifest> <witnesses>   exit 1 unless the manifest
      names at least one part, every part file exists, is the header then
      only chunk definitions and goals closed by the one proof script, and
      holds its listed number of goals; the parts' goals sum to G; W is the
      number of witnesses the caller fed in and the number of lines of
      <base>.fed; and the (checker, verdict, digest) triples the goals state
      are D distinct triples, each stated once, and exactly the set of
      triples <base>.fed lists.
"""
import hashlib
import os
import re
import sys

# The kernel computes once, at Qed: the proof term is `eq_refl` under a VM
# cast, which is what `vm_compute. reflexivity.` builds after computing the
# same normal form a first time in the tactic.
PROOF = "Proof. vm_cast_no_check (eq_refl true). Qed.\n"
PROOF_F = "Proof. vm_cast_no_check (eq_refl false). Qed.\n"
# A text is cut after each `Q` / `C` (a procedure's table position and a
# call's callee, the only places a routine procedure names a position); the
# pieces of at least MIN bytes become chunk definitions.
CUT = re.compile(r"(?:(?<=Q)|(?<=C))\d+")
MIN = 64


def lit(s):
    return '"' + s.replace('"', '""') + '"'


def digest(s):
    return hashlib.sha256(s.encode()).hexdigest()


def pieces(w):
    out, i = [], 0
    for m in CUT.finditer(w):
        out.append(w[i:m.start()])
        out.append(m.group())
        i = m.end()
    out.append(w[i:])
    return out


class Part:
    def __init__(self):
        self.defs, self.order, self.goals, self.size = {}, [], [], 0

    def spell(self, w, new):
        """The expression for `w` and the literal bytes it adds; `new`
        collects the chunks it would define."""
        atoms, pend, cost = [], "", 0
        for p in pieces(w):
            if len(p) >= MIN:
                if pend:
                    atoms.append(lit(pend))
                    cost += len(pend)
                    pend = ""
                if p in self.defs:
                    atoms.append(self.defs[p])
                else:
                    if p not in new:
                        new[p] = "k%d" % (len(self.defs) + len(new))
                        cost += len(p)
                    atoms.append(new[p])
            else:
                pend += p
        if pend or not atoms:
            atoms.append(lit(pend))
            cost += len(pend)
        return (atoms[0] if len(atoms) == 1 else "(" + " ++ ".join(atoms) + ")"), cost

    def goal(self, fn, ok, texts):
        new, exprs, cost = {}, [], 0
        for w in texts:
            e, c = self.spell(w, new)
            exprs.append(e)
            cost += c
        if ok:
            text = "Goal forallb %s [%s] = true.\n%s" % (fn, ";".join(exprs), PROOF)
        else:
            text = "Goal %s %s = false.\n%s" % (fn, exprs[0], PROOF_F)
        return text, cost, new

    def add(self, text, cost, new):
        for p, name in new.items():
            self.defs[p] = name
            self.order.append(p)
        self.goals.append(text)
        self.size += cost

    def render(self, header):
        return header + "".join("Definition %s : string := %s.\n" % (self.defs[p], lit(p)) for p in self.order) + "".join(self.goals)


def write(base, header, fed, part_bytes, limit=60000):
    distinct = {}
    for key in fed:
        distinct[key] = distinct.get(key, 0) + 1
    # One goal per run of accepted texts of one checker up to `limit` bytes,
    # one per refused text.
    groups, by_fn = [], {}
    for (fn, ok, w) in distinct:
        if ok:
            by_fn.setdefault(fn, []).append(w)
        else:
            groups.append((fn, False, [w]))
    accepted = []
    for fn, ws in by_fn.items():
        chunk, size = [], 0
        for w in ws + [None]:
            if w is None or (chunk and size + len(w) > limit):
                if chunk:
                    accepted.append((fn, True, chunk))
                chunk, size = [], 0
            if w is not None:
                chunk.append(w)
                size += len(w)
    groups = accepted + groups
    parts, part = [], Part()
    for fn, ok, texts in groups:
        text, cost, new = part.goal(fn, ok, texts)
        if part.goals and part.size + cost > part_bytes:
            parts.append(part)
            part = Part()
            text, cost, new = part.goal(fn, ok, texts)
        part.add(text, cost, new)
    parts.append(part)
    if sum(len(p.goals) for p in parts) != len(groups):
        sys.exit("kernel parts: the split lost a goal")
    with open(base + ".fed", "w") as f:
        for fn, ok, w in fed:
            f.write("%s\t%s\t%s\n" % (fn, "true" if ok else "false", digest(w)))
    lines = [f"goals {len(groups)}", f"witnesses {len(fed)}", f"distinct {len(distinct)}"]
    for i, p in enumerate(parts):
        path = f"{base}_p{i}.v"
        with open(path, "w") as f:
            f.write(p.render(header))
        lines.append(f"{path}\t{len(p.goals)}")
    with open(base + ".lst", "w") as f:
        f.write("\n".join(lines) + "\n")
    return base + ".lst"


class Bad(Exception):
    pass


def _lit_at(s, i):
    """The string literal opening at s[i] == '"': (its text, the index after)."""
    out, j = [], i + 1
    while True:
        k = s.find('"', j)
        if k < 0:
            raise Bad("an unterminated literal")
        out.append(s[j:k])
        if s.startswith('""', k):
            out.append('"')
            j = k + 2
        else:
            return "".join(out), k + 1


def _expr_at(s, i, defs):
    """One spelled text at s[i]: (the string it states, the index after)."""
    def atom(i):
        if s[i] == '"':
            return _lit_at(s, i)
        m = re.compile(r"k\d+").match(s, i)
        if not m or m.group() not in defs:
            raise Bad("a chunk that the part does not define")
        return defs[m.group()], m.end()
    if s[i] != "(":
        return atom(i)
    out, i = [], i + 1
    while True:
        t, i = atom(i)
        out.append(t)
        if s.startswith(" ++ ", i):
            i += 4
        elif s[i] == ")":
            return "".join(out), i + 1
        else:
            raise Bad("a malformed concatenation")


def stated(path, header):
    """What a part file states: (goals, [(checker, verdict, digest)])."""
    s = open(path).read()
    if not s.startswith(header):
        raise Bad("the header is not the wall's")
    i, defs, goals, out = len(header), {}, 0, []
    name = re.compile(r"Definition (k\d+) : string := ")
    head = re.compile(r"Goal (forallb )?(\w+) ")
    while i < len(s):
        m = name.match(s, i)
        if m:
            if goals:
                raise Bad("a definition after a goal")
            if m.group(1) in defs:
                raise Bad("a chunk defined twice")
            t, i = _lit_at(s, m.end())
            if not s.startswith(".\n", i):
                raise Bad("a malformed definition")
            defs[m.group(1)], i = t, i + 2
            continue
        m = head.match(s, i)
        if not m:
            raise Bad("text that is neither a chunk definition nor a goal")
        fn, i, goals = m.group(2), m.end(), goals + 1
        if m.group(1):
            if s[i] != "[":
                raise Bad("a malformed goal")
            i += 1
            while True:
                t, i = _expr_at(s, i, defs)
                out.append((fn, "true", digest(t)))
                if s[i] == ";":
                    i += 1
                elif s[i] == "]":
                    i += 1
                    break
                else:
                    raise Bad("a malformed witness list")
            tail = " = true.\n" + PROOF
        else:
            t, i = _expr_at(s, i, defs)
            out.append((fn, "false", digest(t)))
            tail = " = false.\n" + PROOF_F
        if not s.startswith(tail, i):
            raise Bad("a goal not closed by the wall's proof script")
        i += len(tail)
    return goals, out


def check(manifest, witnesses, header):
    rows = open(manifest).read().splitlines()
    if len(rows) < 4 or not rows[0].startswith("goals ") or not rows[1].startswith("witnesses ") or not rows[2].startswith("distinct "):
        return "the manifest names no part"
    goals, wit, distinct = int(rows[0].split()[1]), int(rows[1].split()[1]), int(rows[2].split()[1])
    if wit != witnesses:
        return f"the manifest counts {wit} witness(es), the wall fed in {witnesses}"
    fed = [tuple(l.split("\t")) for l in open(manifest[:-4] + ".fed").read().splitlines()]
    if len(fed) != wit:
        return f"the fed list holds {len(fed)} witness(es), the manifest counts {wit}"
    total, seen = 0, []
    for row in rows[3:]:
        path, n = row.split("\t")
        if not os.path.exists(path):
            return f"part {path} is missing"
        try:
            got, out = stated(path, header)
        except (Bad, IndexError) as e:
            return f"part {path} does not read back: {e}"
        if got != int(n):
            return f"part {path} holds {got} goal(s), the manifest lists {n}"
        total += got
        seen += out
    if total != goals:
        return f"the parts hold {total} goal(s), the manifest lists {goals}"
    if len(seen) != len(set(seen)):
        return "a text is stated twice"
    if len(seen) != distinct:
        return f"the parts state {len(seen)} distinct text(s), the manifest counts {distinct}"
    if set(seen) != set(fed):
        missing = len(set(fed) - set(seen))
        return f"{missing} witness(es) fed in are stated by no part, {len(set(seen) - set(fed))} stated text(s) were never fed in"
    return None


if __name__ == "__main__":
    if len(sys.argv) == 5 and sys.argv[1] == "check":
        why = check(sys.argv[2], int(sys.argv[3]), open(sys.argv[4]).read())
        if why:
            print(f"STRUCTURAL WALL FAIL: kernel parts: {why}", file=sys.stderr)
            sys.exit(1)
        sys.exit(0)
    print(__doc__, file=sys.stderr)
    sys.exit(2)
