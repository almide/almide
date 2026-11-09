#!/usr/bin/env bash
# Fetch the C/C++ libraries, wasi-sdk and the nativejson-benchmark corpus into
# $JW_DEPS (default ./deps). Every download is digest-pinned in
# .github/pinned-downloads.txt (scripts/fetch-pinned.sh refuses a mismatch).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
DEPS="${JW_DEPS:-$PWD/deps}"
mkdir -p "$DEPS/simdjson" "$DEPS/corpus"
cd "$DEPS"
YYJSON=0.13.0
SIMDJSON=v5.0.5
WASI_SDK=34
CORPUS=478d5727c2a4048e835a29c65adecc7d795360d5
fetch() { bash "$ROOT/scripts/fetch-pinned.sh" "$1" "$2"; }
fetch "https://github.com/ibireme/yyjson/archive/refs/tags/${YYJSON}.tar.gz" yyjson.tgz
tar xzf yyjson.tgz && rm -rf yyjson && mv "yyjson-${YYJSON}" yyjson
fetch "https://raw.githubusercontent.com/simdjson/simdjson/${SIMDJSON}/singleheader/simdjson.h" simdjson/simdjson.h
fetch "https://raw.githubusercontent.com/simdjson/simdjson/${SIMDJSON}/singleheader/simdjson.cpp" simdjson/simdjson.cpp
fetch "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${WASI_SDK}/wasi-sdk-${WASI_SDK}.0-x86_64-linux.tar.gz" wasi-sdk.tgz
tar xzf wasi-sdk.tgz && rm -rf wasi-sdk && mv "wasi-sdk-${WASI_SDK}.0-x86_64-linux" wasi-sdk
for f in twitter citm_catalog canada; do
  fetch "https://raw.githubusercontent.com/miloyip/nativejson-benchmark/${CORPUS}/data/$f.json" "corpus/$f.json"
done
"$DEPS/wasi-sdk/bin/clang" --version | head -1
echo "yyjson ${YYJSON}, simdjson ${SIMDJSON}, wasi-sdk ${WASI_SDK}, corpus nativejson-benchmark@${CORPUS}" > versions.txt
