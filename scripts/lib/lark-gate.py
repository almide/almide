#!/usr/bin/env python3
"""Anti-rot gate for the published grammar artifact (docs/grammar/almide.lark).

Three independent checks, all of which must pass:

  LEXICAL PARITY   the keyword/operator tables in the .lark are diffed
                   token-for-token against crates/almide-syntax/src/lexer.rs.
                   A keyword added to (or removed from) the lexer without the
                   artifact following fails here. The DELIBERATELY EXCLUDED
                   list is checked too: an exclusion must still be a real lexer
                   token (or it is stale) and must NOT be reachable from the
                   grammar.

  CORPUS SUPERSET  every .almd in the corpus that the COMPILER's parser accepts
                   must be accepted by the published grammar. The compiler is
                   consulted through `almide <file> --emit-ast`, which runs the
                   lexer + parser and nothing else (a type error still exits 0).
                   Only the direction "compiler accepts => grammar accepts" is
                   required: over-acceptance is safe for a decoder, under-
                   acceptance would make a legal program unrepresentable.

  DISCRIMINATION   every fixture in docs/grammar/negative-fixtures.txt must be
                   rejected by the grammar AND by the compiler. Without this a
                   grammar that degenerated into `.*` would pass silently.

Usage: lark-gate.py <repo-root> [corpus-dir …]
Env:   ALMIDE  path to the almide binary (default: target/release/almide, then
                $PATH)
"""

from __future__ import annotations

import multiprocessing
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

try:
    from lark import Lark
except ImportError:  # pragma: no cover - environment guard
    sys.exit(
        "lark-gate: the `lark` package is required.\n"
        "  install it with:  python3 -m pip install lark\n"
        "  (CI: add the install to the step that runs scripts/check-lark-grammar.sh)"
    )

GRAMMAR_REL = "docs/grammar/almide.lark"
FIXTURES_REL = "docs/grammar/negative-fixtures.txt"
LEXER_REL = "crates/almide-syntax/src/lexer.rs"


# ── the artifact ────────────────────────────────────────────────────


def build_parser(grammar_text: str) -> Lark:
    return Lark(
        grammar_text,
        start="start",
        parser="earley",
        lexer="basic",
        propagate_positions=False,
        maybe_placeholders=False,
    )


def block(text: str, name: str) -> str:
    """The region between `BEGIN-<name>` and `END-<name>` marker comments."""
    begin = f"BEGIN-{name}"
    end = f"END-{name}"
    try:
        i = text.index(begin)
        j = text.index(end, i)
    except ValueError:
        sys.exit(f"lark-gate: the {name} marker block moved or was deleted in {GRAMMAR_REL}")
    return text[i:j]


def lark_literals(region: str) -> set[str]:
    """Terminal spellings declared in a marker block: `NAME[.prio]: "text"`."""
    out = set()
    for line in region.splitlines():
        line = line.strip()
        if line.startswith("//") or not line:
            continue
        m = re.match(r'^[A-Z_][A-Z0-9_]*(?:\.\d+)?\s*:\s*"((?:[^"\\]|\\.)*)"\s*$', line)
        if m:
            out.add(m.group(1).replace('\\"', '"').replace("\\\\", "\\"))
    return out


def lark_excluded(region: str) -> set[str]:
    return set(re.findall(r'^//\s*EXCLUDED:\s*"([^"]+)"', region, re.M))


# ── the compiler's tables ───────────────────────────────────────────


def rust_table(lexer_src: str, decl: str) -> str:
    i = lexer_src.find(decl)
    if i < 0:
        sys.exit(f"lark-gate: `{decl}` not found in {LEXER_REL} — the table was renamed")
    j = lexer_src.index("];", i)
    return lexer_src[i:j]


def lexer_keywords(lexer_src: str) -> set[str]:
    tbl = rust_table(lexer_src, "const KEYWORDS: &[(&str, TokenType)]")
    return set(re.findall(r'\("([^"]+)"\s*,\s*TokenType::', tbl))


def lexer_operators(lexer_src: str) -> set[str]:
    tbl = rust_table(lexer_src, "const OPERATORS: &[(&str, TokenType, &str)]")
    return set(re.findall(r'\("((?:[^"\\]|\\.)+)"\s*,\s*TokenType::', tbl))


# ── corpus workers ──────────────────────────────────────────────────

_PARSER: Lark | None = None


def _init(grammar_text: str) -> None:
    global _PARSER
    _PARSER = build_parser(grammar_text)


def _try_parse(path_str: str):
    assert _PARSER is not None
    src = Path(path_str).read_text(encoding="utf-8", errors="replace")
    if not src.endswith("\n"):
        src += "\n"
    try:
        _PARSER.parse(src)
        return (path_str, None)
    except Exception as exc:  # lark raises several unrelated exception types
        return (path_str, str(exc).splitlines()[0])


