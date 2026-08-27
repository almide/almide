#!/usr/bin/env bash
# The qualification dossier (#571): the trust-layer receipts REFORMATTED
# into one versioned bundle per release — never an independent deliverable
# (an independently-authored dossier is certification theater; this one is
# assembled verbatim from the artifacts CI already derives and gates).
#
# Contents (each section names its generating instrument):
#   1. the trust receipt          — proofs/receipt.sh (fingerprint-honest)
#   2. the release seal           — proofs/releases/<tag>.toml (audit freeze)
#   3. the formal-credit table    — proofs/FORMAL-CREDIT.md (#575)
#   4. the derived stage block    — proofs/STAGE-STATUS.md (gen-claims)
#   5. gate-verification summary  — proofs/gate-verification.toml counts
#   6. contract-ledger summary    — docs/contracts/contracts.toml counts +
#                                   the flagged-for-revision list (the
#                                   KNOWN PROBLEMS section, never hidden)
#   7. the MC/DC ledger state     — proofs/mcdc-ledger.toml (#566)
#   8. an input manifest          — sha256 of every source artifact, so the
#                                   bundle is REPRODUCIBLE and externally
#                                   checkable (regenerate, byte-compare)
#
# SIGNING: the release workflow attests the dossier with GitHub artifact
# attestation (Sigstore keyless) — verify externally with
#   gh attestation verify almide-dossier-<tag>.md -R almide/almide
# Key-based signing beyond that is #1534's scope.
#
# Usage: bash scripts/gen-dossier.sh vX.Y.Z [--no-receipt] > dossier.md
#   --no-receipt: skip the receipt run (local smoke without Rocq); the
#   section then states MISSING loudly rather than silently.

set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

TAG="${1:?usage: gen-dossier.sh vX.Y.Z [--no-receipt]}"
NO_RECEIPT="${2:-}"

section() { printf '\n---\n\n## %s\n\n' "$1"; }

echo "# Almide qualification dossier — ${TAG}"
echo
echo "Generated $(date -u +%Y-%m-%dT%H:%M:%SZ) by scripts/gen-dossier.sh."
echo "Every section is the verbatim output of a standing, gated instrument;"
echo "the dossier asserts nothing those instruments do not. Reproduce:"
echo '`git checkout '"$TAG"' && bash scripts/gen-dossier.sh '"$TAG"'`.'

section "1. Trust receipt (proofs/receipt.sh)"
if [ "$NO_RECEIPT" = "--no-receipt" ]; then
  echo '**MISSING — generated with --no-receipt (local smoke).** A release'
  echo 'dossier without this section is not a release dossier.'
else
  if ! bash proofs/receipt.sh 2>&1; then
    echo
    echo '**RECEIPT FAILED — see above. This dossier documents a tree that'
    echo 'did NOT verify.**'
    exit 1
  fi
fi

section "2. Release seal (proofs/releases/${TAG}.toml)"
if [ -f "proofs/releases/${TAG}.toml" ]; then
  echo '```toml'
  cat "proofs/releases/${TAG}.toml"
  echo '```'
else
  echo "**MISSING — no seal for ${TAG} yet.** The seal is written on develop"
  echo "after the tag (release procedure step 7); regenerate the dossier then."
fi

section "3. Formal-credit claims (proofs/FORMAL-CREDIT.md, #575)"
cat proofs/FORMAL-CREDIT.md

section "4. Derived stage block (proofs/STAGE-STATUS.md)"
cat proofs/STAGE-STATUS.md

section "5. Gate-verification summary (proofs/gate-verification.toml)"
python3 - <<'EOF'
import re
s = open('proofs/gate-verification.toml').read()
classes = re.findall(r'class = "(\w+)"', s)
from collections import Counter
c = Counter(classes)
total = sum(c.values())
print(f"{total} verdict-bearing gates, every one classified by how it can fail:")
print()
for k in ["KERNEL_PROVEN", "MUTATION_TESTED", "NEGATIVE_TESTED", "EXERCISED", "UNVERIFIED"]:
    if c.get(k):
        print(f"- {k}: {c[k]}")
print()
print("UNVERIFIED ceiling: 0 (shrink-only; a gate without failure evidence")
print("cannot ship).")
EOF

section "6. Contract ledger summary + KNOWN PROBLEMS"
python3 - <<'EOF'
import re
s = open('docs/contracts/contracts.toml').read()
ids = re.findall(r'^id\s+= "(C-\d+)"', s, re.M)
flagged = re.findall(r'^id\s+= "(C-\d+)"\n(?:[^\[]*?)^status\s+= "flagged-for-revision"', s, re.M)
print(f"{len(ids)} cross-target contracts; every one carries evidence of")
print("class >= fixture and a bidirectional fixture link")
print("(scripts/check-contracts.sh).")
print()
if flagged:
    print(f"KNOWN PROBLEMS — {len(flagged)} contract(s) flagged for revision")
    print("(the count may only shrink):")
    for c in flagged:
        print(f"- {c}")
else:
    print("KNOWN PROBLEMS: no contracts flagged for revision.")
EOF

section "7. MC/DC ledger state (proofs/mcdc-ledger.toml, #566)"
bash proofs/mcdc-ledger.sh 2>&1 | tail -1

section "8. Input manifest (sha256 — reproducibility rail)"
echo '```'
sha256sum \
  proofs/FORMAL-CREDIT.md \
  proofs/STAGE-STATUS.md \
  proofs/gate-verification.toml \
  proofs/mcdc-ledger.toml \
  docs/contracts/contracts.toml \
  scripts/gen-dossier.sh \
  2>/dev/null || shasum -a 256 \
  proofs/FORMAL-CREDIT.md \
  proofs/STAGE-STATUS.md \
  proofs/gate-verification.toml \
  proofs/mcdc-ledger.toml \
  docs/contracts/contracts.toml \
  scripts/gen-dossier.sh
[ -f "proofs/releases/${TAG}.toml" ] && { sha256sum "proofs/releases/${TAG}.toml" 2>/dev/null || shasum -a 256 "proofs/releases/${TAG}.toml"; }
echo '```'
