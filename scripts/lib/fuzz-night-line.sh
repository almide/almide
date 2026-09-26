# THE `fuzz-night:` RECORD LINE, READ IN ONE PLACE (#2390 / #924)
# ================================================================
#
# scripts/fuzz-night-verdict.sh WRITES one greppable line per night into the
# verdict job's log; scripts/fuzz-track-record.sh and scripts/fuzz-green-streak.sh
# READ it back to score the night. Before #2390 the two readers scored nights
# from job conclusions only, so the one field that qualifies the night's count
# — how many shards actually came back — never left the step log. This file
# is the reader both scripts share, so the writer and its readers cannot drift
# on the field names.
#
# The line (fields are `key=value`, space separated, order not significant):
#
#   fuzz-night: shards=k/N reporting=r recovered=1@295s,2@180s|none missing=3,8|none
#               minutes_planned=P minutes_delivered=D delivered_pct=NN budget=full|partial
#               generated=G throughput=Tprog/min findings=F findings_recovered=R
#               correctness=C slow=S
#
#   shards=k/N        k shards FINISHED their budget (the fuzzer's own summary
#                     block is present) out of N planned.
#   reporting=r       r shards RETURNED any output at all. r >= k.
#   recovered=        the shards that UPLOADED NOTHING and whose coverage was
#                     read back out of their job log instead, each tagged with
#                     the second its campaign had reached when the runner took
#                     it (`1@295s,2@180s`), or `none`. A reclaimed runner
#                     uploads nothing — its LOG is not an upload, outlives the
#                     job, and still carries the last progress line, so those
#                     seconds, programs and findings ARE in `minutes_delivered`,
#                     `generated` and `findings=` (#2513).
#   missing=          the shards that returned nothing AND whose log yielded
#                     nothing either, or `none` (or `unknown` on a layout that
#                     did not carry shard numbers). THESE, and only these, are
#                     the shards whose findings, if any, are NOT in `findings=`;
#                     their seeds are `run_id * 16 + shard`, so the evidence is
#                     replayable from the run alone.
#   findings_recovered=R
#                     the part of `findings=` that was read off a TRUNCATED log.
#                     R is "R up to the second each recovered shard is tagged
#                     with" — never R for that shard's whole budget, because the
#                     shard fuzzed no further and nothing after that second was
#                     examined at all. The fuzzer's progress counter is also
#                     class-blind (it does not split correctness from perf-class
#                     Slow), so R > 0 is UNCLASSIFIED, and a night may call
#                     itself green only when R is 0 as well as `findings=`.
#   delivered_pct     100 * minutes_delivered / minutes_planned, integer.
#   budget=           `full` when delivered_pct >= FUZZ_NIGHT_BUDGET_PCT (75),
#                     the condition #924 ratified on 2026-09-21 for the streak;
#                     `partial` otherwise.
#   correctness=C slow=S
#                     the class split of the UPLOADED findings (the fuzzer names
#                     a perf-class one `Slow__*`, #1235). A recovered finding has
#                     no class, so it is in `findings=` and in
#                     `findings_recovered=` and in NEITHER of these two —
#                     C + S = `findings=` only when `findings_recovered=0`.
#
# Lines written before #2390 lack `missing`, `delivered_pct` and `budget`;
# `fuzz_night_budget` derives the last two from the minutes fields so history
# stays scoreable, and reports `missing` as `unknown` for them. Lines written
# before #2513 lack `recovered` and `findings_recovered`; absent there means no
# recovery was ATTEMPTED, so those nights score exactly as they did before.
#
# Source this file; every function prints its answer on stdout.

FUZZ_NIGHT_BUDGET_PCT="${FUZZ_NIGHT_BUDGET_PCT:-75}"

# The LAST record line in a log (a retried verdict job appends), with any
# Actions timestamp prefix stripped. Prints nothing when the log has none —
# the caller decides what an absent record means (it means UNKNOWN, never 0).
fuzz_night_line() {
  local file="$1"
  grep -oE 'fuzz-night: .*' "$file" 2>/dev/null | tail -1 | tr -d '\r' || true
}

