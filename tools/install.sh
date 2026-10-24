#!/bin/sh
# Almide installer — downloads a prebuilt binary from GitHub Releases.
#
# Usage:
#   curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/almide/almide/main/tools/install.sh | sh
#   curl --proto '=https' --tlsv1.2 -fsSL ... | sh -s -- v0.12.3          # specific version
#   ALMIDE_INSTALL=~/bin sh install.sh          # custom install dir

set -eu

REPO="almide/almide"
INSTALL_DIR="${ALMIDE_INSTALL:-${HOME}/.local/bin}"
VERSION="${1:-latest}"

# --- Detect platform ---

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) os="macos" ;;
  Linux)  os="linux" ;;
  *)
    echo "error: unsupported OS: $OS" >&2
    echo "       supported: macOS, Linux" >&2
    echo "       Windows users: use install.ps1 instead" >&2
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64)   arch="x86_64" ;;
  aarch64|arm64)   arch="aarch64" ;;
  *)
    echo "error: unsupported architecture: $ARCH" >&2
    echo "       supported: x86_64, aarch64" >&2
    exit 1
    ;;
esac

ARCHIVE="almide-${os}-${arch}.tar.gz"

# --- Resolve download URLs ---

if [ "$VERSION" = "latest" ]; then
  BASE="https://github.com/${REPO}/releases/latest/download"
else
  BASE="https://github.com/${REPO}/releases/download/${VERSION}"
fi

URL="${BASE}/${ARCHIVE}"
CHECKSUM_URL="${BASE}/almide-checksums.sha256"

# --- Download ---

# Every download (#4181): ignore ~/.curlrc (-q, which must come first), https
# only, a redirect only to https, and TLS 1.2 or newer.
CURL="curl -q --proto =https --proto-redir =https --tlsv1.2 -fsSL"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading ${ARCHIVE}..."
if ! $CURL "$URL" -o "${TMP}/${ARCHIVE}"; then
  echo "error: download failed" >&2
  echo "       check that the version exists: https://github.com/${REPO}/releases" >&2
  exit 1
fi

$CURL "$CHECKSUM_URL" -o "${TMP}/checksums.sha256"

# --- Verify checksum ---

echo "Verifying checksum..."
cd "$TMP"
# Match the file-name field exactly (an optional leading `*` is the binary
# marker), never a substring: a `.sig` / `.sbom` line for the same archive
# must not supply the hash (#4127).
EXPECTED="$(awk -v a="$ARCHIVE" '{ n = $2; sub(/^\*/, "", n) } NF == 2 && n == a { print tolower($1); exit }' checksums.sha256)"
if [ -z "$EXPECTED" ]; then
  echo "error: no checksum listed for ${ARCHIVE}" >&2
  exit 1
fi

# Fail closed (#4127): no hash tool means no install. This used to warn and
# install the archive unverified.
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "$ARCHIVE" | cut -d' ' -f1)"
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL="$(shasum -a 256 "$ARCHIVE" | cut -d' ' -f1)"
else
  echo "error: could not verify checksum (sha256sum/shasum not found); refusing to install" >&2
  exit 1
fi
ACTUAL="$(printf '%s' "$ACTUAL" | tr 'A-F' 'a-f')"

if [ "$EXPECTED" != "$ACTUAL" ]; then
  echo "error: checksum mismatch" >&2
  echo "       expected: $EXPECTED" >&2
  echo "       got:      $ACTUAL" >&2
  exit 1
fi

# --- Verify origin ---
# The checksum came from the same place as the archive: integrity, not origin
# (#4127). Every release asset carries a Sigstore attestation (#1534); verify
# it when the GitHub CLI is here, and say how to otherwise.
if command -v gh >/dev/null 2>&1; then
  echo "Verifying the release attestation..."
  if ! gh attestation verify "$ARCHIVE" -R "$REPO" >/dev/null; then
    echo "error: the release attestation did not verify; refusing to install" >&2
    exit 1
  fi
else
  echo "note: gh not found, so the release attestation was not verified (the checksum proves integrity, not origin)" >&2
  echo "      to verify: gh attestation verify ${ARCHIVE} -R ${REPO}" >&2
fi

# --- Install ---

tar xzf "$ARCHIVE"
mkdir -p "$INSTALL_DIR"
cp "almide-${os}-${arch}/almide" "${INSTALL_DIR}/almide"
chmod +x "${INSTALL_DIR}/almide"

# The independent certificate verifier (#2152) goes NEXT TO almide:
# `almide verify` execs it from there and has no built-in fallback. Archives
# older than the verifier do not carry it; say so instead of failing.
if [ -f "almide-${os}-${arch}/almide-verify" ]; then
  cp "almide-${os}-${arch}/almide-verify" "${INSTALL_DIR}/almide-verify"
  chmod +x "${INSTALL_DIR}/almide-verify"
else
  echo "note: this release predates almide-verify; \`almide verify\` will report it missing" >&2
fi

echo ""
echo "Installed almide to ${INSTALL_DIR}/almide"
"${INSTALL_DIR}/almide" --version
if [ -x "${INSTALL_DIR}/almide-verify" ]; then
  "${INSTALL_DIR}/almide-verify" --version
fi

# --- PATH check ---

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo ""
    SHELL_NAME="$(basename "${SHELL:-/bin/sh}")"
    case "$SHELL_NAME" in
      zsh)  RC="~/.zshrc" ;;
      bash) RC="~/.bashrc" ;;
      fish) RC="~/.config/fish/config.fish" ;;
      *)    RC="your shell config" ;;
    esac
    echo "To add almide to your PATH, add this to ${RC}:"
    echo ""
    if [ "$SHELL_NAME" = "fish" ]; then
      echo "  set -gx PATH \"${INSTALL_DIR}\" \$PATH"
    else
      echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    fi
    echo ""
    ;;
esac
