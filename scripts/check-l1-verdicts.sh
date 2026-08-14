#!/usr/bin/env bash
# L1 verdict ledger gate (edit-locality Stage 4, proofs/l1-verdicts.toml).
#
# Validates the lab book of the prediction loop:
#   - every [[verdict]] block carries all nine fields, one scalar per line
#   - ids are LV-NNN, unique, and strictly ascending
#   - verdict / msr_prediction / msr_measured use the closed vocabularies
#   - landed is YYYY-MM-DD
#   - a "violated" verdict never carries a landed date in the future of its
#     own recording — violated changes are REJECTIONS, they must not land
#     (mechanically: violated entries are allowed; the review question they
#     exist to force is human)
#
# The measured column is filled by almide-dojo's src/l1_loop.almd join tool;
# this gate only checks vocabulary, so a refuted prediction is committable —
# refutations are findings, not failures.
set -u

LEDGER="proofs/l1-verdicts.toml"
fail=0
err() { echo "::error::$1"; fail=1; }

[ -f "$LEDGER" ] || { err "$LEDGER missing"; exit 1; }

awk '
  /^\[\[verdict\]\]/ {
    if (in_block) check_block()
    in_block = 1; n++
    delete f
    next
  }
  in_block && /^[a-z_]+ *=/ {
    key = $1
    f[key] = $0
  }
  function check_block() {
    split("id change landed refs verdict mechanism msr_prediction prediction_basis msr_measured", req, " ")
    for (i in req) if (!(req[i] in f)) { printf "::error::%s: block %d missing field %s\n", FILENAME, n, req[i]; bad = 1 }
    if (f["id"] !~ /^id *= *"LV-[0-9][0-9][0-9]"/) { printf "::error::block %d: bad id format\n", n; bad = 1 }
    idnum = f["id"]; gsub(/[^0-9]/, "", idnum)
    if (idnum + 0 <= last_id) { printf "::error::block %d: id not strictly ascending\n", n; bad = 1 }
    last_id = idnum + 0
    if (f["landed"] !~ /^landed *= *"20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]"/) { printf "::error::block %d: bad landed date\n", n; bad = 1 }
    if (f["verdict"] !~ /^verdict *= *"(preserved|side-condition|violated)"/) { printf "::error::block %d: bad verdict\n", n; bad = 1 }
    if (f["msr_prediction"] !~ /^msr_prediction *= *"(up|neutral|down)"/) { printf "::error::block %d: bad msr_prediction\n", n; bad = 1 }
    if (f["msr_measured"] !~ /^msr_measured *= *"(pending|confirmed|refuted|inconclusive)"/) { printf "::error::block %d: bad msr_measured\n", n; bad = 1 }
  }
  END {
    if (in_block) check_block()
    if (n == 0) { printf "::error::no [[verdict]] blocks\n"; bad = 1 }
    if (bad) exit 1
    printf "l1-verdicts: OK — %d verdicts\n", n
  }
' "$LEDGER" || fail=1

exit $fail
