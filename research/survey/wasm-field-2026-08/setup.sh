#!/bin/bash
# One-time setup for the wasm field survey: builds every lane's artifacts once
# and prepares the toolchains that need local scaffolding. Idempotent.
# Toolchain INSTALL steps (brew/npm/installer commands) are in README.md.
set -euo pipefail
S="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$S/../../.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.moon/bin:$PATH"

mkdir -p "$S"/out/{almide,rust,go,tinygo,grain,assemblyscript}

echo "== almide: build runner + emit-only driver =="
(cd "$REPO" && cargo build --release -p almide-wasm-run)
(cd "$S/tools/emit-only" && cargo build --release)
for k in empty int_loop float_math str_build recursion list_sort sort_by list_pipeline; do
  "$S/tools/emit-only/target/release/almide-emit-only" "$S/src/almide/$k.almd" "$S/out/almide/$k.wasm"
done

echo "== rust: wasm32-wasip1 std overlay sysroot =="
# The local rustc is qusp-managed (no rustup), so the wasip1 std component is
# overlaid onto a symlink copy of the real sysroot instead of rustup target add.
RUSTV="$(rustc --version | awk '{print $2}')"
OV="$S/out/rust-sysroot-overlay"
if [ ! -d "$OV/lib/rustlib/wasm32-wasip1" ]; then
  rm -rf "$OV" && mkdir -p "$OV/lib/rustlib"
  REAL="$(rustc --print sysroot)"
  for e in "$REAL"/*; do [ "$(basename "$e")" = lib ] || ln -s "$e" "$OV/"; done
  for e in "$REAL"/lib/*; do [ "$(basename "$e")" = rustlib ] || ln -s "$e" "$OV/lib/"; done
  for e in "$REAL"/lib/rustlib/*; do ln -s "$e" "$OV/lib/rustlib/"; done
  TB="$S/out/rust-std-$RUSTV-wasm32-wasip1.tar.xz"
  [ -f "$TB" ] || curl -sSL -o "$TB" "https://static.rust-lang.org/dist/rust-std-$RUSTV-wasm32-wasip1.tar.xz"
  tar -xf "$TB" -C "$S/out"
  cp -R "$S/out/rust-std-$RUSTV-wasm32-wasip1/rust-std-wasm32-wasip1/lib/rustlib/wasm32-wasip1" "$OV/lib/rustlib/"
  rm -rf "$S/out/rust-std-$RUSTV-wasm32-wasip1"
fi
for k in empty int_loop float_math str_build recursion list_sort sort_by list_pipeline; do
  rustc --sysroot "$OV" --target wasm32-wasip1 -C opt-level=3 "$S/src/rust/$k.rs" -o "$S/out/rust/$k.wasm"
done

echo "== go (mainline, wasip1) =="
(cd "$S/src/go" && for k in empty int_loop float_math str_build recursion list_sort sort_by list_pipeline; do
  GOOS=wasip1 GOARCH=wasm go build -o "$S/out/go/$k.wasm" "$k.go"
done)

echo "== tinygo (needs go <= 1.26 on PATH: brew keg go@1.26) =="
(export PATH="/opt/homebrew/opt/go@1.26/bin:$PATH" GOROOT="/opt/homebrew/opt/go@1.26/libexec"
 cd "$S/src/tinygo" && for k in empty int_loop float_math str_build recursion list_sort sort_by list_pipeline; do
  tinygo build -target=wasip1 -opt=2 -o "$S/out/tinygo/$k.wasm" "$k.go"
done)

echo "== assemblyscript =="
(cd "$S/src/assemblyscript" && npm install
 for k in empty int_loop float_math str_build recursion list_sort sort_by list_pipeline; do
  ./node_modules/.bin/asc "$k.ts" --config node_modules/@assemblyscript/wasi-shim/asconfig.json -O3 -o "$S/out/assemblyscript/$k.wasm"
done)

echo "== grain =="
(cd "$S/src/grain" && for k in empty int_loop float_math str_build recursion list_sort sort_by list_pipeline; do
  grain compile --release -o "$S/out/grain/$k.wasm" "$k.gr"
done)

echo "== moonbit =="
(cd "$S/src/moonbit" && moon build --target wasm --release)

echo "== kotlin (gradle downloads deps on first run) =="
(cd "$S/src/kotlin" && gradle build -x check)

echo "setup complete"