# One field's value, or the empty string when the line does not carry it.
fuzz_night_field() {
  local line="$1" key="$2"
  printf '%s\n' "$line" | grep -oE "(^| )$key=[^ ]*" | head -1 | sed -E "s/^ ?$key=//" || true
}

# One field replaced in place, or appended when the line does not carry it yet.
# Token-wise, so `findings=` and `findings_recovered=` cannot be confused.
fuzz_night_set_field() {
  local line="$1" key="$2" val="$3"
  printf '%s\n' "$line" | awk -v k="$key" -v v="$val" '{
    out = ""; seen = 0
    for (i = 1; i <= NF; i++) {
      tok = $i
      if (index(tok, k "=") == 1) { tok = k "=" v; seen = 1 }
      out = (out == "" ? tok : out " " tok)
    }
    if (!seen) out = out " " k "=" v
    print out
  }'
}

# ── RECOVERING A RECLAIMED SHARD FROM ITS JOB LOG (#2513) ─────────────────
#
# A reclaimed runner uploads nothing — and its LOG is not an upload. It
# outlives the job, and its last progress line holds the seconds fuzzed, the
# programs generated and the findings up to the kill. The verdict job folds
# that in as the night is scored; these two functions are also what lets a
# reader score a night that was written BEFORE the recovery existed, so #924's
# streak is measured on what the nights ran rather than on what they uploaded.

# One shard's job log on stdout; non-zero when it cannot be read at all.
#   $1 repo       owner/name
#   $2 jobs_json  that run's `actions/runs/<id>/jobs` response (either the whole
#                 object or a bare array of jobs)
#   $3 shard      the shard NUMBER
# $FUZZ_SHARD_LOG_DIR/shard-N.log short-circuits the API, which is how the
# tests forge killed shards with no network, no `gh` and no run.
fuzz_shard_log() {
  local repo="$1" jobs="$2" n="$3" id
  if [ -n "${FUZZ_SHARD_LOG_DIR:-}" ]; then
    cat "$FUZZ_SHARD_LOG_DIR/shard-$n.log" 2>/dev/null
    return
  fi
  # Every early exit says WHY on stderr (#2611): the verdict job printed
  # "could not read shard N's log" for every reclaimed shard of every night
  # from #2513's landing on, while the same logs were readable from outside,
  # and nothing in the job log said which of these steps had failed.
  [ -n "$repo" ] || { echo "fuzz_shard_log: shard $n: no repository to read from" >&2; return 1; }
  [ -n "$jobs" ] || { echo "fuzz_shard_log: shard $n: this run's job list could not be fetched" >&2; return 1; }
  command -v gh >/dev/null 2>&1 || { echo "fuzz_shard_log: shard $n: no gh on PATH" >&2; return 1; }
  command -v jq >/dev/null 2>&1 || { echo "fuzz_shard_log: shard $n: no jq on PATH" >&2; return 1; }
  id=$(printf '%s' "$jobs" \
    | jq -r --arg n "$n" '[(.jobs // .)[] | select(.name | test("shard " + $n + "[,)]")) | .id][0] // empty' 2>/dev/null) || id=""
  [ -n "$id" ] || { echo "fuzz_shard_log: shard $n: no job named 'shard $n' in this run's job list" >&2; return 1; }
  # Quote the URL: a bare `?` is a glob in the caller's shell. Retried: a
  # transient 5xx must not turn a shard's minutes into UNKNOWN.
  #
  # `--allow-escape-sequences` is THE fix for #2611's in-CI half: every job log
  # carries ANSI colour codes (the runner echoes each step's script in
  # `\e[36;1m`), and gh >= 2.10x REFUSES to print such a response ("the
  # response contains terminal escape sequences; pass --allow-escape-sequences
  # to output it anyway", exit 1). The runner image ships that gh, so from
  # #2513's landing on, every in-CI recovery failed, while the same fetch from
  # a workstation's older gh (which has no such flag, and rejects it as
  # unknown) succeeded. So the flag is passed exactly when this gh has it.
  # The help text is captured before it is searched: `gh ... | grep -q` under
  # the verdict's `pipefail` fails whenever grep exits early and gh takes the
  # SIGPIPE, which would silently drop the flag again.
  local attempt err out esc=() help
  help=$(gh api --help 2>&1 || true)
  case "$help" in *--allow-escape-sequences*) esc=(--allow-escape-sequences) ;; esac
  err=$(mktemp)
  for attempt in 1 2 3; do
    if out=$(gh api "repos/$repo/actions/jobs/$id/logs" "${esc[@]}" 2>"$err"); then
      rm -f "$err"
      printf '%s\n' "$out"
      return 0
    fi
    [ "$attempt" -lt 3 ] && sleep $((attempt * 2))
  done
  echo "fuzz_shard_log: shard $n: job $id log fetch failed 3 times: $(tr '\n' ' ' <"$err" | cut -c1-300)" >&2
  rm -f "$err"
  return 1
}

