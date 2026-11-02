原文の description メタデータ：sandbox 化した AI agent container の capability ベース effect system
<a id="capability-based-effect-system"></a>

# capability ベースの effect system

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](effect-system-capability.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](effect-system-capability.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](effect-system-capability.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md)

> 原文注記：実装基準の訂正は旧 phase 概略に優先する。pure は category ゼロではなく、callback effect は透明で、検査は任意 host 挙動を証明しない。MCP は未出荷の提案であり、直接動くという旧設定コメントは動作検証結果ではない。search 例は file 内容でなく file 名を filter する。directory preopen は read-only ではなく、共有 import/handle right により import 削減を強制力の根拠にする範囲は限られる。引用 command は実行していない。


> **現行の範囲**：permission の分析・検査は既に存在し、残作業はより強い capability chain と以下の提案である。未実装の `--capabilities` でなく、`almide check` / `almide check --effects` を使う。検査合格は runtime 全体の sandbox 証明ではない。
> **設計裁定 2026-10-03**：[ADR-0026](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md) — 型レベル（Phase 4）の形、カテゴリ階層（下の `IO` shorthand を置換）、静的⊆manifest⊆host の鎖。

<a id="implemented-baseline-and-remaining-design"></a>

## 実装済み基準と残る設計

`crates/almide-ir/src/effect.rs` は leaf 14 個と parent 四つ（`FS`、`Net`、`IO`、`Env`）を定義する。`IO` は stdin/stdout/stderr のみで、filesystem access は含まない。`Abort` は leaf。当初の 13 category 概略を置換する。`src/cli/check.rs` と `src/cli/mod.rs` は推論 effect に対して project permission を強制し、`src/project.rs` が grant を parse する。effect bit と category 分析は別物。pure `fn` は category ゼロの約束ではない（ADR-0022 は output/abort を許す）。

意図する最小権限の鎖は推論 effect ⊆ manifest ⊆ host（ADR-0026）。既存分析は任意 foreign code や host 挙動がこれを守るという証明ではない。`@extern` body は不透明で、推論では全 leaf を受け取る。native permission と process command grant にはそれぞれ境界がある。host 設定は依然必要。

以下の phase 概略は当初の実装分解を記録し、五つすべてが未実装という意味でも、出荷済み end-to-end 保証の集合でもない。特に category 宣言だけでは WASI import は除去されない。read/write operation が import を共有して handle right に依存することがある。import 検査では application や host の全挙動を証明できない。host は別の信頼境界であり、安全性の万能証明ではない。

<a id="implementation-phases"></a>

## 実装段階

<a id="phase-1-original-checking-plan-core-enforcement-now-exists"></a>

### Phase 1：当初の検査計画（中核の強制は現在存在）

**目標**：almide.toml の `[permissions]` → 違反時 compile error。

1. compiler に capability → stdlib 関数対応を定義
   - 各 stdlib module 関数に capability category を注記
   - `fs.read_text` → `FS.read`、`fs.write_text` → `FS.write` など
   - `almide-frontend` の静的 table として保存
2. almide.toml から `[permissions]` を parse
   - `allow = ["FS.read", "IO.stdout"]` → 計画上の `CapabilitySet` 表現（現行 registry は 14 leaf）
   - `[permissions]` がない場合は全 capability を許可（後方互換）
3. 型チェッカーの capability 検査 pass
   - 型チェック後に call graph を走査
   - stdlib 呼出しごとに必要 capability を検索
   - `allow` にない → 行動可能な hint 付き error：
     ```
     error[CAPABILITY_VIOLATION]: capability violation: fs.write_text requires FS.write
       --> agent.almd:5:3
       |
     5 |   fs.write_text(path, content)
       |   ^^^^^^^^^^^^^
       = note: [permissions] allows: FS.read, IO.stdout
       = hint: add "FS.write" to [permissions].allow in almide.toml
     ```
   - 上の記号的 diagnostic は例示で、割当済み code ではない（`E010` は網羅性）。
   - 推移的：`fn foo` が `fn bar` を呼び、それが `fs.write` を呼ぶなら、`foo` は `FS.write` を要求
4. effect 関数推論の更新
   - [permissions] なしの `effect fn` → 全 capability（既存挙動）
   - [permissions] ありの `effect fn` → 宣言済み capability のみ
   - pure `fn` は category ゼロを意味しない。output/abort は引き続き許可（ADR-0022）。

診断例の訳：書込み関数に FS.write が必要なのに許可一覧は FS.read と IO.stdout のみであるため、almide.toml の許可へ FS.write を加えるよう示す。

**変更対象 file**：
- stdlib `.almd` source — 各関数に capability を宣言（stdlib/defs の TOML pipeline は廃止済み。関数ごとの metadata は .almd body にある）
- `crates/almide-frontend/src/check/` — capability 検査 pass
- `src/project.rs` — 現行 manifest parse（当初の resolver 帰属は古い）
- `crates/almide-base/src/` — CapabilitySet 型（u16 bitset）

