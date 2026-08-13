#!/usr/bin/env bash
# PUBLISHED-GRAMMAR ANTI-ROT GATE (#1310).
#
# docs/grammar/almide.lark is the machine-readable Almide grammar that
# decode-time constraint engines (llguidance, XGrammar, OpenAI custom tools)
# consume to make syntactically invalid Almide UNREPRESENTABLE at sampling
# time. A hand-written grammar rots the moment the parser moves, and a rotted
# grammar is worse than none — it would constrain a decoder to a language the
# compiler no longer accepts. This gate is what stops that.
#
# It asserts three things (scripts/lib/lark-gate.py does the work):
#
#   LEXICAL PARITY   the artifact's keyword/operator tables are diffed
#                    token-for-token against crates/almide-syntax/src/lexer.rs,
#                    including the DELIBERATELY EXCLUDED list (`&&`, `||`,
#                    `++`, `..=` — tokens the lexer emits only so the parser
#                    can answer with a migration hint).
#   CORPUS SUPERSET  every .almd in spec/ + examples/ + stdlib/ that the
#                    COMPILER's parser accepts is accepted by the artifact.
#                    The oracle is `almide <file> --emit-ast`, which runs
#                    lex+parse and nothing else.
#   DISCRIMINATION   every fixture in docs/grammar/negative-fixtures.txt is
#                    rejected by BOTH the artifact and the compiler, so the
#                    grammar cannot pass by degenerating into `.*`.
#
# WHAT IT DOES NOT PROVE: that the artifact accepts ONLY valid Almide. The
# published grammar is deliberately a SUPERSET (over-acceptance costs a decoder
# nothing; under-acceptance would make a legal program unsamplable), and the
# corpus check only ever fails on the under-accept direction. See
# docs/grammar/README.md for the enumerated gap list.
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if ! python3 -c "import lark" 2>/dev/null; then
  echo 'LARK GRAMMAR GATE FAIL — the python `lark` package is missing.' >&2
  echo "  install it:  python3 -m pip install lark" >&2
  echo "  (CI: the step that runs this script must install it first)" >&2
  exit 1
fi

exec python3 "$ROOT/scripts/lib/lark-gate.py" "$ROOT" "$@"
