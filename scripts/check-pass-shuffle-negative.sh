#!/usr/bin/env bash
# Negative controls for the pass-shuffle gate (#2186): prove
# scripts/check-pass-shuffle.sh FIRES on each way a shuffle can go wrong,
# using stand-in compilers whose behaviour is known. The real compiler under
# the real gate is the positive control (the CI step before this one).
set -euo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

GATE="bash scripts/check-pass-shuffle.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Five real fixtures as the corpus: the stand-ins never compile them, they
# only spell their name, so the list just has to exist.
find spec/wasm_cross -name '*.almd' | sort > "$tmp/all"
head -5 "$tmp/all" > "$tmp/corpus"

expect_pass() { ALMIDE_BIN="$1" SEEDS=1 CORPUS="$tmp/corpus" $GATE >/dev/null 2>&1 || { echo "FAIL: $2" >&2; exit 1; }; }
expect_fail() { ALMIDE_BIN="$1" SEEDS=1 CORPUS="$tmp/corpus" $GATE >/dev/null 2>&1 && { echo "FAIL: $2" >&2; exit 1; }; return 0; }

# An empty corpus is a hard FAIL, not a green.
: > "$tmp/empty"
ALMIDE_BIN=/bin/true SEEDS=1 CORPUS="$tmp/empty" $GATE >/dev/null 2>&1 && { echo "FAIL: gate passed an empty corpus" >&2; exit 1; }

# A compiler whose output does not depend on the order: green.
cat >"$tmp/steady" <<'SH'
#!/usr/bin/env bash
echo "[almide] ALMIDE_SHUFFLE_PASSES=${ALMIDE_SHUFFLE_PASSES:-} order: A B C" >&2
echo "fn main() {} // $1"
SH
chmod +x "$tmp/steady"
expect_pass "$tmp/steady" \
  "gate failed a compiler whose emit is order-independent — harness broken, the negatives below would be meaningless"

# A compiler whose output changes under a shuffled order: red.
cat >"$tmp/drifts" <<'SH'
#!/usr/bin/env bash
echo "[almide] ALMIDE_SHUFFLE_PASSES=${ALMIDE_SHUFFLE_PASSES:-} order: B A C" >&2
echo "fn main() {} // $1 ${ALMIDE_SHUFFLE_PASSES:-}"
SH
chmod +x "$tmp/drifts"
expect_fail "$tmp/drifts" \
  "gate passed a compiler that emits different Rust under a shuffled order — the byte-diff is not being judged"

# A compiler that panics under a shuffled order (an undeclared edge the
# runtime validator catches): red, even though it emits nothing different.
cat >"$tmp/panics" <<'SH'
#!/usr/bin/env bash
if [ -n "${ALMIDE_SHUFFLE_PASSES:-}" ]; then
  echo "thread 'main' panicked at pass.rs: Pass 'X' depends on 'Y', but 'Y' has not been executed" >&2
fi
echo "fn main() {} // $1"
SH
chmod +x "$tmp/panics"
expect_fail "$tmp/panics" \
  "gate passed a compiler that panics under the shuffle — a dependency violation went unreported"

echo "pass-shuffle negative controls: 1 positive + 3 negatives all behaved"
