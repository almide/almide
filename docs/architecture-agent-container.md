# Almide Agent Container Architecture

## 概念マッピング

```mermaid
graph TB
    subgraph "開発時 (Developer)"
        SRC[".almd ソースコード"]
        TOML["almide.toml<br/>[permissions]<br/>allow = [FS.read, IO.stdout]"]
    end

    subgraph "コンパイル時 (almide build)"
        COMPILER["Almide Compiler"]
        CAP_CHECK["Capability Checking<br/>Phase 1: 13カテゴリ照合"]
        CODEGEN["WASM Codegen<br/>tail call / multi-memory"]
        PRUNE["Import Pruning<br/>Phase 2: 不許可 WASI 削除"]
    end

    subgraph "ビルド成果物"
        WASM["agent.wasm<br/>WASM 3.0 core module<br/>pruned WASI imports"]
        MANIFEST["manifest.json<br/>capabilities / tool defs<br/>WASI imports 一覧"]
    end

    subgraph "実行時 (hatch)"
        HATCH["hatch<br/>Almide 専用 MCP bridge"]
        WASMTIME["wasmtime<br/>WASM runtime"]
        MCP_SERVER["MCP Server<br/>JSON-RPC 2.0 / stdio"]
    end

    subgraph "AI クライアント"
        CLAUDE["Claude Code"]
        CURSOR["Cursor"]
        COPILOT["GitHub Copilot"]
        CUSTOM["Any MCP Client"]
    end

    subgraph "ランタイム防御 (WASI)"
        SANDBOX["Sandbox<br/>--dir /workspace<br/>--env ALLOWED_VAR"]
    end

    SRC --> COMPILER
    TOML --> COMPILER
    COMPILER --> CAP_CHECK
    CAP_CHECK -->|"違反 → compile error"| CODEGEN
    CODEGEN --> PRUNE
    PRUNE --> WASM
    PRUNE --> MANIFEST

    MANIFEST -->|"tool 定義読み込み"| HATCH
    WASM -->|"ロード"| HATCH
    HATCH --> WASMTIME
    HATCH --> MCP_SERVER
    WASMTIME --> SANDBOX

    MCP_SERVER <-->|"tools/list<br/>tools/call"| CLAUDE
    MCP_SERVER <-->|"tools/list<br/>tools/call"| CURSOR
    MCP_SERVER <-->|"tools/list<br/>tools/call"| COPILOT
    MCP_SERVER <-->|"tools/list<br/>tools/call"| CUSTOM
```

## 三層防御

```mermaid
graph LR
    subgraph "Layer 1: Compiler"
        L1["capability 違反<br/>→ compile error<br/>バイナリが生成されない"]
    end
    subgraph "Layer 2: WASM Binary"
        L2["不許可の WASI import<br/>が物理的に存在しない<br/>呼ぶ手段自体がない"]
    end
    subgraph "Layer 3: WASI Runtime"
        L3["--dir / --env<br/>ファイルシステム・環境変数<br/>のスコーピング"]
    end

    L1 -->|"通過"| L2 -->|"通過"| L3
```

## プロトコルスタック

```mermaid
graph TB
    subgraph "標準プロトコル (外部)"
        MCP["MCP<br/>Model Context Protocol<br/>agent ↔ tool"]
        A2A["A2A<br/>Agent-to-Agent<br/>agent ↔ agent"]
        JSONRPC["JSON-RPC 2.0<br/>transport"]
        JSONSCHEMA["JSON Schema<br/>tool 記述"]
    end

    subgraph "Almide 固有"
        EFFECT["effect fn<br/>pure/impure 分離"]
        CAPS["Capability System<br/>13 カテゴリ<br/>[permissions] in almide.toml"]
        WASM3["WASM 3.0<br/>tail call + multi-memory"]
        MANIFEST2["manifest.json<br/>コンパイル時生成"]
    end

    subgraph "ブリッジ (hatch)"
        BRIDGE["hatch<br/>manifest.json → MCP tool defs<br/>wasmtime → function dispatch"]
    end

    EFFECT --> CAPS
    CAPS --> MANIFEST2
    MANIFEST2 --> BRIDGE
    WASM3 --> BRIDGE
    BRIDGE --> MCP
    BRIDGE --> JSONRPC
    BRIDGE --> JSONSCHEMA
    A2A -.->|"将来"| BRIDGE
```

## 既存エコシステムとの関係

```mermaid
graph LR
    subgraph "Almide が作るもの"
        ALMIDE_COMPILER["almide<br/>(compiler + CLI)"]
        HATCH2["hatch<br/>(MCP bridge)"]
    end

    subgraph "外部ツール (使わない)"
        WASSETTE["Wassette (Microsoft)<br/>汎用 WASM Component → MCP<br/>WIT パーサー内蔵"]
    end

    subgraph "外部ツール (使う)"
        WASMTIME2["wasmtime<br/>(WASM runtime)"]
        WASM_TOOLS["wasm-tools<br/>(validation)"]
    end

    subgraph "将来の拡張"
        COMPONENT["WASM Component Model<br/>WIT interface 出力"]
        A2A_BRIDGE["A2A bridge<br/>agent 間通信"]
    end

    ALMIDE_COMPILER -->|"core module"| HATCH2
    HATCH2 -->|"embeds"| WASMTIME2
    WASSETTE -.->|"代替可能だが不要"| HATCH2
    COMPONENT -.->|"将来: Component 出力時"| WASSETTE
    A2A_BRIDGE -.->|"将来: multi-agent"| HATCH2
```

## hatch の責務

```mermaid
graph TB
    subgraph "hatch がやること"
        READ_MANIFEST["1. manifest.json 読み込み<br/>→ MCP tool 定義生成"]
        LOAD_WASM["2. agent.wasm を wasmtime にロード<br/>→ WASI capability 付与"]
        MCP_LOOP["3. MCP JSON-RPC ループ<br/>initialize → tools/list → tools/call"]
        DISPATCH["4. tools/call → WASM 関数呼び出し<br/>→ 結果を MCP response に変換"]
    end

    subgraph "hatch がやらないこと"
        NO_WIT["WIT パース ✗<br/>(manifest.json で十分)"]
        NO_COMPONENT["Component Model ✗<br/>(core module 直接実行)"]
        NO_A2A["A2A ✗<br/>(MCP のみ、将来別ツール)"]
        NO_HTTP["HTTP server ✗<br/>(stdio transport のみ)"]
    end
```
