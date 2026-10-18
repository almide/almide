#!/usr/bin/env bash
# NIGHTLY FUZZ TRACKING-ISSUE BODY (#2390)
# ========================================
#
# Renders the body the verdict job posts to the tracking issue for one class
# of finding — `correctness` (everything but `Slow__*` and `LeakAtExit__*`,
# the `fuzz-findings` label), `slow` (perf-class, the `fuzz-perf` label,
# #1235) or `leak` (wasm heap blocks live at exit, the `fuzz-leak` label).
#
# Two silent under-counts lived in the workflow YAML this replaces:
#
#   1. The body said "across ${FUZZ_SHARDS} shard(s)" unconditionally — 8,
#      whether 8 shards uploaded or 1. The count is COLLECTED from the shards
#      that came back, so the body now says from how many, names the shards
#      that did not return, and says that their findings are not in the count.
#   2. The listing was `grep ... | head -60`: three matching lines per finding,
#      so a hard cap of exactly 20 findings that never said it was a cap. Run
#      33848741367 titled 23 and listed 20, and the three that fell off read as
#      unaccounted for rather than as truncated. The cap is now counted in
#      FINDINGS, and when it bites the body says "showing 20 of 23".
#
# Usage: fuzz-night-issue-body.sh <night-findings> <correctness|slow|leak> <run-url>
#                                 <reporting> <planned> <missing> [<cap>]
#   <night-findings>  the aggregated findings dir: one subdirectory per unique
#                     finding, each holding the fuzzer's meta.txt
#   <reporting>/<planned>/<missing>  the verdict's own qualification, as
#                     scripts/fuzz-night-verdict.sh wrote them to $GITHUB_OUTPUT
#   <cap>             findings listed inline (default 20); the rest stay in the
#                     artifact and the run log, and the body says so
#
# Every field is read with a byte cap (#3207). Run 37013034169's divergence
# printed a 2^31-1-wide `pad_start`: its `meta.txt` was 2 GiB, the `summary`
# line embedding the whole stdout line, and this script's output — pulled into
# `BODY=$(...)` by the workflow — took the verdict job down with SIGSEGV
# (exit 139), so no issue was filed. The fuzzer now caps what it writes, but
# an artifact written before that (or by any other path) must not be able to
# crash the renderer either: a meta.txt is read through `head -c` / `tail -c`
# only, each listed line is cut at FIELD_CAP bytes, and a cut says so.
#
# Tested with forged nights by tests/fuzz_night_report_test.rs.

set -euo pipefail
export LC_ALL=C

DIR="${1:?night-findings dir}"
CLASS="${2:?correctness|slow|leak}"
RUN_URL="${3:?run url}"
REPORTING="${4:?reporting}"
PLANNED="${5:?planned}"
MISSING="${6:?missing}"
CAP="${7:-20}"
# Bytes of meta.txt read from each end, and bytes kept of each listed line.
# 20 findings x 3 lines x FIELD_CAP stays under GitHub's 65,536-char body.
META_CAP=65536
FIELD_CAP=1024

case "$CLASS" in
  correctness) mapfile -t DIRS < <(find "$DIR" -mindepth 1 -maxdepth 1 -type d ! -name 'Slow__*' ! -name 'LeakAtExit__*' | sort) ;;
  slow)        mapfile -t DIRS < <(find "$DIR" -mindepth 1 -maxdepth 1 -type d -name 'Slow__*' | sort) ;;
  leak)        mapfile -t DIRS < <(find "$DIR" -mindepth 1 -maxdepth 1 -type d -name 'LeakAtExit__*' | sort) ;;
  *) echo "fuzz-night-issue-body.sh: class must be correctness, slow or leak, got '$CLASS'" >&2; exit 2 ;;
esac
COUNT=${#DIRS[@]}
# Further instances of a name live in `<name>/instances/<k>/` (#3518): the
# count stays unique-by-name, and the instances are said and listed.
EXTRA=0
for d in "${DIRS[@]}"; do
  [ -d "$d/instances" ] && EXTRA=$((EXTRA + $(find "$d/instances" -mindepth 1 -maxdepth 1 -type d | wc -l)))
done
if [ "$EXTRA" -gt 0 ]; then
  ALSO=" ($((COUNT + EXTRA)) instances: the same name from more than one program; each instance's replay line is listed)"
else
  ALSO=""
fi
SHOWN=$COUNT
[ "$SHOWN" -le "$CAP" ] || SHOWN=$CAP

# A verdict that never wrote its outputs leaves `reporting` unknown: say so,
# never "of 8" — the unqualified denominator is the defect this file replaces.
if [[ "$REPORTING" =~ ^[0-9]+$ ]] && [[ "$PLANNED" =~ ^[0-9]+$ ]]; then
  FROM="**$REPORTING of $PLANNED** shard(s)"
else
  FROM="an **unknown** number of the $PLANNED planned shard(s) (the verdict step recorded no shard count)"
  REPORTING=-1
  PLANNED=0
fi

case "$CLASS" in
  correctness)
    echo "The nightly generative differential fuzzer recorded **$COUNT** unique finding(s)$ALSO"
    echo "collected from $FROM."
    ;;
  slow)
    echo "The nightly fuzzer recorded **$COUNT** perf-class Slow finding(s)$ALSO: a leg"
    echo "outran the per-program budget but completed byte-identical at the 10x"
    echo "confirm re-run. Not correctness — the night stays green — but each is a"
    echo "real order-of-magnitude perf gap (the #1229 class). Collected from"
    echo "$FROM."
    ;;
  leak)
    echo "The nightly fuzzer recorded **$COUNT** LeakAtExit finding(s)$ALSO: both legs agreed"
    echo "and exited 0, but the wasm leg ended with heap blocks still live (native drops"
    echo "them; the wasm leg releases by hand). Measured with ALMIDE_WASM_ALLOC_COUNT=1 on"
    echo "the embedded host. Not fatal to the night — the corpus live-at-exit ledger is the"
    echo "red gate — but each repro is a leak the corpus does not cover. Collected from"
    echo "$FROM."
    ;;
