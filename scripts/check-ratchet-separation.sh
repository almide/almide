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
#   check-ratchet-separation.sh <base> <head> # every commit in base..head, merges included, each judged by the law that binds it (CI; see below)
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
git cat-file -e "$head^{commit}" 2>/dev/null || { echo "::error::ratchet-separation: head $head is not a commit here"; exit 2; }

# WHICH LAW JUDGES A COMMIT. The range form used to judge every commit by THIS
# file, so a rule added later bound commits made before it existed: the
# v0.63.0-rc1 release range (#2299) failed on a #2191 commit that re-recorded
# an AST golden four hours before #2199 made the goldens verification
# artifacts. Two kinds of commit, two answers:
#
#   * a commit ALREADY ON THE PROTECTED LINE ($RATCHET_PROTECTED; CI passes
#     origin/develop, or the develop tip before a push) was accepted under the
#     law at its parent (at each parent, for a merge), and is judged by that
#     law alone: a rule never binds a change made before it existed.
#   * a commit of the CHANGE SET (not yet on the protected line) must satisfy
#     the law at the protected tip AND the law at its own parent. The tip's law
#     is the accepted one — a change set that edits the gate cannot loosen it
#     for its own commits (gut the gate, commit the mix, restore the gate), and
#     a branch forked before a rule landed is judged by that rule; the parent's
#     law makes a tightening bind the rest of its own change set at once.
#     Without $RATCHET_PROTECTED the range base stands in for the tip (the
#     head, when the base predates the gate) and every commit counts as the
#     change set's.
#
# A law is replayed, not reimplemented: that version of this file runs in its
# pre-commit mode with every `git diff --cached` it issues pinned to the commit
# (`<c>^ <c>`; for a merge, `git show --remerge-diff` — the edits the merge
# made itself, beyond resolving its parents; an octopus merge has none and is
# refused). Every version of this file has served the lefthook pre-commit
# interface, which is what makes an old version replayable without a checkout.
# No floor commit, no exemption list.
#
# A commit on the protected line that broke the law it was judged by can no
# longer be split; it is closed the way a problem report is, by a DISPOSITION
# in proofs/ratchet-dispositions.txt: one record per commit, naming exactly the
# files the law flagged, an author and a different reviewer (each one token,
# no placeholder or template word), and the evidence. Only commits on the
# protected line can be closed, so a change set can never close its own; the
# ledger is a verification artifact above, so a record cannot ride inside an
# implementation commit either.
GATE="scripts/check-ratchet-separation.sh"
DISPOSITIONS="$(git rev-parse --show-toplevel)/proofs/ratchet-dispositions.txt"
PROTECTED="${RATCHET_PROTECTED:-}"
if [ -n "$PROTECTED" ] && ! git cat-file -e "$PROTECTED^{commit}" 2>/dev/null; then
    echo "::error::ratchet-separation: RATCHET_PROTECTED=$PROTECTED is not a commit here"
    exit 2
fi
TRUSTED="${PROTECTED:-$base}"
# A local run with no protected line and a base older than the gate would
# leave the change set with no trusted law at all; the head's law stands in.
# (CI always passes the protected line.)
if [ -z "$PROTECTED" ] && [ -z "$(git rev-parse -q --verify "$base:$GATE")" ]; then
    TRUSTED="$head"
fi

# `git diff --cached <opts> [-- <paths>]` → the commit's own change; every other
# git call as is. Empty arrays are expanded with the ${a[@]+…} form so the
# shim holds under `set -u` on bash 3.2 as well as 5.
REPLAY_SHIM='git() {
    local a cached=0 seen=0 opts=() paths=()
    if [ "${1:-}" != diff ]; then command git "$@"; return; fi
    shift
    for a in "$@"; do
        if [ "$seen" = 1 ]; then paths+=("$a")
        elif [ "$a" = -- ]; then seen=1
        elif [ "$a" = --cached ]; then cached=1
        else opts+=("$a"); fi
    done
    if [ "$cached" = 0 ]; then command git diff "$@"; return; fi
    if [ -n "$RATCHET_MERGE" ]; then
        command git show --remerge-diff --format= ${opts[@]+"${opts[@]}"} "$RATCHET_REPLAY" -- ${paths[@]+"${paths[@]}"}
    else
        command git diff ${opts[@]+"${opts[@]}"} "$RATCHET_REPLAY^" "$RATCHET_REPLAY" -- ${paths[@]+"${paths[@]}"}
    fi
}
'

# judge_under <law rev> <commit>: 0 kept apart, 1 mixed (the law's message
# printed), 3 no gate existed at <law rev>. An octopus merge has no
# remerge-diff (git skips it with a warning, which would read as an empty
# change), so it cannot be judged by its own edits and is refused outright.
judge_under() {
    local law merge="" parents
    law="$(git show "$1:$GATE" 2>/dev/null)" || return 3
    parents=$(( $(git rev-list --parents -n1 "$2" | wc -w) - 1 ))
    if [ "$parents" -gt 2 ]; then
        echo "::error::ratchet-separation: an octopus merge ($parents parents) cannot be judged by its own edits — merge one branch at a time"
        return 1
    fi
    [ "$parents" -eq 2 ] && merge=1
    RATCHET_REPLAY="$2" RATCHET_MERGE="$merge" bash -c "$REPLAY_SHIM$law"
}

