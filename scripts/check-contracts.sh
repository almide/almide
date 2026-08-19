#!/usr/bin/env bash
# CONTRACT-LEDGER TRACEABILITY GATE
# =================================
#
# The cross-target equivalence gate (tests/wasm_runtime_test.rs::wasm_cross_target_spec)
# and the wasm-runtime oracle-pairing registry (rt-oracle-registry.toml) enforce
# equivalence at the TEST and ROUTINE level. This gate adds the CONTRACT level:
# every observable cross-target promise is a named [[contract]] in
# docs/contracts/contracts.toml, and the link to its executable EVIDENCE is
# mandatory and BIDIRECTIONAL:
#   - every contract carries >= 1 piece of real evidence (and an `active` contract
#     carries >= 1 evidence of class >= fixture — prose alone cannot certify
#     observable behaviour);
#   - every spec/wasm_cross/*.almd names the contract(s) it certifies on a
#     `// @contract: C-NNN` header line, and that link must be symmetric.
#
# Pure grep/awk/comm — NO cargo build, NO network — runs in well under 5s, and is
# mutation-testable (each check below flips green->red on a one-line edit; see the
# MUTATION-TESTABILITY block at the bottom). Modeled line-for-line on
# scripts/check-rt-oracle-registry.sh.
#
# It FAILS when:
#   (a) a contract's evidence path is missing, or a named unit is not in that file;
#   (b) an `active` contract has no evidence of class >= fixture;
#   (c) a spec/wasm_cross fixture has no / a malformed // @contract: header, or
#       names a C-NNN that is not in the ledger;
#   (d) a fixture<->contract link is not symmetric (header names a contract that
#       does not list the fixture as evidence, or vice-versa);
#   (e) a schema violation (bad id / duplicate id / bad status / bad class /
#       fuzz without n / missing doc file / a missing REQUIRED scalar —
#       title, statement, or since — or a since that is not MAJOR.MINOR.PATCH);
#   (f) a coverage gap (C-001..C-NNN must be contiguous) or the flagged-contract
#       ratchet ceiling is exceeded;
#   (g) the README claims block (equivalence-claim numbers + exceptions clause)
#       is stale relative to the ledger (scripts/gen-claims.sh --check, #766);
#   (h) docs/contracts/README.md — the generated index — is stale relative to
#       the ledger (the same diff CI's "Emit & Format" job runs);
#   (j) a source path cited in a contract statement or a fixture header lives
#       under a directory that no longer exists (a retired subsystem — #941).
set -uo pipefail

# Byte-order collation, pinned: `sort`'s last-resort comparison follows the ambient
# locale, so an unpinned sort produces different output on differently-configured
# machines. #1031 caught docs/roadmap/README.md changing row order with no content change.
export LC_ALL=C
cd "$(dirname "$0")/.." || { echo "::error::cannot cd to repo root"; exit 2; }

LEDGER="docs/contracts/contracts.toml"
FIXTURE_DIR="spec/wasm_cross"
DOC_DIR="docs/contracts"
CLASS_FILE="scripts/lib/contract-classes.txt"

[ -f "$LEDGER" ]      || { echo "::error::$LEDGER not found (run from repo root)"; exit 2; }
[ -d "$FIXTURE_DIR" ] || { echo "::error::$FIXTURE_DIR not found"; exit 2; }
[ -f "$CLASS_FILE" ]  || { echo "::error::$CLASS_FILE not found"; exit 2; }

# ── The canonical evidence-class vocabulary (shared with the registry gate). ──
# Strip comments/blanks; the LINE ORDER is the rank (line 1 = rank 0). One list
# file = the two gates' enums provably cannot diverge.
CLASSES="$(grep -vE '^[[:space:]]*(#|$)' "$CLASS_FILE")"
# A regex-safe alternation of the valid classes, e.g. (doc-only|by-construction|...).
CLASS_ALT="$(printf '%s' "$CLASSES" | paste -sd'|' -)"
# The rank (0-based line index) at which `fixture` sits = the FLOOR for active.
FIXTURE_RANK="$(printf '%s\n' "$CLASSES" | grep -nxF 'fixture' | cut -d: -f1)"
FIXTURE_RANK=$((FIXTURE_RANK - 1))
class_rank() { printf '%s\n' "$CLASSES" | grep -nxF "$1" | cut -d: -f1 | awk '{print $1-1}'; }

