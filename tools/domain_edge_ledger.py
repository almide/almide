#!/usr/bin/env python3
"""The domain-edges ledger step: diff a measured matrix against proofs/domain-edges.toml.

Split out of scripts/check-domain-edges.sh (#2387) so the ledger's admission rules
are testable with FORGED measurements, without a compiler or an hour-long sweep.
The script still owns the measurement and the flags; this file owns what may
reach the ledger.

ADMISSION RULES
---------------
1. Bidirectional diff (PR #1449): a measured-but-undeclared DIVERGE fails (new
   finding); a declared-but-not-measured row fails (stale; delete it).
2. The same two directions for the SKIPPED population (#2402): a slot the matrix
   could not build is a named `[[skip]]` row; a skip that appears is coverage
   lost and fails, a declared skip that becomes buildable must be deleted.
3. Budget pin (#2387): an EDGE measurement reaches the ledger ONLY if its wasm
   leg was taken under the budget the ledger declares (`wasm_budget_bytes`),
   run-level AND per cell (`pinned`). A reading without a budget, with a
   different budget, or a DIVERGE cell marked unpinned is an AVAILABILITY
   reading — it says how much memory the machine had free, not what the
   compiler does — and is refused before any diff, so it can neither fail the
   gate as a "divergence" nor be written as a row. This is the rule that was
   missing when two `i32_max` phantoms were declared for a month and three more
   were nearly added. A skips-only measurement runs no edge and carries no
   verdicts, so the pin does not apply to it.

Usage:
    python3 tools/domain_edge_ledger.py MEASURED.json LEDGER.toml [--update] [--only MOD]
    python3 tools/domain_edge_ledger.py --selftest
"""

import json
import pathlib
import re
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "tools"))
from domain_edge_matrix import WASM_BUDGET_BYTES  # noqa: E402  (the one declared budget)

ROW_RE = re.compile(r'\[\[divergence\]\]\ncell\s*=\s*"([^"]+)"\s*\nreason\s*=\s*"([^"]*)"')
SKIP_RE = re.compile(r'\[\[skip\]\]\nslot\s*=\s*"([^"]+)"\s*\nreason\s*=\s*"([^"]*)"')
BUDGET_RE = re.compile(r'^wasm_budget_bytes\s*=\s*(\d+)\s*$', re.M)

DEFAULT_HEADER = (
    "# Integer-domain edge divergences — MEASURED, not asserted.\n"
    "#\n"
    "# Regenerate with `scripts/check-domain-edges.sh --update`. Every row must earn a\n"
    "# `reason`; an UNTRIAGED row is a bug nobody has looked at yet, and the count is a\n"
    "# shrink-only ceiling. A row that stops diverging must be DELETED in the same PR\n"
    "# that fixes it — the gate fails on a stale row exactly as it fails on a new one,\n"
    "# because a ledger of things that used to be true measures nothing.\n"
    "#\n"
    "# `wasm_budget_bytes` is the linear-memory budget every wasm reading below was\n"
    "# taken under (tools/domain_edge_matrix.py, #2387): the wasm leg runs on the\n"
    "# wasmtime CLI with `-W max-memory-size=<budget>`, so a request past it takes the\n"
    "# defined C-197 abort on every machine instead of on the machines that happened\n"
    "# to be short of memory. A row means \"diverges at this budget\". A measurement\n"
    "# taken at another budget, or unpinned, is refused by tools/domain_edge_ledger.py\n"
    "# before it can touch this file. The native leg is uncapped (see the tool).\n")

SKIP_BANNER = (
    "\n# ── Skipped slots: coverage the matrix does NOT have, by name (#2402) ──────\n"
    "# One row per public Int-parameter slot the tool could not build, render, or\n"
    "# type-check, with the measured reason (`skip_count` above). The gate diffs\n"
    "# these both ways too: a slot that BECOMES unbuildable is coverage lost and\n"
    "# fails; a slot that becomes buildable must have its row deleted.\n")


def key(c):
    return f'{c["module"]}.{c["fn"]}:{c["param"]}:{c["edge"]}'


def read_ledger(text):
    """(declared cells, reasons, declared skips, header before the counts, declared budget)."""
    header = text.split("\ncount =", 1)[0] if "\ncount =" in text else ""
    declared, reasons, skips = set(), {}, {}
    for m in ROW_RE.finditer(text):
        declared.add(m.group(1))
        reasons[m.group(1)] = m.group(2)
    for m in SKIP_RE.finditer(text):
        skips[m.group(1)] = m.group(2)
    b = BUDGET_RE.search(text)
    return declared, reasons, skips, header, (int(b.group(1)) if b else None)


