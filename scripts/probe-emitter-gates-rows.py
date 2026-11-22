#!/usr/bin/env python3
"""Probe only: read what the emitter crate's two corpus tests printed for one
mutant and write the columns probe-emitter-gates.sh records.

  probe-emitter-gates-rows.py <out-dir> [<detail-prefix>]

Reads <out-dir>/alloc_ledger.{out,rc} and <out-dir>/witness_floor.{out,rc}
and prints one tab-separated line:

  ledger_status ledger_rc ledger_drifts leak live_less watermark count
  armed refusal unrefusal other
  floor_status floor_rc floor_kind admitted witnessed stdlib declined
  certified manifest modes_checked modes_rejected
  first_ledger first_floor

A status is `pass` (the one test ran and passed), `fail` (it ran and failed),
`timeout`, `not-run` (the binary ran no test: a renamed test must not read as
a pass) or `error` (anything else, such as a crash). With <detail-prefix>, the
head of each failing output is kept as <detail-prefix>.ledger.txt / .floor.txt.
"""
import os
import re
import sys

KEEP_LINES = 80
KEEP_BYTES = 16000


def read(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return f.read()
    except OSError:
        return ""


def status_of(text, rc):
    m = re.search(r"test result: \w+\. (\d+) passed; (\d+) failed", text)
    if rc == "124":
        return "timeout"
    if not m:
        return "error"
    passed, failed = int(m.group(1)), int(m.group(2))
    if passed + failed == 0:
        return "not-run"
    if failed:
        return "fail"
    return "pass" if rc == "0" else "error"


def panic_message(text):
    """The first line of the first panic's message, without the location line."""
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if "panicked at" in line:
            for nxt in lines[i + 1:i + 4]:
                if nxt.strip():
                    return nxt.strip()
    return "-"


LEDGER_KINDS = [
    ("leak", "a new leak"),
    ("live_less", "lower the row"),
    ("watermark", ": watermark "),
    ("count", ": count "),
    ("armed", "the armed module's"),
    ("refusal", "structural leg refuses it"),
    ("unrefusal", "now lowers"),
]


def ledger(text):
    counts = {k: 0 for k, _ in LEDGER_KINDS}
    counts["other"] = 0
    first = "-"
    in_list = False
    for line in text.splitlines():
        if line.startswith("allocation ledger ("):
            in_list = True
            continue
        if not in_list:
            continue
        if line.startswith("note: ") or line.startswith("test ") or not line.strip():
            break
        for kind, needle in LEDGER_KINDS:
            if needle in line:
                counts[kind] += 1
                if kind == "leak" and first == "-":
                    first = line.strip()
                break
        else:
            counts["other"] += 1
    m = re.search(r"allocation ledger \((\d+) drift\(s\)\)", text)
    drifts = m.group(1) if m else "0"
    if first == "-":
        first = panic_message(text)
    return drifts, counts, first


FLOOR_KINDS = [
    ("modes", "call-mode witness(es) rejected"),
    ("nondet", "emission passes disagree"),
    ("poison", "poisoned their witness"),
    ("checker", "rejected by the portable checker"),
    ("fixture-lost", "lost their certificate"),
    ("fixture-list", "witness-fixtures.txt is behind"),
    ("declines-grew", "declined frames GREW"),
    ("declines-shrank", "declined frames shrank"),
    ("stdlib-lost", "witnessed NOWHERE"),
    ("floor", "< floor"),
    ("whole-leg", "whole-module leg"),
]


def floor(text):
    m = re.search(
        r"\[witness-floor\] admitted (\d+) function\(s\), (\d+) distinct with RC events \((\d+) stdlib\), (\d+) declined; "
        r"(\d+) of (\d+) spec/wasm_cross fixture\(s\) fully certified; (\d+) call-mode witness\(es\), (\d+) rejected",
        text,
    )
    numbers = list(m.groups()) if m else ["-"] * 8
    first = panic_message(text)
    kind = "-"
    if first != "-":
        kind = "other"
        for name, needle in FLOOR_KINDS:
            if needle in first:
                kind = name
                break
    return numbers, kind, first


def keep(text, path):
    head = "\n".join(text.splitlines()[:KEEP_LINES])[:KEEP_BYTES]
    with open(path, "w", encoding="utf-8") as f:
        f.write(head + "\n")


def clean(s):
    return s.replace("\t", " ").replace("\r", " ")[:300]


def main():
    out = sys.argv[1]
    prefix = sys.argv[2] if len(sys.argv) > 2 else None
    lt, lrc = read(os.path.join(out, "alloc_ledger.out")), read(os.path.join(out, "alloc_ledger.rc")).strip()
    ft, frc = read(os.path.join(out, "witness_floor.out")), read(os.path.join(out, "witness_floor.rc")).strip()
    ls, fs = status_of(lt, lrc), status_of(ft, frc)
    drifts, counts, first_ledger = ledger(lt)
    numbers, kind, first_floor = floor(ft)
    if ls == "pass":
        first_ledger = "-"
    if fs == "pass":
        kind, first_floor = "-", "-"
    row = [ls, lrc or "-", drifts] + [str(counts[k]) for k, _ in LEDGER_KINDS] + [str(counts["other"])]
    row += [fs, frc or "-", kind] + numbers + [clean(first_ledger), clean(first_floor)]
    print("\t".join(row))
    if prefix:
        if ls != "pass":
            keep(lt, prefix + ".ledger.txt")
        if fs != "pass":
            keep(ft, prefix + ".floor.txt")


if __name__ == "__main__":
    main()
