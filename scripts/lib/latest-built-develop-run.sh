# latest_built_develop <job-regex> <step-regex> [limit]
#
# The newest green develop ci.yml run in which a job matching <job-regex>
# actually RAN a step matching <step-regex> (conclusion success). The weekly
# balance ratchets judge a run by the test time its jobs recorded, and a
# green run is not enough:
#   - a docs-only run carries every job with every step SKIPPED and still
#     concludes success — judged, it read as "shard skew inf" (2026-10-12) or
#     "no balance to judge";
#   - `gh run list --limit 1` has once handed back a months-old id (an
#     eventually-consistent read).
# So the candidate is chosen by what it ran. $VIEW_ESC may be set by the
# caller (gh's --allow-escape-sequences probe).
latest_built_develop() {
  local job="$1" step="$2" limit="${3:-20}" id ran
  for id in $(gh run list --workflow ci.yml --branch develop --status success --limit "$limit" --json databaseId --jq '.[].databaseId'); do
    # shellcheck disable=SC2086
    ran=$(gh run view ${VIEW_ESC:-} "$id" --json jobs --jq \
      "[.jobs[] | select(.name | test(\"$job\")) | .steps[] | select((.name | test(\"$step\")) and .conclusion == \"success\")] | length" 2>/dev/null)
    if [ "${ran:-0}" -gt 0 ]; then
      echo "$id"
      return 0
    fi
    echo "skipping run $id: no '$job' job ran '$step' (a docs-only run skips them)" >&2
  done
  echo "::error::none of the last $limit green develop ci.yml runs ran '$step' in a '$job' job" >&2
  return 1
}
