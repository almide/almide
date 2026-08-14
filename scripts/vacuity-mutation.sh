#!/usr/bin/env bash
# VACUITY BY MUTATION (#1411 stage 5, nightly budget).
#
# A test whose assertions survive the corruption of its inputs was not
# testing those inputs. This is what the fuzzer did to C-216 by accident —
# mutating "42" to "ΑΒΓ" was the first thing ever to run that fixture's err
# branch — aimed deliberately at the spec suite's own assertions.
#
# ORACLE, per file with `test` blocks:
#   baseline `almide test <file>` must PASS (else skip: it is already red for
#   its own reasons), then K single-literal mutants each run once. If EVERY
#   mutant also passes, no assertion in the file depends on the mutated
#   inputs — the file is reported SUSPECT-VACUOUS.
#
# This is the complement of xtarget-fuzz, not a duplicate: the fuzzer mutates
# wasm_cross and looks for LEG DISAGREEMENT (a compiler bug); this mutates
# self-asserting tests and looks for ASSERTIONS THAT CANNOT FAIL (a test-suite
# bug). C-216 was the second kind wearing the first kind's clothes.
#
# BUDGETED AND ROTATING: N files per night, window keyed on the day number, so
# the whole suite is swept over successive nights without any night paying for
# all of it. Deterministic — the same date re-runs the same window.
#
# Exit: 0 always (nightly telemetry, not a PR gate — the verdict job surfaces
# the report; a hard red here would block the night on a test-suite smell).
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ALMIDE="${ALMIDE_BIN:-$ROOT/target/release/almide}"
N_FILES="${VACUITY_FILES:-15}"
K_MUTANTS="${VACUITY_MUTANTS:-4}"
DAY="${VACUITY_DAY:-$(date -u +%j)}"   # day-of-year drives the rotation

if ! "$ALMIDE" --version >/dev/null 2>&1; then
  echo "vacuity-mutation: no almide binary (ALMIDE_BIN=$ALMIDE)"; exit 0
fi

# Every spec file that carries its own assertions.
mapfile -t ALL < <(grep -rl '^test "' "$ROOT/spec" --include='*.almd' 2>/dev/null | sort)
TOTAL=${#ALL[@]}
if [ "$TOTAL" -eq 0 ]; then echo "vacuity-mutation: no test files found"; exit 0; fi

START=$(( (DAY * N_FILES) % TOTAL ))
echo "vacuity-mutation: $TOTAL candidate file(s); window $N_FILES from index $START (day $DAY), $K_MUTANTS mutant(s) each"

suspects=()
checked=0
for i in $(seq 0 $((N_FILES - 1))); do
  f="${ALL[$(( (START + i) % TOTAL ))]}"
  rel="${f#"$ROOT"/}"

  # Baseline must pass; a file that is red on its own is not vacuous, it is red.
  if ! "$ALMIDE" test "$f" >/dev/null 2>&1; then
    echo "  skip  $rel (baseline does not pass)"
    continue
  fi
  checked=$((checked + 1))

  # K deterministic single-literal mutants. Python owns the mutation so the
  # k-th mutable literal is chosen structurally, not by fragile sed: integer
  # literals are incremented (1 stays distinguishable from 2), non-empty
  # string literals get their first character cycled. Interpolations, comments
  # and directive lines are left alone.
  survived=0
  produced=0
  for k in $(seq 0 $((K_MUTANTS - 1))); do
    mut="$(mktemp -t vacmut).almd"
    if ! python3 - "$f" "$k" "$K_MUTANTS" > "$mut" <<'PY'
import re, sys
src = open(sys.argv[1]).read()
k = int(sys.argv[2])
# Candidate literals OUTSIDE comments/directives: walk line by line.
spans = []
for m in re.finditer(r'(?m)^(?!\s*//).*$', src):
    line = m.group(0); base = m.start()
    code = line.split("//")[0]                      # strip trailing comment
    for lm in re.finditer(r'\b\d+\b|"(?:[^"\\${]|\\.)+?"', code):
        spans.append((base + lm.start(), base + lm.end(), lm.group(0)))
if not spans or k >= len(spans):
    sys.exit(3)                                     # fewer literals than K
# Spread the K samples across the WHOLE file instead of taking the first K:
# clustered picks land on inert literals (a [1,2,3] whose elements only feed
# list.len survives every element mutation soundly) and read as vacuity noise.
n_samples = min(int(sys.argv[3]), len(spans)) if len(sys.argv) > 3 else len(spans)
idx = (k * len(spans)) // max(n_samples, 1)
s, e, tok = spans[min(idx, len(spans) - 1)]
if tok[0] == '"':
    body = tok[1:-1]
    new = '"' + chr(((ord(body[0]) - 32 + 1) % 90) + 33) + body[1:] + '"'
else:
    new = str(int(tok) + 1)
sys.stdout.write(src[:s] + new + src[e:])
PY
    then rm -f "$mut"; continue; fi
    produced=$((produced + 1))
    if "$ALMIDE" test "$mut" >/dev/null 2>&1; then
      survived=$((survived + 1))
    fi
    rm -f "$mut"
  done

  if [ "$produced" -gt 0 ] && [ "$survived" -eq "$produced" ]; then
    suspects+=("$rel ($survived/$produced mutants survived)")
    echo "  SUSPECT-VACUOUS $rel — every mutant passed"
  else
    echo "  ok    $rel ($((produced - survived))/$produced mutant(s) killed)"
  fi
done

echo
echo "vacuity-mutation verdict: $checked file(s) checked, ${#suspects[@]} suspect(s)"
for s in "${suspects[@]}"; do echo "  - $s"; done
# Telemetry, never a hard red — see header.
exit 0