def parses_with_compiler(almide: str, path: Path) -> bool:
    """The compiler's PARSE oracle: `--emit-ast` runs lex+parse only."""
    r = subprocess.run(
        [almide, str(path), "--emit-ast"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return r.returncode == 0


def find_almide(root: Path) -> str:
    env = os.environ.get("ALMIDE")
    if env:
        return env
    local = root / "target" / "release" / "almide"
    if local.exists():
        return str(local)
    from shutil import which

    found = which("almide")
    if not found:
        sys.exit(
            "lark-gate: no almide binary found.\n"
            "  set ALMIDE=<path>, or run `cargo build --release` first."
        )
    return found


# ── fixtures ────────────────────────────────────────────────────────


def read_fixtures(path: Path) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = []
    name: str | None = None
    buf: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("### "):
            if name is not None:
                out.append((name, "\n".join(buf).strip("\n") + "\n"))
            name = line[4:].strip()
            buf = []
        elif name is None:
            continue  # header comments
        else:
            buf.append(line)
    if name is not None:
        out.append((name, "\n".join(buf).strip("\n") + "\n"))
    return out


# ── main ────────────────────────────────────────────────────────────


def main() -> int:
    if len(sys.argv) < 2:
        sys.exit("usage: lark-gate.py <repo-root> [corpus-dir …]")
    root = Path(sys.argv[1]).resolve()
    corpus_dirs = sys.argv[2:] or ["spec", "examples", "stdlib"]

    grammar_path = root / GRAMMAR_REL
    grammar_text = grammar_path.read_text(encoding="utf-8")
    lexer_src = (root / LEXER_REL).read_text(encoding="utf-8")

    errors: list[str] = []

    # ── 1. lexical parity ──
    kw_lark = lark_literals(block(grammar_text, "KEYWORDS"))
    op_lark = lark_literals(block(grammar_text, "OPERATORS"))
    excluded = lark_excluded(block(grammar_text, "EXCLUDED"))
    kw_rs = lexer_keywords(lexer_src)
    op_rs = lexer_operators(lexer_src)

    for missing in sorted(kw_rs - kw_lark):
        errors.append(
            f"keyword {missing!r} is in the lexer but not in {GRAMMAR_REL}'s KEYWORDS block"
        )
    for extra in sorted(kw_lark - kw_rs):
        errors.append(
            f"keyword {extra!r} is in {GRAMMAR_REL} but no longer in the lexer's KEYWORDS table"
        )
    for missing in sorted(op_rs - op_lark - excluded):
        errors.append(
            f"operator {missing!r} is in the lexer but neither in {GRAMMAR_REL}'s "
            "OPERATORS block nor on its EXCLUDED list"
        )
    for extra in sorted(op_lark - op_rs):
        errors.append(
            f"operator {extra!r} is in {GRAMMAR_REL} but no longer in the lexer's OPERATORS table"
        )
    for stale in sorted(excluded - op_rs):
        errors.append(
            f"EXCLUDED {stale!r} is no longer a lexer operator — drop the stale exclusion"
        )
    for leaked in sorted(excluded & op_lark):
        errors.append(f"EXCLUDED {leaked!r} is ALSO declared in the OPERATORS block")

    print(
        f"lexical parity: {len(kw_rs)} keywords, {len(op_rs)} operators "
        f"({len(excluded)} deliberately excluded)"
    )

    # ── 2. the grammar must build ──
    t0 = time.time()
    try:
        parser = build_parser(grammar_text)
    except Exception as exc:
        errors.append(f"the grammar does not build: {exc}")
        return report(errors)
    print(f"grammar builds: {len(parser.rules)} productions in {time.time() - t0:.2f}s")

    almide = find_almide(root)

    # ── 3. discrimination: negative fixtures ──
    fixtures = read_fixtures(root / FIXTURES_REL)
    if len(fixtures) < 20:
        errors.append(
            f"only {len(fixtures)} negative fixtures — the discrimination floor is 20"
        )
    with tempfile.TemporaryDirectory(prefix="lark-gate-") as tmp:
        tmpdir = Path(tmp)
        for name, src in fixtures:
            accepted_by_grammar = True
            try:
                parser.parse(src)
            except Exception:
                accepted_by_grammar = False
            if accepted_by_grammar:
                errors.append(f"negative fixture {name!r} was ACCEPTED by the grammar")
            f = tmpdir / f"neg_{re.sub(r'[^a-z0-9]+', '_', name)}.almd"
            f.write_text(src, encoding="utf-8")
            if parses_with_compiler(almide, f):
                errors.append(
                    f"negative fixture {name!r} PARSES with the compiler — it is not "
                    "invalid Almide any more; fix or retire the fixture"
                )
    print(f"discrimination: {len(fixtures)} negative fixtures, all rejected by both")

    # ── 4. corpus superset ──
    files: list[Path] = []
    for d in corpus_dirs:
        files += sorted((root / d).rglob("*.almd"))
    if not files:
        errors.append(
            f"corpus is EMPTY over {corpus_dirs} — a find-nothing-exit-0 gate is a blind gate"
        )
    t0 = time.time()
    workers = min(os.cpu_count() or 1, 8)
    with multiprocessing.Pool(workers, initializer=_init, initargs=(grammar_text,)) as pool:
        results = pool.map(_try_parse, [str(f) for f in files], chunksize=8)
    accepted = [p for p, e in results if e is None]
    rejected = [(p, e) for p, e in results if e is not None]

    disagreements = []
    for path_str, why in rejected:
        if parses_with_compiler(almide, Path(path_str)):
            disagreements.append((path_str, why))
    for path_str, why in disagreements:
        errors.append(
            f"UNDER-ACCEPT {Path(path_str).relative_to(root)}: the compiler parses it, "
            f"the published grammar does not — {why}"
        )
    print(
        f"corpus superset: {len(accepted)}/{len(files)} accepted by the grammar, "
        f"{len(rejected) - len(disagreements)} rejected by both, "
        f"{len(disagreements)} disagreements ({time.time() - t0:.1f}s, {workers} workers)"
    )

    return report(errors)


def report(errors: list[str]) -> int:
    if errors:
        print("\nLARK GRAMMAR GATE FAIL", file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        return 1
    print("lark-grammar OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