fail=0
err() { fail=1; echo "::error::$*"; }

# ── PARSER ──────────────────────────────────────────────────────────────────
# Walk [[contract]] blocks. For each block emit one TAB-separated record per
# evidence entry plus per-contract scalar records, all keyed by id, so the bash
# side can group by id. The `statement` field uses ''' triple-quote when multi-
# line; a sentinel skips its body. Single-line scalars parse exactly like the
# registry's awk. Output schema (one per line, TAB-delimited):
#   META<TAB>id<TAB>status<TAB>doc<TAB>title<TAB>statement<TAB>since
#   EV<TAB>id<TAB>path<TAB>class<TAB>name<TAB>n
# (empty name/n render as the literal "-"; title/statement are presence flags
# 0|1, since is the literal value or "" when the key is absent)
# ── SIGPIPE-SAFE MEMBERSHIP ─────────────────────────────────────────────────
# `printf '%s\n' "$SET" | grep -q "$x"` is a RACE under `set -o pipefail`.
# `grep -q` exits the instant it matches, closing the pipe; if `printf` has not
# finished writing, it dies of SIGPIPE and the PIPELINE's status is non-zero even
# though the element WAS found. The gate then reports a contract as "not in the
# ledger" — a false failure whose victim is whichever id happened to lose the
# race, so it names a different contract every time.
#
# Measured on this repo's 286-id ledger: an EARLY match (C-001) false-failed
# 1 run in 400, a LATE match (C-286) never did — exactly the shape the race
# predicts, since a late match leaves nothing left to write. In practice the gate
# failed about 1 local run in 6, each time naming a different contract.
#
# A gate that is red 15% of the time for no reason is worse than no gate: it
# teaches everyone to re-run until green, which is also what you do to a REAL
# failure. `has()` reads from a here-string, so there is no pipe to break.
has() { # $1=needle  $2=haystack (newline-separated)  [$3=-E for regex]
  case "${3:-}" in
    -E) grep -qE -- "$1" <<<"$2" ;;
    *)  grep -qxF -- "$1" <<<"$2" ;;
  esac
}

