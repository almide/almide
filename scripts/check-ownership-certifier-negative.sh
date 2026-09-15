#!/usr/bin/env bash
# Negative controls for the ownership-certifier corpus gate (#2231): prove
# scripts/check-ownership-certifier.sh FIRES on each way the ledger can go
# stale, with stand-in compilers whose stderr is known. The real compiler
# under the real gate is the positive control (the CI step before this one).
set -euo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

GATE="bash scripts/check-ownership-certifier.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

find spec/wasm_cross -name '*.almd' | sort > "$tmp/all"
head -3 "$tmp/all" > "$tmp/corpus"
# The gate keys every line by its corpus file, so the ledger holds one
# prefixed line per corpus file for the stand-in's single violation.
while read -r f; do printf '%s: [C3 clone-at-last-use] f: `x: String` is cloned at its last occurrence\n' "$f"; done < "$tmp/corpus" > "$tmp/ledger"

expect_pass() { ALMIDE_BIN="$1" CORPUS="$tmp/corpus" LEDGER="$tmp/ledger" $GATE >/dev/null 2>&1 || { echo "FAIL: $2" >&2; exit 1; }; }
expect_fail() { ALMIDE_BIN="$1" CORPUS="$tmp/corpus" LEDGER="$tmp/ledger" $GATE >/dev/null 2>&1 && { echo "FAIL: $2" >&2; exit 1; }; return 0; }

# A compiler raising exactly the ledger: green.
cat >"$tmp/steady" <<'SH'
#!/usr/bin/env bash
echo '[CERTIFY OWNERSHIP] [C3 clone-at-last-use] f: `x: String` is cloned at its last occurrence' >&2
SH
chmod +x "$tmp/steady"
expect_pass "$tmp/steady" "gate failed a compiler that raises exactly the ledger — harness broken, the negatives below would be meaningless"

# A NEW violation: red.
cat >"$tmp/regresses" <<'SH'
#!/usr/bin/env bash
echo '[CERTIFY OWNERSHIP] [C3 clone-at-last-use] f: `x: String` is cloned at its last occurrence' >&2
echo '[CERTIFY OWNERSHIP] [C4 owned-never-consumed] g: param `p: String` is rendered owned but never consumed' >&2
SH
chmod +x "$tmp/regresses"
expect_fail "$tmp/regresses" "gate passed a compiler raising a violation the ledger does not hold — a regression went unreported"

# A ledger line no longer raised (a silent fix, or a certifier that stopped looking): red until re-anchored.
expect_fail /bin/true "gate passed a compiler raising nothing while the ledger holds a line — a vanished violation went unreported"

# A panic under report mode: red.
cat >"$tmp/panics" <<'SH'
#!/usr/bin/env bash
echo "thread 'main' panicked at certify_ownership.rs: index out of bounds" >&2
SH
chmod +x "$tmp/panics"
expect_fail "$tmp/panics" "gate passed a compiler that panics under report mode"

# An empty corpus: red.
: > "$tmp/empty"
ALMIDE_BIN="$tmp/steady" CORPUS="$tmp/empty" LEDGER="$tmp/ledger" $GATE >/dev/null 2>&1 && { echo "FAIL: gate passed an empty corpus" >&2; exit 1; }

echo "ownership-certifier negative controls: 1 positive + 4 negatives all behaved"
