#!/usr/bin/env python3
"""#4005 row 4: the heap-access ledger — every load, store, memory.copy and
memory.fill of a structural wall artifact that no certificate covers.

almide-anchor-check --census --census-fns prints one CENSUSFN line per
defined function of an artifact:

    CENSUSFN <pos> <frame|routine> <routine|selfhost|code> <label>
             <accesses> <located> <linear> <header> <bulk>

`located` counts the accesses a witness event names (bounds, init, fields)
and those a verified linear or routine certificate covers. What is left —
`accesses - located` — is the function's UNCOVERED count. The ledger keys it

    routine   <label>      a runtime routine (the anchor file's `routines`
                           line): the most any artifact ships uncovered
    selfhost  <name>       a self-host body (the `selfhost` line, its
                           per-artifact `__selfhost_N.` prefix dropped): the
                           most any artifact ships uncovered
    code      <fixture>    the fixture's own functions (user code, closures,
                           every function neither line names), summed

  heap_ledger.py measure <wall OUT dir>              the measured rows
  heap_ledger.py check <ledger> <measured> <full>    exit 1 when a row rose,
                                                     or (full = 1) a row
                                                     sits above its measure

The ledger only moves DOWN: a measured count above its row (or a key with
no row) fails; on a full run a row above what it measures is stale and must
be lowered to it. A new routine, body or fixture enters with its own row.
"""
import os
import re
import sys

PREFIX = re.compile(r"^__selfhost_\d+\.")


def measure(out):
    index = [row.rstrip("\n").split("\t") for row in open(os.path.join(out, "index")) if row.strip()]
    rows = {}
    tot = {"accesses": 0, "located": 0, "linear": 0, "header": 0, "bulk": 0}
    seen = 0
    for k, fx in index:
        path = os.path.join(out, k + ".anchor")
        if not os.path.exists(path):
            continue
        lines = [l for l in open(path, errors="replace").read().splitlines() if l.startswith("CENSUSFN ")]
        census = [l for l in open(path, errors="replace").read().splitlines() if l.startswith("CENSUS")]
        if not census or any(l.startswith("CENSUS: unreadable") for l in census):
            sys.exit(f"heap ledger: {fx}: no readable census in {path}")
        seen += 1
        code = 0
        for line in lines:
            parts = line.split()
            if len(parts) != 10:
                sys.exit(f"heap ledger: {fx}: a malformed CENSUSFN line: {line}")
            _, _, _, source, label, acc, loc, lin, hdr, bulk = parts
            acc, loc, lin, hdr, bulk = int(acc), int(loc), int(lin), int(hdr), int(bulk)
            for key, v in zip(("accesses", "located", "linear", "header", "bulk"), (acc, loc, lin, hdr, bulk)):
                tot[key] += v
            unc = acc - loc
            if source == "code":
                code += unc
                continue
            cls = "routine" if source == "routine" else "selfhost"
            name = PREFIX.sub("", label) if cls == "selfhost" else label
            rows[(cls, name)] = max(rows.get((cls, name), 0), unc)
        if code:
            rows[("code", fx)] = code
    print(
        f"  [heap ledger] {seen} artifact(s): {tot['accesses']} heap access(es), {tot['located']} covered "
        f"({tot['linear']} by a linear or routine certificate), {tot['accesses'] - tot['located']} uncovered "
        f"({tot['header']} header word(s), {tot['bulk']} bulk)",
        file=sys.stderr,
    )
    return rows


def read(path):
    rows = {}
    if not os.path.exists(path):
        return rows
    for line in open(path):
        line = line.rstrip("\n")
        if not line or line.startswith("#"):
            continue
        cls, key, n = line.split("\t")
        rows[(cls, key)] = int(n)
    return rows


def render(rows):
    out = []
    for (cls, key), n in sorted(rows.items()):
        if n > 0:
            out.append(f"{cls}\t{key}\t{n}")
    return "\n".join(out) + ("\n" if out else "")


def check(ledger_path, measured_path, full):
    ledger, got = read(ledger_path), read(measured_path)
    rose = [(k, n, ledger.get(k, 0)) for k, n in sorted(got.items()) if n > ledger.get(k, 0)]
    stale = [(k, got.get(k, 0), n) for k, n in sorted(ledger.items()) if full and got.get(k, 0) < n]
    for cls in ("routine", "selfhost", "code"):
        m = sum(n for (c, _), n in got.items() if c == cls)
        l = sum(n for (c, _), n in ledger.items() if c == cls)
        keys = sum(1 for (c, _) in got if c == cls)
        print(f"  [heap ledger] {cls}: {m} uncovered access(es) over {keys} key(s) measured; the ledger holds {l}")
    for (cls, key), n, was in rose[:40]:
        print(f"STRUCTURAL WALL FAIL: heap ledger: {cls} {key}: {n} uncovered access(es), the ledger allows {was}", file=sys.stderr)
    for (cls, key), n, was in stale[:40]:
        print(f"STRUCTURAL WALL FAIL: heap ledger: {cls} {key}: the row says {was}, this run measures {n} — lower it", file=sys.stderr)
    if rose or stale:
        print(f"  [heap ledger] {len(rose)} row(s) rose, {len(stale)} stale; regenerate the rows with "
              f"STRUCTURAL_WALL_LEDGER_OUT=<file> (a rise needs the change that removed the coverage undone)", file=sys.stderr)
        return 1
    return 0


def main(argv):
    if len(argv) == 3 and argv[1] == "measure":
        sys.stdout.write(render(measure(argv[2])))
        return 0
    if len(argv) == 5 and argv[1] == "check":
        return check(argv[2], argv[3], argv[4] == "1")
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