parse_ledger() {
  awk '
    # Empty optional scalars render as "-": TAB is IFS whitespace, so bash `read`
    # COLLAPSES adjacent tabs and an empty field would shift every later column.
    function emit_meta() {
      if (id != "") print "META\t" id "\t" status "\t" (doc == "" ? "-" : doc) "\t" title "\t" stmt "\t" (since == "" ? "-" : since)
    }
    function reset() { id=""; status=""; doc=""; title=0; stmt=0; since="" }
    BEGIN { reset(); in_stmt=0 }
    # triple-quote sentinel: toggle, and swallow everything between. A
    # `statement = ""..."` opening line is consumed HERE, so the presence flag
    # has to be set on the opening toggle — the /^statement/ rule never sees it.
    /'"'"''"'"''"'"'/ { in_stmt = !in_stmt; if (in_stmt) stmt=1; next }
    in_stmt { next }
    /^\[\[contract\]\]/ { emit_meta(); reset(); next }
    /^id[ \t]*=/      { v=$0; sub(/^id[ \t]*=[ \t]*"/,"",v); sub(/".*$/,"",v); id=v; next }
    /^status[ \t]*=/  { v=$0; sub(/^status[ \t]*=[ \t]*"/,"",v); sub(/".*$/,"",v); status=v; next }
    /^doc[ \t]*=/     { v=$0; sub(/^doc[ \t]*=[ \t]*"/,"",v); sub(/".*$/,"",v); doc=v; next }
    /^title[ \t]*=/     { title=1; next }
    /^statement[ \t]*=/ { stmt=1; next }
    /^since[ \t]*=/   { v=$0; sub(/^since[ \t]*=[ \t]*"/,"",v); sub(/".*$/,"",v); since=v; next }
    # an evidence inline-table line: { path = "...", class = "...", name = "...", n = N }
    /path[ \t]*=[ \t]*"/ {
      line=$0
      p=line; sub(/^.*path[ \t]*=[ \t]*"/,"",p); sub(/".*$/,"",p)
      c="-"; if (line ~ /class[ \t]*=[ \t]*"/) { c=line; sub(/^.*class[ \t]*=[ \t]*"/,"",c); sub(/".*$/,"",c) }
      nm="-"; if (line ~ /name[ \t]*=[ \t]*"/) { nm=line; sub(/^.*name[ \t]*=[ \t]*"/,"",nm); sub(/".*$/,"",nm) }
      n="-"; if (line ~ /[, {][ \t]*n[ \t]*=[ \t]*[0-9]/) { n=line; sub(/^.*[, {][ \t]*n[ \t]*=[ \t]*/,"",n); sub(/[^0-9].*$/,"",n) }
      print "EV\t" id "\t" p "\t" c "\t" nm "\t" n
      next
    }
    END { emit_meta() }
  ' "$LEDGER"
}

LEDGER_RECORDS="$(parse_ledger)"
META="$(printf '%s\n' "$LEDGER_RECORDS" | grep '^META' || true)"
EV="$(printf '%s\n' "$LEDGER_RECORDS" | grep '^EV' || true)"

ALL_IDS="$(printf '%s\n' "$META" | cut -f2 | grep . || true)"

# ── (e) SCHEMA: id shape + uniqueness, status enum, doc file exists ──────────
# Every REQUIRED scalar is checked here. `since` used to be documented REQUIRED
# but unenforced, so 32 contracts (C-067..C-098) shipped without it and the
# generated README published 32 blank Since cells (#938). A field the schema
# calls required and the gate never reads is a field that silently goes missing.
while IFS=$'\t' read -r _tag id status doc title stmt since; do
  [ -z "$id" ] && continue
  [ "$doc" = "-" ] && doc=""
  [ "$since" = "-" ] && since=""
  has '^C-[0-9]{3}$' "$id" -E || err "bad contract id '$id' (must match ^C-[0-9]{3}\$)"
  case "$status" in
    active|flagged-for-revision) ;;
    *) err "$id: status '$status' is not one of {active, flagged-for-revision}" ;;
  esac
  [ "$title" = "1" ] || err "$id: REQUIRED key 'title' is missing"
  [ "$stmt"  = "1" ] || err "$id: REQUIRED key 'statement' is missing"
  if [ -z "$since" ]; then
    err "$id: REQUIRED key 'since' is missing (the version the contract became normative)"
  elif ! has '^[0-9]+\.[0-9]+\.[0-9]+$' "$since" -E; then
    err "$id: since='$since' is not a MAJOR.MINOR.PATCH version"
  fi
  if [ "$doc" != "" ] && [ ! -f "$DOC_DIR/$doc" ]; then
    err "$id: doc='$doc' does not exist under $DOC_DIR/"
  fi
done <<< "$META"

dupes="$(printf '%s\n' "$ALL_IDS" | sort | uniq -d)"
if [ -n "$dupes" ]; then
  while IFS= read -r d; do [ -n "$d" ] && err "duplicate contract id $d"; done <<< "$dupes"
fi

# ── (a) EVIDENCE EXISTS + (e) class/fuzz-n schema ───────────────────────────
while IFS=$'\t' read -r _tag id path class name n; do
  [ -z "$id" ] && continue
  # class enum
  has "^(${CLASS_ALT})$" "$class" -E || err "$id: class '$class' not one of {$(printf '%s' "$CLASSES" | paste -sd, -)}"
  # fuzz requires n>=1
  if [ "$class" = "fuzz" ]; then
    if ! has '^[0-9]+$' "$n" -E || [ "${n:-0}" -lt 1 ]; then
      err "$id: class='fuzz' evidence ($path) requires n=<int >= 1> (got '$n')"
    fi
  fi
  # path must exist
  if [ ! -f "$path" ]; then
    err "$id evidence path does not exist: $path"
    continue
  fi
  # named-unit grep: required for *.rs / *.lean / *.toml and for fuzz/lean/exhaustive
  needs_name=0
  case "$path" in *.rs|*.lean|*.toml) needs_name=1 ;; esac
  case "$class" in fuzz|lean|exhaustive) needs_name=1 ;; esac
  if [ "$needs_name" -eq 1 ] && [ "$name" = "-" ]; then
    err "$id evidence $path requires a name= (class=$class / non-fixture file needs the unit to grep)"
    continue
  fi
  if [ "$name" != "-" ]; then
    case "$path" in
      *.rs)   grep -qE "fn[[:space:]]+${name}[[:space:]]*\(" "$path" || err "$id evidence '$name' (fn) not found in $path" ;;
      *.lean) grep -qE "(theorem|lemma|def)[[:space:]]+${name}\b" "$path" || err "$id evidence '$name' (theorem/def) not found in $path" ;;
      *.toml) grep -qE "routine = \"${name}\"" "$path" || err "$id evidence '$name' (routine) not found in $path" ;;
      *.almd) grep -qE "test[[:space:]]+\"${name}\"" "$path" || err "$id evidence '$name' (test) not found in $path" ;;
    esac
  fi
