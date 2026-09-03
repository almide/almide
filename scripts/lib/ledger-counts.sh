#!/usr/bin/env bash
# ledger-counts.sh — the STAMPED aggregate counts the generated docs quote, and
# the marked block every generator wraps them in. Sourced, not run.
#
# Every fixture or contract PR used to regenerate the same "N fixtures" /
# "N contracts" lines (README claims + stats, proofs/STAGE-STATUS.md, the
# contract index, the conformance report), so any two such PRs conflicted at
# the merge queue on lines neither of them was about. The per-row content of
# those ledgers (sorted manifest rows, per-contract rows, fixture lists) merges
# cleanly; only the totals collided.
#
# So the totals are no longer derived at render time. They are RECORDED in
# proofs/ledger-counts.toml with the date they were measured, and every
# generator renders that record inside a
#   <!-- counts:generated:start (as of YYYY-MM-DD) … --> … <!-- counts:generated:end -->
# block (same discipline as the wasm-size baseline: measuring and publishing
# are separate acts). A default generator run re-emits the block byte for byte;
# only `bash scripts/gen-ledger-counts.sh` (or a generator's `--counts` flag)
# re-measures and restamps. Drift between the record and the tree is caught by
# scripts/check-ledger-counts.sh — nightly, and as a release step before the
# seal — and red there means "refresh", the fuzz-night ethos: a finding to act
# on, never a PR gate that makes fixture PRs touch the totals again.
#
# API (every function assumes the repo root as cwd):
#   counts_measure          print every count as `key = value`, freshly measured
#   counts_stamp            re-measure and rewrite the ledger with today's date
#   counts_get KEY          the recorded value (exit 2 when the ledger lacks it)
#   counts_date             the recorded measurement date
#   counts_start / counts_end   the block markers (start carries the date)
#   counts_render_claims / _stats / _stages / _index / _conformance
#                           each doc's block, markers included, from the record
#   counts_extract FILE [N] the N-th block as committed in FILE (markers included)

COUNTS_LEDGER="proofs/ledger-counts.toml"
COUNTS_NOTE="stamped totals from proofs/ledger-counts.toml; refreshed only by scripts/gen-ledger-counts.sh, never by a fixture/contract PR; DO NOT EDIT between the markers"

# ── measurement ──────────────────────────────────────────────────────────────
# Each count keeps the exact measurement its generator used before the totals
# were stamped, so the recorded values are the ones the docs always carried.

# The contract ledger walk shared by gen-claims.sh and gen-readme-stats.sh:
# line-start anchors skip schema comments, the ''' toggle skips multi-line
# statements. Prints "total active flagged".
counts_measure_contracts() {
  awk '
    function flush() {
      if (id == "") return
      total++
      if (status == "active") active++; else nflag++
      id = ""; status = ""
    }
    /'"'"''"'"''"'"'/ { in_stmt = !in_stmt; next }
    in_stmt { next }
    /^\[\[contract\]\]/ { flush(); next }
    /^id[ \t]*=/     { v = $0; sub(/^id[ \t]*=[ \t]*"/, "", v); sub(/".*$/, "", v); id = v; next }
    /^status[ \t]*=/ { v = $0; sub(/^status[ \t]*=[ \t]*"/, "", v); sub(/".*$/, "", v); status = v; next }
    END { flush(); printf "%d %d %d\n", total, active, nflag }
  ' docs/contracts/contracts.toml
}