def refuse_unpinned(measured, declared_budget):
    """The AVAILABILITY guard. Returns a list of reasons; empty means admitted."""
    errs = []
    budget = measured.get("wasm_budget_bytes")
    if budget is None:
        errs.append("the measurement carries no `wasm_budget_bytes`: its wasm leg ran "
                    "unpinned, so every allocation-sized cell reads the machine's free "
                    "memory, not the compiler (#2387)")
    elif declared_budget is None:
        errs.append(f"the ledger declares no `wasm_budget_bytes` (measurement was pinned at "
                    f"{budget}) — declare it before recording any row")
    elif budget != declared_budget:
        errs.append(f"the measurement was pinned at wasm_budget_bytes={budget} but the ledger "
                    f"declares {declared_budget}: the rows would mean different things")
    unpinned = sorted(key(c) for c in measured["cells"]
                      if c["verdict"] == "DIVERGE" and not c.get("pinned", False))
    if unpinned:
        errs.append(f"{len(unpinned)} DIVERGE cell(s) are marked unpinned — availability "
                    f"readings, not verdicts: " + ", ".join(unpinned[:10]))
    return errs


def render(header, rows, skips, budget):
    """The whole file. Both counts precede the first table: a top-level key after
    `[[divergence]]` rows would parse as a key of the last row."""
    body = "".join(f'\n[[divergence]]\ncell   = "{k}"\nreason = "{rows[k]}"\n' for k in sorted(rows))
    skip_rows = "".join(f'\n[[skip]]\nslot   = "{k}"\nreason = "{skips[k]}"\n' for k in sorted(skips))
    header = header or DEFAULT_HEADER
    if not BUDGET_RE.search(header):
        header = header.rstrip("\n") + "\n"
    return (f"{header}\nwasm_budget_bytes = {budget}\ncount = {len(rows)}\nskip_count = {len(skips)}\n"
            + body + SKIP_BANNER + skip_rows)


def run(measured, ledger_path, update=False, only="", out=print):
    """Exit code of the ledger step (0 ok, 1 fail). `out` receives the report lines."""
    ledger = pathlib.Path(ledger_path)
    text = ledger.read_text() if ledger.exists() else ""
    declared, reasons, declared_skips, header, declared_budget = read_ledger(text)
    header = BUDGET_RE.sub("", header).rstrip("\n") + "\n" if header else header
    skips_only = bool(measured.get("skips_only"))
    cells = measured["cells"]
    measured_skips = {s["slot"]: s["reason"] for s in measured.get("skipped", [])}
    in_scope = (lambda k: k.startswith(only + ".")) if only else (lambda k: True)

    if not skips_only:
        if update and declared_budget is None:
            declared_budget = WASM_BUDGET_BYTES  # a first ledger declares the tool's budget
        refused = refuse_unpinned(measured, declared_budget)
        if refused:
            out("::error::domain-edges: measurement REFUSED — it is availability-dependent, "
                "and an availability reading must never reach the ledger as AGREE or DIVERGE:")
            for r in refused:
                out(f"    {r}")
            return 1
    elif declared_budget is None:
        declared_budget = WASM_BUDGET_BYTES

    measured_keys = {key(c) for c in cells if c["verdict"] == "DIVERGE"}

    if update:
        # Rows outside the measured scope are carried over untouched; rows inside
        # it are replaced by what was measured. A --skips-only run replaces only
        # the skip rows.
        if not skips_only:
            kept = {k: v for k, v in reasons.items() if not in_scope(k)}
            for k in sorted(measured_keys):
                kept[k] = reasons.get(k, "UNTRIAGED — measured divergent, no cause recorded yet")
        else:
            kept = reasons
        kept_skips = {k: v for k, v in declared_skips.items() if not in_scope(k)}
        kept_skips.update(measured_skips)
        ledger.parent.mkdir(parents=True, exist_ok=True)
        ledger.write_text(render(header, kept, kept_skips, declared_budget))
        out(f"domain-edges: ledger rewritten — {len(kept)} divergent cells, {len(kept_skips)} named skips, "
            f"wasm_budget_bytes={declared_budget}")
        return 0

    # Scoping: a --only run can only speak for the module it measured.
    declared = {k for k in declared if in_scope(k)}
    declared_skips = {k: v for k, v in declared_skips.items() if in_scope(k)}
    fail = False

    if not skips_only:
        new = sorted(measured_keys - declared)
        stale = sorted(declared - measured_keys)
        if new:
            fail = True
            out(f"::error::{len(new)} UNDECLARED divergent cell(s) — a guard was defeated by its own arithmetic, or a new one was written that way:")
            for k in new[:25]:
                out(f"    {k}")
            if len(new) > 25:
                out(f"    … and {len(new) - 25} more")
            out("  Fix it, or record it with a reason: scripts/check-domain-edges.sh --update")
        if stale:
            fail = True
            out(f"::error::{len(stale)} ledger row(s) no longer diverge — delete them (the count is shrink-only):")
            for k in stale[:25]:
                out(f"    {k}")
        untriaged = [k for k in sorted(declared & measured_keys) if reasons.get(k, "").startswith("UNTRIAGED")]
        if untriaged:
            out(f"domain-edges: {len(untriaged)} declared cell(s) still UNTRIAGED (not a failure, but they are open bugs)")

    new_skips = sorted(set(measured_skips) - set(declared_skips))
    regained = sorted(set(declared_skips) - set(measured_skips))
    if new_skips:
        fail = True
        out(f"::error::{len(new_skips)} UNDECLARED skipped slot(s) — coverage the matrix had (or should have) and now does not:")
        for k in new_skips[:25]:
            out(f"    {k}: {measured_skips[k]}")
        if len(new_skips) > 25:
            out(f"    … and {len(new_skips) - 25} more")
        out("  Make the slot buildable (VALUES / RENDER in tools/domain_edge_matrix.py), or declare it by name: scripts/check-domain-edges.sh --update --skips-only")
    if regained:
        fail = True
        out(f"::error::{len(regained)} declared skip row(s) are now buildable — delete them (coverage regained must be recorded):")
        for k in regained[:25]:
            out(f"    {k}")

    exercised = len(measured.get("exercised", []))
    total = exercised + len(measured_skips)
    scope = f" (module {only})" if only else ""
    out(f"domain-edges: coverage {exercised} of {total} signatures exercised{scope}, "
        f"{len(measured_skips)} skipped" + ("" if new_skips or regained else " — every skip declared by name"))
    if not fail:
        if skips_only:
            out("domain-edges: OK — skip ledger matches (no edge was run).")
        else:
            out(f"domain-edges: OK — {len(measured_keys)} divergent cells, all declared, at "
                f"wasm_budget_bytes={declared_budget}.")
    return 1 if fail else 0


