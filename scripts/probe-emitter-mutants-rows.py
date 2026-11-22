#!/usr/bin/env python3
"""Probe only: one compiler's per-program outcomes, read from what the harness
(probe-emitter-mutants.sh) left on disk.

  probe-emitter-mutants-rows.py --baseline <out-dir> <fixtures.txt> <rows.tsv>
      every program as the unmutated compiler emitted it (<out-dir>/base)
  probe-emitter-mutants-rows.py <out-dir> <fixtures.txt> <rows.tsv>
      the programs a mutant refused or changed (<out-dir>/work against
      <out-dir>/base); also writes <out-dir>/changed.txt (programs whose module
      changed) and <out-dir>/counts.tsv (one line: the counts below, then the
      first message of each layer)

A row: program, emit_rc, wasm, bundle, anchors (1 = differs from the baseline;
the bundle is compared without its artifact line, which names the module's
hash), verify_rc (0 certified, 1 rejected, 3 incomplete: the producer declared
a function uncertified, 2 unreadable, 124 timed out), verify_counts
(witnesses,accepted,rejected,uncertified), verify_rejects (the properties with
a REJECT line), anchor_rc (0 accepted, 1 rejected), anchor_kind (the first word
of the rejection), anchor_numbers (every count the accepting byte-level check
printed, census included).
"""
import os
import re
import sys

COUNTS = ["programs", "c_timeout", "c_compiler", "c_producer", "wasm_diff", "bundle_only", "anchors_only",
          "p_reject", "p_incomplete", "p_other", "b_reject", "b_other"]
SUMMARY = re.compile(r"^almide-verify [^:]*: (\d+) witness\(es\), (\d+) accepted, (\d+) rejected, (\d+) uncertified")
LINES = (("ANCHORS OK", "A"), ("FIELDS OK", "F"), ("CENSUS", "C"), ("RC OK", "R"))


def text(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return ""


def one_line(s, limit=200):
    return re.sub(r"\s+", " ", s).strip()[:limit] or "-"


def verify_fields(out):
    counts, rejects = "-", []
    for line in out.splitlines():
        m = SUMMARY.match(line)
        if m:
            counts = ",".join(m.groups())
        elif line.startswith("REJECT"):
            parts = line.split()
            if len(parts) > 1 and parts[1] not in rejects:
                rejects.append(parts[1])
    return counts, ",".join(rejects) or "-"


def anchor_fields(out):
    kind, numbers = "-", []
    for line in out.splitlines():
        if line.startswith("ANCHORS REJECT: ") and kind == "-":
            kind = line[len("ANCHORS REJECT: "):].split()[0].rstrip(":") if line[len("ANCHORS REJECT: "):].split() else "?"
        for head, tag in LINES:
            if line.startswith(head):
                numbers.append(tag + ":" + ".".join(re.findall(r"\d+", line)))
    return kind, ";".join(numbers) or "-"


def first_message(out, *prefixes):
    for prefix in prefixes:
        for line in out.splitlines():
            if line.startswith(prefix):
                return line
    for line in out.splitlines():
        if line.strip():
            return line
    return ""


def main():
    args = sys.argv[1:]
    baseline = args[0] == "--baseline"
    if baseline:
        args = args[1:]
    out, fixtures, rows_path = args
    base, work = os.path.join(out, "base"), os.path.join(out, "work")
    here = base if baseline else work
    counts = dict.fromkeys(COUNTS, 0)
    first = {"compile": "-", "proven": "-", "byte": "-"}
    changed, rows = [], []
    with open(fixtures) as fh:
        programs = [line.strip() for line in fh if line.strip()]
    for prog in programs:
        n = prog.replace("/", "_")
        p = os.path.join(here, n)
        counts["programs"] += 1
        rc = text(p + ".rc").strip() or "999"
        if rc != "0":
            kind = "c_timeout" if rc in ("124", "1124") else ("c_producer" if int(rc) >= 1000 else "c_compiler")
            counts[kind] += 1
            if first["compile"] == "-":
                msg = next((l for l in (text(p + ".cerr") + "\n" + text(p + ".perr")).splitlines() if l.strip()), "")
                first["compile"] = one_line(f"{prog}: rc {rc}: {msg}")
            rows.append([prog, rc] + ["-"] * 9)
            continue
        if baseline:
            w = b = a = "0"
        else:
            w, b, a = (text(p + ".cmp").split() + ["?", "?", "?"])[:3]
            if (w, b, a) == ("0", "0", "0"):
                continue
            if w == "1":
                counts["wasm_diff"] += 1
                changed.append(prog)
            elif b == "1":
                counts["bundle_only"] += 1
            else:
                counts["anchors_only"] += 1
        vrc, arc = text(p + ".vrc").strip() or "?", text(p + ".arc").strip() or "?"
        vout, aout = text(p + ".verify"), text(p + ".anchor")
        vcounts, vrejects = verify_fields(vout)
        akind, anums = anchor_fields(aout)
        if vrc != "0":
            counts["p_reject" if vrc == "1" else ("p_incomplete" if vrc == "3" else "p_other")] += 1
            if first["proven"] == "-":
                first["proven"] = one_line(f"{prog}: rc {vrc}: {first_message(vout, 'REJECT', 'UNCERTIFIED')}")
        if arc != "0":
            counts["b_reject" if arc == "1" else "b_other"] += 1
            if first["byte"] == "-":
                first["byte"] = one_line(f"{prog}: rc {arc}: {first_message(aout, 'ANCHORS REJECT')}")
        rows.append([prog, rc, w, b, a, vrc, vcounts, vrejects, arc, akind, anums])
    with open(rows_path, "w", encoding="utf-8") as fh:
        fh.write("program\temit_rc\twasm\tbundle\tanchors\tverify_rc\tverify_counts\tverify_rejects\tanchor_rc\tanchor_kind\tanchor_numbers\n")
        for r in rows:
            fh.write("\t".join(r) + "\n")
    if not baseline:
        with open(os.path.join(out, "changed.txt"), "w", encoding="utf-8") as fh:
            fh.write("".join(p + "\n" for p in changed))
        with open(os.path.join(out, "counts.tsv"), "w", encoding="utf-8") as fh:
            fh.write("\t".join([str(counts[k]) for k in COUNTS] + [first["compile"], first["proven"], first["byte"]]) + "\n")


if __name__ == "__main__":
    main()
