<!-- description: Elm-grade compiler error experience — hospitality, not hostility -->
<!-- done: 2026-03-14 -->
# Elm-Grade Error Experience

> **Status: Done** — Phase 1-2 shipped. Phase 3-4 (tone/teaching) deferred — current style is accurate, LLM-friendly, and sufficient.
> Improvements: E005/E001 dedup, caret on exact argument, fix code in try: snippet.

## The Standard: Elm

Elm's compiler is the gold standard for error experience. It treats errors as
a conversation, not a punishment. Key properties:

1. **One error, one message** — no cascade, no duplicate diagnostics
2. **Points at the right thing** — caret highlights the exact expression, not a comma
3. **Speaks human** — "I expected X, but you gave me Y", not "E005: argument mismatch"
4. **Shows the fix** — actual corrected code, not just "fix the type"
5. **Teaches** — explains why, links to deeper understanding

## Current State (v0.18.1)

What works:
- E001-E008 error codes with hints
- `suggest_alias` for typo correction ("Did you mean X?")
- `try_replace` for mechanically applicable fixes
- Secondary spans ("defined here")

What's broken:
- **Duplicate errors**: E005 + E001 fire for the same type mismatch
- **Caret misalignment**: points at comma/space instead of the argument expression
- **Tone**: technical and cold — "expected Int but got String"
- **No fix code**: hint says "fix the type" but doesn't show the fixed code
- **No teaching**: doesn't explain why the types don't match

## Design

### Principle: Hospitality

Every error message should feel like a senior engineer sitting next to you:
- Not annoyed that you made a mistake
- Explains what they see and what they expected
- Shows you exactly how to fix it
- If relevant, teaches the underlying concept

### Target Format

```
── TYPE MISMATCH ── src/main.almd:4:20

The 1st argument to `add` is a String, but it needs to be an Int:

4|   let result = add("hello", 2)
                      ^^^^^^^

`add` expects this:

    add(a: Int, b: Int) -> Int

Hint: To convert a String to Int, use:

    int.parse("hello") |> result.unwrap_or(0)
```

### Rules

1. **Never show two errors for the same problem**
2. **Caret must cover the exact source expression, never adjacent tokens**
3. **Always show the expected signature when a call fails**
4. **Always show corrected code when a fix is known**
5. **Use "I/you" language, not passive voice**
6. **Category headers (TYPE MISMATCH, NAMING, SYNTAX) instead of error codes in display**

## Phases

### Phase 1: Deduplicate and Align

- [ ] Suppress E001 when E005 already fired for the same call
- [ ] Fix caret positions to cover the argument expression span, not trailing tokens
- [ ] Audit all E001-E008 for caret accuracy

### Phase 2: Show Fix Code

- [ ] Type mismatch: show conversion expression (`int.parse`, `int.to_string`, etc.)
- [ ] Undefined variable: show corrected line with suggestion applied
- [ ] Undefined function: show corrected call with module prefix
- [ ] Mut parameter: show corrected declaration (`var` instead of `let`)
- [ ] Opaque type: show the module's public API to construct the type

### Phase 3: Conversational Tone

- [ ] Rewrite all error messages to "I expected / you gave" format
- [ ] Add category headers (TYPE MISMATCH, NAMING, SYNTAX, MUTATION)
- [ ] Remove error codes from user-facing display (keep in `--json` output)
- [ ] Add signature display for all call-related errors

### Phase 4: Teaching Hints

- [ ] Common patterns: "Almide uses `var` for mutable, `let` for immutable"
- [ ] Link to CHEATSHEET.md sections from relevant errors
- [ ] First-time hints: detect if this is likely a user's first encounter with a concept

### Phase 5: LLM Integration

- [ ] `--json` output includes structured fix suggestions (before/after AST patches)
- [ ] `almide fix <file>` applies the most likely fix automatically
- [ ] `almide explain E007` shows the full teaching page for an error
- [ ] Measure auto-repair rate (target: 70%+)

## Metrics

| Metric | Target |
|--------|--------|
| Duplicate errors per problem | 0 |
| Caret accuracy (covers exact expression) | 100% |
| Errors with fix code shown | 80%+ |
| Errors with conversational message | 100% |
| "This compiler is helpful" in user feedback | Yes |

## References

- [Elm Compiler Errors](https://elm-lang.org/news/compiler-errors-for-humans)
- [Rust Error Index](https://doc.rust-lang.org/error_codes/)
- [Zig's Compile Errors](https://ziglang.org/documentation/master/#Compile-Errors)