# stdin: a job log. stdout: `<seconds> <generated> <findings>` from its LAST
# progress line, or nothing at all when it holds none.
#
# The shape is the one tools/xtarget-fuzz prints every few seconds:
#
#   [  295s] generated=1108 clean=1084 rejects=20 findings=0 walls=1 skipped=1 | 225.3 prog/min
#
# with an Actions timestamp in front of it in a job log, which is why the match
# is not anchored. Matching the whole prefix is deliberate: a partially written
# or reshaped line yields NOTHING and the shard stays unknown, rather than
# handing back a number read out of something else.
fuzz_last_progress() {
  local last
  last=$(grep -oE '\[ *[0-9]+s\] generated=[0-9]+ clean=[0-9]+ rejects=[0-9]+ findings=[0-9]+' | tail -1) || true
  [ -n "$last" ] || return 0
  printf '%s %s %s\n' \
    "$(printf '%s' "$last" | sed -E 's/^\[ *([0-9]+)s\].*/\1/')" \
    "$(printf '%s' "$last" | sed -E 's/.* generated=([0-9]+) .*/\1/')" \
    "$(printf '%s' "$last" | sed -E 's/.* findings=([0-9]+).*/\1/')"
}

# A night's record line AMENDED with what its reclaimed shards' logs still
# hold: `recovered=`, a narrowed `missing=`, and minutes / programs / findings
# folded in, with `delivered_pct` and `budget` recomputed from them.
#
#   $1 repo  $2 jobs_json  $3 line  [$4 label for the diagnostics]
#
# The line comes back UNCHANGED when there is nothing to do (no `missing=`
# list, `missing=none`, `missing=unknown` — a pre-#2390 line does not say WHICH
# shards were lost, so there is nothing to look up) and when no missing shard's
# log could be read. A shard whose log cannot be read stays in `missing=` and
# is named on stderr: an absent value must arrive as absent.
#
# So a pre-#2390 night stays at the coverage it recorded even though its shards'
# logs are all still there (2026-09-19 reads 64% and 2026-09-13 63%, measured
# 2026-09-22). Recovering THOSE means recomputing the night from all N job logs
# rather than amending what one line says, which is a different instrument with
# its own failure modes — not this one quietly guessing which shards were lost.
fuzz_night_recover() {
  local repo="$1" jobs="$2" line="$3" label="${4:-}"
  local missing n secs gen finds
  local recovered="" unreadable="" rec_findings=0 d g f pct budget
  [ -n "$line" ] || { printf '%s\n' "$line"; return 0; }
  missing=$(fuzz_night_field "$line" missing)
  case "$missing" in ""|none|unknown) printf '%s\n' "$line"; return 0 ;; esac
  d=$(fuzz_night_field "$line" minutes_delivered); d="${d:-0}"
  g=$(fuzz_night_field "$line" generated); g="${g:-0}"
  f=$(fuzz_night_field "$line" findings); f="${f:-0}"
  for n in ${missing//,/ }; do
    read -r secs gen finds <<<"$(fuzz_shard_log "$repo" "$jobs" "$n" | fuzz_last_progress)" || true
    if [ -z "${secs:-}" ]; then
      unreadable="${unreadable:+$unreadable,}$n"
      echo "could not read shard $n's log${label:+ ($label)} — its minutes and findings stay UNKNOWN, not zero" >&2
      continue
    fi
    recovered="${recovered:+$recovered,}$n@${secs}s"
    d=$(awk -v a="$d" -v s="$secs" 'BEGIN{printf "%.1f", a + s/60}')
    g=$((g + gen))
    f=$((f + finds))
    rec_findings=$((rec_findings + finds))
  done
  [ -n "$recovered" ] || { printf '%s\n' "$line"; return 0; }
  line=$(fuzz_night_set_field "$line" minutes_delivered "$d")
  line=$(fuzz_night_set_field "$line" generated "$g")
  line=$(fuzz_night_set_field "$line" findings "$f")
  line=$(fuzz_night_set_field "$line" findings_recovered "$rec_findings")
  line=$(fuzz_night_set_field "$line" recovered "$recovered")
  line=$(fuzz_night_set_field "$line" missing "${unreadable:-none}")
  pct=$(fuzz_night_delivered_pct "$line")
  if [ -n "$pct" ]; then
    line=$(fuzz_night_set_field "$line" delivered_pct "$pct")
    if [ "$pct" -ge "$FUZZ_NIGHT_BUDGET_PCT" ]; then budget=full; else budget=partial; fi
    line=$(fuzz_night_set_field "$line" budget "$budget")
  fi
  printf '%s\n' "$line"
}

