#!/bin/bash
# Build the wide reference implementations (Rust + Rayon 1.10.0, Go 1.22 +
# golang.org/x/sync 0.8.0, Zig 0.13.0 on PATH, linked with libc).
set -euo pipefail
cd "$(dirname "$0")"
(cd rust && cargo build --release -q)
(cd go && GOTOOLCHAIN=local go build -o wide .)
(cd zig && zig build-exe -O ReleaseFast -lc main.zig -femit-bin=wide)
