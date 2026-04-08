#!/bin/sh
# Almide installer — downloads a prebuilt binary from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/almide/almide/main/tools/install.sh | sh
#   curl -fsSL ... | sh -s -- v0.12.3          # specific version
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

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading ${ARCHIVE}..."
if ! curl -fsSL "$URL" -o "${TMP}/${ARCHIVE}"; then
  echo "error: download failed" >&2
  echo "       check that the version exists: https://github.com/${REPO}/releases" >&2
  exit 1
fi

curl -fsSL "$CHECKSUM_URL" -o "${TMP}/checksums.sha256"

# --- Verify checksum ---

echo "Verifying checksum..."
cd "$TMP"
EXPECTED="$(grep "$ARCHIVE" checksums.sha256 | cut -d' ' -f1)"

if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "$ARCHIVE" | cut -d' ' -f1)"
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL="$(shasum -a 256 "$ARCHIVE" | cut -d' ' -f1)"
else
  echo "warning: could not verify checksum (sha256sum/shasum not found)" >&2
  ACTUAL="$EXPECTED"
fi

if [ "$EXPECTED" != "$ACTUAL" ]; then
  echo "error: checksum mismatch" >&2
  echo "       expected: $EXPECTED" >&2
  echo "       got:      $ACTUAL" >&2
  exit 1
fi

# --- Install ---

tar xzf "$ARCHIVE"
mkdir -p "$INSTALL_DIR"
cp "almide-${os}-${arch}/almide" "${INSTALL_DIR}/almide"
chmod +x "${INSTALL_DIR}/almide"

echo ""
echo "Installed almide to ${INSTALL_DIR}/almide"
"${INSTALL_DIR}/almide" --version

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
