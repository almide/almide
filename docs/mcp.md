# Almide for agents: MCP server + Claude Code plugin

> Last updated: 2026-09-26

An agent reaching the compiler through a shell has to parse human-formatted
text to learn anything. That parsing step is where accuracy leaks, and this
repo is scored on exactly one metric: how accurately an LLM writes and modifies
Almide. `almide mcp` removes the step — the same answers arrive as typed tool
calls with JSON results.

The compiler is the only implementation: every tool runs the CLI in a
subprocess and reads the machine-readable output it already produces. There is
no second code path that could drift.

## Install

The plugin configures both servers; it does not ship the binary, so install
Almide first and make sure it is on `PATH`.

```bash
# 1. the compiler (also provides `almide lsp` and `almide mcp`)
make install                     # or: cargo build --release && cp target/release/almide ~/.local/bin/
almide --version

# 2. the plugin, from this repo's own marketplace
/plugin marketplace add almide/almide
/plugin install almide@almide
```

That gives a Claude Code session:

- **MCP tools** — the six below, namespaced `mcp__plugin_almide_compiler__<tool>`
- **LSP** — `almide lsp` for `.almd` files (hover, completion, documentSymbol,
  formatting, definition, signatureHelp, codeAction, and the custom request
  `almide/survive`)

Any other MCP client works too; the server is a plain stdio server:

```json
{
  "mcpServers": {
    "almide": { "command": "almide", "args": ["mcp"] }
  }
}
```

Verify by hand — the server speaks newline-delimited JSON-RPC 2.0 on stdio:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | almide mcp
```

## Tools

| Tool | Runs | Returns |
|---|---|---|
| `almide_check` | `almide check --json` | `{ok, errors, warnings, diagnostics[]}` — each diagnostic is `{level, code, message, hint, here, try, try_replace, repair, context, file, line, col, end_col, secondary}` |
| `almide_test` | `almide test --json` | `{ok, files_passed, files_failed, files[], runner_output_unstructured}` |
| `almide_api` | `almide ide outline --json`, `almide ide stdlib-snapshot --json` | the public declarations of a file, of `@stdlib/<module>`, or of `@stdlib` (core snapshot), with exact signatures |
| `almide_explain` | `almide explain <CODE>` | the diagnostic's reference page (markdown) |
| `almide_fmt_check` | `almide fmt --check --json` | `{checked, unformatted[], unreadable[], verify_failed, ok}` |
| `almide_survive` | `almide survive <file> --with - --json` | the survival delta of a proposed edit, BEFORE it is written: `{schema_version, survives, reach, summary, check, tests, contracts}`, each leg `{unchanged, newly_broken, newly_fixed, removed}` |

`try` is a copy-pasteable fix snippet and `try_replace` is the span it
replaces, so a fix can be applied mechanically rather than re-derived.
`repair` (#2149, present only when the diagnostic has a fix) is the
structured form: `{primary, alternatives, example}`, each edit
`{line, col, end_col, replacement, applicability}` — apply `primary`
unattended only when its `applicability` is `"machine-applicable"`.
`line` is 1-based; `col` / `end_col` are 1-based **character** counts (not
bytes, not display width), `end_col` exclusive — a harness that counts bytes
corrupts UTF-8 (#2250).

## Judging an edit before writing it: `almide_survive`

The failure that costs modification survival is an edit applied without
being verified. `almide_survive` takes `{file, edit}` — `edit` is a unified
diff against the file or its complete new text (`kind`: `auto` / `patch` /
`text`) — applies it **in memory**, and runs, on both sides of the edit:
`almide check --json` on the file and every `.almd` whose imports reach it, the
`test` blocks of those files, and the native⇄wasm run of every reached
`// @contract:` fixture. Each diagnostic / test / contract comes back in one of
`unchanged`, `newly_broken`, `newly_fixed`, `removed`; `survives` is `true`
when no error, test or contract is newly broken. The file is never written.

A diagnostic's `before` / `after` is the object `almide check --json` printed,
carried verbatim, so every field that output grows reaches the agent unchanged.
Diagnostics are paired across the edit by a line map (an inserted line above an
error does not make it a new error); the exact identity rule and the schema are
in [specs/cli.md § `almide survive`](specs/cli.md).

The same report is the answer to the LSP request `almide/survive`
(`{textDocument: {uri}, text?, patch?}`; with neither, the open buffer is the
proposal). To write an edit only if it survives, run
`almide apply <file> --with <edit> --if-survives` in a shell — verify and write
in one step, atomically.

## Rules this surface follows

**Read-only by default.** No tool writes a source file. `fmt` is exposed only
in its `--check` form; applying it is `almide fmt <path>` in a shell, where the
edit shows up in the agent's own transcript. `almide_test` and `almide_survive`
are the tools with a side effect — they compile and execute tests, which writes
build artifacts — and they say so in their descriptions and in their MCP
annotations (`readOnlyHint: false`). `almide_survive` never writes the source
it judges.

**Structured beats prose.** A diagnostic arrives as fields, never as a rendered
block.

**No scraping.** Where the CLI has no machine-readable form, the raw text is
returned verbatim in a field whose name ends in `_unstructured`, and the tool
says which issue tracks the gap. Regex-scraping a rendered diagnostic would
reintroduce exactly the fragility this server exists to remove.

## What is NOT structured yet

- **Per-test failure detail** (#1313). `almide test --json` reports per-FILE
  pass/fail; the failing assertion's expected/found is the test runner's own
  text, returned as `runner_output_unstructured`.
- **Parse-error codes.** A syntax error now reaches `check --json` as JSON (it
  used to be text-only), but those diagnostics carry an empty `code`, so
  `almide_explain` has nothing to look up for them.
- **`almide compile --json`** (the full module interface with ABI layout) is
  CLI-only. `almide_api` covers the signature-level question an agent actually
  asks; the ABI detail belongs to binding generators.
- **`almide ide doc <symbol>`** (signature + doc comment for ONE symbol) has no
  `--json` form, so it is not exposed rather than scraped. `almide_api` answers
  the same question at signature level for a whole module.

## Files

| Path | What |
|---|---|
| `src/cli/mcp.rs` | protocol: stdio JSON-RPC, initialize / tools/list / tools/call / ping |
| `src/cli/mcp_tools.rs` | the catalog and the CLI invocations behind it |
| `tools/claude-plugin/.claude-plugin/plugin.json` | Claude Code plugin (MCP + LSP) |
| `.claude-plugin/marketplace.json` | this repo as a plugin marketplace |
| `tests/mcp_test.rs` | end-to-end gate: drives the real binary over stdio |
| `src/cli/survive*.rs`, `src/source_overlay.rs` | `almide survive` / `almide apply` and the in-memory overlay they judge through |
| `tests/survive_test.rs`, `tests/survive/` | the three golden survival deltas, the no-write and apply gates |
