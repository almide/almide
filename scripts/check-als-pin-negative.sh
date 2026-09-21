#!/usr/bin/env bash
# Negative controls for the ALS pin gate (#2403): prove that
# check-als-pin.sh FIRES on a statement or title that differs from the
# judge's — in either direction — names the id and the field, tolerates a
# listed als-ahead id and only that one, refuses a stale allowance, and still
# refuses an id the judge lacks (the original, id-only half of the gate).
#
# The gate reads two ledgers and an allowance list from files (ALS_LEDGER,
# OUR_LEDGER, ALS_AHEAD), so every forged input is a mutated copy of the
# committed docs/contracts/contracts.toml: the real tree, the pin and the
# network are never touched. The judge is forged by mutating the copy handed
# in as ALS_LEDGER; this tree is forged by mutating the copy handed in as
# OUR_LEDGER; the pristine copy plays the other side.
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

# ALS_PIN_GATE lets an A/B run point these controls at another copy of the
# gate (the pre-#2403 id-only gate must FAIL every drift control below).
GATE="bash ${ALS_PIN_GATE:-scripts/check-als-pin.sh}"
LEDGER="docs/contracts/contracts.toml"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
pristine="$tmp/pristine.toml"
cp "$LEDGER" "$pristine"
: >"$tmp/no-allowance.txt"

# The subject: the first contract in the ledger, its statement line and its
# title line (the first of each after the id, inside the same [[contract]]).
id=$(grep -oE '^\s*id\s*=\s*"C-[0-9]+"' "$pristine" | head -1 | grep -oE 'C-[0-9]+')
[ -n "$id" ] || { echo "FAIL: no contract id in $LEDGER — the controls have no subject" >&2; exit 1; }
line_of() { # key -> line number of the first `key =` after the subject's id line
  awk -v id="$id" -v key="$1" '
    $0 ~ "^[[:space:]]*id[[:space:]]*=[[:space:]]*\"" id "\"" { on = 1; next }
    on && $0 ~ "^[[:space:]]*" key "[[:space:]]*=" { print NR; exit }' "$pristine"
}
stmt_line=$(line_of statement); title_line=$(line_of title)
[ -n "$stmt_line" ] && [ -n "$title_line" ] \
  || { echo "FAIL: $id has no statement/title line to forge" >&2; exit 1; }

# Mutations of the pristine copy, each written to its own file.
append_at() { # line text out — add text before the closing quote of that line
  awk -v n="$1" -v add="$2" 'NR == n { sub(/"$/, ""); $0 = $0 add "\"" } { print }' "$pristine" >"$3"
}
divergent_at() { # line out — rewrite the middle of that line so neither side is a prefix
  awk -v n="$1" 'NR == n { $0 = substr($0, 1, length($0) - 12) "DIVERGED-HERE" substr($0, length($0) - 4) } { print }' "$pristine" >"$2"
}
append_at "$stmt_line"  " FORGED EXTRA SENTENCE."  "$tmp/stmt-longer.toml"
append_at "$title_line" " (forged)"                "$tmp/title-longer.toml"
divergent_at "$stmt_line" "$tmp/stmt-diverged.toml"
# The id-only control: the subject's whole [[contract]] block dropped from the judge.
awk -v id="$id" '
  /^[[:space:]]*\[\[contract\]\]/ { if (buf != "" && !drop) printf "%s", buf; buf = ""; drop = 0 }
  { buf = buf $0 "\n" }
  $0 ~ "^[[:space:]]*id[[:space:]]*=[[:space:]]*\"" id "\"" { drop = 1 }
  END { if (buf != "" && !drop) printf "%s", buf }' "$pristine" >"$tmp/judge-without-id.toml"
grep -qE "^\s*id\s*=\s*\"$id\"" "$tmp/judge-without-id.toml" \
  && { echo "FAIL: the id-removal forge did not remove $id" >&2; exit 1; }
printf '%s  forged in-flight allowance (negative control)\n' "$id" >"$tmp/allow-subject.txt"
other=$(grep -oE '^\s*id\s*=\s*"C-[0-9]+"' "$pristine" | grep -oE 'C-[0-9]+' | grep -vxF "$id" | head -1)
printf '%s  forged stale allowance (negative control)\n' "$other" >"$tmp/allow-other.txt"