counts_measure() {
  local total active flagged sections fixtures streak
  read -r total active flagged <<< "$(counts_measure_contracts)"
  # The conformance report's own join (one code path — its --measure mode).
  read -r sections fixtures <<< "$(bash docs/contracts/generate-conformance.sh --measure)"
  # The streak is a dated meter maintained by fuzz-green-streak.sh — quote its
  # last recorded row (a ledger value, not a re-derivation).
  streak=$(grep -E '^\| [0-9]{4}-' research/benchmark/fuzz-green/README.md | tail -1 | awk -F'|' '{gsub(/ /,"",$3); print $3}')
  printf '%-26s = %s\n' \
    contracts                "$total" \
    contracts_active         "$active" \
    contracts_flagged        "$flagged" \
    contracts_spec_keyed     "$(awk '/'"'"''"'"''"'"'/ { in_stmt = !in_stmt; next } in_stmt { next } /^\[\[contract\]\]/ { if (has) n++; has = 0; next } /^spec[ \t]*=/ { has = 1 } END { if (has) n++; print n + 0 }' docs/contracts/contracts.toml)" \
    wasm_cross_fixtures      "$(ls spec/wasm_cross/*.almd | wc -l | tr -d ' ')" \
    interp_abstains          "$(grep -vc '^#\|^$' crates/almide-interp/interp-abstain-ledger.txt)" \
    conformance_sections     "$sections" \
    conformance_fixtures     "$fixtures" \
    stdlib_functions         "$(grep -h '^## Signature index (' docs/stdlib/*.md | grep -oE '[0-9]+' | awk '{ s += $1 } END { print s + 0 }')" \
    stdlib_modules           "$(grep -l '^## Signature index (' docs/stdlib/*.md | wc -l | tr -d ' ')" \
    spec_test_files          "$(grep -rlE '^[[:space:]]*test "' spec --include='*.almd' | wc -l | tr -d ' ')" \
    scalar_read_arms         "$(grep -c '^\[\[arm\]\]' proofs/scalar-read-audit.toml)" \
    scalar_read_unguarded    "$(grep -c 'class = "UNGUARDED"' proofs/scalar-read-audit.toml || true)" \
    wat_prelude_fns          "$(grep -c '^\[\[fn\]\]' proofs/wat-prelude-audit.toml)" \
    libm_sites               "$(grep -c '^\[\[site\]\]' proofs/libm-determinism-audit.toml)" \
    als_elements             "$(grep -c '^\[\[element\]\]' proofs/als-element-coverage.toml)" \
    als_elements_unwritten   "$(grep -c 'section = "UNWRITTEN"' proofs/als-element-coverage.toml || true)" \
    release_seals            "$(ls proofs/releases/v*.toml 2>/dev/null | wc -l | tr -d ' ')" \
    verification_gates       "$(grep -c '^\[\[gate\]\]' proofs/gate-verification.toml)" \
    verification_unverified  "$(grep -c 'class = "UNVERIFIED"' proofs/gate-verification.toml || true)" \
    tor_rows                 "$(grep -c '^\*\*TOR-' proofs/TOR.md)" \
    fuzz_green_streak_days   "$streak"
}

counts_stamp() {
  local body
  body="$(counts_measure)"
  {
    cat <<'EOF'
# LEDGER COUNTS — the aggregate totals the generated docs quote, STAMPED.
#
# Rendered verbatim (inside a dated `counts:generated` block) into README.md,
# proofs/STAGE-STATUS.md, docs/contracts/README.md and
# docs/contracts/conformance.md by their generators, which never re-measure
# them: deriving the totals at render time made every fixture or contract PR
# rewrite the same lines, so any two such PRs conflicted at the merge queue.
#
# Refresh:  bash scripts/gen-ledger-counts.sh   (restamps this file and
#           regenerates the four docs — a release step before the seal, or
#           whenever the nightly scripts/check-ledger-counts.sh reports drift).
# A fixture or contract PR must NOT refresh it. Measurement recipes:
# scripts/lib/ledger-counts.sh.
EOF
    printf '%-26s = "%s"\n' date "$(date -u +%F)"
    printf '%s\n' "$body"
  } > "$COUNTS_LEDGER"
}

# ── the record ───────────────────────────────────────────────────────────────
counts_get() {
  local v
  [ -f "$COUNTS_LEDGER" ] || { echo "::error::$COUNTS_LEDGER not found — run: bash scripts/gen-ledger-counts.sh" >&2; exit 2; }
  v="$(grep -E "^$1[[:space:]]*=" "$COUNTS_LEDGER" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//; s/^"//; s/"[[:space:]]*$//')"
  [ -n "$v" ] || { echo "::error::$COUNTS_LEDGER has no '$1' — run: bash scripts/gen-ledger-counts.sh" >&2; exit 2; }
  printf '%s\n' "$v"
}
counts_date() { counts_get date; }

counts_start() { printf '<!-- counts:generated:start (as of %s) — %s -->\n' "$(counts_date)" "$COUNTS_NOTE"; }
counts_end()   { printf '<!-- counts:generated:end -->\n'; }