# identity <name>: a disposition's author and reviewer are real identities —
# one token of [A-Za-z0-9._-], not a placeholder word, and not a template
# token that merely contains one (`@REVIEWER@` once passed as a reviewer).
identity() {
    local lower
    lower="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"
    printf '%s' "$1" | grep -qE '^[A-Za-z0-9._-]+$' || return 1
    case "$lower" in *author*|*reviewer*|tbd|todo|pending|none|unknown|nobody|-|.|_) return 1 ;; esac
    return 0
}

# dispositioned <commit> <law output>: 0 when the ledger closes this commit's
# offence (the rules above), printing the record's evidence.
dispositioned() {
    local c="$1" out="$2" full flagged sha files author reviewer evidence hits=0 found=""
    [ -n "$PROTECTED" ] && [ -f "$DISPOSITIONS" ] || return 1
    git merge-base --is-ancestor "$c" "$PROTECTED" 2>/dev/null || return 1
    full="$(git rev-parse "$c")"
    flagged="$(printf '%s\n' "$out" | sed -n 's/^ *ratchet://p' | tr ' ' '\n' | grep . | sort -u | tr '\n' ' ')"
    [ -n "$flagged" ] || return 1
    while read -r sha files author reviewer evidence || [ -n "$sha" ]; do
        case "$sha" in ''|'#'*) continue ;; esac
        [ "$sha" = "$full" ] || continue
        hits=$((hits + 1))
        found="$files|$author|$reviewer|$evidence"
    done < <(tr -d '\r' < "$DISPOSITIONS")
    if [ "$hits" -gt 1 ]; then echo "::error::ratchet-separation: $hits disposition records name $full — one record per commit"; return 1; fi
    [ "$hits" -eq 1 ] || return 1
    IFS='|' read -r files author reviewer evidence <<< "$found"
    if ! identity "$author" || ! identity "$reviewer" || [ -z "$evidence" ] \
        || [ "$(printf '%s' "$author" | tr '[:upper:]' '[:lower:]')" = "$(printf '%s' "$reviewer" | tr '[:upper:]' '[:lower:]')" ]; then
        echo "::error::ratchet-separation: the disposition for $full needs a real author, a different real reviewer and evidence"
        return 1
    fi
    [ "$(printf '%s\n' "$files" | tr ',' '\n' | grep . | sort -u | tr '\n' ' ')" = "$flagged" ] || return 1
    echo "  dispositioned: $(git log -1 --format='%h %s' "$c")"
    echo "    ratchet: $flagged(author $author, reviewer $reviewer) — $evidence"
    return 0
}

rc=0
n=0
unbound=0
closed=0
for c in $(git rev-list --reverse "$base..$head"); do
    n=$((n + 1))
    on_line=""
    [ -n "$PROTECTED" ] && git merge-base --is-ancestor "$c" "$PROTECTED" 2>/dev/null && on_line=1
    # the law at EVERY parent (a merge whose first parent predates a rule is
    # still bound by the rule its other parent carries), plus the protected
    # tip's for a change-set commit; one run per distinct law
    laws=""
    blobs=" "
    candidates="$(git rev-list --parents -n1 "$c" | cut -d' ' -f2-)"
    [ -z "$on_line" ] && candidates="$candidates $TRUSTED"
    for law in $candidates; do
        blob="$(git rev-parse -q --verify "$law:$GATE" || echo "none-$law")"
        case "$blobs" in *" $blob "*) continue ;; esac
        blobs="$blobs$blob "
        laws="$laws $law"
    done
    bound="" offence="" by=""
    for law in $laws; do
        out="$(judge_under "$law" "$c")"
        case $? in
            0) bound=1 ;;
            3) ;;
            *) bound=1; offence="$offence$out"$'\n'; by="$by $(git rev-parse --short "$law:$GATE")" ;;
        esac
    done
    [ -n "$bound" ] || { unbound=$((unbound + 1)); continue; }
    [ -n "$offence" ] || continue
    if [ -n "$on_line" ] && dispositioned "$c" "$offence"; then
        closed=$((closed + 1))
        continue
    fi
    printf '%s' "$offence"
    echo "  in commit $(git log -1 --format='%h %s' "$c") ($([ -n "$on_line" ] && echo "on the protected line" || echo "in the change set"); judged by gate$by)"
    rc=1
done
if [ "$n" -eq 0 ]; then
    echo "::error::ratchet-separation: $base..$head holds no commit — nothing was judged, which is not a pass"
    exit 2
fi
[ "$rc" -eq 0 ] && echo "ratchet-separation OK: $n commit(s) in $base..$head each keep implementation and verification artifacts apart ($closed closed by a disposition; $unbound predate the gate)"
exit "$rc"