# ── Self-test: forged availability-dependent cells must not reach the ledger ──
def _cell(k, verdict, pinned=True):
    mod_fn, param, edge = k.rsplit(":", 2)
    mod, fn = mod_fn.split(".", 1)
    return {"module": mod, "fn": fn, "param": param, "edge": edge, "value": 0,
            "verdict": verdict, "fuzzer_reachable": True, "pinned": pinned}


def selftest():
    B = WASM_BUDGET_BYTES
    phantom = "bytes.new:len:i32_max"          # the cell #2387 measured both ways
    structural = "bytes.new:len:u32_max"       # past the 4 GiB wasm32 bound: declared C-197
    fails = []

    def case(name, measured, ledger_text, update=False, expect_rc=None, expect_in=(), expect_absent=()):
        with tempfile.TemporaryDirectory() as d:
            p = pathlib.Path(d) / "ledger.toml"
            p.write_text(ledger_text)
            lines = []
            rc = run(measured, p, update=update, out=lines.append)
            after = p.read_text()
            report = "\n".join(lines)
            if expect_rc is not None and rc != expect_rc:
                fails.append(f"{name}: exit {rc}, expected {expect_rc}\n{report}")
            for s in expect_in:
                if s not in report:
                    fails.append(f"{name}: report lacks {s!r}\n{report}")
            for s in expect_absent:
                if s in after:
                    fails.append(f"{name}: the ledger now contains {s!r} — a forged cell reached it")
            return after

    pinned_ledger = (f"# header\n\nwasm_budget_bytes = {B}\ncount = 1\nskip_count = 1\n\n[[divergence]]\n"
                     f"cell   = \"{structural}\"\nreason = \"C-197\"\n\n[[skip]]\nslot   = \"bytes.x(n)\"\n"
                     f"reason = \"unbuildable: no VALUES entry for parameter type `W`\"\n")
    skip = [{"slot": "bytes.x(n)", "reason": "unbuildable: no VALUES entry for parameter type `W`"}]

    # 1. An unpinned run (no budget at all) carrying a DIVERGE on the phantom: refused, never diffed.
    case("unpinned run",
         {"cells": [_cell(phantom, "DIVERGE"), _cell(structural, "DIVERGE")], "skipped": skip},
         pinned_ledger, expect_rc=1, expect_in=("REFUSED", "no `wasm_budget_bytes`"))
    # 2. Same, through --update: the ledger must be left untouched.
    after = case("unpinned --update",
                 {"cells": [_cell(phantom, "DIVERGE"), _cell(structural, "DIVERGE")], "skipped": skip},
                 pinned_ledger, update=True, expect_rc=1, expect_absent=(phantom,))
    if after != pinned_ledger:
        fails.append("unpinned --update rewrote the ledger")
    # 3. A run pinned at a DIFFERENT budget: refused (its rows would mean something else).
    case("budget mismatch",
         {"cells": [_cell(structural, "DIVERGE")], "skipped": skip, "wasm_budget_bytes": B * 2},
         pinned_ledger, expect_rc=1, expect_in=("REFUSED", "pinned at wasm_budget_bytes="))
    # 4. A pinned run whose phantom cell is individually marked unpinned: refused by name.
    case("unpinned cell",
         {"cells": [_cell(phantom, "DIVERGE", pinned=False), _cell(structural, "DIVERGE")],
          "skipped": skip, "wasm_budget_bytes": B},
         pinned_ledger, expect_rc=1, expect_in=("REFUSED", phantom))
    case("unpinned cell --update",
         {"cells": [_cell(phantom, "DIVERGE", pinned=False), _cell(structural, "DIVERGE")],
          "skipped": skip, "wasm_budget_bytes": B},
         pinned_ledger, update=True, expect_rc=1, expect_absent=(phantom,))
    # 5. The ledger itself declares no budget: refused (declare it first).
    case("undeclared ledger budget",
         {"cells": [_cell(structural, "DIVERGE")], "skipped": skip, "wasm_budget_bytes": B},
         f"count = 1\nskip_count = 0\n\n[[divergence]]\ncell   = \"{structural}\"\nreason = \"C-197\"\n",
         expect_rc=1, expect_in=("REFUSED", "declares no `wasm_budget_bytes`"))
    # 6. Control: a properly pinned run is admitted and diffed both ways, cells and skips.
    case("pinned ok",
         {"cells": [_cell(structural, "DIVERGE"), _cell(phantom, "AGREE")], "skipped": skip, "wasm_budget_bytes": B},
         pinned_ledger, expect_rc=0, expect_in=("OK", "every skip declared by name"))
    case("pinned new finding",
         {"cells": [_cell(structural, "DIVERGE"), _cell(phantom, "DIVERGE")], "skipped": skip, "wasm_budget_bytes": B},
         pinned_ledger, expect_rc=1, expect_in=("UNDECLARED divergent", phantom))
    case("pinned stale row",
         {"cells": [_cell(structural, "AGREE")], "skipped": skip, "wasm_budget_bytes": B},
         pinned_ledger, expect_rc=1, expect_in=("no longer diverge", structural))
    case("pinned new skip",
         {"cells": [_cell(structural, "DIVERGE")], "wasm_budget_bytes": B,
          "skipped": skip + [{"slot": "bytes.y(k)", "reason": "unbuildable: no RENDER entry for return type `Q`"}]},
         pinned_ledger, expect_rc=1, expect_in=("UNDECLARED skipped slot", "bytes.y(k): unbuildable"))
    # 7. A skips-only run carries no verdicts and no budget: the pin does not apply, the skip diff does.
    case("skips-only ok", {"cells": [], "skipped": skip, "skips_only": True},
         pinned_ledger, expect_rc=0, expect_in=("skip ledger matches",))
    case("skips-only regained", {"cells": [], "skipped": [], "skips_only": True},
         pinned_ledger, expect_rc=1, expect_in=("now buildable", "bytes.x(n)"))
    # 8. A pinned --update writes the budget, only the pinned DIVERGE rows, and the skips.
    after = case("pinned --update",
                 {"cells": [_cell(structural, "DIVERGE"), _cell(phantom, "AGREE")], "skipped": skip, "wasm_budget_bytes": B},
                 pinned_ledger, update=True, expect_rc=0, expect_absent=(phantom,))
    for needle in (f"wasm_budget_bytes = {B}", structural, "skip_count = 1", 'slot   = "bytes.x(n)"'):
        if needle not in after:
            fails.append(f"pinned --update did not write {needle!r}:\n{after}")
    if after.count("wasm_budget_bytes") != 1:
        fails.append("pinned --update duplicated the budget key")

    if fails:
        print("domain-edges selftest FAILED:")
        for f in fails:
            print("  - " + f)
        return 1
    print("domain-edges selftest: OK — an availability-dependent (unpinned / off-budget) "
          "reading is refused before the diff and never written; a pinned one is diffed both "
          "ways, cells and skips")
    return 0


def main(argv):
    if argv[:1] == ["--selftest"]:
        return selftest()
    measured_path, ledger_path = argv[0], argv[1]
    update = "--update" in argv
    only = argv[argv.index("--only") + 1] if "--only" in argv else ""
    return run(json.load(open(measured_path)), ledger_path, update=update, only=only)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
