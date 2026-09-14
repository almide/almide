#!/usr/bin/env bash
# The `ALMIDE_*` environment-switch gate (#2205), the grep half.
#
# The tree used to read its behaviour switches through scattered
# `std::env::var("ALMIDE_…")` calls: 104 names, two documented, no registry, and
# two sites disagreeing on what "set" means. `almide_base::env::SWITCHES` is now
# the ONE roster, `almide_base::env::{flag,var}` the one read path, and this gate
# keeps both true:
#
#   A. every `ALMIDE_*` name the tree spells — a Rust string literal, a shell /
#      workflow variable, a stdlib `env.get` — is a registered switch;
#   B. the compiler proper (`crates/*/src`, `src/`) reads no `ALMIDE_*` switch
#      directly — `std::env::var` / `var_os` on an `ALMIDE_` literal is refused
#      outside the crates that cannot depend on almide-base (`runtime/rs`,
#      `almide-rt-core`) and the registry's own file.
#
# The docs half — the table `docs/specs/cli.md` embeds equals
# `almide switches --md` — reads markdown and generated output, so it is the
# Almide gate `tools/almide-gates env-switches-doc` (#2128's boundary).
#
#   ALMIDE_BIN=target/release/almide bash scripts/check-env-switches.sh
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2
BIN="${ALMIDE_BIN:-target/release/almide}"
[ -x "$BIN" ] || { echo "::error::ALMIDE_BIN not executable: $BIN (cargo build --release)"; exit 2; }

registered="$("$BIN" switches | awk '{sub(/=value$/, "", $1); print $1}' | sort -u)"
[ -n "$registered" ] || { echo "::error::\`$BIN switches\` listed nothing"; exit 2; }

fail=0

# ── A. every spelled name is registered ─────────────────────────────────────
# Rust: the string literal, in every crate (the harness hooks included: a test
# that sets an unregistered switch is exactly the drift the registry exists to
# stop). `git ls-files` keeps target/ and untracked scratch out.
rs_names="$(git ls-files '*.rs' | grep -v '^crates/almide-base/src/env\.rs$' \
  | xargs grep -hoE '"ALMIDE_[A-Z0-9_]+"' 2>/dev/null | tr -d '"' | sort -u)"
# Shell and workflows: a variable position only — `NAME=`, `$NAME`, `${NAME`,
# `export NAME`, a YAML `NAME:` env key. A bare identifier in a comment or a
# table (the runtime's `ALMIDE_REPEAT_MAX_BYTES` constant, say) is not a switch.
sh_names="$(git ls-files '*.sh' '*.yml' '*.yaml' \
  | xargs grep -hoE '(^|[^A-Za-z0-9_$])(ALMIDE_[A-Z0-9_]+)=|\$\{?ALMIDE_[A-Z0-9_]+|^[[:space:]]*ALMIDE_[A-Z0-9_]+:' 2>/dev/null \
  | grep -oE 'ALMIDE_[A-Z0-9_]+' | sort -u)"
# The stdlib's own env reads (a compiled program reading a switch at run time).
almd_names="$(git ls-files 'stdlib/*.almd' \
  | xargs grep -hoE 'env\.get\("ALMIDE_[A-Z0-9_]+"\)' 2>/dev/null | grep -oE 'ALMIDE_[A-Z0-9_]+' | sort -u)"
# Python harness scripts.
py_names="$(git ls-files '*.py' \
  | xargs grep -hoE 'environ[^"]*"ALMIDE_[A-Z0-9_]+"' 2>/dev/null | grep -oE 'ALMIDE_[A-Z0-9_]+' | sort -u)"

spelled="$(printf '%s\n%s\n%s\n%s\n' "$rs_names" "$sh_names" "$almd_names" "$py_names" | sed '/^$/d' | sort -u)"
unregistered="$(comm -23 <(echo "$spelled") <(echo "$registered"))"
if [ -n "$unregistered" ]; then
  echo "::error::ALMIDE_* name(s) spelled in the tree but not in almide_base::env::SWITCHES (crates/almide-base/src/env.rs):"
  for n in $unregistered; do
    echo "  + $n"
    git grep -n --max-count=1 -e "$n" -- '*.rs' '*.sh' '*.yml' '*.yaml' 'stdlib/*.almd' '*.py' | head -3 | sed 's/^/      /'
  done
  fail=1
fi
# The other direction is informational: a registered switch nothing spells is a
# stale row, but the roster is also where a switch's retirement is recorded.
unspelled="$(comm -13 <(echo "$spelled") <(echo "$registered"))"
if [ -n "$unspelled" ]; then
  echo "::error::registered switch(es) nothing in the tree spells — retire the row or spell the read:"
  echo "$unspelled" | sed 's/^/  - /'
  fail=1
fi

# ── B. no direct read in the compiler proper ────────────────────────────────
direct="$(git ls-files 'crates/*/src/*.rs' 'src/*.rs' \
  | grep -vE '^(crates/almide-rt-core/|crates/almide-base/src/env\.rs$)' \
  | xargs grep -nE '(std::env|(^|[^A-Za-z0-9_:])env)::(var|var_os)\("ALMIDE_' 2>/dev/null || true)"
if [ -n "$direct" ]; then
  echo "::error::direct std::env read(s) of an ALMIDE_* switch in the compiler proper — read through almide_base::env::flag / var:"
  echo "$direct" | sed 's/^/  /'
  fail=1
fi

if [ "$fail" = 0 ]; then
  echo "env-switches OK: $(echo "$registered" | wc -l | tr -d ' ') registered switch(es), every spelled name registered, no direct reads in the compiler proper"
fi
exit "$fail"
