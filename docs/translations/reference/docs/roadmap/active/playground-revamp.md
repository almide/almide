<!-- description: Playground UI revamp: file tabs, share URLs, TS-style example gallery -->
<a id="playground-ui-revamp"></a>

# Playground UI Revamp

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](playground-revamp.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](playground-revamp.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](playground-revamp.zh-CN.md)

> Non-normative historical reader snapshot. Pinned original unchanged and authoritative; no automatic synchronization, external playground browser check or new execution. October31 compiler fixes are distinct from July playground deployment/test records; old roadmap Canvas plans differ from the implemented stdout SVG/PPM route, and interactive Canvas remains separate. Free/zero-cost/unlimited claims are the source’s local/static-hosting design rationale, not verified universal hosting or API pricing. All historical code/comments are unchanged, not current runnable guarantees. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/playground-revamp.md)

<a id="compiler-side-status-2026-10-31"></a>

## Compiler-side status (2026-10-31)

The compiler-side problems recorded in July 2026 below have been fixed:

- `io.read_all` is implemented in the structural Wasm emitter ([host_env.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-wasm/src/host_env.rs)). Regression tests: [io_read_all_test.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tests/io_read_all_test.rs), [io_read_all_capacity_test.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tests/io_read_all_capacity_test.rs).
- The CLI guest cwd sets `ALMIDE_CWD` from `std::env::current_dir()`, not `$PWD` ([run.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/cli/run.rs), [component_host.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-wasm-run/src/component_host.rs)). The component route also has a stale-`$PWD` [regression test](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tests/component_route_cwd_test.rs).
- The current shared CLI/library Wasm route is the structural emitter. The MIR/WAT-renderer wall records below describe the time of writing ([wasm_route.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs)).

This checks the Almide repository, not the externally deployed playground's current version or browser behavior. almide-web binding integration, interactive Canvas, remains a separate arc. The rest is design and verification history.

<a id="vision"></a>

## Vision

Rather than fill out the feature list of server-side playgrounds, the Go/Rust approach, focus on **experiences structurally possible because execution is entirely in-browser WASM**:

1. **Always running** — local compilation is free, so compile automatically after typing idles and immediately show errors.
2. **Visual output** — unlike a server returning only stdout, running in the user's browser allows direct canvas/DOM drawing.
3. **Unlimited free embedding** — static hosting only, so making every docs/lander/article code block executable costs zero.

<a id="決定事項-2026-07-26"></a>

## Decisions (2026-07-26)

- **Do not introduce server-side execution.** Share through URL hashes, not Gists: `CompressionStream` deflate + base64url, zero dependencies.
- **Multiple files use file tabs**, neither Go's `-- file --` delimiters nor a full file tree. Tab names correspond to `import self.<name>`, demonstrating the module system. Feed tab contents into browser_wasi_shim's virtual FS, allowing `fs.read` of data files, csv/toml/yaml/json, too.
- **Categorized example gallery in TS Playground style**:
  - Put explanations in code comments rather than a sidebar, 70% explanation and 30% code.
  - Guide users to experience errors: “change this and press Run.”
  - Link to the next example at the end, `?example=<id>`, chaining material for a future language Tour.
- **One example, one URL**, `?example=csv-parse`: promo posts and docs can deep-link to “run now in the browser.”
- **Merge gallery examples into native/WASM byte-parity CI**, playground repository `tests/run.mjs`, to prevent rot.
- Non-goals: external package fetching, clippy/miri-like tools, Compiler Explorer-style multi-compiler comparison panes.

<a id="historical-snapshot-playground-repo-2026-07-25-時点"></a>

## Historical snapshot (playground repo, as of 2026-07-25)

- Plain HTML + ES Modules in one file, index.html 1365 lines; custom textarea editor overlaid with transparent highlighting.
- Entirely in-browser execution: almide-mir `try_render_wasm_source` → `wat` crate → browser_wasi_shim, WASI. Compiler wasm 1.6MB.
- Present: Shiki highlighting, three tabs Output/Compiled Rust/AST, AI generation + automatic repair loop, three BYOK providers.
- Absent: sharing links, example collection, multiple files, completion, inline errors, Ctrl+Enter, Worker isolation; an infinite loop freezes the tab.

<a id="progress"></a>

## Progress

**2026-07-26: Tier 1 + Tier 2 implemented**, playground repository, browser E2E and harness verified.

- [x] 1.1 Worker execution foundation — worker.js/runner.js, streaming stdout, Stop = terminate+respawn, 30s timeout. E2E checked infinite loop → Stop → run again.
- [x] 1.2 CodeMirror 6 + inline diagnostics — esm.sh importmap, keeping it buildless; Almide StreamLanguage; `check_project`, a new crate API with span-bearing JSON diagnostics, runs after 500ms idle. Show hints; `try:` snippets become one-click fixes; Ctrl/Cmd+Enter runs.
- [x] 1.3 File tabs + virtual FS — add an in-memory `import self.*` resolver to the crate, `compile_project_to_*(files, entry)`. Non-.almd tabs become browser_wasi_shim PreopenDirectory("."); `fs.read_text("data.csv")` was verified in-browser.
- [x] 2.1 URL-hash sharing — CompressionStream deflate-raw + base64url, complete tab set, E2E round-trip verified.
- [x] 2.2/2.3 TS-style gallery — six examples, pattern-matching / pipes-and-lists / error-handling / modules / mini-markdown / csv-report, categorized menu, `?example=<id>` deep links; examples section added to `tests/run.mjs`, native/wasm byte parity and immediate failure on a wall.

**2026-07-26: Tier 3 implemented**, same day, E2E and harness verified.