esac

# `missing` is the shards that yielded NOTHING — no artifact and no readable
# job log (#2513 narrowed it; before that it was every shard that did not
# upload). Count it from the list itself so the number and the names cannot
# disagree: `planned - reporting` also counts the shards the verdict recovered
# from their logs, and those ARE in the count. Only an unnumbered layout, whose
# missing list is `unknown`, falls back to the subtraction.
if [ "$REPORTING" -ge 0 ] && [ "$REPORTING" -lt "$PLANNED" ]; then
  case "$MISSING" in
    unknown)  UNRETURNED=$((PLANNED - REPORTING)); WHICH="" ;;
    none|"")  UNRETURNED=0; WHICH="" ;;
    *)        UNRETURNED=$(printf '%s' "$MISSING" | tr ',' '\n' | grep -c .); WHICH=" (shards ${MISSING//,/, })" ;;
  esac
  if [ "$UNRETURNED" -gt 0 ]; then
    echo ""
    echo "**$UNRETURNED shard(s)${WHICH} did not report** — their findings, if any, are not"
    echo "in this count. A reclaimed runner uploads nothing, and for these the verdict"
    echo "could not read the shard's job log either; when it can, that log's last"
    echo "progress line is recovered into the count instead. The seed is derived"
    echo "(\`run_id * 16 + shard\`), so the evidence is replayable from the run alone."
  fi
fi

echo ""
echo "Artifacts (minimized repros + both-target outputs) are attached to"
echo "[run]($RUN_URL)."
echo ""
echo "Each finding's \`meta.txt\` includes a deterministic replay command:"
echo "\`xtarget-fuzz replay --seed <S> --index <I>\`."
echo ""
if [ "$SHOWN" -lt "$COUNT" ]; then
  echo "<details><summary>Finding summaries — showing $SHOWN of $COUNT; the remaining $((COUNT - SHOWN)) are in the artifact and the run log</summary>"
else
  echo "<details><summary>Finding summaries ($COUNT)</summary>"
fi
echo ""
# One finding's listed fields, never more than META_CAP bytes read from either
# end of its meta.txt. `kind` and `summary` come from the head; `reproduce` is
# the last line the fuzzer writes, so on an oversized file it comes from the
# tail (a head read would cut it off). `-a`: a cut can land mid-UTF-8 and the
# excerpt must still be read as text.
meta_fields() {
  local meta="$1" size
  size=$(wc -c <"$meta" | tr -d ' ')
  if [ "$size" -le "$META_CAP" ]; then
    head -c "$META_CAP" "$meta" | grep -aE '^(kind|summary|reproduce)' | cut_lines || true
  else
    head -c "$META_CAP" "$meta" | grep -aE '^(kind|summary)' | cut_lines || true
    tail -c "$META_CAP" "$meta" | grep -aE '^reproduce' | cut_lines || true
    echo "(meta.txt is $size bytes; listed fields are excerpts — the full file is in the artifact)"
  fi
}

# Cut each line at FIELD_CAP bytes, marking the cut. LC_ALL=C: bytes, not chars.
cut_lines() {
  awk -v cap="$FIELD_CAP" '{ if (length($0) > cap) print substr($0, 1, cap) "... (line cut at " cap " bytes)"; else print }'
}

echo '```'
for ((i = 0; i < SHOWN; i++)); do
  meta="${DIRS[$i]}/meta.txt"
  [ -f "$meta" ] || { echo "kind        = ? (no meta.txt in $(basename "${DIRS[$i]}"))"; continue; }
  meta_fields "$meta"
  # #3518: the other programs that produced the same name, by replay line.
  if [ -d "${DIRS[$i]}/instances" ]; then
    while IFS= read -r inst; do
      if [ -f "$inst/meta.txt" ]; then
        tail -c "$META_CAP" "$inst/meta.txt" | grep -aE '^reproduce' | sed 's/^/  also: /' | cut_lines || true
      else
        echo "  also: ? (no meta.txt in instance $(basename "$inst"))"
      fi
    done < <(find "${DIRS[$i]}/instances" -mindepth 1 -maxdepth 1 -type d | sort -V)
  fi
done
if [ "$SHOWN" -lt "$COUNT" ]; then
  echo "... $((COUNT - SHOWN)) more finding(s) not shown (showing $SHOWN of $COUNT)"
fi
echo '```'
echo ""
echo "</details>"
