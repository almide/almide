#!/usr/bin/env python3
"""Generate stdlib documentation pages for almide/docs from TOML definitions.

Usage:
    python3 tools/generate-stdlib-docs.py              # print to stdout
    python3 tools/generate-stdlib-docs.py --write      # write to docs-site/
"""

import os
import sys
import re

# Module metadata: import requirement and short description
MODULE_INFO = {
    "string":   {"import": "auto", "desc": "String manipulation functions."},
    "list":     {"import": "auto", "desc": "List operations — transform, search, and combine."},
    "map":      {"import": "auto", "desc": "Map (dictionary) operations — key-value collections."},
    "set":      {"import": "auto", "desc": "Set operations — unique element collections."},
    "int":      {"import": "auto", "desc": "Integer arithmetic and bitwise operations."},
    "float":    {"import": "auto", "desc": "Floating-point arithmetic and conversion."},
    "value":    {"import": "auto", "desc": "Dynamic value manipulation (JSON-compatible)."},
    "result":   {"import": "auto", "desc": "Result type operations — error handling."},
    "option":   {"import": "auto", "desc": "Option type operations — nullable values."},
    "error":    {"import": "import error", "desc": "Error construction and inspection."},
    "json":     {"import": "import json", "desc": "JSON parsing, serialization, and querying."},
    "regex":    {"import": "import regex", "desc": "Regular expression matching and replacement."},
    "math":     {"import": "import math", "desc": "Mathematical functions and constants."},
    "random":   {"import": "import random", "desc": "Random number generation.", "effect": True},
    "datetime": {"import": "import datetime", "desc": "Date and time operations.", "effect": True},
    "bytes":    {"import": "import bytes", "desc": "Binary data manipulation."},
    "matrix":   {"import": "import matrix", "desc": "2D matrix operations."},
    "testing":  {"import": "import testing", "desc": "Test assertion helpers."},
    "fs":       {"import": "import fs", "desc": "File system operations.", "effect": True},
    "env":      {"import": "import env", "desc": "Environment variables and system info.", "effect": True},
    "process":  {"import": "import process", "desc": "Process execution and control.", "effect": True},
    "io":       {"import": "import io", "desc": "Standard I/O operations.", "effect": True},
    "http":     {"import": "import http", "desc": "HTTP client and server.", "effect": True},
}

def parse_toml_simple(path):
    """Parse stdlib TOML into list of (name, {description, example, params, return, is_effect})."""
    functions = []
    current = None
    in_params = False
    params_buf = ""
    with open(path) as f:
        for line in f:
            line = line.rstrip()
            m = re.match(r'^\[(\w+)\]$', line)
            if m:
                if in_params and current:
                    current["params"] = parse_params(params_buf)
                    in_params = False
                if current:
                    functions.append(current)
                current = {"name": m.group(1), "description": "", "example": "", "params": [], "return": "", "is_effect": False}
                continue
            if current is None:
                continue
            if in_params:
                params_buf += " " + line.strip()
                if "]" in line and line.strip().endswith("]"):
                    current["params"] = parse_params(params_buf)
                    in_params = False
                continue
            if line.startswith("description = "):
                current["description"] = line.split("= ", 1)[1].strip().strip('"').strip("'")
            elif line.startswith("example = "):
                current["example"] = line.split("= ", 1)[1].strip().strip('"').strip("'")
            elif line.startswith("return = "):
                current["return"] = line.split("= ", 1)[1].strip().strip('"').strip("'")
            elif line.startswith("params = ["):
                params_str = line.split("= ", 1)[1]
                if params_str.strip().endswith("]"):
                    current["params"] = parse_params(params_str)
                else:
                    in_params = True
                    params_buf = params_str
            elif "is_effect" in line and "true" in line:
                current["is_effect"] = True
    if in_params and current:
        current["params"] = parse_params(params_buf)
    if current:
        functions.append(current)
    return functions

def parse_params(s):
    """Parse params from TOML inline array."""
    params = []
    for m in re.finditer(r'name\s*=\s*"(\w+)".*?type\s*=\s*"([^"]+)"', s):
        params.append({"name": m.group(1), "type": m.group(2)})
    return params

def format_signature(module, func):
    """Format function signature."""
    params_str = ", ".join(f"{p['name']}: {p['type']}" for p in func["params"])
    ret = func["return"]
    name = func["name"]
    return f"{module}.{name}({params_str}) -> {ret}"

def generate_page(module, functions):
    """Generate a documentation page for a module."""
    info = MODULE_INFO.get(module, {"import": f"import {module}", "desc": ""})

    # Frontmatter
    import_note = "Auto-imported" if info["import"] == "auto" else f"Requires `{info['import']}`"
    lines = [
        "---",
        f"title: {module}",
        f"description: {info['desc']} {import_note}.",
        "---",
        "",
    ]

    # Intro
    if info["import"] == "auto":
        lines.append(f"The `{module}` module is **auto-imported** — no `import` statement needed.")
    else:
        lines.append(f"```almd")
        lines.append(f"{info['import']}")
        lines.append(f"```")
    lines.append("")

    # Summary table
    lines.append("## Functions")
    lines.append("")
    lines.append("| Function | Signature | Description |")
    lines.append("|---|---|---|")
    for func in functions:
        params_str = ", ".join(f"{p['type']}" for p in func["params"])
        ret = func["return"]
        effect = "effect " if func.get("is_effect") else ""
        sig = f"`{effect}({params_str}) -> {ret}`"
        lines.append(f"| `{func['name']}` | {sig} | {func['description']} |")
    lines.append("")

    # Detailed reference
    lines.append("## Reference")
    lines.append("")
    for func in functions:
        sig = format_signature(module, func)
        effect = "effect " if func.get("is_effect") else ""
        lines.append(f"### `{effect}{sig}`")
        lines.append("")
        lines.append(func["description"])
        lines.append("")
        if func["example"]:
            lines.append("```almd")
            lines.append(func["example"])
            lines.append("```")
            lines.append("")

    return "\n".join(lines)


def main():
    defs_dir = "stdlib/defs"
    docs_dir = "docs-site/src/content/docs/stdlib"
    write = "--write" in sys.argv

    for toml_file in sorted(os.listdir(defs_dir)):
        if not toml_file.endswith(".toml"):
            continue
        module = toml_file[:-5]
        if module not in MODULE_INFO:
            continue

        functions = parse_toml_simple(os.path.join(defs_dir, toml_file))
        page = generate_page(module, functions)

        if write:
            out_path = os.path.join(docs_dir, f"{module}.md")
            with open(out_path, "w") as f:
                f.write(page)
            print(f"  wrote {out_path} ({len(functions)} functions)")
        else:
            print(f"=== {module} ({len(functions)} functions) ===")
            print(page[:500])
            print("...")
            print()


if __name__ == "__main__":
    main()