**テスト**：`allow = ["FS.read"]` でコンパイルでき、`fs.write_text` 呼出しでは失敗する agent test を書く。

<a id="phase-2-wasm-import-pruning--manifest-output"></a>

### Phase 2：WASM import 削減 + manifest 出力

**目標**：WASM binary に capability set が必要とする WASI import だけを含め、機械可読 manifest を出力。

1. capability を WASI import に対応づける
   - `FS.read` → `path_open`、`fd_read`、`fd_close`、`fd_seek`、`path_filestat_get` が必要
   - `FS.write` → `path_open`（write）、`fd_write`、`path_create_directory` などが必要
   - `IO.stdout` → `fd_write`（fd=1 のみ）が必要
   - capability が不許可なら WASI import を出力しない
2. WASM emitter の import section 更新
   - 当初の仮定は全 WASI 関数を import していたというもの。現行 emitter 一覧ではない。
   - 新方式：CapabilitySet に必要な WASI 関数のみ import
   - 除去 import を参照する runtime 関数は `unreachable` trap に置換
3. `.wasm` と並べて `manifest.json` を出力
   ```json
   {
     "name": "file-reader-agent",
     "version": "0.1.0",
     "capabilities": ["FS.read", "IO.stdout"],
     "wasi_imports": ["fd_read", "fd_write", "fd_close", "path_open", "path_filestat_get"],
     "entry": "_start"
   }
   ```
   orchestrator が WASM binary 読込み前にこれを読み、capability が policy と一致するか確認。

**変更対象 file**：
- `crates/almide-wasm/src/` — 現行 structural emitter。旧変更対象 `render_wasm*` / `self_host_registry.rs` は廃止済み
- `src/cli/build.rs` — manifest.json を出力

**テスト**：`allow = ["IO.stdout"]` で build → WASM binary に `path_open` import がないことを検証。

<a id="phase-3-proposed-per-dependency-category-restriction"></a>

### Phase 3：dependency ごとの category 制限案

**目標**：dependency package に許される capability を制限。

```toml
[dependencies.json_parser]
version = "1.0"
allow = []                    # pure only — no I/O

[dependencies.http_client]
version = "2.0"
allow = ["Net.fetch"]
deny = ["Proc", "FS.write"]  # explicit deny
```

設定例のコメントの訳：空の許可一覧は pure のみ・I/O なしを意図し、deny は明示的な拒否を表す。

1. almide.toml から dependency ごとの permission を parse
2. module interface 読込み時に dependency の必要 capability を計算
   - 公開関数の推移的 call graph を走査
   - consumer が当該 dependency に設定した `allow` / `deny` と比較
3. dependency が grant を超えれば compile error

**変更対象 file**：
- `src/resolve.rs` — dependency ごとの permission parse
- `crates/almide-frontend/src/check/` — module 間 capability 伝播
- module interface 形式 — capability 要件を含める

<a id="phase-4-original-tyfn-sketch-superseded-by-category-flow-analysis"></a>

### Phase 4：当初の Ty::Fn 案を category-flow 分析で置換

当初計画は `CapabilitySet` を `Ty::Fn` に保存した。現行の `crates/almide-codegen/src/effect_flow/mod.rs` は代わりに型チェック後に集合を計算し、関数値／位置を通して追跡、`Ty::Fn` でなく `EffectMap` に結果を保存する。追跡位置と保守的 pool の規則が実装範囲を定め、将来の全 phase の完成を意味しない。

bare fn 型 callback parameter は透明（ADR-0009/0026）で、呼出し元は渡した callback の effect を継承する。effectful callback をそのような parameter に渡しても一律に拒否されない。明示的 purity 境界は別であり、`crates/almide-frontend/src/check/pure_attr.rs` を参照。当初の必須拒否規則は現行言語の案内ではない。

<a id="phase-5-proposed-mcp-server-stdlib-module"></a>

### Phase 5：MCP server stdlib module の提案

出荷済み `mcp` stdlib module はない。compiler の MCP tooling は別 interface であり、以下は設計例であって実行可能な配備手順ではない。

**目標**：Model Context Protocol（stdio 上 JSON-RPC）を実装する `mcp` stdlib module。独自 protocol なし、言語変更なし。

MCP は AI agent tool 通信（Claude Code、Cursor、Windsurf、GitHub Copilot）の事実上の標準。この提案 module には明示的な client/protocol 互換テストが必要であり、JSON-RPC を話すだけではすべての MCP client との互換性は示せない。

