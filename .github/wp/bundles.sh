#!/usr/bin/env bash
# bundles.sh <outdir>: one witness bundle hash per spec program.
set -u
out=$1; mkdir -p "$out"
cargo build -q --release -p almide-wasm --example emit_structural_bundle
P=target/release/examples/emit_structural_bundle
git ls-files 'spec/*.almd' | sort > /tmp/files.txt
xargs -P4 -I{} sh -c 'r=$(timeout 120 '"$P"' "{}" 2>/dev/null | sha256sum | cut -c1-16); echo "{} $r"' < /tmp/files.txt | sort > "$out/hashes.txt"
wc -l "$out/hashes.txt"
