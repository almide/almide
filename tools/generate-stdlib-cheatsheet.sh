#!/bin/bash
# generate-stdlib-cheatsheet.sh — Generate stdlib reference from TOML definitions
#
# Usage: bash tools/generate-stdlib-cheatsheet.sh
#
# Outputs the "Standard library modules" section for CHEATSHEET.md.
# Pipe to a file or use to replace the section in CHEATSHEET.md.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

DEFS_DIR="stdlib/defs"

# Auto-imported modules (no import needed)
AUTO_IMPORT="string list map set int float value result option"
# Modules requiring explicit import
EXPLICIT_IMPORT="fs path env process io json http regex bytes datetime log math matrix random testing error"

# Generate one module's function list from its TOML
generate_module() {
  local module="$1"
  local toml="$DEFS_DIR/${module}.toml"

  if [ ! -f "$toml" ]; then
    return
  fi

  # Parse TOML: extract function names, params, return types, and effect status
  python3 -c "
import sys
try:
    import tomllib
except ImportError:
    import tomli as tomllib

with open('$toml', 'rb') as f:
    defs = tomllib.load(f)

parts = []
for fn_name, fn_def in defs.items():
    if not isinstance(fn_def, dict) or 'params' not in fn_def:
        continue
    params = fn_def['params']
    ret = fn_def.get('return', '')
    effect = fn_def.get('effect', False)

    # Build param string
    param_strs = []
    for p in params:
        param_strs.append(p['name'])
    param_str = ', '.join(param_strs)

    # Build signature
    sig = f\"\`${module}.{fn_name}({param_str})\`\"

    # Add return type annotation for non-obvious types
    simple_returns = {'String', 'Int', 'Float', 'Bool', 'Unit', 'List[String]', 'List[Int]', ''}
    if ret and ret not in simple_returns:
        sig += f' → \`{ret}\`'

    parts.append(sig)

print(', '.join(parts))
"
}

echo "## Standard library modules"
echo ""

# Auto-imported modules
for module in $AUTO_IMPORT; do
  toml="$DEFS_DIR/${module}.toml"
  [ ! -f "$toml" ] && continue

  funcs=$(generate_module "$module")
  [ -z "$funcs" ] && continue

  echo "### $module (auto-imported)"
  echo "$funcs"
  echo ""
done

# Explicit import modules
for module in $EXPLICIT_IMPORT; do
  toml="$DEFS_DIR/${module}.toml"
  [ ! -f "$toml" ] && continue

  funcs=$(generate_module "$module")
  [ -z "$funcs" ] && continue

  # Check if module has effect functions
  has_effect=$(python3 -c "
try:
    import tomllib
except ImportError:
    import tomli as tomllib
with open('$toml', 'rb') as f:
    defs = tomllib.load(f)
for fn_def in defs.values():
    if isinstance(fn_def, dict) and fn_def.get('effect', False):
        print('effect'); break
" 2>/dev/null || true)

  suffix="requires \`import $module\`"
  if [ -n "$has_effect" ]; then
    suffix="$suffix, effect fns"
  fi

  echo "### $module ($suffix)"
  echo "$funcs"
  echo ""
done
