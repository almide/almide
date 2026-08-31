#!/usr/bin/env python3
"""Regenerate the machine-owned signature index block in docs/stdlib/*.md.

The prose sections of those files are hand-curated; the block between the
BEGIN/END markers is generated from the compiler's own module interface
(`almide compile <module> --json`, served from the bundled self-hosted
stdlib sources) so the documented surface can never drift from reality.

Usage:
    python3 tools/gen-stdlib-doc-index.py            # rewrite blocks in place
    python3 tools/gen-stdlib-doc-index.py --check    # exit 1 if anything is stale

ALMIDE_BIN overrides the compiler binary (default: target/release/almide,
falling back to `almide` on PATH).
"""

import json
import os
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCS = os.path.join(REPO, "docs", "stdlib")
BEGIN = "<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->"
END = "<!-- END GENERATED SIGNATURE INDEX -->"


def almide_bin() -> str:
    if os.environ.get("ALMIDE_BIN"):
        return os.environ["ALMIDE_BIN"]
    local = os.path.join(REPO, "target", "release", "almide")
    return local if os.path.exists(local) else "almide"


def render_ty(t: dict) -> str:
    k = t.get("kind", "?")
    if k == "list":
        return f"List[{render_ty(t['inner'])}]"
    if k == "option":
        return f"Option[{render_ty(t['inner'])}]"
    if k == "result":
        return f"Result[{render_ty(t['ok'])}, {render_ty(t['err'])}]"
    if k == "map":
        return f"Map[{render_ty(t['key'])}, {render_ty(t['value'])}]"
    if k == "set":
        return f"Set[{render_ty(t['inner'])}]"
    if k == "fn":
        params = ", ".join(render_ty(p) for p in t.get("params", []))
        return f"({params}) -> {render_ty(t['return'])}"
    if k == "tuple":
        return "(" + ", ".join(render_ty(p) for p in t.get("items", t.get("elems", []))) + ")"
    if k in ("named", "type_var"):
        name = t.get("name", "?")
        args = t.get("args", [])
        if args:
            return f"{name}[{', '.join(render_ty(a) for a in args)}]"
        return name
    if k == "record":
        fields = t.get("fields", [])
        inner = ", ".join(f"{f['name']}: {render_ty(f['type'])}" for f in fields)
        return "{ " + inner + " }"
    # primitives + anything unmodeled: capitalize the kind tag
    return {
        "string": "String", "int": "Int", "float": "Float", "bool": "Bool",
        "unit": "Unit", "bytes": "Bytes", "matrix": "Matrix", "value": "Value",
        "path": "Path", "unknown": "?",
    }.get(k, k.capitalize())


def signature(module: str, f: dict) -> str:
    params = ", ".join(f"{p['name']}: {render_ty(p['type'])}" for p in f.get("params", []))
    eff = "effect " if f.get("effect") else ""
    # #1735: a deprecated fn carries its steer inline, so the generated
    # index and the E052 warning tell one story.
    dep = f"   (deprecated — {f['deprecated']})" if f.get("deprecated") else ""
    return f"{eff}{module}.{f['name']}({params}) -> {render_ty(f['return'])}{dep}"


def module_block(module: str) -> str:
    out = subprocess.run(
        [almide_bin(), "compile", module, "--json"],
        capture_output=True, text=True, cwd=REPO,
    )
    if out.returncode != 0 or not out.stdout.strip():
        raise SystemExit(
            f"error: `almide compile {module} --json` failed:\n{out.stderr.strip()}"
        )
    iface = json.loads(out.stdout)
    fns = iface.get("functions", [])
    # __-prefixed fns are INTERNAL carriers (e.g. the fallibility-polymorphic
    # __fallible_* bodies, ADR-0006 D3) — never document them.
    fns = [f for f in fns if not f.get("name", "").startswith("__")]
    lines = [BEGIN, "", f"## Signature index ({len(fns)} functions)", "", "```"]
    for f in fns:
        lines.append(signature(module, f))
    lines += ["```", "", END]
    return "\n".join(lines)


def apply(path: str, module: str, check: bool) -> bool:
    with open(path, encoding="utf-8") as fh:
        src = fh.read()
    block = module_block(module)
    if BEGIN in src and END in src:
        head, rest = src.split(BEGIN, 1)
        _, tail = rest.split(END, 1)
        new = head.rstrip("\n") + "\n\n" + block + tail
    else:
        new = src.rstrip("\n") + "\n\n" + block + "\n"
    if new == src:
        return False
    if check:
        return True
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(new)
    return True


def main() -> int:
    check = "--check" in sys.argv
    stale = []
    for name in sorted(os.listdir(DOCS)):
        if not name.endswith(".md"):
            continue
        module = name[:-3]
        # Clock-constructor pages document checker surface, not a module
        # interface (authority: almide_types::time_units::TIME_MODULES; the
        # Rust-side counter in src/cli/docs_gen.rs reads that table directly).
        if module in ("compute", "duration"):
            continue
        path = os.path.join(DOCS, name)
        if apply(path, module, check):
            stale.append(module)
    if check and stale:
        print(f"::error::stale stdlib doc signature index for: {', '.join(stale)} "
              f"— run `make stdlib-docs` and commit")
        return 1
    if not check:
        print(f"updated {len(stale)} module doc(s)" if stale else "all up to date")
    return 0


if __name__ == "__main__":
    sys.exit(main())
