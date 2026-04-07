# Bubblewrap — WASM Agent Orchestration Layer

Almide 製の WASM モジュールが複数の WASM エージェントを統制する。ポリシー判定、ライフサイクル管理、エージェント間通信の仲介を行う。Bubblewrap 自身も WASM で動くため、ポータブルかつ自身もサンドボックス内に閉じ込められる。

---

## Position in the Stack

```
┌─────────────────────────────────────────────┐
│  MCP Client (Claude Code / Cursor / etc.)   │
├─────────────────────────────────────────────┤
│  hatch (Rust binary, MCP bridge)            │  ← existing
├─────────────────────────────────────────────┤
│  Bubblewrap.wasm (Almide)                   │  ← this document
│  policy · lifecycle · routing · audit       │
├──────────┬──────────┬───────────────────────┤
│ Agent A  │ Agent B  │ Agent C               │  ← existing
│ .wasm    │ .wasm    │ .wasm                 │
└──────────┴──────────┴───────────────────────┘
           wasmtime (host runtime)
```

hatch は MCP ↔ WASM のブリッジとして残る。hatch から見ると Bubblewrap は「1つの agent.wasm」だが、内部で複数エージェントのフリートを管理する。

### Current vs Bubblewrap Architecture

| | Current (hatch only) | With Bubblewrap |
|---|---|---|
| エージェント数 | 1 per hatch | N per Bubblewrap |
| ポリシー判定 | コンパイル時 + WASI | コンパイル時 + WASI + **ランタイムポリシー** |
| エージェント間通信 | なし（独立プロセス） | Bubblewrap 経由で仲介 |
| オーケストレーション | MCP クライアント側 | **WASM 内で完結** |

---

## Why WASM-on-WASM

Bubblewrap を native binary（Rust）ではなく WASM で書く理由。

| | Native binary | WASM module |
|---|---|---|
| ポータビリティ | プラットフォームごとにビルド | WASM ランタイムがあればどこでも |
| 自身のサンドボックス | ホスト全権限 | **自身も linear memory 内** |
| ツールチェーン | Rust（エージェントと別） | **Almide（エージェントと同一）** |
| 信頼モデル | バイナリを信頼する必要がある | バイナリもサンドボックス内 |
| 埋め込み | プロセス境界 | モジュール境界 |

**核心**: Bubblewrap 自身がサンドボックス内で動く。オーケストレーターが侵害されてもホストメモリにはアクセスできない。ホストランタイムだけが full host access を持つ。

---

## Host API Contract

Bubblewrap は WASM モジュールなので、直接 WASM モジュールをインスタンス化できない。ホストが最小限の API を提供する。

### Module Lifecycle

| Import | Signature | Description |
|---|---|---|
| `agent_load` | `(manifest_ptr, manifest_len) -> agent_id` | manifest.json からエージェントをロード |
| `agent_invoke` | `(agent_id, tool_ptr, tool_len, args_ptr, args_len) -> (result_ptr, result_len)` | ツール呼び出しを実行 |
| `agent_drop` | `(agent_id)` | インスタンスを破棄 |

### Capability Scoping

ロード時にエージェントの WASI 制約を設定する。

| Import | Signature | Description |
|---|---|---|
| `agent_set_dir` | `(agent_id, path_ptr, path_len, mode: i32)` | ディレクトリマウント（0=ro, 1=rw） |
| `agent_set_env` | `(agent_id, key_ptr, key_len, val_ptr, val_len)` | 環境変数の注入 |
| `agent_set_fuel` | `(agent_id, fuel: i64)` | 命令実行バジェット |
| `agent_set_memory_limit` | `(agent_id, pages: i32)` | メモリ上限（WASM ページ数） |

これが Bubblewrap ↔ ホスト間の全インターフェース。ホストが実際の WASM インスタンス化と WASI 設定を行い、Bubblewrap は **何を** ロードし **どう** 制約するかを決定する。

### Host API Design Principle

ホスト API は意図的に薄い。理由：

