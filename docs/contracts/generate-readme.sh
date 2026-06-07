#!/usr/bin/env bash
# Auto-generate docs/contracts/README.md from docs/contracts/contracts.toml.
# Clone of docs/roadmap/generate-readme.sh in spirit: pure shell/awk, no deps.
#
#   bash docs/contracts/generate-readme.sh > docs/contracts/README.md
#
# Output: the change-discipline header + one sorted table:
#   | ID | Contract | Since | Status | Strongest Evidence | # Fixtures |
# "Strongest Evidence" = the highest-ranked evidence class present (rank from
# scripts/lib/contract-classes.txt, line order = rank); fuzz shows its N. A `doc=`
# renders the title as a link.
set -euo pipefail
cd "$(dirname "$0")/../.." || exit 2

LEDGER="docs/contracts/contracts.toml"
CLASS_FILE="scripts/lib/contract-classes.txt"
# Comma-joined so it survives an awk -v assignment (newlines are illegal there).
CLASSES_CSV="$(grep -vE '^[[:space:]]*(#|$)' "$CLASS_FILE" | paste -sd, -)"

cat << 'HEADER'
# Almide Behavior Contracts

> Auto-generated from [contracts.toml](contracts.toml).
> Run `bash docs/contracts/generate-readme.sh > docs/contracts/README.md` to update.
>
> Each contract is a NORMATIVE, observable promise the compiler keeps on BOTH
> targets (native Rust + wasm32: stdout, stderr, exit code). Native is the oracle;
> native == wasm is a hard invariant. Every contract is traceable to executable
> evidence (a `spec/wasm_cross/*.almd` fixture, a differential fuzz, an emit-time
> Σ-probe, or a Lean theorem) — no claimed behaviour rests on prose alone.

## Change discipline

- **Changing any observable behaviour REQUIRES updating the contract statement
  AND its evidence in the SAME PR.**
- A **new** behaviour = a new `C-NNN` + ≥1 fixture.
- **Removing a divergence** = flip `status` to `active` and drop the flag in the
  same PR. The `flagged-for-revision` count is a ratchet — it may only go **down**.
- The gate (`scripts/check-contracts.sh`, CI + lefthook) enforces that every
  contract has real evidence, every fixture names its contract(s), and the link is
  bidirectional.

Evidence classes (weakest → strongest): `doc-only` < `by-construction` <
`fixture` < `fuzz` < `exhaustive` < `lean`. An **active** contract must carry
≥1 evidence of class ≥ `fixture`.

HEADER

# Emit the table by walking the ledger in awk and printing one row per contract.
awk -v classes="$CLASSES_CSV" '
  BEGIN {
    nclass = split(classes, carr, ",")     # carr[1]=rank0 ... carr[nclass]=rank(nclass-1)
    for (k = 1; k <= nclass; k++) rank[carr[k]] = k - 1
    in_stmt = 0
  }
  function flush() {
    if (id == "") return
    # strongest class label (with N for fuzz)
    best = "-"; bestrank = -1; bestn = ""
    for (k = 1; k <= nev; k++) {
      c = evclass[k]
      if (c in rank && rank[c] > bestrank) { bestrank = rank[c]; best = c; bestn = evn[k] }
    }
    if (best == "fuzz" && bestn != "" && bestn != "-") best = "fuzz(" bestn ")"
    name = title
    if (doc != "") name = "[" title "](" doc ")"
    printf "| %s | %s | %s | %s | %s | %d |\n", id, name, since, status, best, nfix
    id=""; title=""; since=""; status=""; doc=""; nev=0; nfix=0
  }
  /'"'"''"'"''"'"'/ { in_stmt = !in_stmt; next }
  in_stmt { next }
  /^\[\[contract\]\]/ { flush(); next }
  /^id[ \t]*=/      { v=$0; sub(/^id[ \t]*=[ \t]*"/,"",v); sub(/".*$/,"",v); id=v; next }
  /^title[ \t]*=/   { v=$0; sub(/^title[ \t]*=[ \t]*"/,"",v); sub(/".*$/,"",v); title=v; next }
  /^since[ \t]*=/   { v=$0; sub(/^since[ \t]*=[ \t]*"/,"",v); sub(/".*$/,"",v); since=v; next }
  /^status[ \t]*=/  { v=$0; sub(/^status[ \t]*=[ \t]*"/,"",v); sub(/".*$/,"",v); status=v; next }
  /^doc[ \t]*=/     { v=$0; sub(/^doc[ \t]*=[ \t]*"/,"",v); sub(/".*$/,"",v); doc=v; next }
  /path[ \t]*=[ \t]*"/ {
    line=$0
    p=line; sub(/^.*path[ \t]*=[ \t]*"/,"",p); sub(/".*$/,"",p)
    c="-"; if (line ~ /class[ \t]*=[ \t]*"/) { c=line; sub(/^.*class[ \t]*=[ \t]*"/,"",c); sub(/".*$/,"",c) }
    n="-"; if (line ~ /[, {][ \t]*n[ \t]*=[ \t]*[0-9]/) { n=line; sub(/^.*[, {][ \t]*n[ \t]*=[ \t]*/,"",n); sub(/[^0-9].*$/,"",n) }
    nev++; evclass[nev]=c; evn[nev]=n
    if (p ~ /^spec\/wasm_cross\/.*\.almd$/) nfix++
    next
  }
  END { flush() }
' "$LEDGER" | sort > /tmp/contracts_rows.$$

echo "$(grep -c . /tmp/contracts_rows.$$) contracts"
echo ""
echo "| ID | Contract | Since | Status | Strongest Evidence | # Fixtures |"
echo "|----|----------|-------|--------|--------------------|-----------:|"
cat /tmp/contracts_rows.$$
rm -f /tmp/contracts_rows.$$
echo ""