run() { # judge ours allowance -> stdout+stderr, exit code in $rc
  rc=0
  out=$(ALS_LEDGER="$1" OUR_LEDGER="$2" ALS_AHEAD="$3" $GATE 2>&1) || rc=$?
}
expect() { # description rc-expected [must-contain...]
  local desc="$1" want_rc="$2"; shift 2
  if [ "$rc" != "$want_rc" ]; then
    echo "FAIL: $desc — exit $rc, expected $want_rc" >&2; printf '%s\n' "$out" >&2; exit 1
  fi
  local needle
  for needle in "$@"; do
    case "$out" in *"$needle"*) ;; *) echo "FAIL: $desc — output does not name '$needle'" >&2; printf '%s\n' "$out" >&2; exit 1;; esac
  done
}
refute() { # description [must-not-contain...]
  local desc="$1"; shift
  local needle
  for needle in "$@"; do
    case "$out" in *"$needle"*) echo "FAIL: $desc — output names '$needle' but must not" >&2; printf '%s\n' "$out" >&2; exit 1;; esac
  done
}

# Positive control: the committed ledger against an exact copy of itself is
# green with every shared field compared — a harness blind to the fields would
# fail here (0 compared) rather than pass vacuously.
run "$pristine" "$pristine" "$tmp/no-allowance.txt"
expect "identical positive control" 0 "als-pin OK" "shared statement/title field(s) byte-identical"
case "$out" in *" 0 shared statement"*) echo "FAIL: positive control compared 0 fields" >&2; exit 1;; esac

# The #2403 direction: the judge's statement carries text this tree lacks.
run "$tmp/stmt-longer.toml" "$pristine" "$tmp/no-allowance.txt"
expect "judge statement longer (als ahead)" 1 "::error::$id.statement differs" "als ahead" "list $id in" "als-pin FAILED: 1 drifted"

# The direction that let seven contracts drift: this tree's statement carries
# text the judge never received.
run "$pristine" "$tmp/stmt-longer.toml" "$tmp/no-allowance.txt"
expect "our statement longer (almide ahead)" 1 "::error::$id.statement differs" "almide ahead" "land the wording in almide/als first"

# A title drift is a drift too, and is named as the title.
run "$tmp/title-longer.toml" "$pristine" "$tmp/no-allowance.txt"
expect "judge title differs" 1 "::error::$id.title differs" "als ahead"
refute "judge title differs" "$id.statement"

# Neither side a prefix of the other: reported as diverged, not as a direction.
run "$tmp/stmt-diverged.toml" "$pristine" "$tmp/no-allowance.txt"
expect "diverged statement" 1 "::error::$id.statement differs" "diverged (neither side is a prefix"

# The allowance opens exactly the listed id, and says so.
run "$tmp/stmt-longer.toml" "$pristine" "$tmp/allow-subject.txt"
expect "als-ahead drift listed in the allowance" 0 "als-pin OK" "$id.statement differs" "tolerated" "1 tolerated as als-ahead"

# ...but only for the judge being ahead: text HERE that the judge lacks is
# never in flight, so a listed id whose extra text is ours still fails.
run "$pristine" "$tmp/stmt-longer.toml" "$tmp/allow-subject.txt"
expect "almide-ahead drift listed in the allowance" 1 "::error::$id.statement differs" "almide ahead" "covers only text the JUDGE is ahead with"
refute "almide-ahead drift listed in the allowance" "tolerated"

# Shrink-only: an allowance for an id that does not drift is refused.
run "$tmp/stmt-longer.toml" "$pristine" "$tmp/allow-other.txt"
expect "stale allowance" 1 "lists $other as als-ahead but its statement and title match" "1 stale als-ahead"

# The original half of the gate: an id the judge lacks is refused by name.
run "$tmp/judge-without-id.toml" "$pristine" "$tmp/no-allowance.txt"
expect "id unknown to the judge" 1 "::error::$id is in" "not in the judge ledger" "id(s) unknown to the judge"

# A malformed allowance entry is an environment error, not a verdict.
printf 'not-an-id  garbage\n' >"$tmp/allow-bad.txt"
run "$pristine" "$pristine" "$tmp/allow-bad.txt"
expect "malformed allowance entry" 2 "entry that is not a contract id"

echo "als-pin negative controls: 1 positive + 9 forged inputs all behaved (subject $id, stale-control $other)"
