#!/usr/bin/env bash
# RELEASE EVIDENCE SEALS — the per-release audit-freeze the mission-critical
# goal calls for ("リリースごとに証拠を不変固定するアーカイブ").
#
# A seal (proofs/releases/vX.Y.Z.toml) binds a release TAG to the measured
# state of its assurance evidence: the contract ledger's digest and counts,
# the interp abstain ledger, the cross-target fixture corpus, and the pinned
# toolchain — plus hand-recorded facts (the release-gate fuzz run) an auditor
# can follow while the CI artifacts live.
#
# IMMUTABILITY MODEL: git tags are the immutability root. Every `[derived]`
# field is RE-MEASURED from the tag's tree (`git show TAG:path`) by `check`
# using the SAME functions `gen` used to write it (one instrument, the #1176
# rule — the two cannot drift). Editing a seal to disagree with its tag fails
# CI; the only way to "change" a seal is to change the tag, which git forbids
# quietly and the release workflow never does. `[recorded]` fields are not
# re-derivable (run URLs, asset counts) — check enforces presence, not value.
#
# Usage:
#   scripts/release-seal.sh gen vX.Y.Z    # measure the tag, write the seal
#   scripts/release-seal.sh check         # verify every seal (CI gate)
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SEALS_DIR="$ROOT/proofs/releases"

# ── shared measurement functions (gen and check both call EXACTLY these) ──

tag_file() { git -C "$ROOT" show "$1:$2" 2>/dev/null; }

m_cargo_version()   { tag_file "$1" Cargo.toml | sed -n 's/^version = "\(.*\)"/\1/p' | head -1; }
m_contracts_sha()   { tag_file "$1" docs/contracts/contracts.toml | shasum -a 256 | cut -d' ' -f1; }
m_contracts_total() { tag_file "$1" docs/contracts/contracts.toml | grep -c '^\[\[contract\]\]'; }
m_contracts_flagged() {
  # `status` LINES only — the phrase also appears in header docs and contract
  # prose (3 hits at v0.57.0 with 0 actual flagged rows), matching
  # check-contracts.sh's semantic.
  tag_file "$1" docs/contracts/contracts.toml | grep -c '^status.*"flagged-for-revision"' || true
}
m_abstain_sha()     { tag_file "$1" crates/almide-interp/interp-abstain-ledger.txt | shasum -a 256 | cut -d' ' -f1; }
m_abstain_entries() { tag_file "$1" crates/almide-interp/interp-abstain-ledger.txt | grep -vc '^#\|^$'; }
m_fixture_count()   { git -C "$ROOT" ls-tree "$1" spec/wasm_cross/ | grep -c '\.almd$'; }
m_rust_toolchain()  { tag_file "$1" .github/workflows/ci.yml | sed -n 's/.*RUST_TOOLCHAIN: "\(.*\)".*/\1/p' | head -1; }
m_wasmtime() {
  tag_file "$1" .github/workflows/ci.yml | sed -n 's/.*VERSION=\(v[0-9.]*\).*/\1/p' | head -1
}

# Ensure the tag is present (CI clones are shallow and tagless).
ensure_tag() {
  local tag="$1"
  if git -C "$ROOT" rev-parse -q --verify "$tag^{commit}" >/dev/null 2>&1; then
    return 0
  fi
  git -C "$ROOT" fetch --quiet --depth=1 origin "refs/tags/$tag:refs/tags/$tag" 2>/dev/null || true
  git -C "$ROOT" rev-parse -q --verify "$tag^{commit}" >/dev/null 2>&1
}

derived_fields() { # tag -> "key value" lines, one per derived field
  local t="$1"
  echo "cargo_version $(m_cargo_version "$t")"
  echo "contracts_sha256 $(m_contracts_sha "$t")"
  echo "contracts_total $(m_contracts_total "$t")"
  echo "contracts_flagged $(m_contracts_flagged "$t")"
  echo "abstain_ledger_sha256 $(m_abstain_sha "$t")"
  echo "abstain_ledger_entries $(m_abstain_entries "$t")"
  echo "wasm_cross_fixtures $(m_fixture_count "$t")"
  echo "rust_toolchain $(m_rust_toolchain "$t")"
  echo "wasmtime $(m_wasmtime "$t")"
}

cmd="${1:?usage: release-seal.sh gen vX.Y.Z | check}"

