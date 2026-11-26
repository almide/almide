#!/usr/bin/env python3
"""Turn cut_acquires.py output into one Rocq file per frame.

  acquire_goals.py <work dir> <label>=<cut file> [<label>=<cut file>...]

For a frame whose acquires are all placed: <work>/A_<n>.v (`Example c : acquire_ok <alloc>
<rc_inc> [body] [certificates] = true. Proof. vm_compute. reflexivity. Qed.`). For a frame
with an acquire not placed whose body holds fewer calls of the rc_inc routine than the frame
has named acquires: <work>/S_<n>.v (`Example c : acquire_short <rc_inc> [body] <acquires> =
true.`), the kernel's own count. <work>/index.tsv has one line per frame with a named
acquire:

  <goal, or `-` when an acquire is not placed and the count does not settle it> <label>
  <position> <function> <acquires> <placed> <the acquires not placed, or `-`>
"""
import re
import sys

CERT = re.compile(r"^([LCG])(\d+):(\d+):(\d+)$")
KIND = {"L": "HLocal", "C": "HCell", "G": "HGlobal"}
HEAD = ("From AlmideTrust Require Import GuardWindow GuardFresh GuardPath GuardAcquire.\n"
        "From Stdlib Require Import ZArith List.\nImport ListNotations.\nOpen Scope Z_scope.\n\n")


def main():
    work = sys.argv[1]
    rows, na, ns = [], 0, 0
    for arg in sys.argv[2:]:
        label, path = arg.split("=", 1)
        for line in open(path, encoding="utf-8"):
            t = line.split()
            if not t or t[0] == "UNREAD":
                continue
            if t[0] != "FRAME" or line.count("|") != 3:
                sys.exit(f"{path}: not a cutter line: {line.strip()[:80]}")
            head, certs, left, body = line.split("|")
            _, pos, func, alloc, inc, frac, calls = head.split()
            placed, total = frac.split("/")
            left = " ".join(left.split())
            if not all(x.isdigit() for x in (pos, func, alloc, inc, placed, total, calls)) \
                    or not all(b.isdigit() for b in body.split()):
                sys.exit(f"{path}: a FRAME line is not numbers")
            bs = "; ".join(body.split())
            if placed != total:
                if int(calls) < int(total):
                    name = f"S_{ns:05d}"
                    ns += 1
                    with open(f"{work}/{name}.v", "w", encoding="utf-8") as f:
                        f.write(HEAD + f"Example c : acquire_short {inc}\n  [{bs}]\n  {total} = true.\n"
                                "Proof. vm_compute. reflexivity. Qed.\n")
                    rows.append((name, label, pos, func, total, placed, left))
                else:
                    rows.append(("-", label, pos, func, total, placed, left))
                continue
            cs = []
            for c in certs.split():
                m = CERT.match(c)
                if not m:
                    sys.exit(f"{path}: not a certificate: {c}")
                cs.append(f"({KIND[m.group(1)]} {m.group(2)}, {m.group(3)}%nat, {m.group(4)}%nat)")
            name = f"A_{na:05d}"
            na += 1
            with open(f"{work}/{name}.v", "w", encoding="utf-8") as f:
                f.write(HEAD + f"Example c : acquire_ok {alloc} {inc}\n  [{bs}]\n  [{'; '.join(cs)}] = true.\n"
                        "Proof. vm_compute. reflexivity. Qed.\n")
            rows.append((name, label, pos, func, total, placed, left))
    with open(f"{work}/index.tsv", "w", encoding="utf-8") as f:
        for r in rows:
            f.write("\t".join(r) + "\n")
    print(f"{len(rows)} frame(s) with a named acquire; {na} with every acquire placed and {ns} with fewer rc_inc "
          f"calls than acquires, written as statements")


if __name__ == "__main__":
    main()
