#!/usr/bin/env bash
# INSTALLER ATTESTATION GATE (security row 16): both installers refuse to
# install a release whose Sigstore attestation they could not verify.
#
# `tools/install.sh` and `tools/install.ps1` hash the archive against the
# release's checksum file, which proves the archive is the one the release
# lists — not who built it, since both came from the same place. Origin is the
# attestation (#1534), verified with `gh attestation verify`. Until row 16 the
# installers installed with a note when `gh` was missing; now they refuse, as
# `almide self-update` does, unless the command line says `--skip-attestation`.
#
# This gate runs each installer against a stubbed release: `curl` (sh) and
# `Invoke-WebRequest` (ps1) serve files from a local directory, `gh` is absent,
# failing or passing, and PATH holds only the tools the installer needs (a
# runner's own /usr/bin/gh must not answer). The cases, per installer:
#   1. no gh                       -> refused, nothing installed
#   2. no gh, --skip-attestation   -> installed, with a warning
#   3. gh fails the verification   -> refused, nothing installed
#   4. gh verifies                 -> installed
#   5. a checksum that does not match -> refused, nothing installed
#
# Usage: scripts/check-installer-attestation.sh [--require-pwsh]
#   --require-pwsh fails when pwsh is missing instead of skipping install.ps1
#   (CI passes it: ubuntu runners carry pwsh).
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2
root="$(pwd)"
require_pwsh=0
[ "${1:-}" = "--require-pwsh" ] && require_pwsh=1

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
fail=0
bad() { echo "FAIL: $*" >&2; fail=1; }

case "$(uname -s)" in Darwin) os=macos ;; Linux) os=linux ;; *) echo "unsupported host" >&2; exit 2 ;; esac
case "$(uname -m)" in x86_64|amd64) arch=x86_64 ;; aarch64|arm64) arch=aarch64 ;; *) echo "unsupported arch" >&2; exit 2 ;; esac

# ── the stubbed release ─────────────────────────────────────────────────────
rel="$tmp/release"; mkdir -p "$rel/stage/almide-$os-$arch"
printf '#!/bin/sh\necho "almide 0.0.0 (stub)"\n' > "$rel/stage/almide-$os-$arch/almide"
printf '#!/bin/sh\necho "almide-verify 0.0.0 (stub)"\n' > "$rel/stage/almide-$os-$arch/almide-verify"
chmod +x "$rel/stage/almide-$os-$arch/almide" "$rel/stage/almide-$os-$arch/almide-verify"
(cd "$rel/stage" && tar czf "$rel/almide-$os-$arch.tar.gz" "almide-$os-$arch")
mkdir -p "$rel/stage/almide-windows-x86_64"
printf 'stub\n' > "$rel/stage/almide-windows-x86_64/almide.exe"
printf 'stub\n' > "$rel/stage/almide-windows-x86_64/almide-verify.exe"
sha() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

# ── a PATH with only what install.sh needs ──────────────────────────────────
tools="$tmp/tools"; mkdir -p "$tools"
for t in sh uname mktemp rm awk cut tr tar gzip mkdir cp chmod basename cat sha256sum shasum dirname; do
  p="$(command -v "$t" 2>/dev/null)" && ln -s "$p" "$tools/$t"
done
cat > "$tools/curl" <<EOF
#!/bin/sh
# Serves https://…/<name> from $rel; honours -o.
out=""; url=""
while [ \$# -gt 0 ]; do
  case "\$1" in -o) out="\$2"; shift ;; https://*) url="\$1" ;; esac
  shift
done
src="$rel/\${url##*/}"
[ -f "\$src" ] || exit 22
if [ -n "\$out" ]; then cp "\$src" "\$out"; else cat "\$src"; fi
EOF
chmod +x "$tools/curl"
gh_dir() { # <mode: none|pass|fail> -> a dir to prepend to PATH
  local d="$tmp/gh-$1"; mkdir -p "$d"
  case "$1" in
    pass) printf '#!/bin/sh\nexit 0\n' > "$d/gh"; chmod +x "$d/gh" ;;
    fail) printf '#!/bin/sh\necho "verification failed" >&2\nexit 1\n' > "$d/gh"; chmod +x "$d/gh" ;;
  esac
  echo "$d"
}

write_checksums() { # <good|bad>
  local a="almide-$os-$arch.tar.gz" h
  h="$(sha "$rel/$a")"
  [ "$1" = bad ] && h="$(printf '%064d' 0)"
  { echo "$h  $a"; echo "$(sha "$rel/almide-windows-x86_64.zip" 2>/dev/null || printf '%064d' 1)  almide-windows-x86_64.zip"; } > "$rel/almide-checksums.sha256"
}

