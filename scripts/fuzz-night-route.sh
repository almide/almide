#!/usr/bin/env bash
# NIGHTLY FUZZ FINDINGS ROUTE (#3207)
# ===================================
#
# Files (or comments on) the tracking issue for one class of the night's
# findings. The body comes from scripts/fuzz-night-issue-body.sh; this script
# exists so that the body failing cannot stop the delivery.
#
# Run 37013034169 (the v0.66.0-rc6 soak) counted a correctness finding and
# then filed nothing: `BODY=$(bash scripts/fuzz-night-issue-body.sh ...)` died
# with SIGSEGV on a 2 GiB meta.txt, the step failed, and the only trace was a
# red step nobody reads — the measurement was right and its result reached no
# one. Now:
#
#   - the body is rendered to a FILE and posted with `--body-file`, never held
#     in a shell variable;
#   - if rendering fails (any exit, including a signal), or the body is empty,
#     a fallback body is posted instead: the count, the class, the exit status,
#     the artifact name and the run URL — enough to find the evidence by hand;
#   - a body over GitHub's 65,536-char limit is cut, and the cut is said.
#
# A fallback delivery prints `::error::` (the annotation lands on the run page)
# and exits 0: the finding WAS delivered, degraded. Only a failure to post at
# all exits non-zero.
#
# Usage: fuzz-night-route.sh <findings-dir> <correctness|slow|leak> <count>
#                            <run-url> <reporting> <planned> <missing> <artifact>
#   <artifact>  the uploaded findings artifact's name, quoted in the fallback
#
# Environment: GITHUB_TOKEN (for gh). FUZZ_NIGHT_BODY_SCRIPT overrides the
# renderer — the seam tests/fuzz_night_report_test.rs uses to force a failure.

set -uo pipefail
export LC_ALL=C

DIR="${1:?findings dir}"
CLASS="${2:?correctness|slow|leak}"
COUNT="${3:?count}"
RUN_URL="${4:?run url}"
REPORTING="${5:-?}"
PLANNED="${6:-?}"
MISSING="${7:-unknown}"
ARTIFACT="${8:?artifact name}"

HERE="$(cd "$(dirname "$0")" && pwd)"
RENDER="${FUZZ_NIGHT_BODY_SCRIPT:-$HERE/fuzz-night-issue-body.sh}"

case "$CLASS" in
  correctness)
    LABEL=fuzz-findings; COLOR=B60205
    DESC="Correctness findings from the nightly differential fuzzer"
    TITLE="Nightly fuzz: $COUNT cross-target finding(s)" ;;
  slow)
    LABEL=fuzz-perf; COLOR=D93F0B
    DESC="Perf-class (Slow) findings from the nightly fuzzer"
    TITLE="Nightly fuzz: $COUNT slow finding(s) (perf-class)" ;;
  leak)
    LABEL=fuzz-leak; COLOR=5319E7
    DESC="Wasm heap blocks live at exit, from the nightly fuzzer's live-heap rung"
    TITLE="Nightly fuzz: $COUNT wasm leak-at-exit finding(s)" ;;
  *) echo "fuzz-night-route.sh: class must be correctness, slow or leak, got '$CLASS'" >&2; exit 2 ;;
esac

# GitHub refuses an issue/comment body over 65,536 characters.
BODY_LIMIT=60000

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
BODY="$WORK/body.md"
ERR="$WORK/render.err"

bash "$RENDER" "$DIR" "$CLASS" "$RUN_URL" "$REPORTING" "$PLANNED" "$MISSING" >"$BODY" 2>"$ERR"
RC=$?
if [ "$RC" -eq 0 ] && [ ! -s "$BODY" ]; then
  RC=empty
fi

if [ "$RC" != 0 ]; then
  case "$RC" in
    empty) WHY="the renderer exited 0 but wrote nothing" ;;
    *)     WHY="exit $RC$([ "$RC" -gt 128 ] 2>/dev/null && echo ", signal $((RC - 128))")" ;;
  esac
  echo "::error::fuzz-night-route: the $CLASS issue body could not be built ($WHY); filing the fallback body"
  {
    echo "**$COUNT $CLASS finding(s); body generation failed ($WHY).**"
    echo ""
    echo "The findings are in the artifact \`$ARTIFACT\` of [run]($RUN_URL)."
    echo "Each finding's \`meta.txt\` there carries its replay command."
    echo ""
    echo "This is the fallback body (#3207): the night's measurement was recorded,"
    echo "but \`scripts/fuzz-night-issue-body.sh\` failed, so the listing is missing."
    if [ -s "$ERR" ]; then
      echo ""
      echo "<details><summary>renderer stderr (first 2 KiB)</summary>"
      echo ""
      echo '```'
      head -c 2048 "$ERR"
      echo ""
      echo '```'
      echo ""
      echo "</details>"
    fi
  } >"$BODY"
else
  SIZE=$(wc -c <"$BODY" | tr -d ' ')
  if [ "$SIZE" -gt "$BODY_LIMIT" ]; then
    head -c "$BODY_LIMIT" "$BODY" >"$WORK/cut.md"
    {
      echo ""
      echo '```'
      echo ""
      echo "**Body cut at $BODY_LIMIT of $SIZE bytes** (GitHub's limit) — the full"
      echo "findings are in the artifact \`$ARTIFACT\` of [run]($RUN_URL)."
    } >>"$WORK/cut.md"
    mv "$WORK/cut.md" "$BODY"
  fi
fi

EXISTING=$(gh issue list --label "$LABEL" --state open --json number --jq '.[0].number' || echo "")
if [ -n "$EXISTING" ]; then
  gh issue comment "$EXISTING" --body-file "$BODY"
else
  gh label create "$LABEL" --color "$COLOR" --description "$DESC" 2>/dev/null || true
  gh issue create --title "$TITLE" --body-file "$BODY" --label "$LABEL"
fi