if [ "$cmd" = "gen" ]; then
  tag="${2:?gen needs a tag (vX.Y.Z)}"
  ensure_tag "$tag" || { echo "release-seal: tag $tag not found" >&2; exit 1; }
  version="${tag#v}"
  commit=$(git -C "$ROOT" rev-parse "$tag^{commit}")
  # Committer date of the tagged commit — deterministic, not "today".
  date=$(git -C "$ROOT" show -s --format=%cs "$tag^{commit}")
  mkdir -p "$SEALS_DIR"
  out="$SEALS_DIR/$tag.toml"
  {
    echo "# Release evidence seal — $tag. IMMUTABLE: every [derived] field is"
    echo "# re-measured from the tag's tree by \`scripts/release-seal.sh check\`;"
    echo "# an edit that disagrees with the tag fails CI. [recorded] fields are"
    echo "# facts the tag cannot re-derive (fill them in before committing)."
    echo ""
    echo "[release]"
    echo "version = \"$version\""
    echo "tag = \"$tag\""
    echo "tag_commit = \"$commit\""
    echo "date = \"$date\""
    echo "url = \"https://github.com/almide/almide/releases/tag/$tag\""
    echo ""
    echo "[derived]"
    derived_fields "$tag" | while read -r k v; do echo "$k = \"$v\""; done
    echo ""
    echo "[recorded]"
    echo "release_gate = \"FILL: the gate evidence (fuzz run id, shards, findings disposition)\""
    echo "assets = \"FILL: released asset inventory\""
    echo "known_problems = \"FILL: the release's known-problem ledger (issue refs + dispositions), or 'none'\""
  } > "$out"
  echo "release-seal: wrote $out — fill the [recorded] fields, then commit"
  exit 0
fi

if [ "$cmd" != "check" ]; then
  echo "release-seal: unknown command $cmd" >&2
  exit 2
fi

# ── check: every seal re-measured against its tag ──
shopt -s nullglob
seals=("$SEALS_DIR"/v*.toml)
if [ ${#seals[@]} -eq 0 ]; then
  echo "release-seal: no seals to check"
  exit 0
fi
fail=0
for seal in "${seals[@]}"; do
  tag=$(basename "$seal" .toml)
  if ! ensure_tag "$tag"; then
    if [ "${CI:-}" = "true" ]; then
      echo "::error::release-seal: tag $tag (for $(basename "$seal")) is unfetchable — a seal without its tag is unverifiable" >&2
      fail=1
    else
      echo "release-seal: SKIP $tag (tag not available locally; CI enforces)"
    fi
    continue
  fi
  # Parse the seal's flat `key = "value"` lines.
  declare -A want=()
  while IFS= read -r line; do
    if [[ "$line" =~ ^([a-z0-9_]+)\ =\ \"(.*)\"$ ]]; then
      want["${BASH_REMATCH[1]}"]="${BASH_REMATCH[2]}"
    fi
  done < "$seal"
  # Structural: version/tag agreement + recorded fields actually filled.
  [ "${want[tag]:-}" = "$tag" ] || { echo "  $tag: seal names tag '${want[tag]:-}'" >&2; fail=1; }
  [ "${want[version]:-}" = "${tag#v}" ] || { echo "  $tag: version '${want[version]:-}' != '${tag#v}'" >&2; fail=1; }
  commit=$(git -C "$ROOT" rev-parse "$tag^{commit}")
  [ "${want[tag_commit]:-}" = "$commit" ] || { echo "  $tag: tag_commit '${want[tag_commit]:-}' != measured '$commit'" >&2; fail=1; }
  for rk in release_gate assets known_problems; do
    case "${want[$rk]:-}" in
      ""|FILL:*) echo "  $tag: [recorded] $rk is unfilled — a seal without its gate evidence is not a seal" >&2; fail=1 ;;
    esac
  done
  # Derived: re-measure with the SAME functions gen used.
  while read -r k v; do
    if [ "${want[$k]:-}" != "$v" ]; then
      echo "  $tag: $k sealed as '${want[$k]:-}' but the tag measures '$v'" >&2
      fail=1
    fi
  done < <(derived_fields "$tag")
  unset want
done
if [ $fail -ne 0 ]; then
  echo "RELEASE SEAL CHECK FAILED — a seal disagrees with its tag (or is incomplete)." >&2
  exit 1
fi
echo "release-seal: ${#seals[@]} seal(s) verified against their tags"
