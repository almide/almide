#!/usr/bin/env bash
# machine-class.sh — the machine class a timing ledger was stamped on, and
# whether THIS machine is that class.
#
# A stamped timing (docs/benchmarks/wasm-runtime.txt's ratios, the build-speed
# baseline in docs/benchmarks/build-speed.txt) is a claim about one machine
# class: a cross-engine ratio does not cancel hardware (nbody wasm/native read
# 2.09 on an M-series Mac and 19.95 on a 2-core runner, 2026-08-31), and an
# absolute build time does not either. So the ledgers record the class on their
# `machine` line, and the machine-dependent verdicts (the stamped-ratio verdict
# of scripts/check-wasm-runtime-ratio.sh, the 1.5x build-speed ratchet of
# `almide-gates bench --check`) run only where the class matches. Both read the
# rule from here, so they cannot disagree on what "the same class" means.
#
# The class line:  <uname -m> <uname -s>, <CPU model>[ [<host label>]]
#   e.g.  x86_64 Linux, INTEL(R) XEON(R) PLATINUM 8581C CPU @ 2.30GHz [GCE c4-standard-8]
#         arm64 Darwin, Apple M4 Pro
# The KEY is the line without the trailing `[...]` label, whitespace collapsed:
# architecture, OS and CPU model. The label (the GCE machine type, read from
# the metadata server) is provenance for a reader re-creating the machine, not
# part of the comparison. A recorded line in the older shape (`arm64 Darwin`,
# no CPU model) never equals a current key, so it never arms a verdict.
#
#   bash scripts/lib/machine-class.sh               # this machine's class line
#   bash scripts/lib/machine-class.sh --key         # this machine's key
#   bash scripts/lib/machine-class.sh --same "REC"  # exit 0 iff REC's key == this machine's key
#   bash scripts/lib/machine-class.sh --self-test   # the comparison rule's own controls
set -uo pipefail

squash() { tr -s '[:space:]' ' ' | sed -E 's/^ //; s/ $//'; }

cpu_model() {
  case "$(uname -s)" in
    Darwin) sysctl -n machdep.cpu.brand_string 2>/dev/null ;;
    Linux)
      local m
      m=$(grep -m1 -E '^model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2-)
      [ -n "$m" ] || m=$(LC_ALL=C lscpu 2>/dev/null | grep -m1 -E '^Model name' | cut -d: -f2-)
      printf '%s' "$m"
      ;;
  esac | squash
}

host_label() { # the GCE machine type when on GCE, else nothing
  local p=/sys/class/dmi/id/product_name t
  [ -r "$p" ] && grep -q 'Google Compute Engine' "$p" || return 0
  t=$(curl -fsS -m 2 -H 'Metadata-Flavor: Google' \
    http://metadata.google.internal/computeMetadata/v1/instance/machine-type 2>/dev/null) || return 0
  [ -n "$t" ] && printf 'GCE %s' "${t##*/}"
}

here_key() { printf '%s %s, %s' "$(uname -m)" "$(uname -s)" "$(cpu_model)" | squash; }

key_of() { # a recorded class line -> its key (the trailing [label] dropped)
  printf '%s' "$1" | squash | sed -E 's/ \[[^]]*\]$//'
}

same() { [ -n "$2" ] && [ "$(key_of "$1")" = "$(key_of "$2")" ]; }

self_test() {
  local fail=0
  t() { # expect recorded here
    if same "$2" "$3"; then got=same; else got=differ; fi
    [ "$got" = "$1" ] || { echo "::error::machine-class self-test: '$2' vs '$3' -> $got, expected $1"; fail=1; }
  }
  t same   'x86_64 Linux, Intel Xeon 8581C [GCE c4-standard-8]' 'x86_64 Linux, Intel Xeon 8581C'
  t same   'x86_64 Linux,  Intel  Xeon 8581C' 'x86_64 Linux, Intel Xeon 8581C [GCE c4-standard-4]'
  t differ 'x86_64 Linux, Intel Xeon 8581C' 'x86_64 Linux, AMD EPYC 7763 64-Core Processor'
  t differ 'x86_64 Linux, Intel Xeon 8581C' 'arm64 Linux, Intel Xeon 8581C'
  t differ 'arm64 Darwin, Apple M4 Pro' 'arm64 Linux, Apple M4 Pro'
  t differ 'arm64 Darwin' 'arm64 Darwin, Apple M4 Pro'
  t differ 'arm64 Darwin (local stamp; ratios are the gated quantity)' 'arm64 Darwin, Apple M4 Pro'
  t differ '' ''
  [ -n "$(cpu_model)" ] || { echo "::error::machine-class self-test: no CPU model read on this machine"; fail=1; }
  [ "$fail" = 0 ] && echo "machine-class: self-test OK (here: $(here_key))"
  return "$fail"
}

case "${1:-}" in
  '')          label=$(host_label); printf '%s%s\n' "$(here_key)" "${label:+ [$label]}" ;;
  --key)       here_key; echo ;;
  --same)      same "${2:-}" "$(here_key)" ;;
  --self-test) self_test ;;
  *) echo "usage: machine-class.sh [--key | --same RECORDED | --self-test]" >&2; exit 2 ;;
esac
