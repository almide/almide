#!/usr/bin/env bash
# ACTION PIN GATE (#4094) — every `uses:` of a third-party action names a commit.
#
# A tag or a branch can be moved to another commit by whoever controls the
# action's repository. In the tj-actions/changed-files compromise
# (CVE-2025-30066) the tags were moved to a commit that dumped CI secrets, and
# every workflow pinned by tag ran it. A 40-hex commit cannot be moved.
#
# Rule: in .github/workflows/*.yml and .github/actions/**/action.yml, every
# `uses: owner/repo[/path]@ref` has a 40-hex `ref`. Local actions
# (`uses: ./...`) and `docker://` references are exempt. The version the commit
# came from goes in a trailing comment (`# v7.0.1`); Dependabot's
# github-actions updates keep the commit and the comment in step.
#
# Usage: scripts/check-action-pins.sh [root]   (default: the repository root)
#        scripts/check-action-pins.sh --self-test
#   --self-test runs the gate on a forged tree with one tag pin and one branch
#   pin and requires it to fail on both, then on the fixed tree and requires it
#   to pass — the negative control.
set -euo pipefail
export LC_ALL=C

scan() {
  local root="$1" bad=0 n=0
  local files
  files=$(find "$root/.github/workflows" "$root/.github/actions" \
            \( -name '*.yml' -o -name '*.yaml' \) -type f 2>/dev/null | sort)
  [ -n "$files" ] || { echo "check-action-pins: no workflow files under $root/.github" >&2; return 2; }
  while IFS= read -r f; do
    # line number, then the reference after `uses:` (quotes stripped)
    while IFS=: read -r ln ref; do
      ref="${ref#"${ref%%[![:space:]]*}"}"
      ref="${ref%%#*}"; ref="${ref%"${ref##*[![:space:]]}"}"
      ref="${ref#[\"\']}"; ref="${ref%[\"\']}"
      case "$ref" in ./*|docker://*|"") continue ;; esac
      n=$((n + 1))
      if ! [[ "$ref" =~ @[0-9a-f]{40}$ ]]; then
        echo "UNPINNED ${f#"$root"/}:$ln  uses: $ref" >&2
        bad=$((bad + 1))
      fi
    done < <(/usr/bin/grep -nE '^[[:space:]-]*uses:' "$f" | sed -E 's/^([0-9]+):[[:space:]-]*uses:/\1:/')
  done <<< "$files"
  if [ "$bad" -gt 0 ]; then
    echo "check-action-pins: $bad of $n action references are not pinned to a commit." >&2
    echo "  fix: replace @<tag> with @<40-hex commit> # <tag>  (gh api repos/<owner>/<repo>/commits/<tag> -q .sha)" >&2
    return 1
  fi
  echo "check-action-pins: OK — $n action references, all pinned to a commit"
}

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
  mkdir -p "$tmp/.github/workflows"
  cat > "$tmp/.github/workflows/forged.yml" <<'EOF'
on: push
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
      - uses: ./.github/actions/local
      - uses: actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0
EOF
  set +e; out=$(scan "$tmp" 2>&1); rc=$?; set -e
  [ "$rc" = 1 ] || { echo "self-test: the gate passed a tree with a tag pin and a branch pin (exit $rc)" >&2; echo "$out" >&2; exit 1; }
  for want in 'actions/checkout@v7' 'dtolnay/rust-toolchain@stable' '2 of 3'; do
    /usr/bin/grep -qF "$want" <<< "$out" || { echo "self-test: the refusal did not name '$want'" >&2; echo "$out" >&2; exit 1; }
  done
  sed -i.bak -e 's|actions/checkout@v7|actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1|' \
             -e 's|dtolnay/rust-toolchain@stable|dtolnay/rust-toolchain@b725eda525362e7cf34af9a7243cac1e3f01a7ec # stable|' \
             "$tmp/.github/workflows/forged.yml"
  rm -f "$tmp/.github/workflows/forged.yml.bak"
  scan "$tmp" >/dev/null || { echo "self-test: the gate refused a fully pinned tree" >&2; exit 1; }
  echo "check-action-pins self-test: OK — refused a tag pin and a branch pin, passed the pinned tree"
  exit 0
fi

scan "${1:-$(git rev-parse --show-toplevel)}"
