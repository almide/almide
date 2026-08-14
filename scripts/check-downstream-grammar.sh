#!/usr/bin/env bash
# DOWNSTREAM GRAMMAR SYNC GATE.
#
# WHY THIS EXISTS. The grammar repo's tokens.toml calls itself a "descriptive
# mirror of the compiler lexer" — and a mirror with no gate rots silently: the
# editor/tree-sitter/TextMate generators all consume it, so a keyword the
# language dropped (or an operator it gained) quietly mis-highlights and
# mis-parses in every downstream tool while this repo's CI stays green. The
# 2026-08-15 rot audit found exactly that shape: nothing anywhere failed when
# the two inventories drifted.
#
# WHAT IT DOES. Measures BOTH sides from source — the lexer's KEYWORDS and
# OPERATORS consts (crates/almide-syntax/src/lexer.rs) and the grammar's
# tokens.toml — and fails on any two-sided set difference:
#   - a lexer token absent from tokens.toml  = the grammar LAGS the language
#   - a tokens.toml token absent from the lexer = the grammar lists a spelling
#     the language no longer accepts (stale)
# Comment-only documentation does not count; only list entries are checkable.
#
# WHERE IT RUNS. The weekly `downstream-sync` workflow (+ dispatch) — NOT a PR
# gate: a syntax PR here must be able to land before the grammar repo follows,
# so drift is allowed to exist for at most one schedule tick, never silently.
#
# Usage:
#   GRAMMAR_DIR=/path/to/almide-grammar scripts/check-downstream-grammar.sh
#   (unset GRAMMAR_DIR => shallow-clones the public repo into a temp dir)
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -z "${GRAMMAR_DIR:-}" ]; then
  GRAMMAR_DIR=$(mktemp -d)/almide-grammar
  git clone --quiet --depth 1 https://github.com/almide/almide-grammar.git "$GRAMMAR_DIR"
fi

python3 - "$GRAMMAR_DIR" <<'EOF'
import re, sys, tomllib, pathlib

grammar_dir = pathlib.Path(sys.argv[1])
lexer = pathlib.Path("crates/almide-syntax/src/lexer.rs").read_text()

# ── Lexer side: the two flat consts ──────────────────────────────────────
def const_block(name):
    m = re.search(rf"const {name}[^=]*= &\[(.*?)\n\];", lexer, re.S)
    if not m:
        sys.exit(f"downstream-grammar: cannot find `const {name}` in lexer.rs — "
                 "the extraction broke, not the grammar")
    return m.group(1)

# Strip comments so commented-out rows never count.
def strip_comments(s):
    return re.sub(r"//[^\n]*", "", s)

kw_src = strip_comments(const_block("KEYWORDS"))
lexer_keywords = set(re.findall(r'\("([^"]+)",\s*TokenType::', kw_src))

op_src = strip_comments(const_block("OPERATORS"))
lexer_operators = set(re.findall(r'\("([^"]+)",\s*TokenType::', op_src))

# ── Grammar side: every LIST entry in tokens.toml + alias keys ───────────
toml = tomllib.loads((grammar_dir / "tokens.toml").read_text())
grammar_tokens = set()
for section, table in toml.items():
    if section == "keyword_aliases":
        grammar_tokens |= set(table.keys())
        continue
    for values in table.values():
        if isinstance(values, list):
            grammar_tokens |= set(values)

grammar_keywordish = set()
for sec in ("keywords",):
    for values in toml.get(sec, {}).values():
        grammar_keywordish |= set(values)
grammar_keywordish |= set(toml.get("keyword_aliases", {}).keys())

# ── Two-sided diff ───────────────────────────────────────────────────────
lexer_all = lexer_keywords | lexer_operators
missing = sorted(lexer_all - grammar_tokens)
stale_kw = sorted(grammar_keywordish - lexer_keywords)
grammar_opish = grammar_tokens - grammar_keywordish
stale_op = sorted(grammar_opish - lexer_operators)

ok = True
if missing:
    ok = False
    print("::error::downstream-grammar: the LEXER accepts token(s) tokens.toml does not list "
          f"(the grammar LAGS the language): {missing}")
if stale_kw:
    ok = False
    print("::error::downstream-grammar: tokens.toml lists keyword(s) the lexer no longer accepts "
          f"(STALE — every downstream generator still highlights them): {stale_kw}")
if stale_op:
    ok = False
    print("::error::downstream-grammar: tokens.toml lists operator/delimiter(s) the lexer no longer "
          f"accepts (STALE): {stale_op}")
if not ok:
    print("fix: update tokens.toml in almide/almide-grammar (it is the fan-out point the "
          "tree-sitter and TextMate generators consume), then re-run.")
    sys.exit(1)

print(f"downstream-grammar OK — lexer {len(lexer_keywords)} keywords + "
      f"{len(lexer_operators)} operators, all mirrored; no stale grammar entries.")
EOF
