#!/usr/bin/env bash
# Fetch the C/C++ libraries and wasi-sdk into $JW_DEPS (default ./deps).
# Versions are pinned; each archive's sha256 is printed into the log.
set -euo pipefail
DEPS="${JW_DEPS:-$PWD/deps}"
mkdir -p "$DEPS"
cd "$DEPS"
YYJSON=0.13.0
SIMDJSON=v5.0.5
WASI_SDK=34
fetch() { curl -fsSL --retry 5 --retry-all-errors --retry-delay 5 -o "$2" "$1"; sha256sum "$2"; }
fetch "https://github.com/ibireme/yyjson/archive/refs/tags/${YYJSON}.tar.gz" yyjson.tgz
tar xzf yyjson.tgz && rm -rf yyjson && mv "yyjson-${YYJSON}" yyjson
mkdir -p simdjson
fetch "https://raw.githubusercontent.com/simdjson/simdjson/${SIMDJSON}/singleheader/simdjson.h" simdjson/simdjson.h
fetch "https://raw.githubusercontent.com/simdjson/simdjson/${SIMDJSON}/singleheader/simdjson.cpp" simdjson/simdjson.cpp
fetch "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${WASI_SDK}/wasi-sdk-${WASI_SDK}.0-x86_64-linux.tar.gz" wasi-sdk.tgz
tar xzf wasi-sdk.tgz && rm -rf wasi-sdk && mv "wasi-sdk-${WASI_SDK}.0-x86_64-linux" wasi-sdk
"$DEPS/wasi-sdk/bin/clang" --version | head -1
echo "yyjson ${YYJSON}, simdjson ${SIMDJSON}, wasi-sdk ${WASI_SDK}" > versions.txt
