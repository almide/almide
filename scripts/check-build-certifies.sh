#!/usr/bin/env bash
# `almide build --target wasm` judges its own certificate (#4011, row 3 of
# docs/roadmap/active/security-by-construction.md): with `almide-verify`
# installed next to it, the build records its witnesses as it emits and
# refuses to write the artifact when the verifier rejects them. This drill
# proves the refusal is real, in a throwaway worktree:
#   1. CONTROL — unpatched, each reproduction builds (exit 0) and the file is
#      written;
#   2. with a lost-check bug re-injected into the emitter
#      (proofs/fixtures/bounds-witness/reinject/<name>.patch, whose witness
#      almide-verify rejects), the build exits 1, prints
#      error[certificate-rejected], and writes NO file;
#   3. the same patched compiler with the verifier moved away writes the
#      file (exit 0): the refusal is the verifier's verdict, not the patch
#      breaking the build.
#
#   scripts/check-build-certifies.sh [name ...]   (default: every bounds patch)
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
DIR="$ROOT/proofs/fixtures/bounds-witness/reinject"
TARGET="${REINJECT_TARGET_DIR:-$ROOT/target/build-certifies}"
if [ "$#" -gt 0 ]; then names=("$@"); else
  names=(); for p in "$DIR"/*.patch; do names+=("$(basename "$p" .patch)"); done
fi
wt=$(mktemp -d)
trap 'git -C "$ROOT" worktree remove --force "$wt" >/dev/null 2>&1; rm -rf "$wt"' EXIT
git -C "$ROOT" worktree add --detach -q "$wt" HEAD || exit 2
build() { (cd "$wt" && CARGO_TARGET_DIR="$TARGET" cargo build -q --bin almide && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-verify); }
BIN="$TARGET/debug/almide"
VER="$TARGET/debug/almide-verify"
# try <name>: the build's exit, then whether the file was written.
try() {
  rm -f "$wt/out.wasm"
  (cd "$wt" && "$BIN" build "$DIR/$1.almd" --target wasm -o "$wt/out.wasm") > "$wt/build.log" 2>&1
  echo "$? $([ -f "$wt/out.wasm" ] && echo written || echo absent)"
}
build || { echo "FAIL: the unpatched tree does not build"; exit 2; }
fail=0
for name in "${names[@]}"; do
  v=$(try "$name")
  if [ "$v" != "0 written" ]; then echo "FAIL $name: CONTROL — unpatched, the build reads '$v': $(head -3 "$wt/build.log")"; fail=1; continue; fi
  git -C "$wt" checkout -q -- .
  git -C "$wt" apply "$DIR/$name.patch" || { echo "FAIL $name: the patch no longer applies"; fail=1; continue; }
  build || { echo "FAIL $name: the re-injected compiler does not build"; fail=1; git -C "$wt" checkout -q -- .; continue; }
  v=$(try "$name")
  if [ "$v" != "1 absent" ] || ! grep -q "certificate-rejected" "$wt/build.log"; then
    echo "FAIL $name: with the bug re-injected the build reads '$v', not a refusal: $(head -3 "$wt/build.log")"; fail=1
  else
    mv "$VER" "$VER.away"
    w=$(try "$name")
    mv "$VER.away" "$VER"
    if [ "$w" != "0 written" ]; then
      echo "FAIL $name: without the verifier the patched build reads '$w' — the refusal was not the verifier's"; fail=1
    else
      echo "ok   $name: control built (exit 0, written); re-injected, refused (exit 1, no file, certificate-rejected); without the verifier, written"
    fi
  fi
  git -C "$wt" checkout -q -- .
  build >/dev/null 2>&1
done
[ "$fail" -eq 0 ] && echo "build-certifies: every re-injected bug makes almide build refuse to write the artifact"
exit "$fail"
