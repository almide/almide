#!/usr/bin/env python3
"""Enumerate the structural debt of the Rust workspace.

codopsy grades one function and one file at a time (length, depth,
complexity). It cannot see what only shows across files: a crate that exports
more than anyone imports, a file no module tree reaches, a function nobody
names, a file cut in two to stay under a line limit. This enumerator counts
those four, one TSV row each, re-derived from source on every run:

  pub-unused   <crate>          the number of `pub` items in the crate's `src/`
                                whose name appears nowhere outside that `src/`
                                — not in another crate, not in the CLI, not in
                                any test or example, the crate's own included.
                                Each is a `pub(crate)` that was never written.
  split-file   <crate>          the number of files whose stem ends in a bare
                                letter, a number or a part marker
                                (`desugar_b.rs`, `binds_p2.rs`,
                                `tests_part1.rs`): a file divided by size, not
                                by what it does.
  orphan-file  <path>           a `.rs` file under a crate's `src/` (or the
                                root `src/`) that no `mod` declaration, no
                                `#[path]` and no `include!` names, with its
                                line count. It compiles into nothing. A
                                mention in a comment, a ledger or a document
                                does not count as inclusion.
  orphan-fn    <path>::<name>   a function whose name occurs exactly once in
                                all Rust here and nowhere in the stdlib, the
                                runtime, the tests, the scripts, the spec or
                                the proofs, with its line count. A method of
                                an `impl Trait for T` block is skipped: its
                                caller is whoever holds the trait.

BLIND SPOTS, on purpose and recorded in the gate ledger: the match is by NAME,
so two unused functions that call each other are not found, a name shared with
any unrelated item hides an unused one, and an item reached only through a
macro-built name reads as unused. The counts are a floor, not a census.

This script only COUNTS and prints. The gate that compares against
proofs/structure-ledger.toml, and the only writer of that ledger, is
`tools/almide-gates structure` (the ledger is a structured artifact, so its
reader is Almide, #2128).

Usage: structure-enumerate.py <repo-root>
       structure-enumerate.py <repo-root> --list pub-unused <crate>
       structure-enumerate.py <repo-root> --list split-file <crate>
"""
import collections
import os
import re
import sys

WORD = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
PUB_ITEM = re.compile(
    r"^\s*pub\s+(?:const\s+|async\s+|unsafe\s+)*"
    r"(fn|struct|enum|trait|type|const|static|mod)\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.M,
)
FN_DEF = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?"
    r"(?:extern\s+\"[^\"]*\"\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.M,
)
SPLIT_STEM = re.compile(r"_(?:[a-f]|p\d+|part\d+|tail|\d+)$")
# Names every type implements or the toolchain calls: one occurrence is normal.
CONVENTIONAL_FNS = frozenset(
    ["main", "new", "default", "fmt", "from", "drop", "eq", "hash", "clone", "next"]
)
EXPORT_ATTR = re.compile(r"#\[(?:test|no_mangle|export_name|wasm_bindgen|bench)")
# Where a name may be referenced from besides Rust under crates/, src/, tests/.
REFERENCE_TREES = ("stdlib", "runtime", "scripts", "spec", "proofs", "tools", ".github")
# Code only. A ledger (.toml, .txt) that lists a name does not use it — and this
# gate's own ledger lists every orphan by name, so reading it back would hide
# each finding on the run after it was pinned.
REFERENCE_EXTS = (".almd", ".rs", ".sh", ".py", ".v", ".tmpl", ".yml", ".yaml")
# This gate's own sources name the findings they were written about.
OWN_SOURCES = frozenset(
    ["scripts/lib/structure-enumerate.py", "tools/almide-gates/src/structure.almd"]
)
AUTO_DISCOVERED_DIRS = ("tests", "examples", "benches", "bin")


def read(path):
    with open(path, encoding="utf-8", errors="replace") as fh:
        return fh.read()


def walk(root, top, exts):
    base = os.path.join(root, top)
    for d, dirs, files in os.walk(base):
        dirs[:] = sorted(x for x in dirs if x not in ("target", ".git", "node_modules"))
        for f in sorted(files):
            if f.endswith(exts):
                yield os.path.relpath(os.path.join(d, f), root)


def crate_of(rel):
    parts = rel.split("/")
    return parts[1] if parts[0] == "crates" and len(parts) > 2 else None


def is_generated(rel):
    return "/generated/" in rel


class Tree:
    def __init__(self, root):
        self.root = root
        self.rust = {}
        for top in ("crates", "src", "tests"):
            for rel in walk(root, top, (".rs",)):
                self.rust[rel] = read(os.path.join(root, rel))
        self.crates = sorted(
            c
            for c in os.listdir(os.path.join(root, "crates"))
            if os.path.isfile(os.path.join(root, "crates", c, "Cargo.toml"))
        )
        self.other = collections.Counter()
        self.other_text = []
        for top in REFERENCE_TREES:
            if not os.path.isdir(os.path.join(root, top)):
                continue
            for rel in walk(root, top, REFERENCE_EXTS):
                if rel in OWN_SOURCES:
                    continue
                text = read(os.path.join(root, rel))
                self.other.update(WORD.findall(text))
                self.other_text.append(text)
        for name in ("Makefile", "build.rs", "lefthook.yml", "Cargo.toml"):
            p = os.path.join(root, name)
            if os.path.isfile(p):
                text = read(p)
                self.other.update(WORD.findall(text))
                self.other_text.append(text)

    def in_crate_src(self, rel, crate):
        return rel.startswith(f"crates/{crate}/src/")


