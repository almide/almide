#!/usr/bin/env bash
# RATCHET-COMMIT SEPARATION (flight-evidence-gaps F5): a commit that changes a
# VERIFICATION ARTIFACT (a parity baseline, a golden manifest, a test
# expectation, a KnownBroken flag) must not ALSO change implementation —
# otherwise the person making the change is simultaneously moving the bar that
# judges it (verification independence, DO-178C 6.2). Baseline/expectation
# moves go in their OWN commit whose message states the evidence (the solo-run
# record) for why the move is not a regression.
#
# RATCHET set: the files that define "what counts as passing".
# IMPLEMENTATION set: compiler/runtime/stdlib source.
# A commit staging BOTH is rejected.
#
#   check-ratchet-separation.sh               # the staged change (lefthook pre-commit)
#   check-ratchet-separation.sh <base> <head> # every commit in base..head (CI, over the PR's range)
#
# The range form is what makes the rule hold (#2183): as a pre-commit hook
# alone it could not see the commits that re-recorded the parity goldens
# alongside the change they judged — a hook is skipped with --no-verify, and
# a golden regenerated in the implementation's commit is exactly the class it
# exists to split. CI checks each commit of the PR on its own.
set -u
export LC_ALL=C

# classify <diff-args…>: the file list is `git diff --name-only <args>`, the
# per-file hunks `git diff -U0 <args> -- <file>`. Prints the offence and
# returns 1 when the change set mixes the two sets.
classify() {
    local files ratchet="" impl="" f
    files="$(git diff --name-only "$@")"
    [ -n "$files" ] || return 0
    while IFS= read -r f; do
        case "$f" in
            # EVERY ratchet artifact, not just the parity baseline (#988: the list
            # named exactly one file, so walled-real/coverage/embedded-size
            # baselines and the abstain ledger were freely co-committable with
            # implementation). In-source ratchet constants (BASELINE/GENUINE_SKIPS)
            # ride the test-file arm below via their expectation-flip check.
            #
            # #2183 adds the parity goldens the greenfield legs are judged by
            # (the AST / check / run manifests and their exclusions), the
            # gate-verification ledger, and scripts/lib/ — the registers that
            # decide which rows a comparison skips live there.
            proofs/*-baseline.txt|scripts/*-baseline.txt|crates/almide-interp/interp-abstain-ledger.txt)
                ratchet="$ratchet $f" ;;
            crates/almide-spine/tests/golden/*|crates/almide-syntax/tests/golden/*|crates/almide-wasm/tests/golden/*)
                ratchet="$ratchet $f" ;;
            proofs/gate-verification.toml|scripts/lib/*.txt)
                ratchet="$ratchet $f" ;;
            crates/almide-mir/tests/*.rs|crates/almide-mir/src/lower/tests*.rs|crates/almide-mir/src/render_wasm/tests*.rs|tests/*.rs|crates/almide-spine/tests/*.rs|crates/almide-syntax/tests/*.rs|crates/almide-wasm/tests/*.rs)
                # a test-file change is a ratchet move only when it flips an
                # expectation (expect_err/KnownBroken); adding a new test is fine.
                # an expectation flip OR a ratchet-constant move counts (#988):
                # BASELINE / GENUINE_SKIPS / MAX_* / *_FLOOR are "what counts as passing".
                if git diff -U0 "$@" -- "$f" | grep -qE '^\+.*((expect_err)|(KnownBroken)|(BASELINE[: ])|(GENUINE_SKIPS)|(MAX_[A-Z_]+ *=)|([A-Z_]+_FLOOR *=))'; then
                    ratchet="$ratchet $f"
                fi
                ;;
            crates/*/src/*.rs|src/*.rs|src/cli/*.rs|stdlib/*.almd|runtime/*) impl="$impl $f" ;;
        esac
    done <<< "$files"

    if [ -n "$ratchet" ] && [ -n "$impl" ]; then
        echo "::error::ratchet-separation: this commit mixes IMPLEMENTATION changes with"
        echo "  VERIFICATION-ARTIFACT changes. Split it: land the implementation first,"
        echo "  then move the baseline/expectation in its own commit whose message cites"
        echo "  the evidence (solo-run record) that the move is not a regression."
        echo "  ratchet:$ratchet"
        echo "  impl:$(echo $impl | tr ' ' '\n' | head -5 | tr '\n' ' ')..."
        return 1
    fi
    return 0
}

if [ $# -eq 0 ]; then
    classify --cached
    exit $?
fi
[ $# -eq 2 ] || { echo "usage: $0 [<base> <head>]" >&2; exit 2; }
base="$1"; head="$2"
git cat-file -e "$base^{commit}" 2>/dev/null || { echo "::error::ratchet-separation: base $base is not a commit here (fetch-depth?)"; exit 2; }
rc=0
n=0
# Merge commits carry no change of their own (their parents were each
# checked); first-parent-only would hide a squashed side branch, so every
# non-merge commit in the range is judged against its first parent.
for c in $(git rev-list --no-merges --reverse "$base..$head"); do
    n=$((n + 1))
    if ! classify "$c^" "$c"; then
        echo "  in commit $(git log -1 --format='%h %s' "$c")"
        rc=1
    fi
done
[ "$rc" -eq 0 ] && echo "ratchet-separation OK: $n commit(s) in $base..$head each keep implementation and verification artifacts apart"
exit "$rc"
