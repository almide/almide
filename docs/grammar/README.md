# The published Almide grammar

`almide.lark` is the machine-readable Almide grammar. It exists for one
reason: **decode-time constraint engines** — [llguidance](https://github.com/guidance-ai/llguidance),
[XGrammar](https://github.com/mlc-ai/xgrammar), OpenAI custom tools (Lark CFG
input) — can only force syntactic validity at token-sampling time if a
consumable CFG exists. With one, syntactically invalid Almide becomes
*unrepresentable* rather than merely diagnosed. That is the strongest accuracy
lever this repo has: it acts before the model can make the mistake.

Evidence for the approach: CangjieBench (arXiv 2603.14501) — for a
low-resource language, syntax-constrained generation is the best
accuracy/cost trade-off. CRANE (arXiv 2502.09061) — constrain the *emission*,
never the reasoning.

## Files

| File | What it is |
|---|---|
| `almide.lark` | the grammar artifact (Lark syntax, Earley + basic lexer) |
| `negative-fixtures.txt` | invalid programs the grammar must REJECT |
| `../../scripts/check-lark-grammar.sh` | the anti-rot gate |
| `../../scripts/lib/lark-gate.py` | the gate's implementation |

## Using it

```python
from lark import Lark
parser = Lark(open("docs/grammar/almide.lark").read(),
              start="start", parser="earley", lexer="basic")
parser.parse(open("app.almd").read())
```

For llguidance / XGrammar, hand the same file to the engine's Lark front end.

## The anti-rot gate — exactly what it checks

A hand-written grammar rots the moment the parser changes, and a rotted
grammar is *worse than none*: it would constrain a decoder to a language the
compiler no longer accepts. `scripts/check-lark-grammar.sh` is what stops
that. It asserts three independent things.

**1. Lexical parity.** The `BEGIN-KEYWORDS` / `BEGIN-OPERATORS` marker blocks
in `almide.lark` are diffed token-for-token against the `KEYWORDS` and
`OPERATORS` tables in `crates/almide-syntax/src/lexer.rs`. Adding a keyword to
the lexer without adding it here fails the gate, and vice versa. The
`EXCLUDED` list is checked in both directions too: an excluded operator must
still be a real lexer token (or the exclusion is stale) and must not also be
declared in the grammar.

**2. Corpus superset.** Every `.almd` under `spec/`, `examples/` and `stdlib/`
is parsed with the published grammar. For each file the grammar *rejects*, the
compiler is consulted through `almide <file> --emit-ast` — which runs the lexer
and parser and nothing else (a type error still exits 0). If the compiler
parses a file the grammar does not, that is a hard failure.

Only that direction is required. The grammar is a deliberate **superset**:
over-acceptance costs a decoder nothing (it merely fails to block a bad token),
whereas under-acceptance makes a legal program unsamplable.

**3. Discrimination.** Every fixture in `negative-fixtures.txt` must be
rejected by the grammar *and* by the compiler. Without this, a grammar that
degenerated into `.*` would sail through check 2. The gate also enforces a
floor on the fixture count.

## What the gate does NOT prove

- **Not soundness.** It does not prove the grammar accepts *only* valid
  Almide. It is a superset by construction; check 2 can only ever fail in the
  under-accept direction, and check 3 only pins the named counterexamples in
  `negative-fixtures.txt`.
- **Not coverage beyond the corpus.** A syntax feature that no file in
  `spec/`, `examples/` or `stdlib/` uses is not exercised. A new construct
  landed *without* a spec fixture would not be caught here — which is one more
  reason new syntax always ships with a `spec/` test.
- **Nothing semantic.** `--emit-ast` is parse-only: types, effects, module
  resolution and the whole checker are out of scope.
- **Not engine portability.** The gate uses Python `lark` (Earley + basic
  lexer). It does not prove that llguidance or XGrammar compile this exact
  dialect; those engines accept Lark-*like* syntax with their own restrictions.

## Known gaps — what the grammar cannot express

These are honest limits, not bugs. Where the artifact is *looser* than the
compiler a decoder is simply unconstrained there; where it is *tighter* the
corpus check proves no real Almide is affected.

| # | Gap | Direction |
|---|---|---|
| G1 | **Nested block comments.** `/* … */` is a non-greedy regex; the lexer counts nesting depth. `/* /* */ */` leaves a stray `*/`. | tighter |
| G2 | **Heredoc dedent.** `"""…"""` is one opaque terminal. `strip_heredoc_indent` is a *value* transformation, not a syntactic one, so nothing syntactic is lost — but the artifact cannot express the closing-delimiter indentation rule. | n/a |
| G3 | **Interpolation holes.** `${…}` is matched *lexically*, with nested `"…"`/`'…'` literals and up to three levels of `{ }` nesting. The compiler re-lexes and re-parses the hole with a sub-parser, so its contents are full expressions. A decoder is therefore **unconstrained inside a hole**, and holes nested deeper than three brace levels are rejected. This is the one place a single CFG-over-text cannot mirror the compiler's two-phase lexing. | both |
| G4 | **Whitespace-sensitive unary minus.** `a⏎- b` continues a subtraction; `a⏎-b` (attached) starts a new statement. Token-level grammars cannot see the gap, so both readings are accepted. | looser |
| G5 | **Diagnostics-only restrictions.** The `??` line-crossing rule (E038/#1112), terminal `??` (E038), positional-after-named arguments, and the "every parameter after the first default needs one" rule are parser *diagnostics*, not grammar. The artifact accepts them. | looser |
| G6 | **Reserved contextual identifiers.** `as`, `where` and the type name `Fn` are matched by spelling in the compiler but are ordinary identifiers to its lexer. The artifact reserves them, so `let as = 1` — legal Almide — is rejected. No corpus file does this. `self` is deliberately *not* reserved (it is a value in every convention method body), so the bare receiver parameter is spelled as "the first parameter may omit `: Type`" — the artifact accepts `fn f(x) -> Int = 1`, which the compiler rejects. | both |
| G7 | **`fan.<head>`.** The head name is left open (any identifier); the compiler admits only `bounded`/`timeout`/`race`/`settle`/`any`. | looser |
| G8 | **Statement separators are required** (a newline or `;`). The compiler tolerates two statements adjacent on one line with none. Deliberate: it removes a large ambiguity and no formatted Almide relies on it. | tighter |
| G9 | **Arithmetic precedence is flattened.** `\|>`, `>>`, `+ -`, `* / %`, `^` share one level. Precedence shapes the parse *tree*, never the set of accepted token strings, and Earley pays for depth on every column. The binding powers remain authoritative in `grammar/precedence.toml` and `Parser::infix_bp`. Boolean, comparison and range levels ARE kept separate — their non-associativity is a language fact (`a < b < c` and `0..<1..<2` are errors). | equal |
| G10 | **Retired / rejected spellings excluded.** `&&`, `\|\|`, `++` and `..`/`..=`-as-a-range parse in the compiler *only* so it can answer with a migration hint. The artifact excludes them: a decoder must never sample them. | tighter |
| G11 | **No depth limit.** The parser caps expression nesting at 500 (`MAX_DEPTH`); the artifact does not. | looser |
| G12 | **Module-qualified record literals** (`mod.TypeName { … }`) are accepted after any postfix expression, not only after a bare identifier as the parser requires. | looser |

## Relationship to the `grammar/` submodule

[`almide/almide-grammar`](https://github.com/almide/almide-grammar) (the
`grammar/` submodule) holds the *descriptive* keyword / precedence / TextMate
data that the tree-sitter and VS Code generators consume. It has no consumable
CFG, and CI never fetches it — so this artifact lives in the compiler repo,
where the gate can reach both it and the lexer it mirrors. `almide.lark`'s
precedence commentary points back at `grammar/precedence.toml`; the two are
kept honest by pointing at the same source of truth, `Parser::infix_bp`.
