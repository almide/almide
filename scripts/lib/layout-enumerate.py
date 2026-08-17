"""Enumerate Rust type declarations and classify their FIELD LAYOUT (#1316).

Shared by scripts/check-layout-discipline.sh and by anyone auditing the ledger,
so the gate and the ledger cannot drift apart (same pattern as
scripts/lib/scalar-read-enumerate.py).

What it extracts, per `struct`/`enum` declaration under the compiler crates and
the CLI crate:

    (relative path, type name) -> {kind, line, serde, fields, banned}

`serde` is True when the declaration's own attribute block derives
`Serialize`/`Deserialize` — the mechanical signal that this type crosses a
process boundary (a cache, an on-disk artifact, a JSON payload) and is
therefore LONG-LIVED in the sense of #1316.

`banned` lists (field, type expression, reason) for every field whose type
expression names a construct the layout discipline rules out of a new
long-lived structure: owned/borrowed strings, pointer indirection, hash/tree
maps, references, and the pre-existing pointer-rich house trees.

THIS IS A TEXT SCAN, not a type resolver. It sees type EXPRESSIONS as written.
An alias, a type parameter, or a struct from another crate is opaque to it and
is reported as such — see the "cannot check" list in the gate header.
"""

import os
import re

# Directories scanned. `crates/*/src` is the compiler; `src` is the CLI crate.
SCAN_ROOTS = ("crates", "src")

# Not shipped data: a fixture type in a test or example is not long-lived, and
# making authors register them would train everyone to rubber-stamp rows.
SKIP_DIRS = {"target", ".git", "tests", "examples", "benches"}

_DECL = re.compile(r'^(?:pub(?:\([^)]*\))?\s+)?(struct|enum)\s+([A-Za-z_]\w*)')

# (regex over the field's type expression, why the discipline rules it out)
BANNED = [
    (r'\bString\b',
     "owned String — a name belongs in the interner; the row carries a Sym"),
    (r'&\s*(?:\'\w+\s+)?str\b',
     "&str — borrowed text pins the row to a buffer's lifetime"),
    (r'\bPathBuf\b', "PathBuf — owned text with a platform-dependent encoding"),
    (r'\bCow\s*<', "Cow — a maybe-owned string is still a string"),
    (r'\bBox\s*<', "Box — pointer indirection; use a u32 index into a side table"),
    (r'\bRc\s*<', "Rc — pointer indirection + refcount"),
    (r'\bArc\s*<', "Arc — pointer indirection + atomic refcount"),
    (r'\bRefCell\s*<', "RefCell — interior mutability defeats a flat rebuild"),
    (r'\bCell\s*<', "Cell — interior mutability defeats a flat rebuild"),
    (r'\b(?:Mutex|RwLock)\s*<', "lock — a long-lived row is data, not a channel"),
    (r'\b(?:HashMap|BTreeMap|IndexMap)\s*<',
     "map — must be rebuilt entry-by-entry on load; store the pairs flat and index them"),
    (r'\b(?:HashSet|BTreeSet|IndexSet)\s*<',
     "set — must be rebuilt element-by-element on load; store the elements flat"),
    (r'\bVec\s*<\s*Vec\s*<', "Vec<Vec<..>> — nested arenas are not flat; flatten + offsets"),
    (r'&\s*(?:\'\w+)?\s*(?:mut\s+)?[A-Za-z_\[]',
     "reference — a long-lived row must own its bytes"),
    (r'\b(?:Ty|IrExpr|IrStmt|IrPattern|Program|Decl)\b',
     "pre-existing pointer-rich house tree — embedding it inherits its layout"),
]
BANNED = [(re.compile(p), why) for p, why in BANNED]


def _split_top(text, seps=","):
    """Split at top-level separators, tracking <> () [] {} depth. `->` is not a close."""
    out, depth, cur, prev = [], 0, [], ""
    for ch in text:
        if ch in "<([{":
            depth += 1
        elif ch in ">)]}":
            if not (ch == ">" and prev == "-"):
                depth -= 1
        if ch in seps and depth <= 0:
            out.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
        prev = ch
    if "".join(cur).strip():
        out.append("".join(cur))
    return [p.strip() for p in out if p.strip()]


