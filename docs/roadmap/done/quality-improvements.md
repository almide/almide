<!-- description: Quality improvements (error line numbers, heredoc tracking) -->
<!-- done: 2026-03-17 -->
# Quality Improvements

## 1. Error Message Line Numbers ✅

Automatic span attachment to all checker diagnostics via the `emit()` method. 22 locations fixed.

## 2. Heredoc Line Number Tracking

**Status:** `lex_heredoc` consumes `\n` but does not update the lexer's `line` counter
**Fix:** Either have `lex_heredoc` return the number of consumed newlines, or recalculate line numbers from `pos` changes in the lexer's main loop
**Estimate:** 1-2 hours
