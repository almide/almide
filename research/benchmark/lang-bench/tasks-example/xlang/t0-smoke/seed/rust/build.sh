#!/usr/bin/env bash
# Harness contract: `bash build.sh` exits 0 and leaves an executable at ./app
set -e
rustc --edition 2021 -O main.rs -o app
