#!/bin/bash
# Stdlib Parity Check: verify all TOML-defined functions have runtime implementations.
# Usage: ./tools/stdlib-parity-check.sh
# Exit code: 0 = all OK, 1 = missing implementations found

set -uo pipefail

DEFS="stdlib/defs"
RT_TS="runtime/ts"
RT_RS="runtime/rs/src"

missing_ts=0
missing_rs=0
errors=""

for toml in "$DEFS"/*.toml; do
  mod=$(basename "$toml" .toml)

  while IFS= read -r func; do
    # --- TS check ---
    ts_tmpl=$(awk "/^\[${func}\]/{found=1} found && /^ts =/{print; found=0}" "$toml")
    if echo "$ts_tmpl" | grep -q "__almd_${mod}\.${func}"; then
      ts_file="$RT_TS/${mod}.ts"
      if [ ! -f "$ts_file" ]; then
        errors="${errors}\n  TS  MISSING FILE  ${mod}.ts  (needed for ${func})"
        missing_ts=$((missing_ts + 1))
      elif ! grep -q "${func}" "$ts_file" 2>/dev/null; then
        errors="${errors}\n  TS  MISSING FUNC  ${mod}.${func}"
        missing_ts=$((missing_ts + 1))
      fi
    fi

    # --- Rust check ---
    rs_tmpl=$(awk "/^\[${func}\]/{found=1} found && /^rust =/{print; found=0}" "$toml")
    if echo "$rs_tmpl" | grep -q "almide_rt_${mod}_${func}"; then
      rs_file="$RT_RS/${mod}.rs"
      if [ ! -f "$rs_file" ]; then
        errors="${errors}\n  RS  MISSING FILE  ${mod}.rs  (needed for ${func})"
        missing_rs=$((missing_rs + 1))
      elif ! grep -q "almide_rt_${mod}_${func}" "$rs_file" 2>/dev/null; then
        errors="${errors}\n  RS  MISSING FUNC  ${mod}.${func}"
        missing_rs=$((missing_rs + 1))
      fi
    fi
  done < <(grep '^\[' "$toml" | tr -d '[]')
done

# Summary
total_toml=$(grep -rh '^\[' "$DEFS"/*.toml | grep -v '^\[package' | wc -l | tr -d ' ')
echo "=== Stdlib Parity Check ==="
echo "TOML definitions: $total_toml functions"
echo "Missing TS:       $missing_ts"
echo "Missing RS:       $missing_rs"

if [ -n "$errors" ]; then
  echo -e "\nDetails:$errors"
  echo ""
  exit 1
else
  echo "All implementations present."
  exit 0
fi
