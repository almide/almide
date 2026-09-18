#!/usr/bin/env bash
# Harness contract: `bash build.sh` exits 0 and leaves an executable at ./app
set -e
almide build main.almd -o app
