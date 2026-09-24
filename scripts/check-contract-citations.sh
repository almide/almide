#!/usr/bin/env bash
# CONTRACT-CITATION GATE (#2406) — every `C-NNN` spelled anywhere in the tree
# names a contract that exists.
#
# `scripts/check-contracts.sh` gates the LEDGER and the `// @contract:` headers
# of spec/wasm_cross fixtures: those ids are checked for shape and membership
# and the fixture<->contract link is bidirectional. Every OTHER place a contract
# id is written — a stdlib source comment, a compiler comment, a roadmap page,
# the cheatsheet, a proof ledger — was never validated, and one well-formed
# citation in shipped stdlib source named no contract for two months (#2406:
# a fuzz-finding index wearing the `C-` prefix). The reader's only move on a
# `C-NNN` is to look it up in docs/contracts/contracts.toml; a spelling that
# resolves nowhere is worse than no citation, and the LLM-facing surface cannot
# tell the two apart.
#
# The rule, for every token that STARTS with `C-` followed by digits (a token
# boundary on both sides — `RFC-7386` is not a citation, `C-01a` is not an id):
#   (1) SHAPE: exactly three digits. One digit or four digits is malformed — a
#       four-digit id is a line number in a contract's clothes (#2403);
#   (2) MEMBERSHIP: the id is an `id = "C-NNN"` row of the ledger.
#
# Scan set: every tracked text file under the repo root — *.md *.toml *.almd
# *.rs *.sh *.yml *.yaml *.txt *.py *.json *.lean — minus build output
# (target/, node_modules/), the submodules (grammar/, research/.../upstream/),
# the agent worktrees (.claude/) and .git. Two exclusions coordinate with
# other gates rather than loosen this one:
#   - the `// @contract:` header LINE of a spec/wasm_cross fixture is
#     check-contracts.sh's (shape + membership + symmetric link); it is
#     skipped here so one defect is reported by one gate. The rest of the
#     fixture is scanned.
#   - ALLOWLIST (whole files): the shape validator's own negative test
#     vectors. tools/almide-gates/src/contract_audit.almd and
#     tools/almide-gates/src/ledger_schema.almd assert that one-, two- and
#     four-digit spellings are REJECTED; a gate that flags its own self-test is a gate
#     people disable. Nothing else is allowlisted: there is no placeholder
#     prose in the tree that spells a digit id, and none should be added.
#
#   bash scripts/check-contract-citations.sh          # gate (CI `checks` job, lefthook)
#   CONTRACT_CITATION_ROOT=<dir> bash scripts/...     # scan another tree (the negative test)
#
# Exit 0 = every citation resolves; 1 = a malformed or dangling citation;
# 2 = environment (no ledger at the root).
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || { echo "::error::cannot cd to repo root"; exit 2; }

ROOT="${CONTRACT_CITATION_ROOT:-$PWD}"
LEDGER="$ROOT/docs/contracts/contracts.toml"
[ -f "$LEDGER" ] || { echo "::error::$LEDGER not found"; exit 2; }

python3 - "$ROOT" "$LEDGER" <<'PY'
import os
import re
import sys

root, ledger = sys.argv[1], sys.argv[2]

# The ledger's own id rows are the ONLY source of truth for membership.
ledger_ids = set(re.findall(r'^id\s*=\s*"(C-\d+)"', open(ledger, encoding="utf-8").read(), re.M))
if not ledger_ids:
    print(f"::error::{ledger} carries no `id = \"C-NNN\"` row — the scanner is blind, refusing to pass")
    sys.exit(2)

# A citation is `C-` + digits at a token boundary on BOTH sides. The left
# guard rejects `RFC-7386` (the known false positive of a bare `C-` search),
# the right guard rejects an id with a letter glued on. Greedy digits: a
# four-digit spelling is one malformed token, never a three-digit id + a digit.
TOKEN = re.compile(r'(?<![A-Za-z0-9_-])C-(\d+)(?![A-Za-z0-9_])')
SHAPE = re.compile(r'^\d{3}$')

EXTS = {".md", ".toml", ".almd", ".rs", ".sh", ".yml", ".yaml", ".txt", ".py", ".json", ".lean"}
# Build output and vendored trees, by name anywhere; submodules and the agent
# worktrees, by root-relative path (a `grammar/` inside a crate is scanned).
PRUNE_NAMES = {"target", "node_modules", ".git"}
PRUNE_PATHS = {".claude", "grammar", "research/benchmark/lang-bench/upstream"}
# Files whose job is to assert that malformed ids are rejected (see header).
ALLOWLIST = {
    "tools/almide-gates/src/contract_audit.almd",
    "tools/almide-gates/src/ledger_schema.almd",
    # #2403: the als-pin gate refuses a four-or-more-digit id and its negative
    # controls forge such ids (and a three-digit one that names no contract) on purpose.
    "scripts/check-als-pin.sh",
    "scripts/check-als-pin-negative.sh",
}
# The fixture header line belongs to check-contracts.sh (one defect, one gate).
FIXTURE_DIR = "spec/wasm_cross/"
HEADER = re.compile(r'^\s*//\s*@contract:')

malformed = []   # (path, line, id)
dangling = []    # (path, line, id)
n_files = n_cites = 0
allowlisted = 0

for dirpath, dirnames, filenames in os.walk(root):
    reldir = os.path.relpath(dirpath, root)
    reldir = "" if reldir == "." else reldir
    dirnames[:] = sorted(
        d for d in dirnames
        if d not in PRUNE_NAMES and os.path.join(reldir, d) not in PRUNE_PATHS
    )
    for fn in sorted(filenames):
        if os.path.splitext(fn)[1] not in EXTS:
            continue
        full = os.path.join(dirpath, fn)
        rel = os.path.relpath(full, root)
        try:
            text = open(full, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        if "C-" not in text:
            continue
        n_files += 1
        in_fixture = rel.startswith(FIXTURE_DIR)
        for lineno, line in enumerate(text.split("\n"), 1):
            if in_fixture and HEADER.match(line):
                continue
            for m in TOKEN.finditer(line):
                cid = "C-" + m.group(1)
                if rel in ALLOWLIST:
                    allowlisted += 1
                    continue
                n_cites += 1
                if not SHAPE.match(m.group(1)):
                    malformed.append((rel, lineno, cid))
                elif cid not in ledger_ids:
                    dangling.append((rel, lineno, cid))

fail = False
for rel, lineno, cid in malformed:
    fail = True
    digits = len(cid) - 2
    hint = ("a four-or-more-digit id is a line number wearing a contract's clothes (#2403)"
            if digits > 3 else "a contract id is C- plus exactly three digits")
    print(f"::error::{rel}:{lineno}: `{cid}` is not a contract id — {hint}")
for rel, lineno, cid in dangling:
    fail = True
    print(f"::error::{rel}:{lineno}: `{cid}` names no contract — the ledger "
          f"docs/contracts/contracts.toml has no `id = \"{cid}\"` (a citation that resolves "
          f"nowhere: fix the id, or say what identifier space it belongs to without the C- prefix)")
if fail:
    print(f"::error::contract-citation gate FAILED — {len(malformed)} malformed, "
          f"{len(dangling)} dangling citation(s) (ledger has {len(ledger_ids)} ids)")
    sys.exit(1)
print(f"contract-citations: OK — {n_cites} citations in {n_files} files, every one a "
      f"three-digit id among the ledger's {len(ledger_ids)}; {allowlisted} self-test "
      f"vector(s) allowlisted in {len(ALLOWLIST)} file(s).")
PY
