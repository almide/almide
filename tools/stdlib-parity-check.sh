#!/bin/bash
# Stdlib Parity Check: verify all TOML-defined functions have runtime implementations.
# Checks: TypeScript, Rust, and WASM targets.
# Usage: ./tools/stdlib-parity-check.sh
# Exit code: 0 = all OK, 1 = missing implementations found

set -uo pipefail

DEFS="stdlib/defs"
RT_TS="runtime/ts"
RT_RS="runtime/rs/src"
WASM_DIR="src/codegen/emit_wasm"

# ── WASM skip list: functions impossible under WASI ──
WASM_SKIP=(
  # Network — no sockets in WASI
  "http.serve"
  "http.post"
  "http.put"
  "http.patch"
  "http.delete"
  "http.request"
  "http.req_method"
  "http.req_path"
  "http.req_body"
  "http.req_header"
  "http.query_params"
  # No subprocess in WASI
  "process.exec"
  "process.exec_in"
  "process.exec_with_stdin"
  "process.exec_status"
  # No sleep/cwd in WASI
  "env.sleep_ms"
  "env.cwd"
)

is_wasm_skipped() {
  local key="$1"
  for skip in "${WASM_SKIP[@]}"; do
    [ "$skip" = "$key" ] && return 0
  done
  return 1
}

# ── Collect WASM function names from all emit_wasm/*.rs ──
wasm_funcs_file=$(mktemp)
for f in "$WASM_DIR"/*.rs; do
  /usr/bin/grep -oE '"[a-z_0-9]+"' "$f" 2>/dev/null | tr -d '"'
done | sort -u > "$wasm_funcs_file"
trap "rm -f $wasm_funcs_file" EXIT

missing_ts=0
missing_rs=0
missing_wasm=0
skipped_wasm=0
errors=""

for toml in "$DEFS"/*.toml; do
  mod=$(basename "$toml" .toml)

  while IFS= read -r func; do
    # --- TS check ---
    ts_tmpl=$(awk "/^\[${func}\]/{found=1} found && /^ts =/{print; found=0}" "$toml")
    if echo "$ts_tmpl" | /usr/bin/grep -q "__almd_${mod}\.${func}"; then
      ts_file="$RT_TS/${mod}.ts"
      if [ ! -f "$ts_file" ]; then
        errors="${errors}\n  TS   MISSING FILE  ${mod}.ts  (needed for ${func})"
        missing_ts=$((missing_ts + 1))
      elif ! /usr/bin/grep -q "${func}" "$ts_file" 2>/dev/null; then
        errors="${errors}\n  TS   MISSING FUNC  ${mod}.${func}"
        missing_ts=$((missing_ts + 1))
      fi
    fi

    # --- Rust check ---
    rs_tmpl=$(awk "/^\[${func}\]/{found=1} found && /^rust =/{print; found=0}" "$toml")
    if echo "$rs_tmpl" | /usr/bin/grep -q "almide_rt_${mod}_${func}"; then
      rs_file="$RT_RS/${mod}.rs"
      if [ ! -f "$rs_file" ]; then
        errors="${errors}\n  RS   MISSING FILE  ${mod}.rs  (needed for ${func})"
        missing_rs=$((missing_rs + 1))
      elif ! /usr/bin/grep -q "almide_rt_${mod}_${func}" "$rs_file" 2>/dev/null; then
        errors="${errors}\n  RS   MISSING FUNC  ${mod}.${func}"
        missing_rs=$((missing_rs + 1))
      fi
    fi

    # --- WASM check ---
    if is_wasm_skipped "${mod}.${func}"; then
      skipped_wasm=$((skipped_wasm + 1))
    elif ! /usr/bin/grep -qw "${func}" "$wasm_funcs_file"; then
      errors="${errors}\n  WASM MISSING FUNC  ${mod}.${func}"
      missing_wasm=$((missing_wasm + 1))
    fi
  done < <(/usr/bin/grep '^\[' "$toml" | tr -d '[]')
done

# Summary
total_toml=$(/usr/bin/grep -rh '^\[' "$DEFS"/*.toml | /usr/bin/grep -v '^\[package' | wc -l | tr -d ' ')
total_missing=$((missing_ts + missing_rs + missing_wasm))
echo "=== Stdlib Parity Check ==="
echo "TOML definitions: $total_toml functions"
echo ""
echo "  TS:   missing $missing_ts"
echo "  RS:   missing $missing_rs"
echo "  WASM: missing $missing_wasm  (skipped ${skipped_wasm} — no WASI equivalent)"
echo ""

if [ -n "$errors" ]; then
  echo -e "Details:$errors"
  echo ""
  exit 1
else
  echo "All implementations present."
  exit 0
fi