1. **API が小さいほど攻撃面が小さい。** Bubblewrap が呼べる host function が少ないほど、侵害時の影響範囲が限定される。
2. **ポリシーロジックは WASM 側。** 判定ロジックがホストにあると、ホストの変更がセキュリティに直結する。Almide で書かれた Bubblewrap 内に閉じることで、ポリシーの監査・テスト・バージョン管理が容易になる。
3. **ホスト実装の差し替えが容易。** wasmtime 以外のランタイム（wasmer, wazero, browser）でも同じ API を提供すれば動く。

---

## Policy Model

### Policy Definition

Bubblewrap はポリシーを宣言的に定義する。ポリシーはエージェント ID から制約へのマッピング。

```almide
type Policy = {
  capabilities: List[String],
  dirs: List[DirMount],
  envs: List[EnvVar],
  fuel: Int,
  memory_pages: Int,
  allowed_peers: List[String],
}

type DirMount = { path: String, mode: String }
type EnvVar   = { key: String, value: String }
```

### Additive Restriction Principle

ポリシーはコンパイル時 capability の **部分集合** のみ許可できる。拡張は不可能。

```
コンパイル時: [FS.read, FS.write, IO.stdout]   ← バイナリが持つ最大権限
ポリシー:     [FS.read, IO.stdout]              ← Bubblewrap が許可する範囲
実行時:       [FS.read, IO.stdout]              ← 実際に使える権限
```

`FS.write` がコンパイル時に許可されていても、ポリシーが許可しなければ Bubblewrap は `agent_set_dir` で write mount を設定しない。バイナリに `path_open(write)` の import は存在するが、WASI ランタイムが write-capable な fd を開かないため、呼び出しは `EACCES` で失敗する。

逆方向は不可能。コンパイル時に `FS.write` が許可されていないエージェントのバイナリには `path_open(write)` の import 自体が存在しない。ポリシーで `FS.write` を追加しても、リンクする関数がない。

### Policy Resolution

```almide
fn resolve(binary_caps: List[String], policy_caps: List[String]) -> List[String] =
  binary_caps |> list.filter((cap) => list.contains(policy_caps, cap))
```

交差集合。どちらか一方でも欠けていれば権限は付与されない。

---

## Inter-Agent Communication

エージェントは linear memory が隔離されているため、直接通信できない。全メッセージが Bubblewrap を経由する。

```
Agent A ──stdout──→ Bubblewrap ──policy check──→ Agent B ──stdin──→
                         │
                    ┌────┴────┐
                    │ 検証     │
                    │ 1. peer許可? │
                    │ 2. schema適合? │
                    │ 3. budget内? │
                    └─────────┘
```

### Routing Rules

| Check | Fail action | Description |
|---|---|---|
| `allowed_peers` | Drop + error | Agent A のポリシーに Agent B が含まれているか |
| Schema validation | Drop + error | メッセージが受信側の期待する JSON Schema に適合するか |
| Message budget | Drop + error | Agent A の送信回数がバジェット内か |

### Wire Format

新しいプロトコルは導入しない。hatch と同じ JSON over stdin/stdout。

```json
{"from": "agent-a", "to": "agent-b", "tool": "analyze", "args": {"path": "src/main.almd"}}
```

Bubblewrap が `from` を検証し（エージェントは自分の ID を詐称できない — Bubblewrap が付与する）、ルーティングルールに照らし、通過すれば `agent_invoke` でディスパッチする。

---

## Lifecycle Management

### Startup Sequence

```
1. hatch が Bubblewrap.wasm をロード
2. Bubblewrap が fleet manifest（どのエージェントをどのポリシーで起動するか）を読み込む
3. Bubblewrap が host API 経由で各エージェントを agent_load → agent_set_* → ready
4. hatch が MCP tools/list を返す（Bubblewrap が集約した全エージェントのツール一覧）
```

### Per-Call Instance Model

hatch と同様、エージェントは **呼び出しごとにフレッシュインスタンス** がデフォルト。状態はコール間で漏洩しない。

ただし Bubblewrap はオプションで **persistent instance** モードを提供できる。会話的なエージェント（状態を持つ対話）のために、明示的に opt-in する。