done <<< "$EV"

# ── (b) EVERY ACTIVE CONTRACT HAS EVIDENCE OF CLASS >= fixture ──────────────
# For each id, the max evidence rank; an `active` contract must reach FIXTURE_RANK.
while IFS=$'\t' read -r _tag id status _doc; do
  [ -z "$id" ] && continue
  [ "$status" = "flagged-for-revision" ] && continue   # exempt (may rest on doc-only)
  maxrank=-1
  while IFS=$'\t' read -r _t eid _p eclass _n _nn; do
    [ "$eid" = "$id" ] || continue
    r="$(class_rank "$eclass" 2>/dev/null)"; [ -z "$r" ] && r=-1
    [ "$r" -gt "$maxrank" ] && maxrank="$r"
  done <<< "$EV"
  if [ "$maxrank" -lt "$FIXTURE_RANK" ]; then
    err "$id is active but has no evidence of class >= fixture (by-construction/doc-only alone cannot certify observable behaviour; add a fixture or set status=flagged-for-revision)"
  fi
done <<< "$META"

# ── The two BIDIRECTIONAL edge sets: (contract,fixture) pairs ───────────────
# Forward edges: contract --evidence--> a spec/wasm_cross/*.almd fixture.
fwd_edges() {
  while IFS=$'\t' read -r _tag id path _class _name _n; do
    case "$path" in
      "$FIXTURE_DIR"/*.almd) printf '%s\t%s\n' "$id" "$(basename "$path")" ;;
    esac
  done <<< "$EV"
}
FWD="$(fwd_edges | sort -u | grep . || true)"

# ── (c) + reverse edges: fixture --// @contract:--> contract ────────────────
# Every fixture must carry a well-formed // @contract: line; collect its edges.
CONTRACT_RE='^[[:space:]]*//[[:space:]]*@contract:[[:space:]]*C-[0-9]{3}([[:space:]]*,[[:space:]]*C-[0-9]{3})*[[:space:]]*$'
REV=""
for f in "$FIXTURE_DIR"/*.almd; do
  base="$(basename "$f")"
  line="$(grep -nE "$CONTRACT_RE" "$f" | head -1 || true)"
  if [ -z "$line" ]; then
    # Distinguish "present but malformed" from "absent" for a sharper message.
    if grep -qE '@contract' "$f"; then
      err "$base has a malformed // @contract: header (must match: // @contract: C-NNN[, C-MMM])"
    else
      err "$base has no // @contract: header (every cross-target fixture must name the contract(s) it certifies)"
    fi
    continue
  fi
  ids="$(printf '%s' "$line" | sed -E 's/^[0-9]+://; s#^[[:space:]]*//[[:space:]]*@contract:##')"
  for cid in $(printf '%s' "$ids" | tr ',' ' '); do
    cid="$(printf '%s' "$cid" | tr -d '[:space:]')"
    [ -z "$cid" ] && continue
    if ! has "$cid" "$ALL_IDS"; then
      err "$base references $cid which is not in the ledger"
      continue
    fi
    REV="${REV}${cid}	${base}
"
  done
done
REV="$(printf '%s' "$REV" | sort -u | grep . || true)"

# ── (d) NO ORPHANS — the two edge sets must be IDENTICAL (symmetric link) ────
only_fwd="$(comm -23 <(printf '%s\n' "$FWD") <(printf '%s\n' "$REV"))"
only_rev="$(comm -13 <(printf '%s\n' "$FWD") <(printf '%s\n' "$REV"))"
if [ -n "$only_fwd" ]; then
  while IFS=$'\t' read -r id base; do
    [ -z "$id" ] && continue
    err "$id lists $base as evidence but $base does not declare $id in its // @contract: header (link must be symmetric)"
  done <<< "$only_fwd"
fi
if [ -n "$only_rev" ]; then
  while IFS=$'\t' read -r id base; do
    [ -z "$id" ] && continue
    err "$base declares $id but $id does not list $base as evidence (link must be symmetric)"
  done <<< "$only_rev"
fi

# ── (j) CITED SOURCE PATHS MUST NOT NAME A RETIRED SUBSYSTEM ────────────────
# `evidence.path` is already checked to exist. But contract STATEMENTS and
# fixture HEADERS also point the reader at the implementation ("the WASM runtime
# `emit_wasm/rt_dragon.rs` must match it"), and nothing read those. When the v0
# wasm emitter was retired (c71eff7b deleted 115 files under
# crates/almide-codegen/src/emit_wasm/), 16 such citations rotted in place and
# the ledger kept sending readers to code that no longer existed (#941).
#
# The rule is deliberately narrow: flag a cited path only when its PARENT
# DIRECTORY is gone. That is exactly the retired-subsystem signature, and it
# cannot fire on an illustrative filename inside a live directory — statements
# legitimately contain examples like `fs.stat("spec/x.almd")`. A single deleted
# file inside a surviving directory is NOT caught; that is the price of zero
# false positives, and the evidence-path check above covers the paths that
# actually certify a contract.
dead_paths=0
while IFS= read -r cited; do
  [ -z "$cited" ] && continue
  [ -e "$cited" ] && continue
  parent="$(dirname "$cited")"
  [ -d "$parent" ] && continue
  err "cited path '$cited' lives under '$parent', which does not exist (retired subsystem?)"
  dead_paths=$((dead_paths + 1))
done <<< "$(grep -rhoE '(crates|runtime|stdlib|spec|tests|scripts|proofs)/[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)*\.(rs|almd|lean|toml|sh)' \
              "$LEDGER" "$FIXTURE_DIR"/*.almd 2>/dev/null | sort -u)"
[ "$dead_paths" -eq 0 ] && echo "cited-paths: every source path named in a statement or fixture header resolves to a live directory."

# ── (f) COVERAGE: ids must be contiguous C-001..C-NNN, no gaps ──────────────
sorted_ids="$(printf '%s\n' "$ALL_IDS" | sort -u)"
n_contracts="$(printf '%s\n' "$sorted_ids" | grep -c . || true)"
maxnum="$(printf '%s\n' "$sorted_ids" | sed -E 's/^C-//' | sort -n | tail -1)"
maxnum="$((10#${maxnum:-0}))"
i=1
while [ "$i" -le "$maxnum" ]; do
  want="$(printf 'C-%03d' "$i")"
  has "$want" "$sorted_ids" || err "coverage gap: $want is missing (C-001..C-$(printf '%03d' "$maxnum") must be contiguous)"
  i=$((i + 1))
done

# ── (f) RATCHET: flagged contracts may only shrink ──────────────────────────
# Current floor: ZERO. C-033 (aliased-mutable COW) converged first; C-006
# (fan.timeout wall clock) was retired by REMOVING fan.timeout in 0.29.0.
# Every contract in the ledger is active. LOWER only; never raise.
MAX_FLAGGED=0
n_flagged="$(printf '%s\n' "$META" | awk -F'\t' '$3=="flagged-for-revision"' | grep -c . || true)"
n_active=$((n_contracts - n_flagged))
if [ "$n_flagged" -gt "$MAX_FLAGGED" ]; then
  err "flagged-for-revision count $n_flagged exceeds the ratchet ceiling $MAX_FLAGGED — a new behaviour must ship an active contract + a fixture (see docs/contracts/README.md)"
fi

# ── Counts + evidence-by-class histogram ────────────────────────────────────
n_fixtures="$(ls "$FIXTURE_DIR"/*.almd 2>/dev/null | grep -c . || true)"
n_with_header="$(grep -lE "$CONTRACT_RE" "$FIXTURE_DIR"/*.almd 2>/dev/null | grep -c . || true)"
echo "----"
echo "evidence by class:"
printf '%s\n' "$CLASSES" | while IFS= read -r cls; do
  cnt="$(printf '%s\n' "$EV" | awk -F'\t' -v c="$cls" '$4==c' | grep -c . || true)"
  printf '  %-16s %s\n' "$cls" "$cnt"
done

# ── SPEC-KEYING (CG-1 / flight-evidence-gaps F1): a contract carrying a
# `spec = "ALS-xx"` field must point at a real normative section (`## ALS-xx `
# heading in docs/specs/als/), so a claim can never reference a spec that does
# not exist — the third layer of the spec ↔ contract ↔ fixture traceability.
ALS_DIR="docs/specs/als"
if [ -d "$ALS_DIR" ]; then
  # The spec key is REQUIRED on every contract (#565): a claim without its
  # normative section is untraceable. (Triple-quoted statement bodies are
  # skipped so a literal "spec =" inside prose cannot satisfy the check.)
  missing_spec="$(awk '
    /'"'"''"'"''"'"'/ { in_stmt = !in_stmt; next }
    in_stmt { next }
    /^\[\[contract\]\]/ { if (id != "" && !has) print id; id=""; has=0; next }
    /^id[ \t]*=/   { v=$0; sub(/^id[ \t]*=[ \t]*"/,"",v); sub(/".*$/,"",v); id=v; next }
    /^spec[ \t]*=/ { has=1; next }
    END { if (id != "" && !has) print id }
  ' "$LEDGER")"
  if [ -n "$missing_spec" ]; then
    for cid in $missing_spec; do
      echo "::error::contract $cid has NO spec key — every contract must cite its ALS section (#565)"
    done
    fail=1
  fi

  # Lenient spacing, matching the presence check above (#989): the old
  # six-space-aligned grep silently DROPPED any `spec = "..."` written with
  # different spacing from this resolution loop — a bogus ALS key on an
  # unaligned line passed presence and skipped existence.
  specd="$(grep -E '^spec[ \t]*=' "$LEDGER" | sed -E 's/^spec[ \t]*=[ \t]*"([^"]+)".*/\1/' | sort -u)"
  n_specd=0
  for sec in $specd; do
    n_specd=$((n_specd + 1))
    if ! grep -qE "^## $sec( |$)" "$ALS_DIR"/*.md; then
      echo "::error::contract spec key '$sec' has NO normative section (## $sec) under docs/specs/als/"
      fail=1
    fi
  done
  echo "spec-keying: $n_specd distinct ALS section(s) referenced; all resolve."

  # ── REVERSE DIRECTION (#565): every normative section must be cited by at
  # least one contract, so a section cannot make a claim no executable evidence
  # certifies. This is not paperwork: the first run of this check found ALS-T4
  # adjudicating `chunk/windows(n <= 0)` while BOTH targets diverged from it
  # (raw native panic / wasm silently returning len+1 empty windows) — an
  # uncited section is exactly where a spec↔implementation divergence hides.
  n_orphan=0
  for sec in $(grep -hoE '^## ALS-[A-Z0-9]+' "$ALS_DIR"/*.md | sed 's/^## //' | sort -u); do
    if ! has "$sec" "$specd"; then
      echo "::error::ALS section '$sec' is cited by NO contract — every normative section needs >=1 [[contract]] with spec = \"$sec\" (see #565)"
      n_orphan=$((n_orphan + 1))
      fail=1
    fi
  done
  [ "$n_orphan" -eq 0 ] && echo "spec-coverage: every normative ALS section is cited by >=1 contract."
fi

# ── (g) README claims block (#766) ───────────────────────────────────────────
# The equivalence-claim numbers and the exceptions clause quoted in README.md
# are DERIVED from this ledger by scripts/gen-claims.sh. A stale or hand-edited
# block is a public claim drifting from what the gates verify — red.
bash scripts/gen-claims.sh --check || fail=1

# ── (i) ALS CONFORMANCE REPORT freshness (F1, #811) ──────────────────────────
# docs/contracts/conformance.md joins section → contracts → executable fixtures
# so an auditor reads the F1 claim on one page. Derived from the ledger; a stale
# copy silently misstates what is exercised — the same failure mode as (h).
if ! bash docs/contracts/generate-conformance.sh 2>/dev/null | diff -q - docs/contracts/conformance.md >/dev/null; then
  err "docs/contracts/conformance.md is stale — run: bash docs/contracts/generate-conformance.sh > docs/contracts/conformance.md"
fi

# ── (h) contract INDEX freshness ─────────────────────────────────────────────
# docs/contracts/README.md is generated from this ledger too, and CI's
# "Emit & Format" job regenerates and diffs it. It was NOT checked here, so a
# ledger commit that skipped the regen passed every local gate and the
# pre-commit hook, then went red on CI — eight contracts drifted that way. The
# local gate now covers exactly what CI covers, so the hook catches it first.
# The generator writes to STDOUT (`… > docs/contracts/README.md`), which is the
# other half of how the drift happened: redirecting it to /dev/null "runs" it
# while updating nothing.
if ! bash docs/contracts/generate-readme.sh 2>/dev/null | diff -q - docs/contracts/README.md >/dev/null; then
  err "docs/contracts/README.md is stale — run: bash docs/contracts/generate-readme.sh > docs/contracts/README.md"
fi

if [ "$fail" -ne 0 ]; then
  echo "::error::contract-ledger gate FAILED — see messages above."
  exit 1
fi
echo "contract-ledger: OK — $n_contracts contracts (active=$n_active, flagged=$n_flagged / ceiling $MAX_FLAGGED)."
echo "  fixtures: $n_with_header/$n_fixtures carry a // @contract: header; bidirectional links symmetric."

# ── MUTATION-TESTABILITY (each flips green->red on a one-line edit) ──────────
#   (1) delete a fixture path from a contract's evidence  -> (d) only_rev fires.
#   (2) remove a `// @contract:` line from a fixture       -> (c) "no header".
#   (3) downgrade an active contract's only evidence to by-construction -> (b).
#   (4) typo a class                                       -> (e) bad-class.
#   (5) flag any contract                                  -> (f) ratchet.
#   (6) renumber a contract to leave a gap                 -> (f) coverage.
#   (7) hand-edit a number inside README's claims markers  -> (g) stale-claims.
#   (8) add a contract without regenerating the index      -> (h) stale-index.
#   (9) cite a new section without regenerating conformance -> (i) stale-report.
#  (10) delete a `since = ` line from any contract         -> (e) missing-required.
#  (11) point a fixture header back at emit_wasm/rt_*.rs   -> (j) dead-path.
#  (12) unaligned bogus spec key (`spec = "ALS-BOGUS"`, single space)  -> the
#       spec-existence loop fires (#989: the aligned-only grep let it pass).
# (10) and (11) were verified by hand against C-067 and spec/wasm_cross/
# float_parse.almd: each flips the gate red alone and green again on restore.
# (12) verified 2026-07-30: a single-space bogus key turns the gate red.
