#!/usr/bin/env bash
# usage: repro.sh <almide-bin> <label>
set -u
A=$1; L=$2
for f in probe4545/repros/*.almd; do
  n=$(basename $f .almd)
  echo "=== [$L] $n"
  timeout 300 $A build $f --target wasm -o /tmp/r_$n.wasm > /tmp/r_$n.wlog 2>&1; wb=$?
  echo "wasm build exit=$wb"; head -c 600 /tmp/r_$n.wlog; echo
  if [ $wb = 0 ]; then timeout 120 $A run $f --target wasm > /tmp/r_$n.wout 2>&1; echo "wasm run exit=$? out=$(head -c 300 /tmp/r_$n.wout)"; fi
  timeout 600 $A run $f > /tmp/r_$n.nout 2>&1; echo "native run exit=$? out=$(head -c 600 /tmp/r_$n.nout)"
done
