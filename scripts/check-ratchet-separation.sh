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
#   check-ratchet-separation.sh <base> <head> # every commit in base..head, each judged by the gate at its parent (CI)
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
            # this gate and its disposition ledger are verification artifacts
            # themselves: they define which moves count as moving the bar, so
            # editing either is one of them and cannot ride inside an
            # implementation commit
            scripts/check-ratchet-separation.sh|proofs/ratchet-dispositions.txt)
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

# EACH COMMIT IS JUDGED BY THE LAW IN FORCE AT ITS PARENT. The range form used
# to judge every commit by THIS file, so a rule added later was applied to
# commits made before it existed: the v0.63.0-rc1 release range (#2299) failed
# on a #2191 commit that re-recorded an AST golden four hours before #2199 made
# the goldens verification artifacts. A rule cannot bind a change that
# predates it, and a commit that edits this gate must not be judged by its own
# edit — so each commit is judged by the gate as it stood at the commit's
# parent: that version's pre-commit mode runs with every `git diff --cached` it
# issues pinned to the commit's own change (`<commit>^ <commit>`). Every
# version of this file has served the lefthook pre-commit interface, which is
# what makes an old version replayable without a checkout. There is no floor
# commit and no exemption list.
#
# A commit that broke the law in force and is ALREADY on the protected line
# cannot be split any more; it is closed the way a problem report is, by a
# DISPOSITION in proofs/ratchet-dispositions.txt. A record excuses exactly one
# commit, only when it names exactly the files the law flagged, only when it
# names an author and a different reviewer and states its evidence, and only
# for a commit reachable from $RATCHET_PROTECTED (CI passes origin/develop):
# a change set can never excuse its own commits, and without the variable no
# record is honoured at all. The ledger is itself a verification artifact
# above, so a record cannot ride inside an implementation commit either.
GATE="scripts/check-ratchet-separation.sh"
DISPOSITIONS="proofs/ratchet-dispositions.txt"
PROTECTED="${RATCHET_PROTECTED:-}"
# `git diff … --cached …` → `git diff … <c>^ <c> …`; every other git call as is.
REPLAY_SHIM='git() {
    if [ "${1:-}" = diff ]; then
        local a argv=()
        for a in "$@"; do
            if [ "$a" = --cached ]; then argv+=("$RATCHET_REPLAY^" "$RATCHET_REPLAY"); else argv+=("$a"); fi
        done
        command git "${argv[@]}"
    else
        command git "$@"
    fi
}
'

# judge <commit>: 0 kept apart, 1 mixed (the law's own message printed),
# 3 no gate existed at the parent (no law was in force).
judge() {
    local law
    law="$(git show "$1^:$GATE" 2>/dev/null)" || return 3
    RATCHET_REPLAY="$1" bash -c "$REPLAY_SHIM$law"
}

# dispositioned <commit> <law output>: 0 when the ledger closes this commit's
# offence (the rules above), printing the record's evidence.
dispositioned() {
    local c="$1" out="$2" full flagged sha files author reviewer evidence
    [ -n "$PROTECTED" ] && [ -f "$DISPOSITIONS" ] || return 1
    git merge-base --is-ancestor "$c" "$PROTECTED" 2>/dev/null || return 1
    full="$(git rev-parse "$c")"
    # every version of the law prints the offending artifacts on one
    # `ratchet:` line
    flagged="$(printf '%s\n' "$out" | sed -n 's/^ *ratchet://p' | tr ' ' '\n' | grep . | sort -u | tr '\n' ' ')"
    [ -n "$flagged" ] || return 1
    while read -r sha files author reviewer evidence; do
        case "$sha" in ''|'#'*) continue ;; esac
        [ "$sha" = "$full" ] || continue
        [ -n "$author" ] && [ -n "$reviewer" ] && [ "$author" != "$reviewer" ] && [ -n "$evidence" ] || return 1
        [ "$(printf '%s\n' "$files" | tr ',' '\n' | grep . | sort -u | tr '\n' ' ')" = "$flagged" ] || return 1
        echo "  dispositioned: $(git log -1 --format='%h %s' "$c")"
        echo "    ratchet: $flagged(author $author, reviewer $reviewer) — $evidence"
        return 0
    done < "$DISPOSITIONS"
    return 1
}

rc=0
n=0
unbound=0
closed=0
laws=""
# Merge commits carry no change of their own (their parents were each
# checked); first-parent-only would hide a squashed side branch, so every
# non-merge commit in the range is judged against its first parent.
for c in $(git rev-list --no-merges --reverse "$base..$head"); do
    n=$((n + 1))
    out="$(judge "$c")"
    case $? in
        0) laws="$laws $(git rev-parse --short "$c^:$GATE")" ;;
        3) unbound=$((unbound + 1)) ;;
        *)
            if dispositioned "$c" "$out"; then
                closed=$((closed + 1))
                laws="$laws $(git rev-parse --short "$c^:$GATE")"
            else
                echo "$out"
                echo "  in commit $(git log -1 --format='%h %s' "$c") (judged by the gate at its parent, $(git rev-parse --short "$c^:$GATE"))"
                rc=1
            fi
            ;;
    esac
done
versions="$(echo $laws | tr ' ' '\n' | sort -u | grep -c . || true)"
[ "$rc" -eq 0 ] && echo "ratchet-separation OK: $n commit(s) in $base..$head each keep implementation and verification artifacts apart, each judged by the gate at its parent ($versions gate version(s); $closed closed by a disposition; $unbound predate the gate)"
exit "$rc"
