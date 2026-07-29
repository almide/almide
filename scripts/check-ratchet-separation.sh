#!/usr/bin/env bash
# RATCHET-COMMIT SEPARATION (flight-evidence-gaps F5): a commit that changes a
# VERIFICATION ARTIFACT (a parity baseline, a test expectation, a KnownBroken
# flag) must not ALSO change implementation — otherwise the person making the
# change is simultaneously moving the bar that judges it (verification
# independence, DO-178C 6.2). Baseline/expectation moves go in their OWN commit
# whose message states the evidence (the solo-run record) for why the move is
# not a regression.
#
# RATCHET set: the files that define "what counts as passing".
# IMPLEMENTATION set: compiler/runtime/stdlib source.
# A commit staging BOTH is rejected.
set -u
export LC_ALL=C
staged="$(git diff --cached --name-only)"
[ -n "$staged" ] || exit 0

ratchet=""
impl=""
while IFS= read -r f; do
    case "$f" in
        # EVERY ratchet artifact, not just the parity baseline (#988: the list
        # named exactly one file, so walled-real/coverage/embedded-size
        # baselines and the abstain ledger were freely co-committable with
        # implementation). In-source ratchet constants (BASELINE/GENUINE_SKIPS)
        # ride the test-file arm below via their expectation-flip check.
        proofs/*-baseline.txt|scripts/*-baseline.txt|crates/almide-interp/interp-abstain-ledger.txt)
            ratchet="$ratchet $f" ;;
        crates/almide-mir/tests/*.rs|crates/almide-mir/src/lower/tests*.rs|crates/almide-mir/src/render_wasm/tests*.rs|tests/*.rs)
            impl_test="$f"
            # a test-file change is a ratchet move only when it flips an
            # expectation (expect_err/KnownBroken); adding a new test is fine.
            # an expectation flip OR a ratchet-constant move counts (#988):
            # BASELINE / GENUINE_SKIPS / MAX_* are "what counts as passing".
            if git diff --cached -U0 -- "$f" | grep -qE '^\+.*((expect_err)|(KnownBroken)|(BASELINE[: ])|(GENUINE_SKIPS)|(MAX_[A-Z_]+ *=))'; then
                ratchet="$ratchet $f"
            fi
            ;;
        crates/*/src/*.rs|src/*.rs|src/cli/*.rs|stdlib/*.almd|runtime/*) impl="$impl $f" ;;
    esac
done <<< "$staged"

if [ -n "$ratchet" ] && [ -n "$impl" ]; then
    echo "::error::ratchet-separation: this commit mixes IMPLEMENTATION changes with"
    echo "  VERIFICATION-ARTIFACT changes. Split it: land the implementation first,"
    echo "  then move the baseline/expectation in its own commit whose message cites"
    echo "  the evidence (solo-run record) that the move is not a regression."
    echo "  ratchet:$ratchet"
    echo "  impl:$(echo $impl | tr ' ' '\n' | head -5 | tr '\n' ' ')..."
    exit 1
fi
exit 0