| Mode | Instance lifetime | State leakage | Use case |
|---|---|---|---|
| `per-call` | 1 tool invocation | なし | 分析、変換、生成 |
| `persistent` | Bubblewrap の lifetime | 意図的に保持 | 対話、学習、キャッシュ |

### Resource Limits

| Resource | Enforcement | Default |
|---|---|---|
| CPU | wasmtime fuel（命令数カウント） | 1,000,000 per call |
| Memory | WASM ページ上限 | 64 pages (4MB) |
| Time | epoch-based interrupt | 30s per call |
| Messages | Bubblewrap 内カウンタ | 100 per session |

Fuel 消費は `agent_invoke` の戻り値で報告され、Bubblewrap はエージェントごとの消費量を追跡できる。

---

## Fleet Manifest

Bubblewrap の起動設定。どのエージェントをどのポリシーで動かすかを宣言する。

```json
{
  "schema_version": "1.0",
  "name": "code-review-fleet",
  "agents": [
    {
      "id": "linter",
      "wasm": "lint-agent.wasm",
      "manifest": "lint-agent.manifest.json",
      "policy": {
        "capabilities": ["FS.read", "IO.stdout"],
        "dirs": [{"path": "/workspace", "mode": "ro"}],
        "fuel": 2000000,
        "memory_pages": 32,
        "allowed_peers": ["reporter"]
      }
    },
    {
      "id": "security",
      "wasm": "security-agent.wasm",
      "manifest": "security-agent.manifest.json",
      "policy": {
        "capabilities": ["FS.read", "IO.stdout"],
        "dirs": [{"path": "/workspace", "mode": "ro"}],
        "fuel": 5000000,
        "memory_pages": 64,
        "allowed_peers": ["reporter"]
      }
    },
    {
      "id": "reporter",
      "wasm": "report-agent.wasm",
      "manifest": "report-agent.manifest.json",
      "policy": {
        "capabilities": ["FS.read", "FS.write", "IO.stdout"],
        "dirs": [
          {"path": "/workspace", "mode": "ro"},
          {"path": "/workspace/reports", "mode": "rw"}
        ],
        "fuel": 1000000,
        "memory_pages": 32,
        "allowed_peers": []
      }
    }
  ]
}
```

### Tool Namespace

MCP クライアントから見えるツール名は `{agent_id}_{tool_name}` で名前空間化される。

```
tools/list response:
  - linter_check_file
  - linter_check_dir
  - security_scan_file
  - security_scan_deps
  - reporter_generate_report
```

MCP クライアント（Claude Code 等）はフラットなツールリストとして受け取る。Bubblewrap がプレフィックスからエージェントを特定してルーティングする。

---

## Scenarios

### Multi-Agent Code Review

```
Bubblewrap
  ├── lint-agent.wasm        [FS.read, IO.stdout]           peers: [reporter]
  ├── security-agent.wasm    [FS.read, IO.stdout]           peers: [reporter]
  └── report-agent.wasm      [FS.read, FS.write, IO.stdout] peers: []
```

lint-agent と security-agent はソースを読んで findings を生成。report-agent が集約して最終レポートを書き出す。report-agent だけが書き込みできる。どのエージェントもネットワークにアクセスできない。reporter は他エージェントへの送信ができない（`allowed_peers: []`）ため、情報の流れは一方向。

### Healthcare Data Pipeline

```
Bubblewrap
  ├── ingest-agent.wasm      [FS.read, IO.stdout]   peers: [transform]
  ├── transform-agent.wasm   [IO.stdin, IO.stdout]   peers: [store]
  └── store-agent.wasm       [FS.write, IO.stdin]    peers: []
```

データフローが一方向に制約される。transform-agent はファイルシステムアクセスを一切持たない純粋な stdin → stdout パイプ。侵害されてもディスクからの患者データの読み取りもネットワーク経由の外部送信も物理的に不可能。

### Plugin Sandbox

```
Bubblewrap
  ├── trusted-core.wasm      [FS.read, FS.write, Net.fetch, IO.*]  peers: [*]
  └── untrusted-plugin.wasm  [IO.stdin, IO.stdout]                  peers: [trusted-core]
```