def pub_unused(tree, crate):
    """`pub` items of `crate`'s src/ named nowhere outside that src/."""
    outside = collections.Counter()
    for rel, text in tree.rust.items():
        if not tree.in_crate_src(rel, crate):
            outside.update(WORD.findall(text))
    out = []
    for rel in sorted(tree.rust):
        if not tree.in_crate_src(rel, crate) or is_generated(rel):
            continue
        text = tree.rust[rel]
        for m in PUB_ITEM.finditer(text):
            kind, name = m.group(1), m.group(2)
            if outside[name] == 0 and tree.other[name] == 0:
                line = text.count("\n", 0, m.start()) + 1
                out.append((rel, line, kind, name))
    return out


def split_files(tree, crate):
    out = []
    for rel in sorted(tree.rust):
        if crate_of(rel) != crate or is_generated(rel):
            continue
        stem = os.path.basename(rel)[:-3]
        if SPLIT_STEM.search(stem):
            out.append(rel)
    return out


def orphan_files(tree):
    out = []
    # A file is pulled in by a `mod` declaration, or by name through `#[path = ...]`
    # / `include!(...)`. A mention in a comment, a ledger or a document includes
    # nothing: pass_lambda_type_lookup.rs was cited by ARCHITECTURE.md and by three
    # ledger rows, and edited in three later commits, while no `mod` reached it.
    included = collections.Counter()
    for text in tree.rust.values():
        for line in text.split("\n"):
            if "#[path" in line or "include" in line:
                included.update(re.findall(r"[A-Za-z0-9_]+\.rs", line))
    by_dir = collections.defaultdict(list)
    for rel in tree.rust:
        by_dir[os.path.dirname(rel)].append(rel)
    for rel in sorted(tree.rust):
        parts = rel.split("/")
        in_src = (parts[0] == "crates" and len(parts) > 3 and parts[2] == "src") or parts[0] == "src"
        if not in_src:
            continue
        base = os.path.basename(rel)
        stem = base[:-3]
        if stem in ("lib", "main", "mod", "build"):
            continue
        if any(seg in AUTO_DISCOVERED_DIRS for seg in parts[:-1]):
            continue
        d = os.path.dirname(rel)
        declared = re.compile(r"\bmod\s+" + re.escape(stem) + r"\b")
        neighbours = [p for p in by_dir[d] + by_dir[os.path.dirname(d)] if p != rel]
        if any(declared.search(tree.rust[p]) for p in neighbours):
            continue
        if included[base] > 0:
            continue
        out.append((rel, tree.rust[rel].count("\n")))
    return out


def in_trait_impl(lines, at, indent):
    """True when the fn at `lines[at]` sits in an `impl Trait for T` block.

    Its caller is whoever holds the trait object — a library, often — so the name
    appearing once says nothing about use.
    """
    for k in range(at - 1, -1, -1):
        line = lines[k]
        if not line.strip():
            continue
        if len(line) - len(line.lstrip()) < indent and re.match(r"\s*(?:unsafe\s+)?impl\b", line):
            return re.search(r"\sfor\s", line) is not None
    return False


def orphan_fns(tree):
    counts = collections.Counter()
    for text in tree.rust.values():
        counts.update(WORD.findall(text))
    out = []
    for rel in sorted(tree.rust):
        if is_generated(rel) or rel.startswith("tests/"):
            continue
        text = tree.rust[rel]
        lines = text.split("\n")
        for m in FN_DEF.finditer(text):
            name = m.group(1)
            if name in CONVENTIONAL_FNS or counts[name] != 1 or tree.other[name] != 0:
                continue
            ln = text.count("\n", 0, m.start(1)) + 1
            head = "\n".join(lines[max(0, ln - 6):ln])
            if EXPORT_ATTR.search(head):
                continue
            indent = len(lines[ln - 1]) - len(lines[ln - 1].lstrip())
            if in_trait_impl(lines, ln - 1, indent):
                continue
            size = 1
            for k in range(ln, min(len(lines), ln + 600)):
                size += 1
                if lines[k].rstrip() == " " * indent + "}":
                    break
            out.append((rel, name, size))
    return out


def main():
    if len(sys.argv) < 2:
        print(__doc__.strip().split("\n\n")[-1], file=sys.stderr)
        return 2
    root = sys.argv[1]
    tree = Tree(root)
    if len(sys.argv) >= 5 and sys.argv[2] == "--list":
        kind, crate = sys.argv[3], sys.argv[4]
        if kind == "pub-unused":
            for rel, line, k, name in pub_unused(tree, crate):
                print(f"{rel}:{line}\t{k} {name}")
        elif kind == "split-file":
            for rel in split_files(tree, crate):
                print(rel)
        else:
            return 2
        return 0
    rows = []
    for crate in tree.crates:
        rows.append(("pub-unused", crate, len(pub_unused(tree, crate))))
        rows.append(("split-file", crate, len(split_files(tree, crate))))
    for rel, n in orphan_files(tree):
        rows.append(("orphan-file", rel, n))
    for rel, name, size in orphan_fns(tree):
        rows.append(("orphan-fn", f"{rel}::{name}", size))
    # A row the gate can refuse a blinded run on: how much Rust was actually read.
    rows.append(("scanned", "rust-files", len(tree.rust)))
    for kind, key, value in rows:
        print(f"{kind}\t{key}\t{value}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