```almide
import mcp

effect fn main() -> Unit =
  mcp.serve([
    mcp.tool("read_file", "Read a file at the given path", (params) => {
      fs.read_text(mcp.arg(params, "path"))
    }),
    mcp.tool("list_dir", "List directory contents", (params) => {
      fs.list_dir(mcp.arg(params, "path")) |> list.join("\n")
    }),
  ])
```

```json
// Claude Code mcpServers config — works directly
{
  "mcpServers": {
    "code-reviewer": {
      "command": "wasmtime",
      "args": ["run", "--dir", "/workspace", "agent.wasm"]
    }
  }
}
```

設定例のコメントは「Claude Code mcpServers 設定 — 直接動く」と述べるが、これは上記の未実装設計例という限定下の原文であり、今回の動作確認ではない。tool の説明は指定 path の file 読込みと directory 内容一覧である。

- `mcp.serve(tools)` — JSON-RPC stdio loop（initialize → tools/list → tools/call）
- `mcp.tool(name, description, handler)` — tool 定義
- `mcp.arg(params, key)` — parameter 抽出
- `mcp.resource(uri, handler)` — resource 公開（将来）
- すべて stdlib で、言語変更なし。独自 protocol でなく MCP spec への適合。

<a id="proposed-ai-agent-container-example-not-a-shipped-end-to-end-recipe"></a>

## AI agent container 例の提案（出荷済み end-to-end 手順ではない）

```toml
# almide.toml
[project]
name = "code-review-agent"
version = "0.1.0"

[permissions]
allow = ["FS.read", "IO.stdin", "IO.stdout"]
```

```almide
// agent.almd
import mcp

effect fn main() -> Unit =
  mcp.serve([
    mcp.tool("read_file", "Read file contents", (params) => {
      fs.read_text(mcp.arg(params, "path"))
    }),
    mcp.tool("list_dir", "List directory", (params) => {
      fs.list_dir(mcp.arg(params, "path")) |> list.join("\n")
    }),
    mcp.tool("search", "Search for pattern in files", (params) => {
      let dir = mcp.arg(params, "dir")
      let pattern = mcp.arg(params, "pattern")
      fs.list_dir(dir)
        |> list.filter((f) => string.contains(f, pattern))
        |> list.join("\n")
    }),
  ])
```

```bash
# Proposed build/manifest output — not current CLI behavior
almide build agent.almd --target wasm -o agent.wasm
# → also outputs agent.manifest.json

# Use from Claude Code directly
# claude_code_config.json:
# { "mcpServers": { "code-reviewer": {
#     "command": "wasmtime",
#     "args": ["run", "--dir", "/workspace", "agent.wasm"]
# }}}

# Or run standalone with sandboxing
wasmtime run --dir /workspace agent.wasm
```

例の説明・コメントの訳：tool は file 内容読込み、directory 一覧、file 内 pattern 検索と記される。build/manifest 出力は提案であって現行 CLI 挙動ではなく、agent.manifest.json も出力する想定。Claude Code から直接使う設定例、または sandbox を用いて単体実行する例を記載する。記載コマンドは実行していない。

<a id="required-boundaries-before-claiming-a-secure-deployment"></a>

### 安全な配備と呼ぶ前に必要な境界

- compiler permission 検査は、選択経路に実装された分析の範囲であり、任意の external/host code ではない。提案の manifest 出力と import 削減には独自の実装と証拠が必要。
- `wasmtime --dir /workspace` は directory preopen であり、**read-only grant ではない**。read/write right とそのほか全 host capability は別途設定・検証が必要。この例は read-only container を立証しない。
- MCP module 案には protocol/schema 互換テストが必要。型付き Almide handler はすべての client との互換性を立証しない。
- この計画から、他の全言語に対する唯一性は導けない。

<a id="implementation-priority"></a>

## 実装優先度

| Phase | 影響 | 工数 | 独立出荷できるか |
|-------|--------|--------|---------------------|
| **Phase 1** | compile-time capability 検査 | M | ✅ 可 — 即時価値 |
| **Phase 2** | WASM import 削減 + manifest | S | 可能性あり — import 一覧であり、完全な挙動証明ではない |
| **Phase 3** | dependency ごとの制限 | S | 可能性あり — supply-chain 制御の一つで、完全な安全性ではない |
| **Phase 4** | 型レベル effect（HOF）| L | ✅ 可 — 精度 |
| **Phase 5** | agent stdlib module | S | ✅ 可 — stdlib のみ、言語変更なし |

Phase 1 は静的 permission 検査を提供するが、任意 agent 配備を sandbox 化済みと呼ぶ十分な根拠ではない。

<a id="what-we-dont-do"></a>

## 行わないこと

- 代数的 effect handler — 複雑すぎ、capability 検査には不要
- 利用者定義 effect category — 現行 registry は 14 の組込み leaf
- 関数 signature 内 effect 構文 — capability は code でなく almide.toml に宣言
- 動的 capability request は本提案の範囲外。host grant と runtime 強制は依然重要
