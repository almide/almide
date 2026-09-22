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
crates/almide-types/src/time_units.rs instead (see clock_iface). Every
docs/stdlib/*.md page is under the generator — a page without a block is
stale, not exempt. Everything OUTSIDE the block (the prose above it and
anything after the END marker) is the author's and is preserved byte for
byte; the generator owns the block and nothing else.

Every public name in the block carries three things (#1469):

  - its signature (or type declaration);
  - its doc comment: the `///` run directly above the declaration in
    stdlib/<module>.almd, read through the interface JSON's `doc` slot
    (crates/almide-tools/src/interface.rs). The by-name route serves the
    EMBEDDED source, which crates/almide-types/build.rs blanks of every
    whole-line `//` comment EXCEPT `///` lines (#2436), so a `///` line
    reaches this block and a design-note `//` line does not. The clock pages
    derive theirs from the unit table;
  - its `@since` release: the first release whose committed signature index
    carried the name, recorded in docs/stdlib/since.toml (see below).

A public name without a doc, or without a since.toml row, is an ERROR in both
modes — the block is complete by construction or the gate is red.

THE SINCE TABLE. docs/stdlib/since.toml maps `module.name` to the release
that first shipped it. It is DERIVED, not hand-written:
`--since-from-tags` rebuilds it from every local `vX.Y.Z[-rcN]` tag's tree —
the committed signature indexes (the same tree-only reading
scripts/check-interface-diff.sh does, so it needs no old compiler), plus the
declaration sources for what the index could not see yet (since_from_tags
says exactly which). An rc tag counts as its final version. A value is exact
only when the index looked one release earlier and the name was absent;
everything else is a `<=X.Y.Z` bound ("X.Y.Z or earlier": the first release
at which any reading saw the name), and is rendered as such. A name no
release tag carries yet is `unreleased`; the plain regenerate adds such a
row for a new fn by itself, and rerunning `--since-from-tags` after the next
tag is pushed stamps it. The table is committed so `--check` stays a pure
function of the tree (CI checks out no tags).

Usage:
    python3 tools/gen-stdlib-doc-index.py                   # rewrite blocks (+ since rows) in place
    python3 tools/gen-stdlib-doc-index.py --check           # exit 1 if anything is stale or incomplete
    python3 tools/gen-stdlib-doc-index.py --since-from-tags # rebuild since.toml from release tags, then regenerate

ALMIDE_BIN overrides the compiler binary (default: target/release/almide,
falling back to `almide` on PATH).
"""

import json
import os
import re
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCS = os.path.join(REPO, "docs", "stdlib")
SINCE = os.path.join(DOCS, "since.toml")
BEGIN = "<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->"
END = "<!-- END GENERATED SIGNATURE INDEX -->"
UNRELEASED = "unreleased"


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


def since_line(value: str) -> str:
    if value.startswith("<="):
        return f"// @since {value[2:]} or earlier"
    return f"// @since {value}"


def entry(doc, since: str, decl: str) -> list:
    """The lines one public name contributes to the index: its doc comment
    (#2436), rendered as `// ` lines so the fence reads like the source it came
    from, its `@since` release, then the declaration itself. The declaration
    stays the LAST line of the entry and the only line that does not start
    with `//` — scripts/check-interface-diff.sh reads exactly those lines."""
    lines = [f"// {line}".rstrip() for line in (doc or "").split("\n") if doc]
    lines.append(since_line(since))
    lines.append(decl)
    return lines


def module_iface(module: str) -> dict:
    out = subprocess.run(
        [almide_bin(), "compile", module, "--json"],
        capture_output=True, text=True, cwd=REPO,
    )
    if out.returncode != 0 or not out.stdout.strip():
        raise SystemExit(
            f"error: `almide compile {module} --json` failed:\n{out.stderr.strip()}"
        )
    iface = json.loads(out.stdout)
    # __-prefixed fns are INTERNAL carriers (e.g. the fallibility-polymorphic
    # __fallible_* bodies, ADR-0006 D3) — never document them.
    iface["functions"] = [f for f in iface.get("functions", []) if not f.get("name", "").startswith("__")]
    iface.setdefault("types", [])
    return iface


def module_block(module: str, iface: dict, since: dict) -> str:
    fns = iface["functions"]
    lines = [BEGIN, "", f"## Signature index ({len(fns)} functions)", "", "```"]
    for i, f in enumerate(fns):
        if i:
            lines.append("")
        lines += entry(f.get("doc"), since[f"{module}.{f['name']}"], signature(module, f))
    lines += ["```"]
    # Exported types are public names too (#1469): a module's record /
    # variant / alias declarations are part of the surface a caller can
    # spell, so they are derived here rather than hand-written in the prose.
    types = iface["types"]
    if types:
        lines += ["", f"## Type index ({len(types)} types)", "", "```"]
        for i, t in enumerate(types):
            if i:
                lines.append("")
            lines += entry(t.get("doc"), since[f"{module}.{t['name']}"], type_decl(module, t))
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
# The doc a clock constructor carries: the unit spelled out and the clock it
# measures. The negative-argument abort is ADR-0001's rule (docs/stdlib/
# compute.md, duration.md). A unit or clock missing here is a loud failure —
# a new table row needs its words, not a silent bare signature.
UNIT_WORDS = {"ns": "nanoseconds", "us": "microseconds", "ms": "milliseconds",
              "s": "seconds", "min": "minutes", "h": "hours"}
CLOCK_WORDS = {"compute": "deterministic compute time", "duration": "wall-clock time"}


def clock_table() -> tuple[list[str], dict[str, str]]:
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


def clock_iface(module: str, units: list[str], clock_type: str) -> dict:
    missing = [u for u in units if u not in UNIT_WORDS] + ([module] if module not in CLOCK_WORDS else [])
    if missing:
        raise SystemExit(f"error: no doc words for {', '.join(missing)} — extend UNIT_WORDS / CLOCK_WORDS")
    return {
        "functions": [{
            "name": u,
            "params": [{"name": "n", "type": {"kind": "int"}}],
            "return": {"kind": "named", "name": clock_type},
            "doc": f"n {UNIT_WORDS[u]} of {CLOCK_WORDS[module]}; negative n aborts.",
        } for u in units],
        "types": [],
    }


def public_names(ifaces: dict) -> dict:
    """`module.name` -> has-doc, for every fn and type the blocks render."""
    names = {}
    for module, iface in ifaces.items():
        for f in iface["functions"]:
            names[f"{module}.{f['name']}"] = bool(f.get("doc"))
        for t in iface["types"]:
            names[f"{module}.{t['name']}"] = bool(t.get("doc"))
    return names


# ── the since table ──

SINCE_HEADER = """\
# @since ledger for the public stdlib surface (#1469) — GENERATED.
#
# `module.name` -> the release that first shipped it, derived by
# `python3 tools/gen-stdlib-doc-index.py --since-from-tags` from the committed
# signature indexes (docs/stdlib/*.md generated blocks) at every vX.Y.Z[-rcN]
# tag; an rc counts as its final version. The index first shipped in v0.35.0,
# so the declaration source at each tag is read too (stdlib/<module>.almd,
# stdlib/defs/<module>.toml, crates/almide-types/src/time_units.rs). A bare
# X.Y.Z is exact: the index looked one release earlier and the name was not
# there. `<=X.Y.Z` = "X.Y.Z or earlier": the first release at which any
# reading saw the name; the trees cannot see further back.
# `unreleased` = no release tag carries it yet; the plain regenerate adds that
# row for a new fn, and rerunning --since-from-tags after the next tag stamps it.
# The rendered `// @since` lines in docs/stdlib/*.md read from this file.
"""


def load_since() -> dict:
    if not os.path.exists(SINCE):
        return {}
    table = {}
    for n, line in enumerate(open(SINCE, encoding="utf-8").read().split("\n"), 1):
        s = line.strip()
        if not s or s.startswith("#") or s == "[since]":
            continue
        m = re.fullmatch(r'"([a-z_0-9]+\.[A-Za-z_0-9]+)" = "(unreleased|(?:<=)?\d+\.\d+\.\d+)"', s)
        if not m:
            raise SystemExit(f"error: {os.path.relpath(SINCE, REPO)}:{n}: unreadable row: {line!r}")
        table[m.group(1)] = m.group(2)
    return table


def render_since_table(table: dict) -> str:
    rows = [f'"{k}" = "{v}"' for k, v in sorted(table.items())]
    return SINCE_HEADER + "\n[since]\n" + "\n".join(rows) + "\n"


TAG = re.compile(r"^v(\d+)\.(\d+)\.(\d+)(?:-rc(\d+))?$")
SIG_NAME = re.compile(r"^(?:effect )?([a-z_0-9]+)\.([a-z_0-9]+)\(")
TYPE_NAME = re.compile(r"^type ([a-z_0-9]+)\.([A-Za-z_0-9]+)")


def _version_key(v: str) -> tuple:
    """Order since values: earlier release first; at one release the exact
    value beats the `<=` bound (it says more)."""
    bound = v.startswith("<=")
    return tuple(int(p) for p in v.lstrip("<=").split(".")) + (0 if not bound else 1,)


def since_from_tags(ifaces: dict, clocks: dict) -> dict:
    """The release that first shipped each public name (module docstring).

    Primary source: the committed signature index at every release tag. A
    name first seen at tag T is exact when the index could already have seen
    it one release earlier (its module's page carried a block, and — for a
    type — the index already rendered types); otherwise it is only a `<=`
    bound: the first time the index LOOKED. The index first shipped in
    v0.35.0, and the exported types and clock constructors entered it later
    still, so the declaration SOURCE at each tag is read as well: a
    top-level `fn name` / `type Name` in stdlib/<module>.almd, a `[name]`
    section in the pre-self-hosting stdlib/defs/<module>.toml, and the unit
    and clock rows of crates/almide-types/src/time_units.rs for the clock
    constructors. A source match is always a `<=` bound (a sighting, not a
    proof the name was public, and the stdlib lived in the compiler before
    either file existed). An exact index reading wins; otherwise the
    earliest bound does. A name no reading found is `unreleased`."""
    def git(*a):
        r = subprocess.run(["git", "-C", REPO, *a], capture_output=True, text=True)
        if r.returncode != 0:
            raise SystemExit(f"error: git {' '.join(a[:2])} failed:\n{r.stderr.strip()}")
        return r.stdout

    def read_blobs(pairs: list) -> dict:
        """(tag, path) -> text for every pair that exists, in ONE
        `git cat-file --batch` process (a `git show` per pair is ~10k forks)."""
        query = "".join(f"{tag}:{path}\n" for tag, path in pairs).encode()
        raw = subprocess.run(["git", "-C", REPO, "cat-file", "--batch"], input=query,
                             capture_output=True, check=True).stdout
        blobs, pos = {}, 0
        for pair in pairs:
            nl = raw.index(b"\n", pos)
            header = raw[pos:nl].decode()
            pos = nl + 1
            if header.endswith(" missing"):
                continue
            size = int(header.split()[2])
            blobs[pair] = raw[pos:pos + size].decode("utf-8", "replace")
            pos += size + 1
        return blobs

    tags = []
    for t in git("tag", "-l", "v*").split():
        m = TAG.match(t)
        if m:
            a, b, c, rc = m.groups()
            tags.append(((int(a), int(b), int(c), int(rc) if rc else 1 << 30), t, f"{a}.{b}.{c}"))
    if not tags:
        raise SystemExit("error: no vX.Y.Z tags in this clone — `git fetch --tags` first")
    tags.sort()

    # (1) the committed signature index at each tag
    pages = {tag: [f for f in git("ls-tree", "-r", "--name-only", tag, "--", "docs/stdlib/").split()
                   if f.endswith(".md")] for _, tag, _ in tags}
    page_text = read_blobs([(tag, f) for tag, files in pages.items() for f in files])
    first = {}
    prev_blocks, prev_types_indexed = set(), False
    for _, tag, version in tags:
        seen, blocks, types_indexed = {}, set(), False
        for f in pages[tag]:
            module = os.path.basename(f)[:-3]
            on = False
            for line in page_text[(tag, f)].split("\n"):
                # The marker substrings, not the full lines: the same reading
                # scripts/check-interface-diff.sh applies to any past tree.
                if "BEGIN GENERATED SIGNATURE INDEX" in line:
                    on = True
                    blocks.add(module)
                elif "END GENERATED SIGNATURE INDEX" in line:
                    on = False
                elif on:
                    m = SIG_NAME.match(line)
                    t = TYPE_NAME.match(line)
                    if t:
                        types_indexed = True
                    m = m or t
                    if m and not m.group(2).startswith("__"):
                        seen[f"{m.group(1)}.{m.group(2)}"] = bool(t)
        for name, is_type in seen.items():
            if name in first:
                continue
            looked = name.split(".")[0] in prev_blocks and (prev_types_indexed or not is_type)
            first[name] = version if looked else f"<={version}"
        prev_blocks, prev_types_indexed = blocks, types_indexed

    # (2) the declaration source at each tag — only ever a `<=` bound
    readers = {}  # name -> [(path, predicate over the file text)]
    for module, iface in ifaces.items():
        almd, defs = f"stdlib/{module}.almd", f"stdlib/defs/{module}.toml"
        for f in iface["functions"]:
            n = re.escape(f["name"])
            if module in clocks:
                unit_row = re.compile(rf'\("{n}",\s*[\d_]+\)')
                clock_row = re.compile(rf'\("{re.escape(module)}",\s*"\w+"\)')
                readers[f"{module}.{f['name']}"] = [(
                    "crates/almide-types/src/time_units.rs",
                    lambda s, u=unit_row, c=clock_row: u.search(s) and c.search(s))]
            else:
                readers[f"{module}.{f['name']}"] = [
                    (almd, re.compile(rf"^(?:effect )?fn {n}\b", re.M).search),
                    (defs, re.compile(rf"^\[{n}\]\s*$", re.M).search)]
        for t in iface["types"]:
            readers[f"{module}.{t['name']}"] = [(almd, re.compile(rf"^type {re.escape(t['name'])}\b", re.M).search)]
    paths = sorted({p for rs in readers.values() for p, _ in rs})
    blobs = read_blobs([(tag, p) for _, tag, _ in tags for p in paths])
    from_source = {}
    for name, rs in readers.items():
        for _, tag, version in tags:
            if any((tag, p) in blobs and present(blobs[(tag, p)]) for p, present in rs):
                from_source[name] = f"<={version}"
                break

    # The index is the public surface itself (the interface JSON rendered), so
    # an EXACT index reading — the index looked one release earlier and the
    # name was not there — wins over an earlier source match (a top-level `fn`
    # in the file is not proof it was public). Otherwise the earliest bound of
    # either reading is the value. The late kinds are read from source at
    # every tag, so a name no reading found is one no release carried.
    out = {}
    for name in public_names(ifaces):
        idx, src = first.get(name), from_source.get(name)
        if idx and not idx.startswith("<="):
            out[name] = idx
        else:
            readings = [v for v in (idx, src) if v]
            out[name] = min(readings, key=_version_key) if readings else UNRELEASED
    return out


def apply(path: str, check: bool, block: str) -> bool:
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
    from_tags = "--since-from-tags" in sys.argv
    if check and from_tags:
        raise SystemExit("error: --check and --since-from-tags are exclusive")
    units, clocks = clock_table()
    pages = sorted(n[:-3] for n in os.listdir(DOCS) if n.endswith(".md"))
    ifaces = {m: clock_iface(m, units, clocks[m]) if m in clocks else module_iface(m) for m in pages}
    names = public_names(ifaces)
    errors = []

    table = load_since()
    if from_tags:
        table = since_from_tags(ifaces, clocks)
    no_row = sorted(n for n in names if n not in table)
    stale_rows = sorted(n for n in table if n not in names)
    if check:
        if no_row:
            errors.append(f"{len(no_row)} public name(s) without a docs/stdlib/since.toml row: "
                          f"{', '.join(no_row)} — run `make stdlib-docs` and commit")
        if stale_rows:
            errors.append(f"{len(stale_rows)} since.toml row(s) for names that are not public: "
                          f"{', '.join(stale_rows)} — run `make stdlib-docs` and commit")
    else:
        for n in no_row:
            table[n] = UNRELEASED
        for n in stale_rows:
            del table[n]
    since_text = render_since_table({n: table[n] for n in names if n in table})
    current = open(SINCE, encoding="utf-8").read() if os.path.exists(SINCE) else ""
    since_stale = since_text != current
    if since_stale and not check:
        with open(SINCE, "w", encoding="utf-8") as fh:
            fh.write(since_text)

    undocumented = sorted(n for n, has_doc in names.items() if not has_doc)
    if undocumented:
        errors.append(f"{len(undocumented)} public name(s) without a `///` doc comment in "
                      f"stdlib/<module>.almd: {', '.join(undocumented)}")

    stale = []
    since = {n: table.get(n, UNRELEASED) for n in names}
    for module in pages:
        if apply(os.path.join(DOCS, f"{module}.md"), check, module_block(module, ifaces[module], since)):
            stale.append(module)
    if check and since_stale and not (no_row or stale_rows):
        stale.append("since.toml")
    if check and stale:
        errors.append(f"stale stdlib doc signature index for: {', '.join(stale)} "
                      f"— run `make stdlib-docs` and commit")
    for e in errors:
        print(f"::error::{e}")
    if errors:
        return 1
    if not check:
        print(f"updated {len(stale)} module doc(s)" if stale or since_stale else "all up to date")
    return 0


if __name__ == "__main__":
    sys.exit(main())