# case <label> <gh mode> <checksums> <expect: installed|refused> <want text> [installer args...]
run_sh() {
  local label="$1" gh="$2" sums="$3" expect="$4" want="$5"; shift 5
  local dest="$tmp/dest-sh-$label"; rm -rf "$dest"
  write_checksums "$sums"
  local out rc
  out="$(cd "$tmp" && env -i HOME="$tmp" PATH="$(gh_dir "$gh"):$tools" ALMIDE_INSTALL="$dest" sh "$root/tools/install.sh" "$@" 2>&1)"; rc=$?
  case "$expect" in
    installed) { [ "$rc" = 0 ] && [ -x "$dest/almide" ]; } || bad "install.sh [$label]: expected an install, got exit $rc"$'\n'"$out" ;;
    refused)   { [ "$rc" != 0 ] && [ ! -e "$dest/almide" ]; } || bad "install.sh [$label]: expected a refusal, got exit $rc"$'\n'"$out" ;;
  esac
  /usr/bin/grep -qF -- "$want" <<< "$out" || bad "install.sh [$label]: the output does not say '$want'"$'\n'"$out"
  echo "install.sh [$label]: exit $rc ($expect)"
}

run_sh no-gh        none good refused   "gh not found"
run_sh skip         none good installed "--skip-attestation: installing without verifying"  --skip-attestation
run_sh gh-fails     fail good refused   "did not verify"
run_sh gh-verifies  pass good installed "Verifying the release attestation"
run_sh bad-checksum pass bad  refused   "checksum mismatch"

# ── install.ps1 ─────────────────────────────────────────────────────────────
if command -v pwsh >/dev/null 2>&1; then
  pw="$(command -v pwsh)"
  (cd "$rel/stage" && "$pw" -NoProfile -Command "Compress-Archive -Path almide-windows-x86_64 -DestinationPath '$rel/almide-windows-x86_64.zip' -Force" >/dev/null)
  run_ps1() {
    local label="$1" gh="$2" sums="$3" expect="$4" want="$5"; shift 5
    local dest="$tmp/dest-ps1-$label"; rm -rf "$dest"
    write_checksums "$sums"
    local stub="$tmp/stub-$label.ps1"
    {
      echo "function Invoke-WebRequest { param([string]\$Uri, [string]\$OutFile, [switch]\$UseBasicParsing)"
      echo "  \$src = Join-Path '$rel' ([System.IO.Path]::GetFileName(\$Uri)); if (-not (Test-Path \$src)) { throw 'not found' }"
      echo "  Copy-Item \$src (\$OutFile -replace '\\\\','/') }"
      case "$gh" in
        pass) echo "function gh { \$global:LASTEXITCODE = 0 }" ;;
        fail) echo "function gh { Write-Host 'verification failed'; \$global:LASTEXITCODE = 1 }" ;;
      esac
      echo "\$env:PATH = '$tools'"
      echo "\$env:ALMIDE_INSTALL = '$dest'"
      echo "& '$root/tools/install.ps1' $*"
      echo "exit \$LASTEXITCODE"
    } > "$stub"
    local out rc
    out="$("$pw" -NoProfile -NonInteractive -File "$stub" 2>&1)"; rc=$?
    case "$expect" in
      installed) /usr/bin/grep -qF "Installed almide to" <<< "$out" && ls "$dest" 2>/dev/null | /usr/bin/grep -q 'almide.exe' \
                   || bad "install.ps1 [$label]: expected an install, got exit $rc"$'\n'"$out" ;;
      refused)   { [ "$rc" != 0 ] && ! ls "$dest" 2>/dev/null | /usr/bin/grep -q 'almide.exe'; } \
                   || bad "install.ps1 [$label]: expected a refusal, got exit $rc"$'\n'"$out" ;;
    esac
    /usr/bin/grep -qF -- "$want" <<< "$out" || bad "install.ps1 [$label]: the output does not say '$want'"$'\n'"$out"
    echo "install.ps1 [$label]: exit $rc ($expect)"
  }
  run_ps1 no-gh        none good refused   "gh not found"
  run_ps1 skip         none good installed "--skip-attestation: installing without verifying"  --skip-attestation
  run_ps1 gh-fails     fail good refused   "did not verify"
  run_ps1 gh-verifies  pass good installed "Verifying the release attestation"
  run_ps1 bad-checksum pass bad  refused   "checksum mismatch"
elif [ "$require_pwsh" = 1 ]; then
  bad "pwsh not found, and --require-pwsh was given: install.ps1 went unchecked"
else
  echo "note: pwsh not found — install.ps1 not exercised (CI passes --require-pwsh)"
fi

if [ "$fail" = 0 ]; then
  echo "installer-attestation OK: both installers refuse without a verified attestation unless --skip-attestation is given"
fi
exit "$fail"
