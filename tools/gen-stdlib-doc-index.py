#!/usr/bin/env python3
"""Regenerate the machine-owned signature index block in docs/stdlib/*.md.

The prose sections of those files are hand-curated; the block between the
BEGIN/END markers is generated from the compiler's own module interface
(`almide compile <module> --json`, served from the bundled self-hosted
stdlib sources) so the documented surface can never drift from reality.
The block carries every public name a caller can spell: the function
signatures and, where a module exports any, its type declarations. The two
clock-constructor pages (compute, duration) are checker surface with no
module interface; their block is derived from the unit table in
crates/almide-types/src/time_units.rs instead (see clock_table). Every
docs/stdlib/*.md page is under the generator — a page without a block is
stale, not exempt.

What the block carries per function beyond the signature: its `///` doc
comment, when the source has one. The interface JSON's `doc` slot is the
`///` run directly above the declaration (crates/almide-tools/src/interface.rs);
the by-name route serves the EMBEDDED source, which crates/almide-types/build.rs
blanks of every whole-line `//` comment EXCEPT `///` lines (#2436, the
compiler half of #1469), so a `///` line written in stdlib/<m>.almd reaches
this block and a design-note `//` line does not. A fn without a `///` run
renders as the bare signature, byte-identical to before — the remaining gap
is the stdlib content work of writing the lines (#1469). What the block still
does NOT carry: a `@since` epoch.

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
        # The interface JSON spells tuple members `elements` (#1832); the
        # old `items`/`elems` lookups rendered every tuple as `()`.
        return "(" + ", ".join(render_ty(p) for p in t.get("elements", t.get("items", t.get("elems", [])))) + ")"
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


def entry(module: str, f: dict) -> list:
    """The lines one function contributes to the index: its `///` doc comment
    (#2436), rendered as `// ` lines directly above the signature so the fence
    reads like the source it came from, then the signature. No doc → just the
    signature, so an undocumented module's block is byte-identical to before."""
    doc = f.get("doc")
    lines = []
    if doc:
        lines += [f"// {line}".rstrip() for line in doc.split("\n")]
    lines.append(signature(module, f))
    return lines


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
    for i, f in enumerate(fns):
        # A documented fn is set off by a blank line on each side so its
        # comment is not read as belonging to the neighbour above.
        e = entry(module, f)
        if len(e) > 1 and i > 0 and lines[-1] != "":
            lines.append("")
        lines += e
        if len(e) > 1 and i + 1 < len(fns):
            lines.append("")
    lines += ["```"]
    # Exported types are public names too (#1469): a module's record /
    # variant / alias declarations are part of the surface a caller can
    # spell, so they are derived here rather than hand-written in the prose.
    # Modules without types keep the block byte-identical to before.
    types = iface.get("types", [])
    if types:
        lines += ["", f"## Type index ({len(types)} types)", "", "```"]
        for t in types:
            lines.append(type_decl(module, t))
        lines += ["```"]
    lines += ["", END]
    return "\n".join(lines)


def type_decl(module: str, t: dict) -> str:
    kind = t.get("kind", {})
    k = kind.get("kind", "?")
    name = f"{module}.{t.get('name', '?')}"
    if k == "record":
        fields = ", ".join(f"{f['name']}: {render_ty(f['type'])}" for f in kind.get("fields", []))
        return f"type {name} = {{ {fields} }}"
    if k == "variant":
        cases = " | ".join(case_decl(c) for c in kind.get("cases", []))
        return f"type {name} = {cases}"
    if k == "alias":
        return f"type {name} = {render_ty(kind['target'])}"
    return f"type {name}   ({k})"


def case_decl(c: dict) -> str:
    # CasePayload (crates/almide-tools/src/interface.rs): a tuple payload
    # lists TypeRefs under `fields`; a record payload lists named fields.
    p = c.get("payload")
    if not p:
        return c["name"]
    if p.get("kind") == "record":
        inner = ", ".join(f"{f['name']}: {render_ty(f['type'])}" for f in p.get("fields", []))
        return f"{c['name']}({{ {inner} }})"
    return f"{c['name']}({', '.join(render_ty(t) for t in p.get('fields', []))})"


# The two clock-constructor pages (compute, duration) document CHECKER
# surface, not a bundled module: `almide compile compute --json` has nothing
# to serve. Their surface is the closed unit set × the two nominal clocks,
# owned by crates/almide-types/src/time_units.rs (TIME_UNITS, TIME_MODULES;
# the Rust-side counter in src/cli/docs_gen.rs reads the same table
# in-process). Deriving the block from that table keeps all 45 pages under
# one generator; a table the regex below cannot read is a loud failure, never
# a silent empty index.
TIME_UNITS_RS = os.path.join(REPO, "crates", "almide-types", "src", "time_units.rs")


def clock_table() -> tuple[list[str], dict[str, str]]:
    import re
    with open(TIME_UNITS_RS, encoding="utf-8") as fh:
        src = fh.read()
    units_m = re.search(r"pub const TIME_UNITS: &\[\(&str, i64\)\] = &\[(.*?)\];", src, re.S)
    mods_m = re.search(r"pub const TIME_MODULES: &\[\(&str, &str\)\] = &\[(.*?)\];", src, re.S)
    if not units_m or not mods_m:
        raise SystemExit(f"error: TIME_UNITS / TIME_MODULES not found in {TIME_UNITS_RS}")
    units = re.findall(r'\("(\w+)",\s*[\d_]+\)', units_m.group(1))
    mods = dict(re.findall(r'\("(\w+)",\s*"(\w+)"\)', mods_m.group(1)))
    if not units or not mods:
        raise SystemExit(f"error: TIME_UNITS / TIME_MODULES parsed empty from {TIME_UNITS_RS}")
    return units, mods


def clock_block(module: str, units: list[str], clock_type: str) -> str:
    lines = [BEGIN, "", f"## Signature index ({len(units)} functions)", "", "```"]
    for u in units:
        lines.append(f"{module}.{u}(n: Int) -> {clock_type}")
    lines += ["```", "", END]
    return "\n".join(lines)


def apply(path: str, module: str, check: bool, block: str) -> bool:
    with open(path, encoding="utf-8") as fh:
        src = fh.read()
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
    units, clocks = clock_table()
    for name in sorted(os.listdir(DOCS)):
        if not name.endswith(".md"):
            continue
        module = name[:-3]
        path = os.path.join(DOCS, name)
        if module in clocks:
            block = clock_block(module, units, clocks[module])
        else:
            block = module_block(module)
        if apply(path, module, check, block):
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