# The integer percentage of planned fuzz-minutes the night delivered, from the
# minutes fields (so pre-#2390 lines score too). Empty when it cannot be known.
fuzz_night_delivered_pct() {
  local line="$1" planned delivered
  planned=$(fuzz_night_field "$line" minutes_planned)
  delivered=$(fuzz_night_field "$line" minutes_delivered)
  if [ -z "$planned" ] || [ -z "$delivered" ] || [ "$planned" = "0" ]; then
    return 0
  fi
  awk -v p="$planned" -v d="$delivered" 'BEGIN{printf "%d", (100*d)/p}'
}

# The night's budget verdict, four words on one line:
#
#   <full|partial|unknown> <pct|?> <reporting/planned|?> <missing>
#
#   full      the night delivered >= FUZZ_NIGHT_BUDGET_PCT of its planned minutes
#   partial   it delivered less — a streak night it is NOT
#   unknown   no record line (the verdict job's log is gone or never wrote one):
#             the night cannot be scored either way, and a reader must not read
#             the absence as a full night OR as zero findings
#
# `missing` is the unreturned shard list, `none`, or `unknown` for a pre-#2390
# line that did not record it.
fuzz_night_budget() {
  local line="$1" pct planned reporting shards missing
  if [ -z "$line" ]; then
    echo "unknown ? ? unknown"
    return 0
  fi
  pct=$(fuzz_night_delivered_pct "$line")
  shards=$(fuzz_night_field "$line" shards)
  planned="${shards#*/}"
  reporting=$(fuzz_night_field "$line" reporting)
  missing=$(fuzz_night_field "$line" missing)
  [ -n "$missing" ] || missing="unknown"
  if [ -z "$reporting" ]; then
    # Legacy (unsharded) or the no-shard line: reporting == completed.
    reporting="${shards%/*}"
  fi
  local reported="${reporting:-?}/${planned:-?}"
  if [ -z "$pct" ]; then
    echo "unknown ? $reported $missing"
  elif [ "$pct" -ge "$FUZZ_NIGHT_BUDGET_PCT" ]; then
    echo "full $pct $reported $missing"
  else
    echo "partial $pct $reported $missing"
  fi
}

