#!/usr/bin/env bash
# Regenerate the derived ledgers a new spec/wasm_cross fixture moves, in the documented order.
set -u
export ALMIDE_MANIFEST_TREE_CHECK=off
O=$PWD/target/release/almide
step() { echo "::group::$*"; "$@"; echo "exit=$? :: $*"; echo "::endgroup::"; }
step env ORACLE=$O bash scripts/gen-run-manifest.sh
step env ORACLE=$O bash scripts/gen-ast-manifest.sh
step env ORACLE=$O bash scripts/gen-check-manifest.sh
step env ALMIDE_UPDATE_ALLOC=1 cargo test --release -p almide-wasm --test alloc_ledger
step env ALMIDE_UPDATE_SIZES=1 cargo test --release -p almide-wasm --test size_ratchet
step env ALMIDE_UPDATE_WITNESS_FLOOR=1 cargo test --release -p almide-wasm --test witness_floor
step env ALMIDE_UPDATE_SURFACE=1 cargo test --release -p almide-wasm --test surface_matrix
step env ALMIDE_BIN=$O ALMIDE_UPDATE_NATIVE_VERIFIED_FLOOR=1 cargo test --release --test native_verified_floor_test
step env ALMIDE_BIN=$O ALMIDE_UPDATE_INTERP_LEDGER=1 cargo test --release --test wasm_runtime_interp_ledger
step bash docs/contracts/generate-conformance.sh > docs/contracts/conformance.md
step bash scripts/gen-readme-stats.sh
step bash scripts/gen-claims.sh
git status --short
git diff -- . ':!probe4545' ':!.github' > /tmp/ledgers.patch
wc -l /tmp/ledgers.patch
