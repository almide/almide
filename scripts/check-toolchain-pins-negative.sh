#!/usr/bin/env bash
# Negative controls for the toolchain pins gate (#4010). Each case copies the
# workflows, the embedded engine's Cargo.toml, the pins list and the trusted
# base ledger into a throwaway tree, changes one thing, and checks the gate's
# verdict. The real tree is never touched.
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
root="$PWD"
GATE="$root/scripts/check-toolchain-pins.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fresh() {
  rm -rf "$tmp/t"
  mkdir -p "$tmp/t/.github" "$tmp/t/scripts" "$tmp/t/proofs" "$tmp/t/crates/almide-wasm-run"
  cp -R "$root/.github/workflows" "$tmp/t/.github/workflows"
  cp "$root/scripts/toolchain-pins.txt" "$tmp/t/scripts/"
  cp "$root/proofs/TRUSTED_BASE.md" "$tmp/t/proofs/"
  cp "$root/crates/almide-wasm-run/Cargo.toml" "$tmp/t/crates/almide-wasm-run/"
}
run() { TOOLCHAIN_PINS_ROOT="$tmp/t" bash "$GATE" >"$tmp/out" 2>&1; }
green() { if run; then echo "ok   $1"; else echo "FAIL: $1 — expected green" >&2; cat "$tmp/out" >&2; exit 1; fi; }
red() { if run; then echo "FAIL: $1 — expected red" >&2; cat "$tmp/out" >&2; exit 1; else echo "ok   $1 (red)"; fi; }

fresh
green "the tree as committed"

fresh
printf 'jobs:\n  sneak:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: dtolnay/rust-toolchain@stable\n      - run: cargo build\n' > "$tmp/t/.github/workflows/sneak.yml"
red "a new job that installs rustc with no version"

fresh
printf 'jobs:\n  sneak:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: dtolnay/rust-toolchain@stable\n        with:\n          toolchain: "1.97.0"\n' > "$tmp/t/.github/workflows/sneak.yml"
red "a rustc version the list does not name"

fresh
printf 'jobs:\n  sneak:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: dtolnay/rust-toolchain@stable\n        with:\n          toolchain: ${{ env.RUST_TOOLCHAIN }}\n' > "$tmp/t/.github/workflows/sneak.yml"
green "a new job pinned to RUST_TOOLCHAIN"

fresh
sed -E 's/^(wasmtime = (\{ *version = )?)"49\.0\.4"/\1"50.0.0"/' "$root/crates/almide-wasm-run/Cargo.toml" > "$tmp/t/crates/almide-wasm-run/Cargo.toml"
red "the embedded wasmtime bumped without the list"

fresh
printf 'jobs:\n  sneak:\n    steps:\n      - name: Install wasmtime\n        run: VERSION=v51.0.0\n' > "$tmp/t/.github/workflows/sneak.yml"
red "a stock wasmtime CLI version the list does not name"

fresh
first_floating=$(grep -m1 '^floating ' "$root/scripts/toolchain-pins.txt" | awk '{print $2}')
[ -n "$first_floating" ] || { echo "FAIL: no floating line left to delete — retire this control with the last floating step" >&2; exit 1; }
grep -v "^floating $first_floating " "$root/scripts/toolchain-pins.txt" > "$tmp/t/scripts/toolchain-pins.txt"
red "a floating step whose list line was deleted"

fresh
printf 'floating ci.yml:gone stable — a job that no longer exists\n' >> "$tmp/t/scripts/toolchain-pins.txt"
red "a list line for a floating step that no longer exists (stale)"

fresh
sed 's/49\.0\.1/(removed)/g' "$root/proofs/TRUSTED_BASE.md" > "$tmp/t/proofs/TRUSTED_BASE.md"
red "TRUSTED_BASE.md no longer naming a version the list pins"

echo "toolchain-pins negative controls: all verdicts as expected"