# One sharded night scored, from the verdict job's conclusion and its record
# line — the rule scripts/fuzz-track-record.sh and scripts/fuzz-green-streak.sh
# both apply, kept in one place. Prints one tab-separated row:
#
#   <counts_toward_verdict_streak 0|1> <counts_toward_green_streak 0|1> <coverage> <verdict text>
#
#   verdict streak (#924, ratified 2026-09-21): the night delivered a verdict
#     AND >= FUZZ_NIGHT_BUDGET_PCT of its planned fuzz-minutes. A partial night
#     breaks it. A night with NO record line breaks it too: a value that cannot
#     be read must not be read as full — that is the #2390 shape (a missing
#     shard read as zero findings) one level up.
#   green streak (#796 / the 90-day meter): a verdict-streak night whose
#     verdict concluded success AND whose recovered findings are zero too
#     (#2513: a count read off a truncated log is unclassified, so it cannot
#     make a night clean). Shards that did not report are NAMED in the text —
#     their findings are unknown, not zero — but at >= 75% delivered the night
#     still counts, exactly as the ruling says.
#
# <coverage> is `r/N pct%` (reporting shards over planned, delivered minutes as
# a percentage), or `?` when there is no record line.
fuzz_night_score() {
  local vjob="$1" line="$2"
  local budget pct reported missing coverage text full=0 green=0 qual=""
  local recovered rec_findings rec_dirty=0
  read -r budget pct reported missing <<<"$(fuzz_night_budget "$line")"
  if [ "$budget" = "unknown" ]; then coverage="?"; else coverage="$reported ${pct}%"; fi
  case "$missing" in
    none|unknown|"") ;;
    *) qual=" (shard(s) ${missing//,/, } did not report: findings unknown, not zero)" ;;
  esac
  # #2513: recovered shards uploaded nothing but their job log was read. Their
  # minutes and findings are already folded into the line's totals; what the
  # text has to carry is that each count stops at the second it is tagged with.
  recovered=$(fuzz_night_field "$line" recovered)
  rec_findings=$(fuzz_night_field "$line" findings_recovered)
  case "$rec_findings" in ""|0) ;; *) rec_dirty=1 ;; esac
  case "$recovered" in
    none|unknown|"") ;;
    *) qual="$qual (shard(s) ${recovered//,/, } recovered from their job logs: counted only up to the second named)" ;;
  esac
  case "$vjob" in
    success) text="GREEN" ;;
    failure) text="FINDINGS (verdict delivered, red on findings)" ;;
    *)
      printf '0\t0\t%s\t%s\n' "$coverage" "NO VERDICT (verdict job: $vjob)"
      return 0
      ;;
  esac
  # The verdict job only ever sees the shards that UPLOADED, so it can conclude
  # success over a night whose reclaimed shards had already recorded findings.
  # The word GREEN does not survive that, whatever the job concluded.
  if [ "$rec_dirty" -eq 1 ]; then
    if [ "$vjob" = "success" ]; then
      text="NOT GREEN (the shards that uploaded were clean, but ${rec_findings} finding(s) came back in a reclaimed shard's log, class unknown, counted only to the second named)"
    else
      qual="$qual [${rec_findings} of them recovered from a reclaimed shard's log, class unknown]"
    fi
  fi
  case "$budget" in
    full)
      full=1
      [ "$vjob" = "success" ] && [ "$rec_dirty" -eq 0 ] && green=1
      text="$text$qual"
      ;;
    partial)
      text="PARTIAL ${pct}% of planned fuzz-minutes — $text$qual; below the ${FUZZ_NIGHT_BUDGET_PCT}% line, not a streak night"
      ;;
    unknown)
      text="NO RECORD (verdict job wrote no fuzz-night: line) — $text; coverage unknown, not a streak night"
      ;;
  esac
  printf '%s\t%s\t%s\t%s\n' "$full" "$green" "$coverage" "$text"
}