# ── the rendered blocks ──────────────────────────────────────────────────────
# README claims block, first paragraph. The markers sit INSIDE the blockquote
# (`> <!-- … -->`): a bare comment line between two `>` lines would split the
# quote in two, and the point is that the page does not change.
counts_render_claims() {
  printf '> %s\n' "$(counts_start)"
  printf '> **Ledger: %s contracts — %s active, %s flagged-for-revision.**\n' \
    "$(counts_get contracts)" "$(counts_get contracts_active)" "$(counts_get contracts_flagged)"
  printf '> %s\n' "$(counts_end)"
}

# README stats block, the derived-count table (every cell is a total).
counts_render_stats() {
  counts_start
  cat <<EOF
| Derived count | Value |
|---|---|
| Stdlib | $(counts_get stdlib_functions) functions across $(counts_get stdlib_modules) modules — self-hosted \`.almd\`, signature indexes regenerated from the compiler by \`tools/gen-stdlib-doc-index.py\` |
| Tests | $(counts_get spec_test_files) \`.almd\` test files under \`spec/\` (\`almide test spec/\`) + the $(counts_get contracts)-contract cross-target ledger |
EOF
  counts_end
}

# proofs/STAGE-STATUS.md — every number in the stage block is a total.
counts_render_stages() {
  local corpus voting els unwritten_els
  corpus="$(counts_get wasm_cross_fixtures)"
  voting=$((corpus - $(counts_get interp_abstains)))
  els="$(counts_get als_elements)"; unwritten_els="$(counts_get als_elements_unwritten)"
  counts_start
  printf '> **Stage 1 (accept-and-wrong extinction): audits COMPLETE and gated** —\n'
  printf '> scalar-read %s arms / %s UNGUARDED; WAT prelude %s fns classified;\n' \
    "$(counts_get scalar_read_arms)" "$(counts_get scalar_read_unguarded)" "$(counts_get wat_prelude_fns)"
  printf '> platform-libm %s sites classified. New entries cannot land unclassified.\n' "$(counts_get libm_sites)"
  printf '>\n'
  printf '> **Stage 2 (translation validation): %s/%s fixtures cast a real 3-way vote (%s%%)** —\n' "$voting" "$corpus" "$((voting * 100 / corpus))"
  printf '> the abstain remainder is classified and shrink-only (the interp-heap arc, #1226).\n'
  printf '>\n'
  printf '> **Stage 3 (semantics freeze): %s/%s contracts spec-keyed; syntax-element coverage\n' \
    "$(counts_get contracts_spec_keyed)" "$(counts_get contracts)"
  printf '> %s/%s sectioned (%s UNWRITTEN, shrink-only — the freeze precondition is 0).**\n' "$((els - unwritten_els))" "$els" "$unwritten_els"
  printf '>\n'
  printf '> **Stage 4 (durability): fuzz true-green streak = %s day(s)** (dated meter;\n' "$(counts_get fuzz_green_streak_days)"
  printf '> the correctness-only night verdict shipped 2026-08-12 — 90 days is the milestone).\n'
  printf '>\n'
  printf '> **Stage 5 (auditability): %s release seal(s); %s verification gates classified\n' \
    "$(counts_get release_seals)" "$(counts_get verification_gates)"
  printf '> (%s UNVERIFIED under a shrink-only ceiling); TOR with %s enforced rows;\n' \
    "$(counts_get verification_unverified)" "$(counts_get tor_rows)"
  printf '> gap analysis consolidated in proofs/DO330-GAP.md (reference-gated).**\n'
  counts_end
}

# docs/contracts/README.md — the "N contracts" line above the index table.
counts_render_index() {
  counts_start
  printf '%s contracts\n' "$(counts_get contracts)"
  counts_end
}

# docs/contracts/conformance.md — the totals line above the section table.
counts_render_conformance() {
  counts_start
  printf '%s normative sections; %s distinct executable fixtures.\n' \
    "$(counts_get conformance_sections)" "$(counts_get conformance_fixtures)"
  counts_end
}

# The N-th block (default: the first) as committed in a doc: from the start
# marker through the end marker, markers included (a `> ` blockquote prefix is
# part of the line). README.md carries two: the claims line, then the stats table.
counts_extract() {
  awk -v want="${2:-1}" '
    /<!-- counts:generated:start \(as of / { n++; if (n == want) on = 1 }
    on { print }
    /<!-- counts:generated:end -->/ { if (on) exit }
  ' "$1"
}