サードパーティプラグインを最小権限で実行。プラグインは stdin/stdout だけで trusted-core とやりとりし、ファイルシステムもネットワークも触れない。

---

## Security Properties

### Invariant 1: Bubblewrap Cannot Escalate

Bubblewrap 自身が WASM モジュールであるため、ホスト API で提供された操作以外は実行できない。`agent_load` / `agent_invoke` / `agent_drop` と capability scoping API のみ。ホストのファイルシステムに直接アクセスすることも、任意のプロセスを起動することもできない。

### Invariant 2: Policy Cannot Exceed Binary

[Additive Restriction Principle](#additive-restriction-principle) により、ポリシーはコンパイル時の capability を超えられない。バイナリに存在しない import をポリシーで追加することは物理的に不可能。

### Invariant 3: Agents Cannot Bypass Routing

エージェント間の通信は全て Bubblewrap を経由する。エージェントの stdout は Bubblewrap のみが読み取り、直接他のエージェントの stdin に接続されることはない。ホスト API にエージェント間の直接パイプは存在しない。

### Invariant 4: Identity Cannot Be Spoofed

メッセージの `from` フィールドは Bubblewrap が付与する。エージェントが自分の stdout に書いた JSON に含まれる `from` は無視され、Bubblewrap が管理する agent_id で上書きされる。

### Defense Depth

```
Layer 0: Almide compiler          — capability violation → compile error
Layer 1: WASM binary              — disallowed imports physically absent
Layer 2: Bubblewrap policy        — runtime restriction (additive only)
Layer 3: WASI runtime             — --dir / --env scoping
Layer 4: Host runtime             — memory isolation, fuel limits
```

既存の 3 層防御（[Capability System](./capability-system.md)）に Layer 2（Bubblewrap ランタイムポリシー）が加わり、4 + 1 層になる。

---

## Relation to Existing Components

| Component | Role | Doc |
|---|---|---|
| **Capability System** | コンパイル時の 13 カテゴリ権限チェック | [capability-system.md](./capability-system.md) |
| **hatch** | MCP ↔ WASM ブリッジ（シングルエージェント） | [hatch-design.md](./hatch-design.md) |
| **Agent Container** | Claude Code との統合モデル | [agent-container.md](./agent-container.md) |
| **Bubblewrap** | マルチエージェントオーケストレーション（WASM 内） | this document |

### Composition

```
Claude Code ──MCP──→ hatch ──WASM──→ Bubblewrap.wasm ──host API──→ Agent A/B/C.wasm
```

hatch は変更不要。hatch から見ると Bubblewrap は「1 つのエージェント」であり、Bubblewrap の内部構造を知る必要がない。hatch の manifest.json には Bubblewrap が集約した全ツールが列挙される。

### hatch Configuration

```json
{
  "mcpServers": {
    "fleet": {
      "type": "stdio",
      "command": "hatch",
      "args": [
        "serve", "bubblewrap.wasm",
        "--fleet", "fleet-manifest.json",
        "--dir", "/workspace"
      ]
    }
  }
}
```

`--fleet` フラグが hatch に「この WASM は Bubblewrap であり、fleet manifest に従ってサブエージェントをロードする」ことを伝える。hatch は fleet manifest を読み、ホスト API を Bubblewrap に提供する。

---

## Implementation Path

| Phase | Deliverable | Dependency |
|---|---|---|
| 1 | Host API 仕様確定 + wasmtime 側の実装 | wasmtime embedding API |
| 2 | Bubblewrap コア（agent_load / invoke / drop のラッパー）を Almide で実装 | Almide WASM codegen（既存） |
| 3 | Policy engine（JSON パース → capability scoping 呼び出し） | Almide json stdlib（既存） |
| 4 | Inter-agent routing + message validation | Almide string/json stdlib（既存） |
| 5 | hatch の `--fleet` モード追加 | hatch（既存） |
| 6 | Fleet manifest spec + CLI (`almide fleet`) | almide CLI（既存） |
