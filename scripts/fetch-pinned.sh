#!/usr/bin/env bash
# PINNED DOWNLOAD (security row 16) — the one way CI fetches a binary or archive.
#
#   scripts/fetch-pinned.sh <url> <out>
#       <url> must be listed in .github/pinned-downloads.txt (`<sha256>  <url>`).
#       If <out> already exists (an actions/cache restore) it is hashed first and
#       kept only on a match. Otherwise it is downloaded (retried: release hosts
#       rate-limit) and hashed. Any mismatch deletes <out> and exits 1, so the
#       step aborts before the bytes are unpacked or run.
#
#   scripts/fetch-pinned.sh --attested <owner/repo> <url> <out>
#       For a first-party release fetched by a moving name (`releases/latest`),
#       which no digest can pin: <url> must be a release asset of <owner/repo>,
#       and `gh attestation verify --repo <owner/repo> --signer-workflow
#       <owner/repo>/.github/workflows/release.yml` must accept it (Sigstore
#       provenance from that repository's release workflow). Needs GH_TOKEN.
#
# A tag or a release asset can be replaced by whoever controls the upstream
# repository; a digest stated here cannot. scripts/check-action-pins.sh refuses
# a download in .github/ or scripts/ that bypasses this file.
# PINNED_DOWNLOADS_MANIFEST overrides the manifest path (the gate's self-test).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
manifest="${PINNED_DOWNLOADS_MANIFEST:-$here/../.github/pinned-downloads.txt}"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

download() { # <url> <out>
  local i
  for i in 1 2 3 4 5; do
    if curl -fsSL --retry 3 --retry-all-errors --retry-delay 10 -o "$2.part" "$1"; then
      mv -f "$2.part" "$2"; return 0
    fi
    echo "fetch-pinned: download failed (rate-limit?), retry ${i}/5: $1" >&2
    rm -f "$2.part"; sleep 15
  done
  echo "::error::fetch-pinned: could not download $1" >&2
  return 1
}

if [ "${1:-}" = "--attested" ]; then
  [ $# -eq 4 ] || { echo "usage: fetch-pinned.sh --attested <owner/repo> <url> <out>" >&2; exit 2; }
  repo="$2" url="$3" out="$4"
  case "$url" in
    "https://github.com/$repo/releases/"*) ;;
    *) echo "::error::fetch-pinned: $url is not a release asset of $repo" >&2; exit 1 ;;
  esac
  download "$url" "$out"
  if ! gh attestation verify "$out" --repo "$repo" \
         --signer-workflow "$repo/.github/workflows/release.yml" >/dev/null; then
    rm -f "$out"
    echo "::error::fetch-pinned: $url carries no attestation from $repo's release workflow — refused" >&2
    exit 1
  fi
  echo "fetch-pinned: $out attested by $repo"
  exit 0
fi

[ $# -eq 2 ] || { echo "usage: fetch-pinned.sh <url> <out>  |  --attested <owner/repo> <url> <out>" >&2; exit 2; }
url="$1" out="$2"
[ -r "$manifest" ] || { echo "::error::fetch-pinned: no manifest at $manifest" >&2; exit 2; }
want="$(awk -v u="$url" '$1 !~ /^#/ && $2 == u { print $1 }' "$manifest")"
if [ -z "$want" ]; then
  echo "::error::fetch-pinned: $url has no pinned sha256 in $manifest — add \`<sha256>  <url>\`" >&2
  exit 1
fi
if [ -s "$out" ] && [ "$(sha256_of "$out")" = "$want" ]; then
  echo "fetch-pinned: $out (cached) sha256 $want"
  exit 0
fi
rm -f "$out"
download "$url" "$out"
got="$(sha256_of "$out")"
if [ "$got" != "$want" ]; then
  rm -f "$out"
  echo "::error::fetch-pinned: sha256 mismatch for $url — pinned $want, got $got. Aborting." >&2
  exit 1
fi
echo "fetch-pinned: $out sha256 $want"
