#!/usr/bin/env python3
"""Generate docs/STDLIB-SPEC.md from stdlib/defs/*.toml"""

import tomllib
import os
import glob

DEFS_DIR = "stdlib/defs"
BUNDLED_DIR = "stdlib"
RUNTIME_DIR = "runtime/rs/src"
OUTPUT = "docs/STDLIB-SPEC.md"

# Module classification
CORE = {"string", "list", "map", "int", "float", "math", "json", "regex", "result", "error", "testing", "log", "value"}
PLATFORM = {"fs", "process", "io", "env", "http", "random", "datetime", "crypto", "uuid"}
BUNDLED = {"args", "compress", "csv", "encoding", "hash", "path", "term", "time", "toml", "url"}

# Check which runtime functions actually exist
def get_implemented_fns():
    """Scan runtime directories for almide_rt_* function definitions."""
    fns = set()
    for rs_file in glob.glob(os.path.join(RUNTIME_DIR, "*.rs")):
        with open(rs_file) as f:
            for line in f:
                for prefix in ["pub fn almide_rt_", "pub fn almide_"]:
                    if prefix in line:
                        start = line.index(prefix[7:])  # skip "pub fn "
                        rest = line[start:]
                        paren = rest.index("(") if "(" in rest else len(rest)
                        fn_name = rest[:paren].split("<")[0]
                        fns.add(fn_name)
                        break
    return fns


def format_params(params):
    """Format parameter list as signature string."""
    parts = []
    for p in params:
        opt = "?" if p.get("optional") else ""
        parts.append(f"{p['name']}: {p['type']}{opt}")
    return ", ".join(parts)


def parse_module(toml_path):
    """Parse a TOML module definition and return structured data."""
    with open(toml_path, "rb") as f:
        data = tomllib.load(f)

    module_name = os.path.splitext(os.path.basename(toml_path))[0]
    functions = []

    for fn_name, fn_def in data.items():
        if not isinstance(fn_def, dict):
            continue
        params = fn_def.get("params", [])
        ret = fn_def.get("return", "Unit")
        desc = fn_def.get("description", "")
        example = fn_def.get("example", "")
        effect = fn_def.get("effect", False)
        type_params = fn_def.get("type_params", [])
        rust_template = fn_def.get("rust", "")

        # Extract runtime function name from rust template
        runtime_fn = ""
        if "almide_rt_" in rust_template:
            start = rust_template.index("almide_rt_")
            rest = rust_template[start:]
            paren = rest.index("(") if "(" in rest else len(rest)
            runtime_fn = rest[:paren]
        elif "almide_" in rust_template:
            start = rust_template.index("almide_")
            rest = rust_template[start:]
            paren = rest.index("(") if "(" in rest else len(rest)
            runtime_fn = rest[:paren]

        generics = f"[{', '.join(type_params)}]" if type_params else ""
        sig = f"{fn_name}{generics}({format_params(params)}) -> {ret}"
        if effect:
            sig = f"effect {sig}"

        functions.append({
            "name": fn_name,
            "signature": sig,
            "description": desc,
            "example": example,
            "effect": effect,
            "runtime_fn": runtime_fn,
        })

    return module_name, functions


def main():
    implemented = get_implemented_fns()

    # Parse all TOML modules
    modules = []
    total_fns = 0
    total_implemented = 0

    for toml_path in sorted(glob.glob(os.path.join(DEFS_DIR, "*.toml"))):
        mod_name, functions = parse_module(toml_path)

        impl_count = 0
        for fn in functions:
            if fn["runtime_fn"] and fn["runtime_fn"] in implemented:
                fn["implemented"] = True
                impl_count += 1
            elif not fn["runtime_fn"]:
                # Inline template (no runtime function needed)
                fn["implemented"] = True
                impl_count += 1
            else:
                fn["implemented"] = False

        if mod_name in CORE:
            layer = "core"
        elif mod_name in PLATFORM:
            layer = "platform"
        else:
            layer = "native"

        modules.append({
            "name": mod_name,
            "functions": functions,
            "layer": layer,
            "impl_count": impl_count,
        })
        total_fns += len(functions)
        total_implemented += impl_count

    # Parse bundled modules
    bundled_modules = []
    for almd_path in sorted(glob.glob(os.path.join(BUNDLED_DIR, "*.almd"))):
        mod_name = os.path.splitext(os.path.basename(almd_path))[0]
        # Count exported functions (fn/effect fn at top level)
        fn_count = 0
        with open(almd_path) as f:
            for line in f:
                stripped = line.strip()
                if stripped.startswith("fn ") or stripped.startswith("effect fn "):
                    if "=" in stripped:
                        fn_count += 1
        bundled_modules.append({"name": mod_name, "fn_count": fn_count})

    # Generate markdown
    lines = []
    lines.append("# Almide Standard Library Specification")
    lines.append("")
    lines.append(f"Auto-generated from `stdlib/defs/*.toml`. {total_fns} native functions across {len(modules)} modules.")
    lines.append(f"Runtime implementation: {total_implemented}/{total_fns} ({total_implemented*100//total_fns}%).")
    lines.append("")

    # Module index
    lines.append("## Module Index")
    lines.append("")
    lines.append("### Native Modules (TOML-defined)")
    lines.append("")
    lines.append("| Module | Layer | Functions | Implemented | Status |")
    lines.append("|--------|-------|-----------|-------------|--------|")
    for m in modules:
        fn_count = len(m["functions"])
        impl = m["impl_count"]
        if impl == fn_count:
            status = "Ready"
        elif impl == 0:
            status = "TOML only"
        else:
            status = f"Partial ({impl}/{fn_count})"
        lines.append(f"| {m['name']} | {m['layer']} | {fn_count} | {impl}/{fn_count} | {status} |")

    lines.append("")
    lines.append("### Bundled Modules (pure Almide)")
    lines.append("")
    lines.append("| Module | Functions |")
    lines.append("|--------|-----------|")
    for b in bundled_modules:
        lines.append(f"| {b['name']} | {b['fn_count']} |")

    lines.append("")

    # Per-module API
    lines.append("---")
    lines.append("")

    for m in modules:
        lines.append(f"## {m['name']}")
        lines.append("")
        lines.append(f"Layer: **{m['layer']}** | {len(m['functions'])} functions | {m['impl_count']}/{len(m['functions'])} implemented")
        lines.append("")

        for fn in m["functions"]:
            marker = "" if fn["implemented"] else " (not implemented)"
            lines.append(f"### `{m['name']}.{fn['name']}`{marker}")
            lines.append("")
            if fn["description"]:
                lines.append(f"{fn['description']}")
                lines.append("")
            lines.append(f"```")
            lines.append(f"{fn['signature']}")
            lines.append(f"```")
            lines.append("")
            if fn["example"]:
                lines.append(f"Example: `{fn['example']}`")
                lines.append("")

    # Write output
    with open(OUTPUT, "w") as f:
        f.write("\n".join(lines))

    print(f"Generated {OUTPUT}: {total_fns} functions, {total_implemented} implemented")


if __name__ == "__main__":
    main()
