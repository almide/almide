#!/usr/bin/env python3
"""Record the signature of every stdlib fn a dialect epoch renamed (#4525).

`almide fix` applies a ledger rename (`` `ffi.address(b)` → rewrite as
`ffi.region(b)` ``) unattended only when the old and the new fn have the same
type. The new fn's signature is in the stdlib; the old one left it, so its
type survives only in the history of the machine-owned signature index
(docs/stdlib/*.md, written by tools/gen-stdlib-doc-index.py from the
compiler's interface JSON, aliases already resolved).

This script reads, for each rename the ledger (`proofs/dialect-epochs.toml`)
states, the index line of the old fn at the last commit that still had it,
and the index line of the new fn now, and writes both to
`crates/almide-frontend/src/generated/removed-signatures.tsv`:

    <old name> TAB <old index line> TAB <new index line> TAB <commit that removed the old line>

The checker compares the two lines' types. tests/removed_signatures_test.rs
fails when a ledger rename has no row, or a row's new line is not the
current index line — rerun this script then.

Usage: python3 tools/gen-removed-signatures.py   (needs the git history)
"""
import pathlib
import re
import subprocess
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parent.parent
OUT = REPO / "crates/almide-frontend/src/generated/removed-signatures.tsv"
# One declaration line of the index, exactly the shape the index writer emits
# (`[effect ]module.fn(params) -> ret`); prose and examples in the same files
# never end in `-> <type>` after a parameter list of `name: Type`s.
SIG = re.compile(r"^(effect )?([a-z_0-9]+\.[a-z_0-9]+)\(((?:mut )?[a-z_0-9]+: .*)?\) -> \S.*$")
DEPRECATED = re.compile(r"\s+\(deprecated — .*\)$")


def between(s, o, c):
    i = s.find(o)
    if i < 0:
        return None
    i += len(o)
    j = s[i:].find(c)
    if j < 0:
        return None
    return s[i:i + j], s[i + j + len(c):]


def callee(call):
    name = call.split("(")[0].split("[")[0].strip()
    if "." not in name:
        return None
    m, f = name.split(".", 1)
    ok = lambda x: x and all(ch.isascii() and (ch.isalnum() or ch == "_") for ch in x)
    return name if ok(m) and ok(f) else None


def renames():
    """Mirror of `removed_spellings::same_shape_rename`: the first ledger
    entry per old name whose `rewrite as` call keeps the argument list."""
    ledger = tomllib.loads((REPO / "proofs/dialect-epochs.toml").read_text())
    out = {}
    for e in ledger.get("epoch", []):
        for text in e.get("breaks", []):
            if not text.startswith("`"):
                continue
            b = between(text, "`", "`")
            if not b:
                continue
            old, rest = b
            name = callee(old)
            if not name or name in out:
                continue
            rest = rest.lstrip()
            if not rest.startswith("→"):
                continue
            rest = rest[1:].lstrip()
            if not rest.startswith("rewrite as "):
                continue
            b2 = between(rest[len("rewrite as "):], "`", "`")
            if not b2:
                continue
            new = b2[0]
            args = lambda c: c.split("(", 1)[1] if "(" in c else None
            if args(old) is None or args(new) is None or args(old) != args(new):
                continue
            to = callee(new)
            if to:
                out[name] = to
    return out


def index_lines(lines):
    on = False
    for line in lines:
        if "BEGIN GENERATED SIGNATURE INDEX" in line:
            on = True
        elif "END GENERATED SIGNATURE INDEX" in line:
            on = False
        elif on:
            yield line


def current_index():
    sigs = {}
    for md in sorted((REPO / "docs/stdlib").glob("*.md")):
        for line in index_lines(md.read_text().splitlines()):
            m = SIG.match(line)
            if m:
                sigs.setdefault(m.group(2), DEPRECATED.sub("", line.rstrip()))
    return sigs


def removed_lines():
    """name -> (last index line, commit that removed it), newest removal first."""
    log = subprocess.run(
        ["git", "-C", str(REPO), "log", "-p", "--format=COMMIT %H", "--", "docs/stdlib/"],
        capture_output=True, text=True, check=True,
    ).stdout
    found = {}
    commit = None
    for line in log.splitlines():
        if line.startswith("COMMIT "):
            commit = line.split()[1]
            continue
        if not line.startswith("-") or line.startswith("---"):
            continue
        m = SIG.match(line[1:])
        if m and m.group(2) not in found:
            found[m.group(2)] = (DEPRECATED.sub("", line[1:].rstrip()), commit)
    return found


def main():
    cur = current_index()
    gone = removed_lines()
    rows, missing = [], []
    for old, new in sorted(renames().items()):
        new_line = cur.get(new)
        # The last line the index had for the old name wins over a current
        # one: a name the stdlib reused later (`http.status`) is another fn.
        if old in gone:
            old_line, commit = gone[old]
        elif old in cur:
            old_line, commit = cur[old], "present"
        else:
            old_line, commit = None, None
        if old_line is None:
            missing.append(f"{old}: no index line in the history of docs/stdlib/")
            continue
        if new_line is None:
            # The rewrite names no fn the index has (`string.chars` "folded
            # with `list.fold`"): there is no type to compare, so `-`, and the
            # checker never applies it unattended.
            print(f"  {old} -> {new}: the new fn has no index line; recorded as `-`")
            new_line = "-"
        rows.append("\t".join([old, old_line, new_line, commit[:12]]))
    header = (
        "# Generated by tools/gen-removed-signatures.py (#4525) — do not edit.\n"
        "# old name\told index line (last commit that had it)\tnew index line (now)\tcommit that removed the old line\n"
    )
    OUT.write_text(header + "\n".join(rows) + "\n")
    print(f"{len(rows)} rename(s) recorded in {OUT.relative_to(REPO)}")
    for m in missing:
        print(f"  missing: {m}", file=sys.stderr)
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