def _strip_noise(body):
    """Drop line comments and attribute lines from a declaration body."""
    keep = []
    for line in body.splitlines():
        s = line.split("//")[0].strip()
        if not s or s.startswith("#[") or s.startswith("#!"):
            continue
        keep.append(s)
    return " ".join(keep)


def _fields_of_struct(body):
    fields = []
    for part in _split_top(_strip_noise(body)):
        halves = _split_top(part, ":")
        if len(halves) >= 2:
            name = halves[0].replace("pub", "").strip()
            fields.append((name, ":".join(halves[1:]).strip()))
        else:  # tuple-struct element
            fields.append((str(len(fields)), part.replace("pub", "").strip()))
    return fields


def _fields_of_enum(body):
    fields = []
    for variant in _split_top(_strip_noise(body)):
        m = re.match(r'([A-Za-z_]\w*)\s*(.*)$', variant, re.S)
        if not m:
            continue
        vname, payload = m.group(1), m.group(2).strip()
        if payload.startswith("{"):
            for fname, fty in _fields_of_struct(payload[1:payload.rfind("}")]):
                fields.append((f"{vname}.{fname}", fty))
        elif payload.startswith("("):
            for i, fty in enumerate(_split_top(payload[1:payload.rfind(")")])):
                fields.append((f"{vname}.{i}", fty))
    return fields


def _classify(fields):
    out = []
    for name, ty in fields:
        for rx, why in BANNED:
            if rx.search(ty):
                out.append((name, ty, why))
                break
    return out


def _scan_file(path, rel):
    text = open(path, encoding="utf-8", errors="replace").read()
    lines = text.splitlines()
    found = {}
    attrs, i = [], 0
    while i < len(lines):
        s = lines[i].strip()
        if s.startswith("#["):
            depth = s.count("[") - s.count("]")
            attrs.append(s)
            while depth > 0 and i + 1 < len(lines):
                i += 1
                attrs.append(lines[i].strip())
                depth += lines[i].count("[") - lines[i].count("]")
            i += 1
            continue
        m = _DECL.match(s)
        if m:
            kind, name = m.group(1), m.group(2)
            attr_text = " ".join(attrs)
            serde = ("Serialize" in attr_text or "Deserialize" in attr_text) and "derive" in attr_text
            # locate the body, starting at this line
            body, opener = "", None
            joined = "\n".join(lines[i:])
            for ch in joined:
                if ch in "{(;":
                    opener = ch
                    break
            if opener in ("{", "("):
                close = "}" if opener == "{" else ")"
                start = joined.index(opener)
                depth, end = 0, None
                for j in range(start, len(joined)):
                    if joined[j] == opener:
                        depth += 1
                    elif joined[j] == close:
                        depth -= 1
                        if depth == 0:
                            end = j
                            break
                if end is not None:
                    body = joined[start + 1:end]
            fields = _fields_of_enum(body) if kind == "enum" else _fields_of_struct(body)
            found[(rel, name)] = {
                "kind": kind,
                "line": i + 1,
                "serde": serde,
                "fields": fields,
                "banned": _classify(fields),
            }
            attrs = []
            i += 1
            continue
        if s and not s.startswith("//"):
            attrs = []
        i += 1
    return found


def enumerate_types(root):
    """All type declarations under SCAN_ROOTS, keyed by (relative path, name)."""
    out = {}
    for base in SCAN_ROOTS:
        for dirpath, dirnames, filenames in os.walk(os.path.join(root, base)):
            dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
            for fn in filenames:
                if not fn.endswith(".rs"):
                    continue
                full = os.path.join(dirpath, fn)
                rel = os.path.relpath(full, root)
                out.update(_scan_file(full, rel))
    return out


if __name__ == "__main__":  # ad-hoc audit helper
    import sys
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    types = enumerate_types(root)
    serde_by_file = {}
    for (rel, name), info in types.items():
        if info["serde"]:
            serde_by_file[rel] = serde_by_file.get(rel, 0) + 1
    print(f"{len(types)} type declaration(s)")
    for rel, n in sorted(serde_by_file.items(), key=lambda kv: -kv[1]):
        print(f"  serde {n:3d}  {rel}")
    for key in sys.argv[2:]:
        rel, name = key.split("::")
        info = types[(rel, name)]
        print(f"\n{key}: {info}")
