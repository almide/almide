#!/bin/bash
# Runs the full measurement matrix (all lanes, or only the lanes named as args).
# Prerequisite: ./setup.sh once. Results land in out/results.json + out/results.md.
set -euo pipefail
S="$(cd "$(dirname "$0")" && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.moon/bin:$PATH"
exec python3 "$S/measure.py" "$@"
