#!/usr/bin/env python3
"""Replace the stdlib section in CHEATSHEET.md with auto-generated content from TOML definitions."""

import subprocess
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CHEATSHEET = ROOT / "docs" / "CHEATSHEET.md"
GENERATOR = ROOT / "tools" / "generate-stdlib-cheatsheet.sh"

# Generate new stdlib section
result = subprocess.run(["bash", str(GENERATOR)], capture_output=True, text=True, cwd=ROOT)
if result.returncode != 0:
    print(f"Error running generator: {result.stderr}")
    exit(1)

new_section = result.stdout.rstrip()

# Append manual sections (path, args — no TOML definitions)
manual = """
### path (requires `import path`)
`path.join(base, child)`, `path.dirname(p)`, `path.basename(p)`, `path.extension(p)` → `Option[String]`, `path.is_absolute(p)` → `Bool`

### args (requires `import args`)
`args.flag(name)` → `Bool`, `args.option(name)` → `Option[String]`, `args.option_or(name, fallback)` → `String`, `args.positional()` → `List[String]`"""

new_section = new_section + "\n" + manual

# Read current cheatsheet
content = CHEATSHEET.read_text()

# Find and replace the stdlib section
# Starts with "<!-- AUTO-GENERATED" or "## Standard library modules"
# Ends just before "## Key rules"
pattern = r"(<!-- AUTO-GENERATED[^\n]*\n)?## Standard library modules\n.*?(?=\n## Key rules)"
replacement = f"<!-- AUTO-GENERATED from stdlib/defs/*.toml — do not edit manually. Run: make cheatsheet-update -->\n{new_section}"

new_content, count = re.subn(pattern, replacement, content, flags=re.DOTALL)

if count == 0:
    print("ERROR: Could not find stdlib section in CHEATSHEET.md")
    exit(1)

CHEATSHEET.write_text(new_content)
print(f"Replaced stdlib section ({count} substitution)")
