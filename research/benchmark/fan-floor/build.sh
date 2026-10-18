#!/bin/bash
# Build the three reference implementations (Rust 1.x + Rayon 1.10.0, Go
# 1.22, Zig 0.13.0 on PATH) next to their sources.
set -euo pipefail
cd "$(dirname "$0")"
(cd rust && cargo build --release -q)
(cd go && GOTOOLCHAIN=local go build -o floor .)
(cd zig && zig build-exe -O ReleaseFast main.zig -femit-bin=floor)
