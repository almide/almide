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
# A commit pin still runs whatever the commit FETCHES (security row 16), so
# the gate also runs scripts/lib/action_supply.py:
#   versions  (always, offline) a toolchain / runtime / installer step names an
#             exact version — no `dtolnay/rust-toolchain` without `toolchain:`
#             or with `stable`, no `rustup … stable`, no `node-version: 20`, no
#             `cargo install` without `--locked --version`, no unpinned
#             `pip install`; values are resolved through env / inputs first;
#   closure   (`--closure`, network) every pinned action's own action.yml AT
#             ITS COMMIT pins each nested `uses:` and docker image, recursively.
# and scripts/lib/download_pins.py:
#   downloads (always, offline) every curl / wget / Invoke-WebRequest of a binary
#             or archive in .github and scripts/*.sh is checked against a pinned
#             sha256 — scripts/fetch-pinned.sh with a line in
#             .github/pinned-downloads.txt, or a 64-hex literal — or, for a
#             first-party `latest`, `gh attestation verify` of the same repo.
#
# Usage: scripts/check-action-pins.sh [root]   (default: the repository root)
#        scripts/check-action-pins.sh --closure [root]
#        scripts/check-action-pins.sh --self-test
#   --self-test runs the gate on a forged tree with one tag pin and one branch
#   pin, a moving toolchain / runtime / installer of every kind, and a pinned
#   composite whose own `uses:` is a tag (a local mirror stands in for the
#   network), and requires each refusal; then on the fixed trees and requires a
#   pass — the negative control.
set -euo pipefail
export LC_ALL=C

here="$(cd "$(dirname "$0")" && pwd)"
supply() { python3 -I "$here/lib/action_supply.py" "$@"; }
dlpins() { python3 -I "$here/lib/download_pins.py" "$@"; }

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

