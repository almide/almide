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
#   fuzz-night: shards=k/N reporting=r missing=3,8|none
#               minutes_planned=P minutes_delivered=D delivered_pct=NN budget=full|partial
#               generated=G throughput=Tprog/min findings=F correctness=C slow=S
#
#   shards=k/N        k shards FINISHED their budget (the fuzzer's own summary
#                     block is present) out of N planned.
#   reporting=r       r shards RETURNED any output at all. r >= k. A reclaimed
#                     runner uploads nothing, so N - r is the number of shards
#                     whose findings, if any, are NOT in `findings=`.
#   missing=          the unreturned shard NUMBERS, so the reader can name them
#                     (their seeds are `run_id * 16 + shard`, replayable from
#                     the run alone), or `none`.
#   delivered_pct     100 * minutes_delivered / minutes_planned, integer.
#   budget=           `full` when delivered_pct >= FUZZ_NIGHT_BUDGET_PCT (75),
#                     the condition #924 ratified on 2026-09-21 for the streak;
#                     `partial` otherwise.
#
# Lines written before #2390 lack `missing`, `delivered_pct` and `budget`;
# `fuzz_night_budget` derives the last two from the minutes fields so history
# stays scoreable, and reports `missing` as `unknown` for them.
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
#     verdict concluded success. Shards that did not report are NAMED in the
#     text — their findings are unknown, not zero — but at >= 75% delivered the
#     night still counts, exactly as the ruling says.
#
# <coverage> is `r/N pct%` (reporting shards over planned, delivered minutes as
# a percentage), or `?` when there is no record line.
fuzz_night_score() {
  local vjob="$1" line="$2"
  local budget pct reported missing coverage text full=0 green=0 qual=""
  read -r budget pct reported missing <<<"$(fuzz_night_budget "$line")"
  if [ "$budget" = "unknown" ]; then coverage="?"; else coverage="$reported ${pct}%"; fi
  case "$missing" in
    none|unknown|"") ;;
    *) qual=" (shard(s) ${missing//,/, } did not report: findings unknown, not zero)" ;;
  esac
  case "$vjob" in
    success) text="GREEN" ;;
    failure) text="FINDINGS (verdict delivered, red on findings)" ;;
    *)
      printf '0\t0\t%s\t%s\n' "$coverage" "NO VERDICT (verdict job: $vjob)"
      return 0
      ;;
  esac
  case "$budget" in
    full)
      full=1
      [ "$vjob" = "success" ] && green=1
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
