#!/usr/bin/env bash
# gen-claims.sh — regenerate the machine-derived claims block in README.md
# from docs/contracts/contracts.toml (#766).
#
# The README's "Equivalence Claim" section quotes ledger numbers (contract
# count, active/flagged split) and the exceptions clause (the list of
# flagged-for-revision contracts). Those must be DERIVED, never hand-written:
# this script rewrites everything between the claims markers, and
# scripts/check-contracts.sh runs `--check` in CI so a drifted block is red.
#
#   bash scripts/gen-claims.sh          # rewrite README.md in place
#   bash scripts/gen-claims.sh --check  # exit 1 if the block is stale
#
# Pure shell/awk, no deps — same discipline as docs/contracts/generate-readme.sh.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 2

LEDGER="docs/contracts/contracts.toml"
README="README.md"
START="<!-- claims:generated:start — derived from docs/contracts/contracts.toml by scripts/gen-claims.sh; DO NOT EDIT between the markers -->"
END="<!-- claims:generated:end -->"

[ -f "$LEDGER" ] || { echo "::error::$LEDGER not found (run from repo root)"; exit 2; }
[ -f "$README" ] || { echo "::error::$README not found (run from repo root)"; exit 2; }
grep -qxF "$START" "$README" || { echo "::error::claims start marker missing from $README"; exit 2; }
grep -qxF "$END" "$README"   || { echo "::error::claims end marker missing from $README"; exit 2; }

# Walk the ledger with the same block parser as generate-readme.sh: line-start
# anchors skip schema comments, the ''' toggle skips multi-line statements.
blockfile="$(mktemp)"
trap 'rm -f "$blockfile"' EXIT
awk '
  function flush() {
    if (id == "") return
    total++
    if (status == "active") active++
    else { nflag++; fid[nflag] = id; ftitle[nflag] = title; fdoc[nflag] = doc }
    id = ""; title = ""; status = ""; doc = ""
  }
  /'"'"''"'"''"'"'/ { in_stmt = !in_stmt; next }
  in_stmt { next }
  /^\[\[contract\]\]/ { flush(); next }
  /^id[ \t]*=/     { v = $0; sub(/^id[ \t]*=[ \t]*"/, "", v); sub(/".*$/, "", v); id = v; next }
  /^title[ \t]*=/  { v = $0; sub(/^title[ \t]*=[ \t]*"/, "", v); sub(/".*$/, "", v); title = v; next }
  /^status[ \t]*=/ { v = $0; sub(/^status[ \t]*=[ \t]*"/, "", v); sub(/".*$/, "", v); status = v; next }
  /^doc[ \t]*=/    { v = $0; sub(/^doc[ \t]*=[ \t]*"/, "", v); sub(/".*$/, "", v); doc = v; next }
  END {
    flush()
    printf "> **Ledger: %d contracts — %d active, %d flagged-for-revision.**\n", total, active, nflag
    print ">"
    if (nflag == 0) {
      # "Divergences awaiting a fix", not "Exceptions" — this block sits directly
      # under the equivalence claim, and the ratchet counts contracts flagged for
      # revision, NOT carve-outs in the law itself. The one by-design carve-out
      # (platform-reporting env.os / env.temp_dir, C-189) is active, not flagged,
      # so a bare "Exceptions: none" read as covering it would be false.
      # NOTE: this awk program is single-quoted — no apostrophes in these strings.
      print "> **Divergences awaiting a fix: none.** Every contract in the ledger is"
      print "> `active`, carrying executable evidence of class >= `fixture`. The one"
      print "> by-design carve-out in the law — the platform-reporting fns `env.os`"
      print "> and `env.temp_dir` — is bounded by C-189."
    } else {
      printf "> **Divergences awaiting a fix (%d)** — contracts flagged for revision; the ratchet says this list may only shrink:\n", nflag
      print ">"
      for (k = 1; k <= nflag; k++) {
        link = (fdoc[k] != "") ? "docs/contracts/" fdoc[k] : "docs/contracts/contracts.toml"
        printf "> - [%s — %s](%s)\n", fid[k], ftitle[k], link
      }
    }
  }
' "$LEDGER" > "$blockfile"

rendered="$(awk -v start="$START" -v end="$END" -v bf="$blockfile" '
  $0 == start { print; while ((getline line < bf) > 0) print line; skip = 1; next }
  $0 == end   { skip = 0; print; next }
  !skip { print }