expect_refusal() { # <label> <output> <rc> <wanted substrings...>
  local label="$1" out="$2" rc="$3"; shift 3
  [ "$rc" = 1 ] || { echo "self-test: $label passed a tree it must refuse (exit $rc)" >&2; echo "$out" >&2; exit 1; }
  local want
  for want in "$@"; do
    /usr/bin/grep -qF -- "$want" <<< "$out" || { echo "self-test: $label did not name '$want'" >&2; echo "$out" >&2; exit 1; }
  done
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
  expect_refusal "the pin rule" "$out" "$rc" 'actions/checkout@v7' 'dtolnay/rust-toolchain@stable' '2 of 3'
  sed -i.bak -e 's|actions/checkout@v7|actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1|' \
             -e 's|dtolnay/rust-toolchain@stable|dtolnay/rust-toolchain@b725eda525362e7cf34af9a7243cac1e3f01a7ec # stable|' \
             "$tmp/.github/workflows/forged.yml"
  rm -f "$tmp/.github/workflows/forged.yml.bak"
  scan "$tmp" >/dev/null || { echo "self-test: the gate refused a fully pinned tree" >&2; exit 1; }

  # Moving names behind a pinned action (row 16): every kind the rule names.
  rm -f "$tmp/.github/workflows/forged.yml"
  mkdir -p "$tmp/.github/actions/leg"
  cat > "$tmp/.github/workflows/moving.yml" <<'EOF'
on: push
env:
  TC: stable
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - uses: dtolnay/rust-toolchain@b725eda525362e7cf34af9a7243cac1e3f01a7ec # stable
      - uses: dtolnay/rust-toolchain@b725eda525362e7cf34af9a7243cac1e3f01a7ec # stable
        with:
          toolchain: ${{ env.TC }}
      - uses: actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1 # v7.1.0
        with:
          node-version: 20
      - uses: ./.github/actions/leg
        with:
          toolchain: nightly
      - run: rustup toolchain install stable --profile minimal
      - run: cargo install cargo-audit --locked
      - run: python3 -m pip install --quiet lark
EOF
  cat > "$tmp/.github/actions/leg/action.yml" <<'EOF'
runs:
  using: composite
  steps:
    - uses: dtolnay/rust-toolchain@b725eda525362e7cf34af9a7243cac1e3f01a7ec # stable
      with:
        toolchain: ${{ inputs.toolchain }}
EOF
  set +e; out=$(supply versions "$tmp" 2>&1); rc=$?; set -e
  expect_refusal "the versions rule" "$out" "$rc" \
    'moving.yml:8  dtolnay/rust-toolchain without `toolchain:`' 'resolves to `stable`' 'resolves to `20`' \
    'resolves to `nightly`' 'rustup names the `stable` channel' 'cargo install without' 'pip install `lark`' '7 step(s)'
  python3 -I - "$tmp/.github/workflows/moving.yml" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
for a, b in [("  TC: stable", '  TC: "1.96.0"'), ("node-version: 20", 'node-version: "20.20.2"'),
             ("toolchain: nightly\n", "toolchain: nightly-2026-08-01\n"), ("install stable", "install 1.96.0"),
             ("--locked\n", "--locked --version 0.22.2\n"), ("lark\n", "lark==1.3.1\n")]:
    assert a in s, a
    s = s.replace(a, b, 1)
first = "      - uses: dtolnay/rust-toolchain@b725eda525362e7cf34af9a7243cac1e3f01a7ec # stable\n"
s = s.replace(first, first + '        with:\n          toolchain: "1.96.0"\n', 1)
open(p, "w").write(s)
PY
  supply versions "$tmp" >/dev/null || { echo "self-test: the versions rule refused the exact tree" >&2; supply versions "$tmp" >&2; exit 1; }

  # A pinned composite whose own `uses:` is a tag, served from a mirror.
  mirror="$tmp/mirror"; inner=1111111111111111111111111111111111111111; leaf=2222222222222222222222222222222222222222
  mkdir -p "$mirror/ex/comp/$inner" "$mirror/ex/node/$leaf"
  printf 'runs:\n  using: node24\n  main: index.js\n' > "$mirror/ex/node/$leaf/action.yml"
  printf 'runs:\n  using: composite\n  steps:\n    - uses: ex/tagged@v1\n    - uses: ./sub\n' > "$mirror/ex/comp/$inner/action.yml"
  rm -f "$tmp/.github/workflows/moving.yml"; rm -rf "$tmp/.github/actions/leg"
  printf 'on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: ex/comp@%s\n' "$inner" > "$tmp/.github/workflows/closure.yml"
  set +e; out=$(ACTION_SUPPLY_FETCH_ROOT="$mirror" supply closure "$tmp" 2>&1); rc=$?; set -e
  expect_refusal "the closure rule" "$out" "$rc" 'runs `uses: ex/tagged@v1`'
  printf 'runs:\n  using: composite\n  steps:\n    - uses: ex/node@%s\n    - uses: ./sub\n' "$leaf" > "$mirror/ex/comp/$inner/action.yml"
  ACTION_SUPPLY_FETCH_ROOT="$mirror" supply closure "$tmp" >/dev/null \
    || { echo "self-test: the closure rule refused a fully pinned closure" >&2; ACTION_SUPPLY_FETCH_ROOT="$mirror" supply closure "$tmp" >&2; exit 1; }

  # Downloads (row 16): a fetched binary or archive must be checked against a
  # pinned sha256 (or, for a first-party `latest`, its own attestation).
  rm -rf "$tmp/.github" "$tmp/mirror"; mkdir -p "$tmp/.github/workflows" "$tmp/.github/actions/leg" "$tmp/scripts"
  cp "$here/fetch-pinned.sh" "$tmp/scripts/fetch-pinned.sh"
  good=https://github.com/ex/tool/releases/download/v1.0.0/tool-v1.0.0-x86_64-linux.tar.xz
  printf '%s  %s\n' "$(printf 'a%.0s' $(seq 64))" "$good" > "$tmp/.github/pinned-downloads.txt"
  cat > "$tmp/.github/workflows/dl.yml" <<'EOF'
on: push
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - name: raw release asset
        run: |
          VERSION=v1.0.0
          curl -fLO "https://github.com/ex/tool/releases/download/${VERSION}/tool-${VERSION}-x86_64-linux.tar.xz"
      - name: installer piped into a shell
        run: curl -fsSL https://ex.example/install | sh
      - name: helper, unlisted version
        run: |
          VERSION=v9.9.9
          bash scripts/fetch-pinned.sh "https://github.com/ex/tool/releases/download/${VERSION}/tool-${VERSION}-x86_64-linux.tar.xz" t.tar.xz
      - name: latest, attested by the wrong repository
        run: |
          curl -fsSL -o a.tar.gz https://github.com/ex/tool/releases/latest/download/a.tar.gz
          gh attestation verify a.tar.gz --repo other/repo
      - uses: ./.github/actions/leg
        with:
          v: v2.0.0
      - name: not downloads
        run: |
          # curl -fLO https://github.com/ex/tool/releases/download/v1/x.tar.gz
          curl --fail-with-body -X POST https://api.example/hook -d '{}'
          curl -fsS -m 2 -H 'Metadata-Flavor: Google' http://metadata.google.internal/x
          curl -fsSL -o "$tmp" "https://raw.githubusercontent.com/ex/als/0123/contracts.toml"
EOF
  cat > "$tmp/.github/actions/leg/action.yml" <<'EOF'
inputs:
  v:
    required: true
runs:
  using: composite
  steps:
    - shell: bash
      run: bash scripts/fetch-pinned.sh "https://github.com/ex/tool/releases/download/${{ inputs.v }}/tool-${{ inputs.v }}-x86_64-linux.tar.xz" t.tar.xz
EOF
  printf '#!/bin/sh\nwget https://ex.example/tool.tar.gz\n' > "$tmp/scripts/get.sh"
  echo "not a digest line" >> "$tmp/.github/pinned-downloads.txt"
  set +e; out=$(dlpins "$tmp" 2>&1); rc=$?; set -e
  expect_refusal "the download rule" "$out" "$rc" \
    'dl.yml:9  curl -fLO' 'dl.yml:11  curl -fsSL https://ex.example/install | sh' \
    'tool-v9.9.9-x86_64-linux.tar.xz — no line in' 'not a release asset of that repository' \
    'leg/action.yml:8' 'tool-v2.0.0-x86_64-linux.tar.xz — no line in' 'get.sh:2  wget' \
    'pinned-downloads.txt:2  not `<sha256>  <https url>`' '7 finding(s)'
  python3 -I - "$tmp/.github/workflows/dl.yml" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
for a, b in [('          curl -fLO "https://github.com/ex/tool/releases/download/${VERSION}/tool-${VERSION}-x86_64-linux.tar.xz"\n',
              '          bash scripts/fetch-pinned.sh "https://github.com/ex/tool/releases/download/${VERSION}/tool-${VERSION}-x86_64-linux.tar.xz" t.tar.xz\n'),
             ("curl -fsSL https://ex.example/install | sh", "echo no installer"), ("VERSION=v9.9.9", "VERSION=v1.0.0"),
             ("--repo other/repo", "--repo ex/tool"), ("v: v2.0.0", "v: v1.0.0")]:
    assert a in s, a
    s = s.replace(a, b, 1)
open(p, "w").write(s)
PY
  printf '#!/bin/sh\nwget https://ex.example/tool.tar.gz\necho "%s  tool.tar.gz" | sha256sum -c -\n' "$(printf 'b%.0s' $(seq 64))" > "$tmp/scripts/get.sh"
  sed -i.bak '/not a digest line/d' "$tmp/.github/pinned-downloads.txt"; rm -f "$tmp/.github/pinned-downloads.txt.bak"
  dlpins "$tmp" >/dev/null || { echo "self-test: the download rule refused the pinned tree" >&2; dlpins "$tmp" >&2; exit 1; }

  # The helper at run time: a wrong digest aborts the step and leaves no file.
  asset="$tmp/asset.tar.gz"; printf 'release bytes\n' > "$asset"
  url="file://$asset"; man="$tmp/manifest.txt"
  right=$( (command -v sha256sum >/dev/null && sha256sum "$asset" || shasum -a 256 "$asset") | cut -d' ' -f1)
  printf '%s  %s\n' "$(printf '0%.0s' $(seq 64))" "$url" > "$man"
  set +e; out=$(PINNED_DOWNLOADS_MANIFEST="$man" bash -c 'set -e; bash "$1" "$2" "$3"; echo REACHED' _ "$tmp/scripts/fetch-pinned.sh" "$url" "$tmp/got" 2>&1); rc=$?; set -e
  [ "$rc" = 1 ] && ! /usr/bin/grep -q REACHED <<< "$out" && [ ! -e "$tmp/got" ] && /usr/bin/grep -qF 'sha256 mismatch' <<< "$out" \
    || { echo "self-test: a wrong digest did not abort the download step (exit $rc)" >&2; echo "$out" >&2; exit 1; }
  printf '%s  %s\n' "$right" "$url" > "$man"
  PINNED_DOWNLOADS_MANIFEST="$man" bash "$tmp/scripts/fetch-pinned.sh" "$url" "$tmp/got" >/dev/null \
    || { echo "self-test: the helper refused the pinned digest" >&2; exit 1; }
  printf 'tampered cache\n' > "$tmp/got"   # a poisoned cache restore is re-fetched, not trusted
  PINNED_DOWNLOADS_MANIFEST="$man" bash "$tmp/scripts/fetch-pinned.sh" "$url" "$tmp/got" >/dev/null \
    && cmp -s "$asset" "$tmp/got" || { echo "self-test: the helper kept a cached file whose digest is wrong" >&2; exit 1; }
  set +e; PINNED_DOWNLOADS_MANIFEST="$man" bash "$tmp/scripts/fetch-pinned.sh" "file://$tmp/other" "$tmp/x" >/dev/null 2>&1; rc=$?; set -e
  [ "$rc" = 1 ] || { echo "self-test: the helper fetched a URL with no pinned digest (exit $rc)" >&2; exit 1; }

  echo "check-action-pins self-test: OK — refused a tag pin, a branch pin, seven moving toolchain / runtime / installer names and a composite's nested tag, six unpinned downloads and a malformed manifest line, and a wrong digest at run time; passed the fixed trees"
  exit 0
fi

if [ "${1:-}" = "--closure" ]; then
  supply closure "${2:-$(git rev-parse --show-toplevel)}"
  exit $?
fi

root="${1:-$(git rev-parse --show-toplevel)}"
rc=0
scan "$root" || rc=1
supply versions "$root" || rc=1
dlpins "$root" || rc=1
exit "$rc"