- [x] 3.1 Visual output — settled on “print SVG / PPM P3 to stdout; the playground renders an image.” No compiler changes. Native emits regular .svg/.ppm from the same program, so byte-parity CI applies unchanged. Added Graphics category, Mandelbrot PPM / generated SVG; 8/8 examples green. almide-web binding integration, interactive Canvas, is a separate arc.
- [x] 3.2 Embed mode — `?embed=1`, hiding header/AI bar, slim Run bar, Open in Playground; `&hide=` enables hidden setup, tabs hidden but compiled. No localStorage in embed mode.
- [x] 3.3 stdin + zip — the `stdin.txt` tab becomes fd0; io.read_line verified. At the time io.read_all walled because it was unregistered in the wasm registry; resolved in the current compiler, see above. Export produces a project zip runnable with `almide run src/main.almd`, dependency-free store-mode zip writer.

Problems found then, historical; current compiler-side status is above:
- `almide run --target wasm` derived guest cwd from `$PWD`; if it differed from `getcwd`, relative fs reads returned ENOENT only on wasm, exposed under parents such as execFileSync that do not update PWD. Harness worked around this by explicitly setting PWD. **Almide itself now uses current_dir.**
- Cross-module ADTs then, a module-defined variant type used in entry, hit the v1 wasm renderer wall, `main is outside the MIR-lowering subset`. The modules example avoided it by switching to a function module, stats.
- Fixed AI system-prompt examples using old `effect fn main(_args)` and triggering E028.

<a id="roadmap"></a>

## Roadmap

<a id="tier-1--土台done-2026-07-26"></a>

### Tier 1 — Foundation, done 2026-07-26

<a id="11-worker-実行基盤"></a>

#### 1.1 Worker execution foundation
Isolate compilation and execution in a Web Worker. Infinite loops do not freeze UI; cancel button, stdout streaming and execution timeout. Prerequisite for automatic compilation, 1.2, and always-running execution.

<a id="12-codemirror-6-化--インライン診断"></a>

#### 1.2 CodeMirror 6 + inline diagnostics
Replace custom textarea with CodeMirror 6, lighter than Monaco and mobile-capable. Connect compiler error spans to CM6 diagnostics; idle automatic compilation displays squiggles and line numbers. Ctrl+Enter runs. Reuse existing tmLanguage highlighting.

<a id="13-ファイルタブ--仮想-fs"></a>

#### 1.3 File tabs + virtual FS
Fixed `main.almd`, plus add/rename/delete tabs. Place all tabs in the virtual FS and compile. Goals: working `import self.<sub>` and `fs.read` of a `data.csv` tab.

<a id="tier-2--共有とギャラリー"></a>

### Tier 2 — Sharing and gallery

<a id="21-url-hash-共有"></a>

#### 2.1 URL-hash sharing
Put the full tab set, `{name: content}`, into `#code=...` with deflate + base64url. Share button copies URL. Same mechanism for single and multiple files.

<a id="22-ts-方式サンプルギャラリー"></a>

#### 2.2 TS-style example gallery
Categorized dropdown/sidebar. Manage examples in simple repository directories, each an .almd file set plus manifest; CI packs gallery JSON. Categories, all fillable with existing assets:

| Category | Example candidates |
|---|---|
| Language features | Pattern matching, pipes, result/error handling, `import self.*` multi-file organization |
| Parsers and data processing | Mini Markdown, current default; CSV aggregation; TOML/YAML/JSON; base64/hex |
| Numeric / Matrix | Measure matmul benchmark in-browser, Transformer 1-block |
| Canvas / graphics | wasm-canvas drawing, Game of Life, SVG generation, after Tier 3 Canvas output |
| Crypto | sha1 / aes / rsa + bigint |
| AI, heavy | bonsai small-model inference, separate category with lazily loaded weights |

<a id="23-サンプルフォーマットts-方式"></a>

#### 2.3 Example format, TS style
```almide
// パターンマッチ入門
//
// Almide の match は全ケース網羅をコンパイラが検査します。
// 試しに最後の `_` の腕を消して Run してみてください —
// String の残りの値を扱う腕がなくなり、網羅性エラーになります。
// "float" の腕だけを消しても `_` が拾うので、網羅性エラーにはなりません。

fn rust_type(kind: String) -> String =
  match kind {
    "int"    => "i64",
    "float"  => "f64",
    "string" => "String",
    _        => "unknown",
  }

// 次: ?example=pipe-basics
```

<a id="tier-3--見せ場と配布"></a>

### Tier 3 — Showcase and distribution

<a id="31-canvas-出力モード"></a>

#### 3.1 Canvas output mode
Canvas tab alongside Console in the output pane, for almide-web / wasm-canvas examples. Once enough exist, promote to a card gallery with output thumbnails.

<a id="32-embed-モード"></a>

#### 3.2 Embed mode
`?embed=1` provides minimal iframe UI, Kotlin Playground style. Hidden setup, hiding helper files to show only a few intended lines, uses multi-file support. Extend into lander/docs/dev.to article code blocks.

<a id="33-stdin--実行引数入力欄zip-エクスポート"></a>

#### 3.3 stdin / execution-argument inputs and zip export
An exit to “take it home as a project”: download all tabs in a zip structure runnable by `almide run`.

<a id="関連"></a>

### Related

- Remaining AI-repair-loop items, Accept/Reject, partial repair, repair history: done/playground-repair.md Tier 2. Build on this arc's foundation, 1.1, where local compilation makes validation free.
- Execution of the strategy item “playground accelerates adoption” in on-hold/rumbling.md.
- active/determinism-belt.md — browser wasm32 determinism is this arc's prerequisite.

<a id="success-metric"></a>

## Success Metric

A Reddit/X post deep-links with `?example=csv-parse`; a first-time reader experiences “open → run → edit → error → fix” within one minute, without installation. CI protects examples so compiler updates do not break the gallery.
