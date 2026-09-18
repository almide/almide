Rename the `show` command to `print`. The old name goes away: a `show` line is now an unknown command.

Preserve:
- `add`, `sub`, the error lines and the integer semantics behave exactly as before.
- Unknown commands (now including `show`) are reported as `error: unknown command <cmd>` and skipped.