' "$README")"

# ── TCB-numbers block in docs/TRUST-SPINE.md (#914) ─────────────────────────
# The checker size and theorem counts are DERIVED here, never hand-written:
# "a few hundred lines" drifted 1.7x past the real checker before anyone
# noticed, and the README said 22 Lean theorems when the belt held 41. Same
# marker discipline as the README claims block; --check makes drift red.
SPINE="docs/TRUST-SPINE.md"
TSTART="<!-- tcb:generated:start — derived by scripts/gen-claims.sh; DO NOT EDIT between the markers -->"
TEND="<!-- tcb:generated:end -->"
tcb_block() {
  local ml mli coqn leann
  # proofs/checker.ml is EXTRACTED (build-checker.sh), not committed — the light
  # CI jobs run --check on a fresh checkout where it does not exist. When absent,
  # carry the RECORDED numbers forward (they re-derive on any machine that has
  # run the extraction, e.g. make verify-trust); the theorem counts always
  # re-derive, since the .v/.lean sources are committed.
  if [ -f proofs/checker.ml ]; then
    ml=$(wc -l < proofs/checker.ml | tr -d ' ')
    mli=$(wc -l < proofs/checker.mli | tr -d ' ')
  else
    # Carrying the recorded number forward is SELF-FULFILLING for the checker
    # size: the value is read out of the document --check then verifies. Jobs
    # that have the extracted checker (trust-spine, after make verify-trust)
    # set GEN_CLAIMS_REQUIRE_MEASURED=1 to forbid this branch entirely (#989);
    # the light checks-job keeps the fallback for the theorem counts, which
    # always re-derive from committed sources.
    if [ "${GEN_CLAIMS_REQUIRE_MEASURED:-}" = "1" ]; then
      # >&2: tcb_block's stdout is redirected into the block file, so an error
      # on stdout would vanish into the temp file instead of the log.
      echo "::error::tcb block: proofs/checker.ml absent but this job requires the MEASURED size (run make verify-trust first) — the carried-forward fallback is self-fulfilling (#989)" >&2
      exit 2
    fi
    ml=$(grep -oE '`proofs/checker\.ml` = \*\*[0-9]+ lines\*\*' "$SPINE" | grep -oE '[0-9]+' | head -1)
    mli=$(grep -oE '\(\+ [0-9]+' "$SPINE" | grep -oE '[0-9]+' | head -1)
    [ -n "$ml" ] && [ -n "$mli" ] || { echo "::error::tcb block: proofs/checker.ml absent and no recorded size to carry forward" >&2; exit 2; }
  fi
  coqn=$(grep -hcE '^(Theorem|Lemma) ' proofs/*.v | paste -sd+ - | bc)
  leann=$(grep -hc '^theorem' crates/almide-perceus-belt/AlmidePerceusBelt/*.lean | paste -sd+ - | bc)
  printf '> **Measured, regenerated:** extracted checker `proofs/checker.ml` = **%s lines** (+ %s\n' "$ml" "$mli"
  printf '> `.mli`); Rocq spine = **%s theorems+lemmas** (axiom-clean, asserted by `proofs/check.sh`);\n' "$coqn"
  printf '> Lean Perceus belt = **%s theorems**, 0 sorry (CI-gated).\n' "$leann"
}
if grep -qxF "$TSTART" "$SPINE"; then
  tblockfile="$(mktemp)"
  tcb_block > "$tblockfile"
  spine_rendered="$(awk -v start="$TSTART" -v end="$TEND" -v bf="$tblockfile" '
    $0 == start { print; while ((getline line < bf) > 0) print line; skip = 1; next }
    $0 == end   { skip = 0; print; next }
    !skip { print }
  ' "$SPINE")"
else
  echo "::error::tcb markers missing from $SPINE"; exit 2
fi

# ── Stage-status block in proofs/STAGE-STATUS.md ────────────────────────────
# The five-stage adoption roadmap's single checkable status artifact: every
# number measured from committed ledgers (no session prose). Same marker
# discipline; --check makes drift red.
STAGES="proofs/STAGE-STATUS.md"
SSTART="<!-- stages:generated:start — derived from the proofs/ ledgers by scripts/gen-claims.sh; DO NOT EDIT between the markers -->"
SEND="<!-- stages:generated:end -->"
stage_block() {
  local arms unguarded watfns libms corpus abstains voting contracts keyed
  local seals gates unverified torrows streak
  arms=$(grep -c '^\[\[arm\]\]' proofs/scalar-read-audit.toml)
  unguarded=$(grep -c 'class = "UNGUARDED"' proofs/scalar-read-audit.toml || true)
  watfns=$(grep -c '^\[\[fn\]\]' proofs/wat-prelude-audit.toml)
  libms=$(grep -c '^\[\[site\]\]' proofs/libm-determinism-audit.toml)
  corpus=$(ls spec/wasm_cross/*.almd | wc -l | tr -d ' ')
  abstains=$(grep -vc '^#\|^$' crates/almide-interp/interp-abstain-ledger.txt)
  voting=$((corpus - abstains))
  contracts=$(grep -c '^\[\[contract\]\]' docs/contracts/contracts.toml)
  keyed=$(grep -c '^spec ' docs/contracts/contracts.toml)
  seals=$(ls proofs/releases/v*.toml 2>/dev/null | wc -l | tr -d ' ')
  gates=$(grep -c '^\[\[gate\]\]' proofs/gate-verification.toml)
  unverified=$(grep -c 'class = "UNVERIFIED"' proofs/gate-verification.toml || true)
  torrows=$(grep -c '^\*\*TOR-' proofs/TOR.md)
  # The streak is a dated meter maintained by fuzz-green-streak.sh — quote its
  # last recorded row (a ledger value, not a re-derivation).
  streak=$(grep -E '^\| [0-9]{4}-' research/benchmark/fuzz-green/README.md | tail -1 | awk -F'|' '{gsub(/ /,"",$3); print $3}')
  printf '> **Stage 1 (accept-and-wrong extinction): audits COMPLETE and gated** —\n'
  printf '> scalar-read %s arms / %s UNGUARDED; WAT prelude %s fns classified;\n' "$arms" "$unguarded" "$watfns"
  printf '> platform-libm %s sites classified. New entries cannot land unclassified.\n' "$libms"
  printf '>\n'
  printf '> **Stage 2 (translation validation): %s/%s fixtures cast a real 3-way vote (%s%%)** —\n' "$voting" "$corpus" "$((voting * 100 / corpus))"
  printf '> the abstain remainder is classified and shrink-only (the interp-heap arc, #1226).\n'
  printf '>\n'
  printf '> **Stage 3 (semantics freeze): %s/%s contracts spec-keyed, both directions gated** —\n' "$keyed" "$contracts"
  printf '> the ALS authoring sweep (syntax-element coverage) is the remaining half.\n'
  printf '>\n'
  printf '> **Stage 4 (durability): fuzz true-green streak = %s day(s)** (dated meter;\n' "$streak"
  printf '> the correctness-only night verdict shipped 2026-08-12 — 90 days is the milestone).\n'
  printf '>\n'
  printf '> **Stage 5 (auditability): %s release seal(s); %s verification gates classified\n' "$seals" "$gates"
  printf '> (%s UNVERIFIED under a shrink-only ceiling); TOR with %s enforced rows.**\n' "$unverified" "$torrows"
}
if grep -qxF "$SSTART" "$STAGES"; then
  sblockfile="$(mktemp)"
  stage_block > "$sblockfile"
  stages_rendered="$(awk -v start="$SSTART" -v end="$SEND" -v bf="$sblockfile" '
    $0 == start { print; while ((getline line < bf) > 0) print line; skip = 1; next }
    $0 == end   { skip = 0; print; next }
    !skip { print }
  ' "$STAGES")"
else
  echo "::error::stage markers missing from $STAGES"; exit 2
fi

if [ "${1:-}" = "--check" ]; then
  if [ "$stages_rendered" != "$(cat "$STAGES")" ]; then
    echo "::error::proofs/STAGE-STATUS.md stage block is stale — run: bash scripts/gen-claims.sh"
    exit 1
  fi
  if [ "$rendered" != "$(cat "$README")" ]; then
    echo "::error::README.md claims block is stale — run: bash scripts/gen-claims.sh"
    exit 1
  fi
  if [ "$spine_rendered" != "$(cat "$SPINE")" ]; then
    echo "::error::docs/TRUST-SPINE.md tcb block is stale — run: bash scripts/gen-claims.sh"
    exit 1
  fi
  exit 0
fi

printf '%s\n' "$rendered" > "$README"
printf '%s\n' "$spine_rendered" > "$SPINE"
printf '%s\n' "$stages_rendered" > "$STAGES"
